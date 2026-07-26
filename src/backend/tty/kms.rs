use std::{
    num::NonZeroU32,
    os::fd::{AsFd, BorrowedFd, OwnedFd},
};

use drm::control::{Mode as DrmMode, connector, crtc};
use smithay::{
    backend::{
        allocator::{dmabuf::Dmabuf, gbm::GbmDevice},
        drm::{
            DrmDevice, DrmDeviceFd, DrmSurface, PlaneConfig, PlaneState,
            gbm::{GbmFramebuffer, framebuffer_from_dmabuf},
        },
    },
    utils::{Rectangle, Transform},
};
use thiserror::Error;
use tracing::warn;

use crate::backend::{BackendOutputId, OutputDescriptor};

use super::{BackendError, TtyBackend};

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
                            target.reset_after_device_reset();
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
            .get(&(output.device_id as libc::dev_t))
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
        let device_id = output.device_id as libc::dev_t;
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
            .get_mut(&(output.device_id as libc::dev_t))
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
    surface: DrmSurface,
    size: (i32, i32),
    slots: Vec<KmsSlot>,
    scanout: ScanoutState,
}

impl KmsOutput {
    pub(super) fn new(
        drm: &mut DrmDevice,
        gbm: &GbmDevice<DrmDeviceFd>,
        descriptor: &OutputDescriptor,
        mode: DrmMode,
        buffers: Vec<(u8, Dmabuf)>,
    ) -> Result<Self, KmsError> {
        let crtc = crtc_handle(descriptor.crtc)?;
        let connector = connector_handle(descriptor.id.connector_id)?;
        let surface = drm
            .create_surface(crtc, mode, &[connector])
            .map_err(|source| KmsError::CreateSurface(source.to_string()))?;
        if surface.is_legacy() {
            return Err(KmsError::LegacySurface);
        }

        let mut slots = Vec::with_capacity(buffers.len());
        for (slot, dmabuf) in buffers {
            let framebuffer =
                framebuffer_from_dmabuf(surface.device_fd(), gbm, &dmabuf, true, false).map_err(
                    |source| KmsError::CreateFramebuffer {
                        slot,
                        message: source.to_string(),
                    },
                )?;
            slots.push(KmsSlot {
                slot,
                _dmabuf: dmabuf,
                framebuffer,
            });
        }
        slots.sort_by_key(|slot| slot.slot);

        let output = Self {
            id: descriptor.id,
            name: descriptor.name.clone(),
            surface,
            size: (descriptor.mode.width, descriptor.mode.height),
            slots,
            scanout: ScanoutState::default(),
        };
        let first_slot = output.slots.first().ok_or(KmsError::NoFramebuffers)?.slot;
        let plane = output.plane_state(first_slot, None)?;
        output
            .surface
            .test_state([plane], true)
            .map_err(|source| KmsError::TestState(source.to_string()))?;
        Ok(output)
    }

    pub(super) fn ready_for(&self, slot: u8) -> bool {
        self.scanout.ready_for(slot) && self.slots.iter().any(|candidate| candidate.slot == slot)
    }

    pub(super) fn crtc(&self) -> crtc::Handle {
        self.surface.crtc()
    }

    pub(super) fn submit(
        &mut self,
        slot: u8,
        timeline_value: u64,
        sync_fd: OwnedFd,
    ) -> Result<(), KmsError> {
        self.scanout.validate_queue(slot, timeline_value)?;
        let plane = self.plane_state(slot, Some(sync_fd.as_fd()))?;
        let result = if self.surface.commit_pending() {
            self.surface.commit([plane], true)
        } else {
            self.surface.page_flip([plane], true)
        };
        if let Err(source) = result {
            self.scanout.faulted = true;
            return Err(KmsError::Commit(source.to_string()));
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
        self.surface
            .reset_state()
            .map_err(|source| KmsError::ResetState(source.to_string()))?;
        self.scanout.resume_after_session_loss();
        Ok(self
            .slots
            .iter()
            .any(|slot| self.scanout.ready_for(slot.slot)))
    }

    pub(super) fn reset_after_device_reset(&mut self) {
        self.scanout = ScanoutState::default();
    }

    fn plane_state<'a>(
        &'a self,
        slot: u8,
        fence: Option<BorrowedFd<'a>>,
    ) -> Result<PlaneState<'a>, KmsError> {
        let slot = self
            .slots
            .iter()
            .find(|candidate| candidate.slot == slot)
            .ok_or(KmsError::UnknownSlot(slot))?;
        let src = Rectangle::from_size(self.size.into()).to_f64();
        let dst = Rectangle::from_size(self.size.into());
        Ok(PlaneState {
            handle: self.surface.plane(),
            config: Some(PlaneConfig {
                src,
                dst,
                transform: Transform::Normal,
                alpha: 1.0,
                damage_clips: None,
                fb: *slot.framebuffer.as_ref(),
                fence,
            }),
        })
    }
}

impl Drop for KmsOutput {
    fn drop(&mut self) {
        let _ = self.surface.clear();
    }
}

struct KmsSlot {
    slot: u8,
    _dmabuf: Dmabuf,
    framebuffer: GbmFramebuffer,
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
    #[error("failed to create Smithay DRM surface: {0}")]
    CreateSurface(String),
    #[error("legacy DRM surfaces cannot satisfy Tensor's explicit-sync contract")]
    LegacySurface,
    #[error("failed to create framebuffer for output slot {slot}: {message}")]
    CreateFramebuffer { slot: u8, message: String },
    #[error("renderer supplied no native output framebuffers")]
    NoFramebuffers,
    #[error("failed to test atomic KMS state: {0}")]
    TestState(String),
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
    #[error("atomic KMS commit/page-flip failed: {0}")]
    Commit(String),
    #[error("failed to refresh KMS surface state after session resume: {0}")]
    ResetState(String),
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
