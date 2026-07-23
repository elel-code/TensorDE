use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use smithay::{
    backend::{
        allocator::{
            Format as DrmFormat,
            gbm::{GbmBufferFlags, GbmDevice},
        },
        drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{UdevBackend, UdevEvent},
    },
    output::{Mode, Subpixel},
    reexports::{
        calloop::{Dispatcher, LoopHandle, RegistrationToken},
        input::Libinput,
        rustix::fs::{OFlags, makedev},
    },
    utils::DeviceFd,
};
use smithay_drm_extras::drm_scanner::DrmScanner;
use thiserror::Error;
use tracing::{debug, info, trace, warn};

use super::{
    BackendConfig, BackendOutputEvent,
    output::{ConnectorSnapshot, ConnectorState, OutputPlan, OutputPolicy, diff_output_plans},
};
use crate::{
    protocol::RuntimeState,
    render::{GbmFormatCapability, OutputFormat, VulkanFormatCapability, negotiate_output_formats},
};

mod buffers;
mod kms;

pub(crate) struct TtyBackend {
    loop_handle: LoopHandle<'static, RuntimeState>,
    session: LibSeatSession,
    libinput: Libinput,
    udev: Dispatcher<'static, UdevBackend, RuntimeState>,
    primary_node: DrmNode,
    render_node: DrmNode,
    devices: HashMap<libc::dev_t, OpenDevice>,
    output_policy: OutputPolicy,
    renderer_formats: Vec<VulkanFormatCapability>,
    outputs: OutputPlan,
    pending_outputs: Vec<BackendOutputEvent>,
    topology_generation: u64,
}

struct OpenDevice {
    token: RegistrationToken,
    drm: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,
    scanner: DrmScanner,
    connectors: BTreeMap<super::BackendOutputId, ConnectorSnapshot>,
    output_formats: BTreeMap<super::BackendOutputId, Vec<OutputFormat>>,
    native_targets: BTreeMap<super::BackendOutputId, kms::KmsOutput>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackendStatus {
    pub(crate) seat: String,
    pub(crate) primary_node: PathBuf,
    pub(crate) render_node: PathBuf,
    pub(crate) drm_devices: usize,
    pub(crate) primary_gbm_ready: bool,
    pub(crate) session_active: bool,
    pub(crate) topology_generation: u64,
    pub(crate) outputs: usize,
    pub(crate) native_format_candidates: usize,
}

impl TtyBackend {
    pub(crate) fn new(
        loop_handle: LoopHandle<'static, RuntimeState>,
        config: &BackendConfig,
    ) -> Result<Self, BackendError> {
        let (session, notifier) =
            LibSeatSession::new().map_err(|error| BackendError::Session(error.to_string()))?;
        let seat = session.seat();
        let udev_backend = UdevBackend::new(&seat).map_err(BackendError::Udev)?;
        let initial_devices = udev_backend
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

        let udev = Dispatcher::new(udev_backend, |event, _, state: &mut RuntimeState| {
            state.dispatch_udev_event(event);
        });

        let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
        libinput
            .udev_assign_seat(&seat)
            .map_err(|()| BackendError::LibinputSeat(seat.clone()))?;
        if !session.is_active() {
            libinput.suspend();
        }
        let input_backend = LibinputInputBackend::new(libinput.clone());

        let mut backend = Self {
            loop_handle: loop_handle.clone(),
            session,
            libinput,
            udev: udev.clone(),
            primary_node,
            render_node,
            devices: HashMap::new(),
            output_policy: OutputPolicy,
            renderer_formats: config.renderer_formats.clone(),
            outputs: OutputPlan::new(),
            pending_outputs: Vec::new(),
            topology_generation: 0,
        };

        if backend.session.is_active() {
            backend.reconcile_devices(initial_devices, true)?;
        }

        loop_handle
            .register_dispatcher(udev)
            .map_err(|error| BackendError::Source(error.to_string()))?;
        loop_handle
            .insert_source(input_backend, |event, _, state| {
                state.process_input_event(event);
            })
            .map_err(|error| BackendError::Source(error.to_string()))?;
        loop_handle
            .insert_source(notifier, |event, _, state| {
                state.dispatch_session_event(event);
            })
            .map_err(|error| BackendError::Source(error.to_string()))?;

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

    pub(crate) fn status(&self) -> BackendStatus {
        let primary_gbm_ready =
            self.devices
                .get(&self.primary_node.dev_id())
                .is_some_and(|device| {
                    let _ = &device.gbm;
                    true
                });
        BackendStatus {
            seat: self.session.seat(),
            primary_node: node_path(self.primary_node),
            render_node: node_path(self.render_node),
            drm_devices: self.devices.len(),
            outputs: self.outputs.len(),
            native_format_candidates: self
                .devices
                .values()
                .flat_map(|device| device.output_formats.values())
                .map(Vec::len)
                .sum(),
            primary_gbm_ready,
            session_active: self.session.is_active(),
            topology_generation: self.topology_generation,
        }
    }

    pub(crate) fn take_output_events(&mut self) -> Vec<BackendOutputEvent> {
        std::mem::take(&mut self.pending_outputs)
    }

    pub(crate) fn handle_udev_event(&mut self, event: UdevEvent) {
        match event {
            UdevEvent::Added { device_id, path } => {
                if !self.session.is_active() {
                    return;
                }
                if let Err(error) = self.add_device(device_id, &path) {
                    warn!(%error, path = %path.display(), "failed to add DRM device");
                }
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

    pub(crate) fn handle_session_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::PauseSession => {
                debug!("pausing tty session");
                self.libinput.suspend();
                for device in self.devices.values_mut() {
                    device.drm.pause();
                }
            }
            SessionEvent::ActivateSession => {
                debug!("activating tty session");
                if self.libinput.resume().is_err() {
                    warn!("failed to resume libinput");
                }
                for device in self.devices.values_mut() {
                    if let Err(error) = device.drm.activate(false) {
                        warn!(%error, "failed to reactivate DRM device");
                    }
                }
                let devices = self
                    .udev
                    .as_source_ref()
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
            }
        }
    }

    fn reconcile_devices(
        &mut self,
        mut available: Vec<(libc::dev_t, PathBuf)>,
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

    fn add_device(&mut self, device_id: libc::dev_t, path: &Path) -> Result<(), BackendError> {
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
        let (drm, notifier) =
            DrmDevice::new(device_fd.clone(), false).map_err(|error| BackendError::Device {
                path: path.to_owned(),
                message: error.to_string(),
            })?;
        let gbm = GbmDevice::new(device_fd).map_err(|error| BackendError::Device {
            path: path.to_owned(),
            message: format!("GBM initialization failed: {error}"),
        })?;
        let token = self
            .loop_handle
            .insert_source(
                notifier,
                move |event, metadata, state: &mut RuntimeState| match event {
                    DrmEvent::VBlank(crtc) => {
                        trace!(device_id, ?crtc, ?metadata, "DRM vblank");
                        state.dispatch_drm_vblank(device_id, crtc, *metadata);
                    }
                    DrmEvent::Error(error) => warn!(device_id, %error, "DRM event error"),
                },
            )
            .map_err(|error| BackendError::Source(error.to_string()))?;

        self.devices.insert(
            device_id,
            OpenDevice {
                token,
                drm,
                gbm,
                scanner: DrmScanner::new(),
                connectors: BTreeMap::new(),
                output_formats: BTreeMap::new(),
                native_targets: BTreeMap::new(),
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

    fn remove_device(&mut self, device_id: libc::dev_t) {
        let Some(device) = self.devices.remove(&device_id) else {
            return;
        };
        self.loop_handle.remove(device.token);
        self.topology_generation = self.topology_generation.wrapping_add(1);
        self.reconcile_outputs();
        info!(device_id, "DRM/GBM device removed");
    }

    fn rescan_device(&mut self, device_id: libc::dev_t) -> Result<(), BackendError> {
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
        self.pending_outputs
            .extend(diff_output_plans(&self.outputs, &current));
        self.outputs = current;
    }
}

fn negotiate_device_output_formats(
    device_id: libc::dev_t,
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
            if !kms_scanout.contains(&format) {
                kms_scanout.push(format);
            }
        }

        let usage = GbmBufferFlags::SCANOUT | GbmBufferFlags::RENDERING;
        let gbm = kms_scanout
            .iter()
            .copied()
            .map(|format| GbmFormatCapability {
                format,
                scanout: device.gbm.is_format_supported(format.code, usage),
                plane_count: device
                    .gbm
                    .format_modifier_plane_count(format.code, format.modifier),
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
            modifier = %format_args!("{:#x}", u64::from(preferred.format.modifier)),
            planes = preferred.plane_count,
            candidates = candidates.len(),
            "native output formats negotiated"
        );
        negotiated.insert(output.id, candidates);
    }
    Ok(negotiated)
}

fn describe_connector(
    device_id: libc::dev_t,
    connector: &smithay::reexports::drm::control::connector::Info,
    crtc: Option<smithay::reexports::drm::control::crtc::Handle>,
) -> ConnectorSnapshot {
    let modes = connector
        .modes()
        .iter()
        .copied()
        .map(Mode::from)
        .collect::<Vec<_>>();
    let preferred_mode = connector
        .modes()
        .iter()
        .position(|mode| {
            mode.mode_type()
                .contains(smithay::reexports::drm::control::ModeTypeFlags::PREFERRED)
        })
        .or_else(|| (!modes.is_empty()).then_some(0))
        .map(|index| modes[index]);
    let physical_size = connector.size().unwrap_or((0, 0));
    ConnectorSnapshot {
        id: super::BackendOutputId {
            device_id,
            connector_id: connector.handle().into(),
        },
        name: connector.to_string(),
        state: match connector.state() {
            smithay::reexports::drm::control::connector::State::Connected => {
                ConnectorState::Connected
            }
            smithay::reexports::drm::control::connector::State::Disconnected => {
                ConnectorState::Disconnected
            }
            smithay::reexports::drm::control::connector::State::Unknown => ConnectorState::Unknown,
        },
        physical_size: (physical_size.0 as i32, physical_size.1 as i32),
        subpixel: Subpixel::from(connector.subpixel()),
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
    #[error("selected Vulkan DRM node {node} is unavailable to Smithay: {message}")]
    SelectedNode {
        node: crate::render::DrmNodeId,
        message: String,
    },
    #[error("failed to assign seat {0} to libinput")]
    LibinputSeat(String),
    #[error("failed to initialize DRM device {path}: {message}")]
    Device { path: PathBuf, message: String },
    #[error("configured DRM node {path} has unsupported type {node_type}")]
    UnsupportedNode { path: PathBuf, node_type: String },
    #[error("DRM node {path} has no usable {target} node")]
    MissingPairedNode { path: PathBuf, target: String },
    #[error("primary DRM device {path} is unavailable")]
    PrimaryUnavailable { path: PathBuf },
    #[error("failed to register a tty event source: {0}")]
    Source(String),
    #[error("unknown DRM device {device_id}")]
    UnknownDevice { device_id: libc::dev_t },
    #[error("unknown output {0:?}")]
    UnknownOutput(super::BackendOutputId),
    #[error("failed to scan DRM connectors for device {device_id}: {message}")]
    ConnectorScan {
        device_id: libc::dev_t,
        message: String,
    },
    #[error("failed to negotiate a native output format for {output}: {message}")]
    OutputFormats { output: String, message: String },
    #[error("failed to install native output buffers for {output}: {message}")]
    OutputBuffers { output: String, message: String },
    #[error("failed to submit an atomic KMS frame for {output}: {message}")]
    KmsFrame { output: String, message: String },
}
