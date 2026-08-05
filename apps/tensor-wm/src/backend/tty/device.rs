//! Tensor-owned DRM file descriptor, atomic device state, and tty restoration.
//!
//! The initial property snapshot and restore order are adapted from Smithay's
//! atomic DRM device. See `LICENSES/Smithay-MIT.txt`.

use std::{
    cell::Cell,
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd},
    rc::Rc,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use drm::{
    ClientCapability, Device as BasicDevice,
    control::{
        AtomicCommitFlags, Device as ControlDevice, PropertyValueSet, ResourceHandle,
        ResourceHandles, atomic::AtomicModeReq, connector, crtc, framebuffer, plane, property,
    },
};
use thiserror::Error;
use tracing::{error, warn};

type DeviceSnapshot = (
    Vec<(connector::Handle, PropertyValueSet)>,
    Vec<(crtc::Handle, PropertyValueSet)>,
    Vec<(framebuffer::Handle, PropertyValueSet)>,
    Vec<(plane::Handle, PropertyValueSet)>,
);

#[derive(Debug)]
struct DrmDeviceFdInner {
    fd: OwnedFd,
    privileged: bool,
    master_held: AtomicBool,
}

impl Drop for DrmDeviceFdInner {
    fn drop(&mut self) {
        if self.privileged
            && self.master_held.swap(false, Ordering::AcqRel)
            && let Err(error) = self.release_master_lock()
        {
            error!(%error, "failed to release DRM master while closing device");
        }
    }
}

impl AsFd for DrmDeviceFdInner {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl BasicDevice for DrmDeviceFdInner {}
impl ControlDevice for DrmDeviceFdInner {}

/// Shared DRM file description used by GBM, KMS framebuffers, and syncobj.
#[derive(Clone, Debug)]
pub(crate) struct DrmDeviceFd(Arc<DrmDeviceFdInner>);

impl DrmDeviceFd {
    fn new(fd: OwnedFd) -> Self {
        let mut inner = DrmDeviceFdInner {
            fd,
            privileged: false,
            master_held: AtomicBool::new(false),
        };
        if inner.acquire_master_lock().is_ok() {
            inner.privileged = true;
            inner.master_held.store(true, Ordering::Release);
        } else {
            warn!("could not explicitly acquire DRM master; using kernel-managed access");
        }
        Self(Arc::new(inner))
    }

    pub(crate) fn downgrade(&self) -> WeakDrmDeviceFd {
        WeakDrmDeviceFd(Arc::downgrade(&self.0))
    }

    fn pause(&self) {
        if self.0.privileged
            && self.0.master_held.swap(false, Ordering::AcqRel)
            && let Err(error) = self.release_master_lock()
        {
            warn!(%error, "failed to release DRM master while pausing session");
        }
    }

    fn activate(&self) -> io::Result<()> {
        if self.0.privileged && !self.0.master_held.load(Ordering::Acquire) {
            self.acquire_master_lock()?;
            self.0.master_held.store(true, Ordering::Release);
        }
        Ok(())
    }
}

impl PartialEq for DrmDeviceFd {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for DrmDeviceFd {}

impl AsFd for DrmDeviceFd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.fd.as_fd()
    }
}

impl AsRawFd for DrmDeviceFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.fd.as_raw_fd()
    }
}

impl BasicDevice for DrmDeviceFd {}
impl ControlDevice for DrmDeviceFd {}

#[derive(Clone, Debug, Default)]
pub(crate) struct WeakDrmDeviceFd(Weak<DrmDeviceFdInner>);

impl WeakDrmDeviceFd {
    pub(crate) fn upgrade(&self) -> Option<DrmDeviceFd> {
        self.0.upgrade().map(DrmDeviceFd)
    }
}

/// Atomic-only DRM device owned by the compositor thread.
#[derive(Debug)]
pub(super) struct DrmDevice {
    fd: DrmDeviceFd,
    active: Rc<Cell<bool>>,
    resources: ResourceHandles,
    initial_state: DeviceSnapshot,
}

impl DrmDevice {
    pub(super) fn new(fd: OwnedFd) -> Result<Self, DrmDeviceError> {
        let fd = DrmDeviceFd::new(fd);
        fd.set_client_capability(ClientCapability::UniversalPlanes, true)
            .map_err(DrmDeviceError::UniversalPlanes)?;
        fd.set_client_capability(ClientCapability::Atomic, true)
            .map_err(DrmDeviceError::Atomic)?;

        let resources = fd.resource_handles().map_err(DrmDeviceError::Resources)?;
        let planes = fd.plane_handles().map_err(DrmDeviceError::Planes)?;
        let initial_state = (
            snapshot_properties(&fd, resources.connectors())?,
            snapshot_properties(&fd, resources.crtcs())?,
            snapshot_properties(&fd, resources.framebuffers())?,
            snapshot_properties(&fd, &planes)?,
        );
        Ok(Self {
            fd,
            active: Rc::new(Cell::new(true)),
            resources,
            initial_state,
        })
    }

    pub(super) fn device_fd(&self) -> &DrmDeviceFd {
        &self.fd
    }

    pub(super) fn active_handle(&self) -> Rc<Cell<bool>> {
        Rc::clone(&self.active)
    }

    pub(super) fn is_active(&self) -> bool {
        self.active.get()
    }

    pub(super) fn crtcs(&self) -> &[crtc::Handle] {
        self.resources.crtcs()
    }

    pub(super) fn pause(&self) {
        self.active.set(false);
        self.fd.pause();
    }

    pub(super) fn activate(&self) -> Result<(), DrmDeviceError> {
        self.fd.activate().map_err(DrmDeviceError::AcquireMaster)?;
        self.active.set(true);
        Ok(())
    }

    pub(super) fn reset_state(&self) -> Result<(), DrmDeviceError> {
        if !self.active.get() {
            return Err(DrmDeviceError::Inactive);
        }
        let resources = self
            .fd
            .resource_handles()
            .map_err(DrmDeviceError::Resources)?;
        let planes = self.fd.plane_handles().map_err(DrmDeviceError::Planes)?;
        let mut request = AtomicModeReq::new();
        for &connector in resources.connectors() {
            request.add_property(
                connector,
                required_property(&self.fd, connector, "CRTC_ID")?,
                property::Value::CRTC(None),
            );
        }
        for plane in planes {
            request.add_property(
                plane,
                required_property(&self.fd, plane, "CRTC_ID")?,
                property::Value::CRTC(None),
            );
            request.add_property(
                plane,
                required_property(&self.fd, plane, "FB_ID")?,
                property::Value::Framebuffer(None),
            );
        }
        for &crtc in resources.crtcs() {
            request.add_property(
                crtc,
                required_property(&self.fd, crtc, "ACTIVE")?,
                property::Value::Boolean(false),
            );
            request.add_property(
                crtc,
                required_property(&self.fd, crtc, "MODE_ID")?,
                property::Value::Blob(0),
            );
        }
        self.fd
            .atomic_commit(AtomicCommitFlags::ALLOW_MODESET, request)
            .map_err(DrmDeviceError::Reset)
    }

    fn restore_initial_state(&self) -> Result<(), DrmDeviceError> {
        let mut request = AtomicModeReq::new();
        add_snapshot(&mut request, &self.initial_state.0);
        add_snapshot(&mut request, &self.initial_state.1);
        add_snapshot(&mut request, &self.initial_state.2);
        add_snapshot(&mut request, &self.initial_state.3);
        self.fd
            .atomic_commit(AtomicCommitFlags::ALLOW_MODESET, request)
            .map_err(DrmDeviceError::Restore)
    }
}

impl AsFd for DrmDevice {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl BasicDevice for DrmDevice {}
impl ControlDevice for DrmDevice {}

impl Drop for DrmDevice {
    fn drop(&mut self) {
        if self.active.get()
            && let Err(error) = self.restore_initial_state()
        {
            error!(%error, "failed to restore the pre-compositor DRM state");
        }
    }
}

fn snapshot_properties<T>(
    device: &impl ControlDevice,
    handles: &[T],
) -> Result<Vec<(T, PropertyValueSet)>, DrmDeviceError>
where
    T: ResourceHandle,
{
    let mut snapshot = Vec::with_capacity(handles.len());
    for &handle in handles {
        snapshot.push((
            handle,
            device
                .get_properties(handle)
                .map_err(DrmDeviceError::Properties)?,
        ));
    }
    Ok(snapshot)
}

fn add_snapshot<T: ResourceHandle>(
    request: &mut AtomicModeReq,
    snapshot: &[(T, PropertyValueSet)],
) {
    for (resource, properties) in snapshot {
        for (&property, &value) in properties {
            request.add_raw_property((*resource).into(), property, value);
        }
    }
}

fn required_property(
    device: &impl ControlDevice,
    resource: impl ResourceHandle,
    name: &'static str,
) -> Result<property::Handle, DrmDeviceError> {
    let object: drm::control::RawResourceHandle = resource.into();
    let properties = device
        .get_properties(resource)
        .map_err(DrmDeviceError::Properties)?;
    for (handle, _) in properties {
        let info = device
            .get_property(handle)
            .map_err(DrmDeviceError::Property)?;
        if info.name().to_bytes() == name.as_bytes() {
            return Ok(handle);
        }
    }
    Err(DrmDeviceError::MissingProperty {
        object: u32::from(object),
        name,
    })
}

#[derive(Debug, Error)]
pub(super) enum DrmDeviceError {
    #[error("failed to enable universal DRM planes: {0}")]
    UniversalPlanes(io::Error),
    #[error("failed to enable atomic DRM modesetting: {0}")]
    Atomic(io::Error),
    #[error("failed to acquire DRM master after session activation: {0}")]
    AcquireMaster(io::Error),
    #[error("DRM device is inactive")]
    Inactive,
    #[error("failed to enumerate DRM resources: {0}")]
    Resources(io::Error),
    #[error("failed to enumerate DRM planes: {0}")]
    Planes(io::Error),
    #[error("failed to read DRM object properties: {0}")]
    Properties(io::Error),
    #[error("failed to read DRM property metadata: {0}")]
    Property(io::Error),
    #[error("DRM object {object} is missing property {name}")]
    MissingProperty { object: u32, name: &'static str },
    #[error("failed to reset atomic DRM state: {0}")]
    Reset(io::Error),
    #[error("failed to restore pre-compositor DRM state: {0}")]
    Restore(io::Error),
}
