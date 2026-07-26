use std::{
    num::NonZeroU32,
    os::fd::{AsFd, OwnedFd},
    rc::Rc,
};

use drm::control::{Mode as DrmMode, connector, crtc, plane};
use gbm::Device as GbmDevice;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd};
use thiserror::Error;
use tracing::warn;

use crate::{
    backend::{BackendOutputId, OutputDescriptor},
    render::ExportedDmabuf,
};

use super::{BackendError, TtyBackend};

mod atomic;
mod framebuffer;

pub(super) use atomic::primary_plane_formats;
use atomic::{AtomicError, AtomicSurface, select_primary_plane};
use framebuffer::{ScanoutFramebuffer, framebuffer_from_dmabuf};

impl TtyBackend {
    pub(crate) fn reset_outputs_after_session_resume(&mut self) {
        for (device_id, device) in &mut self.devices {
            let mut needs_device_reset = false;
            for target in device.native_targets.values_mut() {
                match target.reset_after_session_resume() {
                    Ok(true) => {}
                    Ok(false) => {
                        needs_device_reset = true;
                        warn!(
                            device_id = *device_id,
                            output = %target.name,
                            "no reusable KMS slot remains after session resume"
                        );
                    }
                    Err(error) => {
                        needs_device_reset = true;
                        warn!(
                            device_id = *device_id,
                            output = %target.name,
                            %error,
                            "failed to reset KMS output after session resume"
                        );
                    }
                }
            }
            if needs_device_reset {
                match device.drm.reset_state() {
                    Ok(()) => {
                        for target in device.native_targets.values_mut() {
                            if let Err(error) = target.reset_after_device_reset() {
                                warn!(
                                    device_id = *device_id,
                                    output = %target.name,
                                    %error,
                                    "failed to rebuild KMS output after device reset"
                                );
                                target.mark_faulted();
                            }
                        }
                    }
                    Err(error) => {
                        warn!(device_id = *device_id, %error, "failed to reset DRM device after session resume");
                        for target in device.native_targets.values_mut() {
                            target.mark_faulted();
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn output_ready_for_slot(&self, output: BackendOutputId, slot: u8) -> bool {
        if !self.session.is_active() {
            return false;
        }
        self.devices
            .get(&output.device_id)
            .filter(|device| device.active.get())
            .and_then(|device| device.native_targets.get(&output))
            .is_some_and(|target| target.ready_for(slot))
    }

    pub(crate) fn submit_output_frame(
        &mut self,
        output: BackendOutputId,
        slot: u8,
        timeline_value: u64,
        sync_fd: OwnedFd,
    ) -> Result<(), BackendError> {
        let device_id = output.device_id;
        let device = self
            .devices
            .get_mut(&device_id)
            .ok_or(BackendError::UnknownDevice { device_id })?;
        let target = device
            .native_targets
            .get_mut(&output)
            .ok_or(BackendError::UnknownOutput(output))?;
        target
            .submit(slot, timeline_value, sync_fd)
            .map_err(|source| BackendError::KmsFrame {
                output: target.name.clone(),
                message: source.to_string(),
            })
    }

    pub(crate) fn mark_output_faulted(&mut self, output: BackendOutputId) {
        if let Some(target) = self
            .devices
            .get_mut(&output.device_id)
            .and_then(|device| device.native_targets.get_mut(&output))
        {
            target.mark_faulted();
        }
    }

    pub(crate) fn handle_drm_vblank(
        &mut self,
        device_id: u64,
        crtc_id: u32,
    ) -> Option<KmsPresentation> {
        self.devices
            .get_mut(&device_id)?
            .native_targets
            .values_mut()
            .find(|output| u32::from(output.crtc()) == crtc_id)?
            .frame_submitted()
    }
}

pub(super) struct KmsOutput {
    id: BackendOutputId,
    name: String,
    surface: AtomicSurface,
    slots: Vec<KmsSlot>,
    scanout: ScanoutState,
}

impl KmsOutput {
    pub(super) fn new(
        drm: &mut DrmDevice,
        gbm: &GbmDevice<DrmDeviceFd>,
        active: Rc<std::cell::Cell<bool>>,
        descriptor: &OutputDescriptor,
        mode: DrmMode,
        buffers: Vec<(u8, ExportedDmabuf)>,
        claimed_planes: &[plane::Handle],
    ) -> Result<Self, KmsError> {
        let crtc = crtc_handle(descriptor.crtc)?;
        let connector = connector_handle(descriptor.id.connector_id)?;
        let plane =
            select_primary_plane(drm, crtc, descriptor.native_format.format, claimed_planes)?;
        let device_fd = drm.device_fd().clone();

        let mut slots = Vec::with_capacity(buffers.len());
        for (slot, dmabuf) in buffers {
            let framebuffer =
                framebuffer_from_dmabuf(&device_fd, gbm, &dmabuf).map_err(|source| {
                    KmsError::CreateFramebuffer {
                        slot,
                        message: source.to_string(),
                    }
                })?;
            slots.push(KmsSlot {
                slot,
                framebuffer,
                _dmabuf: dmabuf,
            });
        }
        slots.sort_by_key(|slot| slot.slot);
        let first_framebuffer = slots
            .first()
            .ok_or(KmsError::NoFramebuffers)?
            .framebuffer
            .handle();
        let surface = AtomicSurface::new(
            device_fd,
            active,
            connector,
            crtc,
            plane,
            mode,
            first_framebuffer,
        )?;

        Ok(Self {
            id: descriptor.id,
            name: descriptor.name.clone(),
            surface,
            slots,
            scanout: ScanoutState::default(),
        })
    }

    pub(super) fn ready_for(&self, slot: u8) -> bool {
        self.scanout.ready_for(slot) && self.slots.iter().any(|candidate| candidate.slot == slot)
    }

    pub(super) fn crtc(&self) -> crtc::Handle {
        self.surface.crtc()
    }

    pub(super) fn plane(&self) -> plane::Handle {
        self.surface.plane()
    }

    pub(super) fn submit(
        &mut self,
        slot: u8,
        timeline_value: u64,
        sync_fd: OwnedFd,
    ) -> Result<(), KmsError> {
        self.scanout.validate_queue(slot, timeline_value)?;
        let framebuffer = self.framebuffer(slot)?;
        if let Err(source) = self.surface.submit(framebuffer, sync_fd.as_fd()) {
            self.scanout.faulted = true;
            return Err(source.into());
        }
        self.scanout.queue(slot, timeline_value);
        Ok(())
    }

    pub(super) fn frame_submitted(&mut self) -> Option<KmsPresentation> {
        let completed = self.scanout.present()?;
        Some(KmsPresentation {
            output: self.id,
            slot: completed.presented.slot,
            timeline_value: completed.presented.timeline_value,
            released_timeline: completed.released.map(|frame| frame.timeline_value),
        })
    }

    pub(super) fn mark_faulted(&mut self) {
        self.scanout.faulted = true;
    }

    pub(super) fn reset_after_session_resume(&mut self) -> Result<bool, KmsError> {
        let framebuffer = self
            .slots
            .first()
            .ok_or(KmsError::NoFramebuffers)?
            .framebuffer
            .handle();
        self.surface.reset_after_session_resume(framebuffer)?;
        self.scanout.resume_after_session_loss();
        Ok(self
            .slots
            .iter()
            .any(|slot| self.scanout.ready_for(slot.slot)))
    }

    pub(super) fn reset_after_device_reset(&mut self) -> Result<(), KmsError> {
        let framebuffer = self
            .slots
            .first()
            .ok_or(KmsError::NoFramebuffers)?
            .framebuffer
            .handle();
        self.surface.reset_after_session_resume(framebuffer)?;
        self.scanout = ScanoutState::default();
        Ok(())
    }

    fn framebuffer(&self, slot: u8) -> Result<drm::control::framebuffer::Handle, KmsError> {
        self.slots
            .iter()
            .find(|candidate| candidate.slot == slot)
            .map(|slot| slot.framebuffer.handle())
            .ok_or(KmsError::UnknownSlot(slot))
    }
}

impl Drop for KmsOutput {
    fn drop(&mut self) {
        let _ = self.surface.clear();
    }
}

struct KmsSlot {
    slot: u8,
    framebuffer: ScanoutFramebuffer,
    _dmabuf: ExportedDmabuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScanoutFrame {
    slot: u8,
    timeline_value: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CompletedScanout {
    presented: ScanoutFrame,
    released: Option<ScanoutFrame>,
}

#[derive(Debug, Default)]
struct ScanoutState {
    pending: Option<ScanoutFrame>,
    current: Option<ScanoutFrame>,
    quarantined: Vec<u8>,
    faulted: bool,
}

impl ScanoutState {
    fn ready(&self) -> bool {
        !self.faulted && self.pending.is_none()
    }

    fn ready_for(&self, slot: u8) -> bool {
        self.ready()
            && !self.quarantined.contains(&slot)
            && self.current.is_none_or(|current| current.slot != slot)
    }

    fn validate_queue(&self, slot: u8, timeline_value: u64) -> Result<(), KmsError> {
        if self.faulted {
            return Err(KmsError::Faulted);
        }
        if self.quarantined.contains(&slot) {
            return Err(KmsError::QuarantinedSlot(slot));
        }
        if let Some(pending) = self.pending {
            return Err(KmsError::Busy(pending.timeline_value));
        }
        if self.current.is_some_and(|current| current.slot == slot) {
            return Err(KmsError::CurrentSlotReuse(slot));
        }
        if timeline_value == 0 {
            return Err(KmsError::InvalidTimeline);
        }
        Ok(())
    }

    fn queue(&mut self, slot: u8, timeline_value: u64) {
        self.pending = Some(ScanoutFrame {
            slot,
            timeline_value,
        });
    }

    fn present(&mut self) -> Option<CompletedScanout> {
        let presented = self.pending.take()?;
        let released = self.current.replace(presented);
        self.quarantined.clear();
        Some(CompletedScanout {
            presented,
            released,
        })
    }

    fn resume_after_session_loss(&mut self) {
        for frame in [self.current, self.pending].into_iter().flatten() {
            if !self.quarantined.contains(&frame.slot) {
                self.quarantined.push(frame.slot);
            }
        }
        self.pending = None;
        self.current = None;
        self.faulted = false;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct KmsPresentation {
    pub(crate) output: BackendOutputId,
    pub(crate) slot: u8,
    pub(crate) timeline_value: u64,
    pub(crate) released_timeline: Option<u64>,
}

fn crtc_handle(raw: u32) -> Result<crtc::Handle, KmsError> {
    NonZeroU32::new(raw)
        .map(Into::into)
        .ok_or(KmsError::InvalidCrtc(raw))
}

fn connector_handle(raw: u32) -> Result<connector::Handle, KmsError> {
    NonZeroU32::new(raw)
        .map(Into::into)
        .ok_or(KmsError::InvalidConnector(raw))
}

#[derive(Debug, Error)]
pub(super) enum KmsError {
    #[error("CRTC handle {0} is invalid")]
    InvalidCrtc(u32),
    #[error("connector handle {0} is invalid")]
    InvalidConnector(u32),
    #[error(transparent)]
    Atomic(#[from] AtomicError),
    #[error("failed to create framebuffer for output slot {slot}: {message}")]
    CreateFramebuffer { slot: u8, message: String },
    #[error("renderer supplied no native output framebuffers")]
    NoFramebuffers,
    #[error("output scanout is waiting for timeline {0}")]
    Busy(u64),
    #[error("output scanout is faulted and requires reprobe")]
    Faulted,
    #[error("output slot {0} is still the current scanout buffer")]
    CurrentSlotReuse(u8),
    #[error("output slot {0} may still be scanned out after session resume")]
    QuarantinedSlot(u8),
    #[error("output slot {0} is not installed in the KMS target")]
    UnknownSlot(u8),
    #[error("timeline value zero is reserved")]
    InvalidTimeline,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vblank_releases_the_previous_scanout_only_after_replacement() {
        let mut state = ScanoutState::default();
        state.validate_queue(0, 1).unwrap();
        state.queue(0, 1);
        let first = state.present().unwrap();
        assert_eq!(first.presented.timeline_value, 1);
        assert_eq!(first.released, None);

        state.validate_queue(1, 2).unwrap();
        state.queue(1, 2);
        let second = state.present().unwrap();
        assert_eq!(second.released.unwrap().timeline_value, 1);
    }

    #[test]
    fn pending_faulted_and_current_slots_are_not_reused() {
        let mut state = ScanoutState::default();
        state.queue(0, 3);
        assert!(!state.ready_for(1));
        assert!(matches!(state.validate_queue(1, 4), Err(KmsError::Busy(3))));
        state.present().unwrap();
        assert!(state.ready_for(1));
        assert!(!state.ready_for(0));
        assert!(matches!(
            state.validate_queue(0, 4),
            Err(KmsError::CurrentSlotReuse(0))
        ));
        state.faulted = true;
        assert!(matches!(state.validate_queue(1, 4), Err(KmsError::Faulted)));
    }

    #[test]
    fn resume_quarantines_old_slots_until_the_first_new_vblank() {
        let mut state = ScanoutState::default();
        state.queue(0, 1);
        state.present().unwrap();
        state.queue(1, 2);

        state.resume_after_session_loss();
        assert!(state.present().is_none());
        assert!(!state.ready_for(0));
        assert!(!state.ready_for(1));
        assert!(state.ready_for(2));
        assert!(matches!(
            state.validate_queue(0, 3),
            Err(KmsError::QuarantinedSlot(0))
        ));

        state.queue(2, 3);
        state.present().unwrap();
        assert!(state.ready_for(0));
        assert!(state.ready_for(1));
    }

    #[test]
    fn repeated_resume_can_escalate_when_every_slot_is_quarantined() {
        let mut state = ScanoutState {
            quarantined: vec![0, 1],
            ..ScanoutState::default()
        };
        state.queue(2, 7);
        state.resume_after_session_loss();

        assert!(!state.ready_for(0));
        assert!(!state.ready_for(1));
        assert!(!state.ready_for(2));
        state = ScanoutState::default();
        assert!(state.ready_for(0));
    }
}
