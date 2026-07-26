use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use gbm::{BufferObjectFlags, Device as GbmDevice};
use rustix::fs::{OFlags, makedev};
use smithay::{
    backend::drm::{DrmDevice, DrmDeviceFd, DrmNode, NodeType},
    utils::DeviceFd,
};
use thiserror::Error;
use tracing::{debug, info, warn};

use super::{
    BackendConfig, BackendOutputEvent,
    host_map::{host_drm_format, physical_mode_from_smithay, subpixel_from_smithay},
    output::{ConnectorSnapshot, OutputPlan, OutputPolicy, diff_output_plans},
};
use crate::render::{
    GbmFormatCapability, OutputFormat, VulkanFormatCapability, negotiate_output_formats,
};
use tensor_host::{ConnectorState, DrmFormat};
use tensor_runtime::{
    OpaqueFdCompletion, OpaqueFdCompletionRuntime, WakeSink, WorkerBridge, WorkerRx,
};

mod buffers;
mod completion;
mod gamma;
mod kms;
mod libinput;
mod management;
mod scanner;
mod session;
mod status;
mod udev;

pub(crate) use libinput::LibinputEvent;
use libinput::LibinputSource;
use scanner::DrmScanner;
use session::SeatSession;
pub(crate) use udev::UdevEvent;
use udev::UdevMonitor;

const MAX_PENDING_SESSION_COMPLETIONS: usize = 1;
const MAX_PENDING_SESSION_FAILURES: usize = 1;
const MAX_PENDING_UDEV_COMPLETIONS: usize = 1;
const MAX_PENDING_UDEV_FAILURES: usize = 1;
const MAX_PENDING_LIBINPUT_COMPLETIONS: usize = 1;
const MAX_PENDING_LIBINPUT_FAILURES: usize = 1;

pub(crate) struct TtyBackend {
    session: SeatSession,
    session_completions: WorkerRx<OpaqueFdCompletion>,
    session_failures: WorkerRx<String>,
    active_session_completion: Option<OpaqueFdCompletion>,
    _session_completion_runtime: OpaqueFdCompletionRuntime,
    libinput: LibinputSource,
    libinput_completions: WorkerRx<OpaqueFdCompletion>,
    libinput_failures: WorkerRx<String>,
    active_libinput_completion: Option<OpaqueFdCompletion>,
    _libinput_completion_runtime: OpaqueFdCompletionRuntime,
    udev: UdevMonitor,
    udev_completions: WorkerRx<OpaqueFdCompletion>,
    udev_failures: WorkerRx<String>,
    active_udev_completion: Option<OpaqueFdCompletion>,
    _udev_completion_runtime: OpaqueFdCompletionRuntime,
    primary_node: DrmNode,
    render_node: DrmNode,
    devices: HashMap<u64, OpenDevice>,
    output_policy: OutputPolicy,
    renderer_formats: Vec<VulkanFormatCapability>,
    outputs: OutputPlan,
    pending_outputs: Vec<BackendOutputEvent>,
    topology_generation: u64,
}

struct OpenDevice {
    drm: DrmDevice,
    monotonic_timestamps: bool,
    gbm: GbmDevice<DrmDeviceFd>,
    scanner: DrmScanner,
    connectors: BTreeMap<super::BackendOutputId, ConnectorSnapshot>,
    output_formats: BTreeMap<super::BackendOutputId, Vec<OutputFormat>>,
    native_targets: BTreeMap<super::BackendOutputId, kms::KmsOutput>,
    /// Per-output gamma LUT state (atomic blob or legacy). Not on the flip path.
    gamma: BTreeMap<super::BackendOutputId, gamma::OutputGamma>,
}

impl TtyBackend {
    pub(crate) fn new(
        config: &BackendConfig,
        completion_wake: Arc<dyn WakeSink>,
    ) -> Result<Self, BackendError> {
        let session =
            SeatSession::new().map_err(|error| BackendError::Session(error.to_string()))?;
        let seat = session.seat();
        let udev = UdevMonitor::new(&seat).map_err(BackendError::Udev)?;
        let initial_devices = udev
            .device_list()
            .map(|(device_id, path)| (device_id, path.to_owned()))
            .collect::<Vec<_>>();
        let selected_node =
            DrmNode::from_dev_id(makedev(config.drm_node.major(), config.drm_node.minor()))
                .map_err(|error| BackendError::SelectedNode {
                    node: config.drm_node,
                    message: error.to_string(),
                })?;
        let selected_path = node_path(selected_node);
        let (primary_node, render_node) = resolve_node_pair(selected_node, &selected_path)?;

        let libinput = LibinputSource::new(session.clone(), &seat, session.is_active())
            .map_err(|()| BackendError::LibinputSeat(seat.clone()))?;

        let (session_completion_sender, session_completions) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_SESSION_COMPLETIONS,
            Arc::clone(&completion_wake),
        );
        let (session_failure_sender, session_failures) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_SESSION_FAILURES,
            Arc::clone(&completion_wake),
        );
        let session_completion_runtime = OpaqueFdCompletionRuntime::start(
            "tensor-libseat-completions",
            &session,
            session_completion_sender,
            session_failure_sender,
        )
        .map_err(|error| BackendError::SessionCompletion(error.to_string()))?;
        let (udev_completion_sender, udev_completions) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_UDEV_COMPLETIONS,
            Arc::clone(&completion_wake),
        );
        let (udev_failure_sender, udev_failures) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_UDEV_FAILURES,
            Arc::clone(&completion_wake),
        );
        let udev_completion_runtime = OpaqueFdCompletionRuntime::start(
            "tensor-udev-completions",
            &udev,
            udev_completion_sender,
            udev_failure_sender,
        )
        .map_err(|error| BackendError::UdevCompletion(error.to_string()))?;
        let (libinput_completion_sender, libinput_completions) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_LIBINPUT_COMPLETIONS,
            Arc::clone(&completion_wake),
        );
        let (libinput_failure_sender, libinput_failures) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_LIBINPUT_FAILURES, completion_wake);
        let libinput_completion_runtime = OpaqueFdCompletionRuntime::start(
            "tensor-libinput-completions",
            &libinput,
            libinput_completion_sender,
            libinput_failure_sender,
        )
        .map_err(|error| BackendError::LibinputCompletion(error.to_string()))?;

        let mut backend = Self {
            session,
            session_completions,
            session_failures,
            active_session_completion: None,
            _session_completion_runtime: session_completion_runtime,
            libinput,
            libinput_completions,
            libinput_failures,
            active_libinput_completion: None,
            _libinput_completion_runtime: libinput_completion_runtime,
            udev,
            udev_completions,
            udev_failures,
            active_udev_completion: None,
            _udev_completion_runtime: udev_completion_runtime,
            primary_node,
            render_node,
            devices: HashMap::new(),
            output_policy: OutputPolicy::new(config.output_rules.clone()),
            renderer_formats: config.renderer_formats.clone(),
            outputs: OutputPlan::new(),
            pending_outputs: Vec::new(),
            topology_generation: 0,
        };

        if backend.session.is_active() {
            backend.reconcile_devices(initial_devices, true)?;
        }

        let status = backend.status();
        info!(
            seat = status.seat,
            primary_node = %status.primary_node.display(),
            render_node = %status.render_node.display(),
            drm_devices = status.drm_devices,
            outputs = status.outputs,
            native_format_candidates = status.native_format_candidates,
            primary_gbm_ready = status.primary_gbm_ready,
            session_active = status.session_active,
            "Smithay tty backend initialized"
        );
        Ok(backend)
    }

    pub(crate) fn take_output_events(&mut self) -> Vec<BackendOutputEvent> {
        std::mem::take(&mut self.pending_outputs)
    }

    /// Runtime output policy table (for IPC introspection).
    pub(crate) fn output_rules(
        &self,
    ) -> std::collections::BTreeMap<String, crate::config::OutputRule> {
        self.output_policy.rules()
    }

    /// Upsert one named rule and replan (position / enable / scale intent).
    ///
    /// Mode/CRTC rebinding still needs a completed modeset generation for
    /// buffer replacement; plan diffs surface as topology events.
    pub(crate) fn upsert_output_rule(&mut self, name: String, rule: crate::config::OutputRule) {
        self.output_policy.upsert_rule(name, rule);
        self.reconcile_outputs();
    }

    /// Replace output policy atomically and produce one topology diff.
    pub(crate) fn replace_output_rules(
        &mut self,
        rules: std::collections::BTreeMap<String, crate::config::OutputRule>,
    ) {
        self.output_policy = OutputPolicy::new(rules);
        self.reconcile_outputs();
    }

    /// Primary DRM fd shared with Smithay's syncobj protocol owner.  This is
    /// the same primary/render identity selected by Vulkan; the backend never
    /// performs an independent GPU choice.
    pub(crate) fn syncobj_device(&self) -> Option<DrmDeviceFd> {
        if !self.session.is_active() {
            return None;
        }
        self.devices
            .get(&self.primary_node.dev_id())
            .map(|device| device.drm.device_fd().clone())
    }

    /// LUT size for `zwlr_gamma_control_v1`, when the CRTC exposes gamma.
    pub(crate) fn gamma_size(&self, output: &smithay::output::Output) -> Option<u32> {
        let id = *output.user_data().get::<super::BackendOutputId>()?;
        let device = self.devices.get(&id.device_id)?;
        let state = device.gamma.get(&id)?;
        state.gamma_size(&device.drm)
    }

    /// Apply or reset a gamma ramp for an output (protocol boundary).
    ///
    /// Does not touch scanout or renderer state; cost is one property/blob
    /// ioctl (or legacy gamma ioctl) proportional to the hardware LUT length.
    pub(crate) fn set_gamma(
        &mut self,
        output: &smithay::output::Output,
        ramp: Option<&[u16]>,
    ) -> Option<()> {
        let id = *output.user_data().get::<super::BackendOutputId>()?;
        let session_active = self.session.is_active();
        let device = self.devices.get_mut(&id.device_id)?;
        let state = device.gamma.get_mut(&id)?;
        match state.set_gamma(&device.drm, ramp, session_active) {
            Ok(()) => Some(()),
            Err(error) => {
                warn!(
                    output = %output.name(),
                    %error,
                    "failed to apply gamma ramp"
                );
                None
            }
        }
    }

    pub(crate) fn change_vt(&mut self, vt: i32) {
        if let Err(error) = self.session.change_vt(vt) {
            warn!(%error, vt, "failed to switch virtual terminal through libseat");
        }
    }

    pub(crate) fn handle_udev_event(&mut self, event: UdevEvent) {
        match event {
            UdevEvent::Added { device_id } => {
                if !self.session.is_active() {
                    return;
                }
                let Some(path) = self.udev.take_device_path(device_id) else {
                    warn!(device_id, "udev added an untracked DRM device");
                    return;
                };
                if let Err(error) = self.add_device(device_id, &path) {
                    warn!(%error, path = %path.display(), "failed to add DRM device");
                }
                self.udev.restore_device_path(device_id, path);
            }
            UdevEvent::Changed { device_id } => {
                if self.session.is_active() && self.devices.contains_key(&device_id) {
                    if let Err(error) = self.rescan_device(device_id) {
                        warn!(%error, device_id, "failed to rescan DRM connectors");
                    }
                    debug!(device_id, "DRM connector topology changed");
                }
            }
            UdevEvent::Removed { device_id } => {
                if self.session.is_active() {
                    self.remove_device(device_id);
                }
            }
        }
    }

    pub(crate) fn handle_session_event(&mut self, event: tensor_host::SessionEvent) {
        match event {
            tensor_host::SessionEvent::Paused => {
                debug!("pausing tty session");
                self.libinput.suspend();
                for device in self.devices.values_mut() {
                    device.drm.pause();
                }
            }
            tensor_host::SessionEvent::Activated => {
                debug!("activating tty session");
                if self.libinput.resume().is_err() {
                    warn!("failed to resume libinput");
                }
                for device in self.devices.values_mut() {
                    if let Err(error) = device.drm.activate(false) {
                        warn!(%error, "failed to reactivate DRM device");
                    }
                    for gamma in device.gamma.values_mut() {
                        gamma.restore_after_session_resume(&device.drm);
                    }
                }
                let devices = self
                    .udev
                    .device_list()
                    .map(|(device_id, path)| (device_id, path.to_owned()))
                    .collect::<Vec<_>>();
                if let Err(error) = self.reconcile_devices(devices, false) {
                    warn!(%error, "failed to reconcile DRM devices after session activation");
                }
                let device_ids = self.devices.keys().copied().collect::<Vec<_>>();
                for device_id in device_ids {
                    if let Err(error) = self.rescan_device(device_id) {
                        warn!(%error, device_id, "failed to rescan DRM connectors after activation");
                    }
                }
                self.reset_outputs_after_session_resume();
            }
        }
    }

    fn reconcile_devices(
        &mut self,
        mut available: Vec<(u64, PathBuf)>,
        require_primary: bool,
    ) -> Result<(), BackendError> {
        let available_ids = available
            .iter()
            .map(|(device_id, _)| *device_id)
            .collect::<Vec<_>>();
        let removed = self
            .devices
            .keys()
            .copied()
            .filter(|device_id| !available_ids.contains(device_id))
            .collect::<Vec<_>>();
        for device_id in removed {
            self.remove_device(device_id);
        }

        available.sort_by_key(|(device_id, _)| *device_id != self.primary_node.dev_id());
        let mut primary_error = None;
        for (device_id, path) in available {
            if self.devices.contains_key(&device_id) {
                continue;
            }
            if let Err(error) = self.add_device(device_id, &path) {
                if device_id == self.primary_node.dev_id() {
                    primary_error = Some(error);
                } else {
                    warn!(%error, path = %path.display(), "skipping unavailable DRM device");
                }
            }
        }

        if require_primary && !self.devices.contains_key(&self.primary_node.dev_id()) {
            return Err(
                primary_error.unwrap_or_else(|| BackendError::PrimaryUnavailable {
                    path: node_path(self.primary_node),
                }),
            );
        }
        Ok(())
    }

    fn add_device(&mut self, device_id: u64, path: &Path) -> Result<(), BackendError> {
        let node = DrmNode::from_dev_id(device_id).map_err(|error| BackendError::Device {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
        if node.ty() != NodeType::Primary || self.devices.contains_key(&device_id) {
            return Ok(());
        }

        let flags = OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK;
        let fd = self
            .session
            .open(path, flags)
            .map_err(|error| BackendError::Device {
                path: path.to_owned(),
                message: format!("libseat open failed: {error:?}"),
            })?;
        let device_fd = DrmDeviceFd::new(DeviceFd::from(fd));
        let (drm, _notifier) =
            DrmDevice::new(device_fd.clone(), false).map_err(|error| BackendError::Device {
                path: path.to_owned(),
                message: error.to_string(),
            })?;
        let monotonic_timestamps =
            drm::Device::get_driver_capability(&drm, drm::DriverCapability::MonotonicTimestamp)
                .unwrap_or(0)
                == 1;
        let gbm = GbmDevice::new(device_fd).map_err(|error| BackendError::Device {
            path: path.to_owned(),
            message: format!("GBM initialization failed: {error}"),
        })?;

        self.devices.insert(
            device_id,
            OpenDevice {
                drm,
                monotonic_timestamps,
                gbm,
                scanner: DrmScanner::new(),
                connectors: BTreeMap::new(),
                output_formats: BTreeMap::new(),
                native_targets: BTreeMap::new(),
                gamma: BTreeMap::new(),
            },
        );
        self.topology_generation = self.topology_generation.wrapping_add(1);
        if let Err(error) = self.rescan_device(device_id) {
            self.remove_device(device_id);
            return Err(error);
        }
        info!(device_id, path = %path.display(), "DRM/GBM device opened through libseat");
        Ok(())
    }

    fn remove_device(&mut self, device_id: u64) {
        let Some(_device) = self.devices.remove(&device_id) else {
            return;
        };
        self.topology_generation = self.topology_generation.wrapping_add(1);
        self.reconcile_outputs();
        info!(device_id, "DRM/GBM device removed");
    }

    fn rescan_device(&mut self, device_id: u64) -> Result<(), BackendError> {
        let renderer_formats = &self.renderer_formats;
        let changed = {
            let device = self
                .devices
                .get_mut(&device_id)
                .ok_or(BackendError::UnknownDevice { device_id })?;
            device
                .scanner
                .scan_connectors(&device.drm)
                .map_err(|error| BackendError::ConnectorScan {
                    device_id,
                    message: error.to_string(),
                })?;

            let mut current = device
                .scanner
                .connectors()
                .values()
                .map(|connector| {
                    describe_connector(
                        device_id,
                        connector,
                        device.scanner.crtc_for_connector(&connector.handle()),
                    )
                })
                .map(|connector| (connector.id, connector))
                .collect::<BTreeMap<_, _>>();
            let output_formats =
                negotiate_device_output_formats(device_id, device, &current, renderer_formats)?;
            for (output_id, formats) in &output_formats {
                current
                    .get_mut(output_id)
                    .expect("format negotiation returned an unknown output")
                    .native_format = formats.first().copied();
            }
            if current == device.connectors && output_formats == device.output_formats {
                false
            } else {
                let unchanged = current
                    .iter()
                    .filter_map(|(id, connector)| {
                        (device.connectors.get(id) == Some(connector)).then_some(*id)
                    })
                    .collect::<std::collections::BTreeSet<_>>();
                device.native_targets.retain(|id, _| unchanged.contains(id));
                device.gamma.retain(|id, _| unchanged.contains(id));
                device.connectors = current;
                device.output_formats = output_formats;
                true
            }
        };

        if changed {
            self.topology_generation = self.topology_generation.wrapping_add(1);
            self.reconcile_outputs();
        }
        Ok(())
    }

    fn reconcile_outputs(&mut self) {
        let current = self.output_policy.plan(
            self.devices
                .values()
                .flat_map(|device| device.connectors.values()),
        );
        self.ensure_gamma_for_plan(&current);
        self.pending_outputs
            .extend(diff_output_plans(&self.outputs, &current));
        self.outputs = current;
    }

    /// Bind gamma state for every planned output (cheap: property probe once).
    fn ensure_gamma_for_plan(&mut self, plan: &OutputPlan) {
        for descriptor in plan.values() {
            let device_id = descriptor.id.device_id;
            let Some(device) = self.devices.get_mut(&device_id) else {
                continue;
            };
            if device.gamma.contains_key(&descriptor.id) {
                continue;
            }
            let Some(crtc) = gamma::crtc_handle(descriptor.crtc) else {
                continue;
            };
            let state = gamma::OutputGamma::new(&device.drm, crtc);
            device.gamma.insert(descriptor.id, state);
        }
        for device in self.devices.values_mut() {
            device.gamma.retain(|id, _| plan.contains_key(id));
        }
    }
}

fn negotiate_device_output_formats(
    device_id: u64,
    device: &OpenDevice,
    connectors: &BTreeMap<super::BackendOutputId, ConnectorSnapshot>,
    renderer_formats: &[VulkanFormatCapability],
) -> Result<BTreeMap<super::BackendOutputId, Vec<OutputFormat>>, BackendError> {
    let mut negotiated = BTreeMap::new();
    for output in connectors.values().filter(|connector| {
        connector.state == ConnectorState::Connected
            && connector.preferred_mode.is_some()
            && connector.mapped_crtc.is_some()
    }) {
        let mapped_crtc = output
            .mapped_crtc
            .expect("filtered output must have a mapped CRTC");
        let crtc = device
            .drm
            .crtcs()
            .iter()
            .copied()
            .find(|crtc| u32::from(*crtc) == mapped_crtc)
            .ok_or_else(|| BackendError::OutputFormats {
                output: output.name.clone(),
                message: format!("mapped CRTC {mapped_crtc} disappeared"),
            })?;
        let planes = device
            .drm
            .planes(&crtc)
            .map_err(|error| BackendError::OutputFormats {
                output: output.name.clone(),
                message: error.to_string(),
            })?;
        let mut kms_scanout = Vec::<DrmFormat>::new();
        for format in planes
            .primary
            .iter()
            .flat_map(|plane| plane.formats.iter())
            .copied()
        {
            let host = host_drm_format(format);
            if !kms_scanout.contains(&host) {
                kms_scanout.push(host);
            }
        }

        let usage = BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING;
        let gbm = kms_scanout
            .iter()
            .copied()
            .filter_map(|format| {
                let code = gbm::Format::try_from(format.code.raw()).ok()?;
                Some(GbmFormatCapability {
                    format,
                    scanout: device.gbm.is_format_supported(code, usage),
                    plane_count: device.gbm.format_modifier_plane_count(
                        code,
                        gbm::Modifier::from(format.modifier.raw()),
                    ),
                })
            })
            .collect::<Vec<_>>();
        let candidates =
            negotiate_output_formats(renderer_formats, &kms_scanout, &gbm).map_err(|error| {
                BackendError::OutputFormats {
                    output: output.name.clone(),
                    message: error.to_string(),
                }
            })?;
        let preferred = candidates[0];
        debug!(
            device_id,
            output = output.name,
            format = %preferred.format.code,
            modifier = %preferred.format.modifier,
            planes = preferred.plane_count,
            candidates = candidates.len(),
            "native output formats negotiated"
        );
        negotiated.insert(output.id, candidates);
    }
    Ok(negotiated)
}

fn describe_connector(
    device_id: u64,
    connector: &drm::control::connector::Info,
    crtc: Option<drm::control::crtc::Handle>,
) -> ConnectorSnapshot {
    let modes = connector
        .modes()
        .iter()
        .copied()
        .filter(|mode| !mode.flags().contains(drm::control::ModeFlags::INTERLACE))
        .map(|mode| physical_mode_from_smithay(smithay::output::Mode::from(mode)))
        .collect::<Vec<_>>();
    let preferred_mode = connector
        .modes()
        .iter()
        .copied()
        .find(|mode| {
            mode.mode_type()
                .contains(drm::control::ModeTypeFlags::PREFERRED)
                && !mode.flags().contains(drm::control::ModeFlags::INTERLACE)
        })
        .map(|mode| physical_mode_from_smithay(smithay::output::Mode::from(mode)))
        .or_else(|| modes.first().copied());
    let physical_size = connector.size().unwrap_or((0, 0));
    ConnectorSnapshot {
        id: super::BackendOutputId::new(device_id, connector.handle().into()),
        name: connector.to_string(),
        state: match connector.state() {
            drm::control::connector::State::Connected => ConnectorState::Connected,
            drm::control::connector::State::Disconnected => ConnectorState::Disconnected,
            drm::control::connector::State::Unknown => ConnectorState::Unknown,
        },
        physical_size: (physical_size.0 as i32, physical_size.1 as i32),
        subpixel: subpixel_from_smithay(smithay::output::Subpixel::from(connector.subpixel())),
        modes,
        preferred_mode,
        mapped_crtc: crtc.map(Into::into),
        native_format: None,
    }
}

fn resolve_node_pair(node: DrmNode, path: &Path) -> Result<(DrmNode, DrmNode), BackendError> {
    let (primary, render) = match node.ty() {
        NodeType::Primary => {
            let render = paired_node(node, NodeType::Render, path)?;
            (node, render)
        }
        NodeType::Render => {
            let primary = paired_node(node, NodeType::Primary, path)?;
            (primary, node)
        }
        node_type => {
            return Err(BackendError::UnsupportedNode {
                path: path.to_owned(),
                node_type: format!("{node_type:?}"),
            });
        }
    };
    Ok((primary, render))
}

fn paired_node(node: DrmNode, target: NodeType, path: &Path) -> Result<DrmNode, BackendError> {
    node.node_with_type(target)
        .ok_or_else(|| BackendError::MissingPairedNode {
            path: path.to_owned(),
            target: format!("{target:?}"),
        })?
        .map_err(|error| BackendError::Device {
            path: path.to_owned(),
            message: error.to_string(),
        })
}

fn node_path(node: DrmNode) -> PathBuf {
    node.dev_path()
        .unwrap_or_else(|| PathBuf::from(format!("{node}")))
}

#[derive(Debug, Error)]
pub(crate) enum BackendError {
    #[error("failed to create the libseat session: {0}")]
    Session(String),
    #[error("failed to enumerate DRM devices through udev: {0}")]
    Udev(std::io::Error),
    #[error("failed to initialize the udev completion runtime: {0}")]
    UdevCompletion(String),
    #[error("selected Vulkan DRM node {node} is unavailable to Smithay: {message}")]
    SelectedNode {
        node: crate::render::DrmNodeId,
        message: String,
    },
    #[error("failed to assign seat {0} to libinput")]
    LibinputSeat(String),
    #[error("failed to initialize the libinput completion runtime: {0}")]
    LibinputCompletion(String),
    #[error("failed to initialize the libseat completion runtime: {0}")]
    SessionCompletion(String),
    #[error("failed to initialize DRM device {path}: {message}")]
    Device { path: PathBuf, message: String },
    #[error("configured DRM node {path} has unsupported type {node_type}")]
    UnsupportedNode { path: PathBuf, node_type: String },
    #[error("DRM node {path} has no usable {target} node")]
    MissingPairedNode { path: PathBuf, target: String },
    #[error("primary DRM device {path} is unavailable")]
    PrimaryUnavailable { path: PathBuf },
    #[error("unknown DRM device {device_id}")]
    UnknownDevice { device_id: u64 },
    #[error("unknown output {0:?}")]
    UnknownOutput(super::BackendOutputId),
    #[error("failed to scan DRM connectors for device {device_id}: {message}")]
    ConnectorScan { device_id: u64, message: String },
    #[error("failed to negotiate a native output format for {output}: {message}")]
    OutputFormats { output: String, message: String },
    #[error("failed to install native output buffers for {output}: {message}")]
    OutputBuffers { output: String, message: String },
    #[error("failed to submit an atomic KMS frame for {output}: {message}")]
    KmsFrame { output: String, message: String },
}
