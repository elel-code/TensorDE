use std::os::fd::OwnedFd;

use drm::control::{Device as _, Event as DrmEvent};
use tensor_host::{VblankBatch, VblankClock, VblankEvent, VblankMetadata};
use tracing::trace;

use super::{BackendError, LibinputEvent, TtyBackend, UdevEvent};

impl TtyBackend {
    pub(crate) fn drm_completion_generation(&self) -> u64 {
        self.topology_generation
    }

    pub(crate) fn write_drm_completion_device_ids(
        &self,
        destination: &mut [u64],
    ) -> Result<usize, String> {
        if self.devices.len() > destination.len() {
            return Err(format!(
                "{} DRM devices exceed the fixed completion-source capacity of {}",
                self.devices.len(),
                destination.len()
            ));
        }
        let len = self.devices.len();
        for (slot, device_id) in destination.iter_mut().zip(self.devices.keys()) {
            *slot = *device_id;
        }
        destination[..len].sort_unstable();
        Ok(len)
    }

    pub(crate) fn duplicate_drm_completion_fd(&self, device_id: u64) -> Result<OwnedFd, String> {
        let device = self
            .devices
            .get(&device_id)
            .ok_or_else(|| BackendError::UnknownDevice { device_id }.to_string())?;
        rustix::io::fcntl_dupfd_cloexec(&device.drm, 0)
            .map_err(|error| format!("failed to duplicate DRM completion fd: {error}"))
    }

    /// Read one drm-rs event buffer after the submitted fd operation completes.
    /// This deliberately does not loop to `EAGAIN`; a remaining record produces
    /// another CQE after explicit rearm.
    pub(crate) fn receive_drm_events(&self, device_id: u64) -> Result<VblankBatch, String> {
        let device = self
            .devices
            .get(&device_id)
            .ok_or_else(|| BackendError::UnknownDevice { device_id }.to_string())?;
        let events = device
            .drm
            .receive_events()
            .map_err(|error| format!("failed to receive completed DRM events: {error}"))?;
        let mut batch = VblankBatch::new();
        for event in events {
            let DrmEvent::PageFlip(event) = event else {
                continue;
            };
            let metadata = VblankMetadata {
                timestamp: event.duration,
                sequence: event.frame,
                clock: if device.monotonic_timestamps {
                    VblankClock::Monotonic
                } else {
                    VblankClock::Realtime
                },
            };
            let crtc_id = u32::from(event.crtc);
            trace!(device_id, crtc = crtc_id, ?metadata, "DRM vblank");
            let event = VblankEvent {
                device_id,
                crtc_id,
                metadata,
            };
            batch
                .push(event)
                .map_err(|_| "DRM event read exceeded its fixed page-flip capacity".to_owned())?;
        }
        Ok(batch)
    }

    pub(crate) fn drain_session_completions(
        &mut self,
    ) -> Result<Vec<tensor_host::SessionEvent>, String> {
        let mut events = Vec::new();
        while let Some(completion) = self.session_completions.try_recv() {
            match self.session.drain() {
                Ok(completed) => events.extend(completed),
                Err(error) => {
                    let _ = completion.finish();
                    return Err(format!(
                        "failed to dispatch completed libseat events: {error}"
                    ));
                }
            }
            completion
                .rearm()
                .map_err(|error| format!("libseat completion rearm was rejected: {error:?}"))?;
        }
        if let Some(message) = self.session_failures.try_recv() {
            return Err(message);
        }
        Ok(events)
    }

    pub(crate) fn drain_udev_completions(&mut self) -> Result<Vec<UdevEvent>, String> {
        let mut events = Vec::new();
        while let Some(completion) = self.udev_completions.try_recv() {
            events.extend(self.udev.drain());
            completion
                .rearm()
                .map_err(|error| format!("udev completion rearm was rejected: {error:?}"))?;
        }
        if let Some(message) = self.udev_failures.try_recv() {
            return Err(message);
        }
        Ok(events)
    }

    pub(crate) fn drain_libinput_completions(&mut self) -> Result<Vec<LibinputEvent>, String> {
        let mut events = Vec::new();
        while let Some(completion) = self.libinput_completions.try_recv() {
            match self.libinput.drain() {
                Ok(completed) => events.extend(completed),
                Err(error) => {
                    let _ = completion.finish();
                    return Err(format!(
                        "failed to dispatch completed libinput events: {error}"
                    ));
                }
            }
            completion
                .rearm()
                .map_err(|error| format!("libinput completion rearm was rejected: {error:?}"))?;
        }
        if let Some(message) = self.libinput_failures.try_recv() {
            return Err(message);
        }
        Ok(events)
    }
}
