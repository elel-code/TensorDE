//! DRM timeline and point operations for the syncobj protocol adapter.
//!
//! Adapted from Smithay's `wayland::drm_syncobj::sync_point` implementation.
//! See `LICENSES/Smithay-MIT.txt`.

use std::{
    io,
    os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd},
    sync::{Arc, Mutex},
};

use drm::control::Device as ControlDevice;

use crate::backend::{DrmDeviceFd, WeakDrmDeviceFd};

#[derive(Debug)]
pub(super) struct DrmTimelineInner {
    timeline_fd: OwnedFd,
    device: Mutex<DrmTimelineDevice>,
}

impl DrmTimelineInner {
    pub(super) fn update_device(&self, device: &DrmDeviceFd) -> io::Result<()> {
        let imported = DrmTimelineDevice::import(self.timeline_fd.as_fd(), device)?;
        *self.device.lock().unwrap() = imported;
        Ok(())
    }

    pub(super) fn invalidate(&self) {
        self.device.lock().unwrap().invalidate();
    }
}

#[derive(Debug)]
struct DrmTimelineDevice {
    device: WeakDrmDeviceFd,
    syncobj: drm::control::syncobj::Handle,
}

impl DrmTimelineDevice {
    fn import(fd: BorrowedFd<'_>, device: &DrmDeviceFd) -> io::Result<Self> {
        Ok(Self {
            device: device.downgrade(),
            syncobj: device.fd_to_syncobj(fd, false)?,
        })
    }

    fn invalidate(&mut self) {
        if let Some(device) = self.device.upgrade() {
            let _ = device.destroy_syncobj(self.syncobj);
        }
        self.device = WeakDrmDeviceFd::default();
    }
}

impl Drop for DrmTimelineDevice {
    fn drop(&mut self) {
        self.invalidate();
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DrmTimeline(pub(super) Arc<DrmTimelineInner>);

impl PartialEq for DrmTimeline {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for DrmTimeline {}

impl DrmTimeline {
    pub(super) fn new(device: &DrmDeviceFd, fd: OwnedFd) -> io::Result<Self> {
        let imported = DrmTimelineDevice::import(fd.as_fd(), device)?;
        Ok(Self(Arc::new(DrmTimelineInner {
            timeline_fd: fd,
            device: Mutex::new(imported),
        })))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DrmSyncPoint {
    pub(super) timeline: DrmTimeline,
    pub(super) point: u64,
}

impl DrmSyncPoint {
    pub(crate) fn timeline(&self) -> &DrmTimeline {
        &self.timeline
    }

    pub(crate) fn point(&self) -> u64 {
        self.point
    }

    pub(crate) fn signal(&self) -> io::Result<()> {
        let context = self.timeline.0.device.lock().unwrap();
        context
            .device
            .upgrade()
            .ok_or_else(invalid_device)?
            .syncobj_timeline_signal(&[context.syncobj], &[self.point])
    }

    pub(crate) fn export_sync_file(&self) -> io::Result<OwnedFd> {
        let context = self.timeline.0.device.lock().unwrap();
        let device = context.device.upgrade().ok_or_else(invalid_device)?;
        let binary = device.create_syncobj(false)?;
        if let Err(error) = device.syncobj_timeline_transfer(context.syncobj, binary, self.point, 0)
        {
            let _ = device.destroy_syncobj(binary);
            return Err(error);
        }
        let result = device.syncobj_to_fd(binary, true);
        let _ = device.destroy_syncobj(binary);
        result
    }

    pub(crate) fn import_sync_file(&self, fd: BorrowedFd<'_>) -> io::Result<()> {
        let context = self.timeline.0.device.lock().unwrap();
        let device = context.device.upgrade().ok_or_else(invalid_device)?;
        let binary = device.create_syncobj(false)?;
        if let Err(error) = import_sync_file_handle(&device, binary, fd) {
            let _ = device.destroy_syncobj(binary);
            return Err(error);
        }

        let result = device.syncobj_timeline_transfer(binary, context.syncobj, 0, self.point);
        let _ = device.destroy_syncobj(binary);
        result
    }
}

#[allow(unsafe_code)]
fn import_sync_file_handle(
    device: &DrmDeviceFd,
    syncobj: drm::control::syncobj::Handle,
    fd: BorrowedFd<'_>,
) -> io::Result<()> {
    use rustix::ioctl::{Updater, ioctl, opcode::read_write};

    const DRM_IOCTL_SYNCOBJ_FD_TO_HANDLE: rustix::ioctl::Opcode =
        read_write::<drm_ffi::drm_syncobj_handle>(drm_ffi::DRM_IOCTL_BASE, 0xC2);
    let mut arguments = drm_ffi::drm_syncobj_handle {
        handle: syncobj.into(),
        flags: drm_ffi::DRM_SYNCOBJ_FD_TO_HANDLE_FLAGS_IMPORT_SYNC_FILE,
        fd: fd.as_raw_fd(),
        pad: 0,
        point: 0,
    };
    // SAFETY: drm-ffi supplies the kernel ABI struct and rustix owns the
    // ioctl encoding; both fds and the existing syncobj handle are live.
    unsafe {
        ioctl(
            device.as_fd(),
            Updater::<DRM_IOCTL_SYNCOBJ_FD_TO_HANDLE, _>::new(&mut arguments),
        )
        .map(|_| ())
        .map_err(Into::into)
    }
}

fn invalid_device() -> io::Error {
    io::ErrorKind::InvalidInput.into()
}
