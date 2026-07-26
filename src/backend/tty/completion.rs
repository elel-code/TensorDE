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

    /// Advance one libseat completion by at most one lifecycle event. The
    /// completion token stays parked until the source is exhausted, allowing
    /// the caller to apply each event before requesting the next one.
    pub(crate) fn next_session_completion_event(
        &mut self,
    ) -> Result<Option<tensor_host::SessionEvent>, String> {
        if self.active_session_completion.is_none() {
            let Some(completion) = self.session_completions.try_recv() else {
                return match self.session_failures.try_recv() {
                    Some(message) => Err(message),
                    None => Ok(None),
                };
            };
            self.session.begin_drain();
            self.active_session_completion = Some(completion);
        }

        match self.session.next_event() {
            Ok(Some(event)) => Ok(Some(event)),
            Ok(None) => {
                self.active_session_completion
                    .take()
                    .expect("active libseat completion")
                    .rearm()
                    .map_err(|error| format!("libseat completion rearm was rejected: {error:?}"))?;
                match self.session_failures.try_recv() {
                    Some(message) => Err(message),
                    None => Ok(None),
                }
            }
            Err(error) => {
                if let Some(completion) = self.active_session_completion.take() {
                    let _ = completion.finish();
                }
                Err(format!(
                    "failed to dispatch completed libseat events: {error}"
                ))
            }
        }
    }

    pub(crate) fn next_udev_completion_event(&mut self) -> Result<Option<UdevEvent>, String> {
        if self.active_udev_completion.is_none() {
            let Some(completion) = self.udev_completions.try_recv() else {
                return match self.udev_failures.try_recv() {
                    Some(message) => Err(message),
                    None => Ok(None),
                };
            };
            self.udev.begin_drain();
            self.active_udev_completion = Some(completion);
        }

        if let Some(event) = self.udev.next_event() {
            return Ok(Some(event));
        }
        self.active_udev_completion
            .take()
            .expect("active udev completion")
            .rearm()
            .map_err(|error| format!("udev completion rearm was rejected: {error:?}"))?;
        match self.udev_failures.try_recv() {
            Some(message) => Err(message),
            None => Ok(None),
        }
    }

    pub(crate) fn next_libinput_completion_event(
        &mut self,
    ) -> Result<Option<LibinputEvent>, String> {
        if self.active_libinput_completion.is_none() {
            let Some(completion) = self.libinput_completions.try_recv() else {
                return match self.libinput_failures.try_recv() {
                    Some(message) => Err(message),
                    None => Ok(None),
                };
            };
            if let Err(error) = self.libinput.begin_drain() {
                let _ = completion.finish();
                return Err(format!(
                    "failed to dispatch completed libinput events: {error}"
                ));
            }
            self.active_libinput_completion = Some(completion);
        }

        if let Some(event) = self.libinput.next_event() {
            return Ok(Some(event));
        }
        self.active_libinput_completion
            .take()
            .expect("active libinput completion")
            .rearm()
            .map_err(|error| format!("libinput completion rearm was rejected: {error:?}"))?;
        match self.libinput_failures.try_recv() {
            Some(message) => Err(message),
            None => Ok(None),
        }
    }
}
