use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use smithay::{
    backend::{
        allocator::gbm::GbmDevice,
        drm::{DrmDevice, DrmDeviceFd, DrmEvent, DrmNode, NodeType},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{self, UdevBackend, UdevEvent},
    },
    reexports::{
        calloop::{Dispatcher, LoopHandle, RegistrationToken},
        input::Libinput,
        rustix::fs::OFlags,
    },
    utils::DeviceFd,
};
use thiserror::Error;
use tracing::{debug, info, trace, warn};

use super::BackendConfig;
use crate::protocol::RuntimeState;

pub(crate) struct TtyBackend {
    loop_handle: LoopHandle<'static, RuntimeState>,
    session: LibSeatSession,
    libinput: Libinput,
    udev: Dispatcher<'static, UdevBackend, RuntimeState>,
    primary_node: DrmNode,
    render_node: DrmNode,
    devices: HashMap<libc::dev_t, OpenDevice>,
    topology_generation: u64,
}

struct OpenDevice {
    token: RegistrationToken,
    drm: DrmDevice,
    gbm: GbmDevice<DrmDeviceFd>,
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
        let selected_path = match &config.render_device {
            Some(path) => path.clone(),
            None => udev::primary_gpu(&seat)
                .map_err(BackendError::PrimaryGpu)?
                .ok_or(BackendError::NoGpu)?,
        };
        let (primary_node, render_node) = resolve_node_pair(&selected_path)?;

        let udev = Dispatcher::new(udev_backend, |event, _, state: &mut RuntimeState| {
            if let Some(backend) = state.backend.as_mut() {
                backend.handle_udev_event(event);
            }
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
                if let Some(backend) = state.backend.as_mut() {
                    backend.handle_session_event(event);
                }
            })
            .map_err(|error| BackendError::Source(error.to_string()))?;

        let status = backend.status();
        info!(
            seat = status.seat,
            primary_node = %status.primary_node.display(),
            render_node = %status.render_node.display(),
            drm_devices = status.drm_devices,
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
            primary_gbm_ready,
            session_active: self.session.is_active(),
            topology_generation: self.topology_generation,
        }
    }

    fn handle_udev_event(&mut self, event: UdevEvent) {
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
                    self.topology_generation = self.topology_generation.wrapping_add(1);
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

    fn handle_session_event(&mut self, event: SessionEvent) {
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
                self.topology_generation = self.topology_generation.wrapping_add(1);
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
            .insert_source(notifier, move |event, metadata, _| match event {
                DrmEvent::VBlank(crtc) => {
                    trace!(device_id, ?crtc, ?metadata, "DRM vblank");
                }
                DrmEvent::Error(error) => warn!(device_id, %error, "DRM event error"),
            })
            .map_err(|error| BackendError::Source(error.to_string()))?;

        self.devices
            .insert(device_id, OpenDevice { token, drm, gbm });
        self.topology_generation = self.topology_generation.wrapping_add(1);
        info!(device_id, path = %path.display(), "DRM/GBM device opened through libseat");
        Ok(())
    }

    fn remove_device(&mut self, device_id: libc::dev_t) {
        let Some(device) = self.devices.remove(&device_id) else {
            return;
        };
        self.loop_handle.remove(device.token);
        self.topology_generation = self.topology_generation.wrapping_add(1);
        info!(device_id, "DRM/GBM device removed");
    }
}

fn resolve_node_pair(path: &Path) -> Result<(DrmNode, DrmNode), BackendError> {
    let node = DrmNode::from_path(path).map_err(|error| BackendError::Device {
        path: path.to_owned(),
        message: error.to_string(),
    })?;
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
    #[error("failed to select the primary GPU through udev: {0}")]
    PrimaryGpu(std::io::Error),
    #[error("udev did not find a GPU for the active seat")]
    NoGpu,
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
}
