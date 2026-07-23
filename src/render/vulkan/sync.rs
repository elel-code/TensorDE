#![allow(unsafe_code)]

use std::{
    collections::{HashMap, HashSet},
    os::fd::{AsRawFd, IntoRawFd, OwnedFd},
    sync::Arc,
};

use thiserror::Error;
use vulkanalia::vk::{DeviceV1_0, HasBuilder, KhrExternalSemaphoreFdExtensionDeviceCommands};
use vulkanalia::{Device, vk};

use crate::{ecs::SurfaceId, render::FrameSubmission};

/// A completion fence which can be handed to Smithay's DRM-syncobj release
/// point.  Timeline values stay on the Vulkan side; they are only exposed as
/// a recovery state when exporting a binary sync file failed.
#[derive(Debug, Clone)]
pub(crate) enum ClientReleaseFence {
    /// The client image was never submitted, or its completion is already
    /// known to have retired.
    Ready,
    /// A binary `SYNC_FD` exported from the frame submission.
    SyncFile(Arc<OwnedFd>),
    /// The frame was accepted by Vulkan but a sync-file export failed.  The
    /// protocol owner must wait for the renderer timeline to reach this value
    /// before signalling a client release point.
    PendingTimeline(u64),
}

/// Vulkan-side owner of client acquire semaphores and completion fences.
///
/// The ledger is keyed by the stable surface identity rather than a Wayland
/// object or a raw descriptor.  A surface has one active explicit-sync
/// attachment at a time; replacing it first retires the old entry in the
/// protocol layer.
#[derive(Debug, Default)]
pub(super) struct ClientSyncManager {
    ledger: SyncLedger<vk::Semaphore, Arc<OwnedFd>>,
    retired: Vec<(vk::Semaphore, u64)>,
}

impl ClientSyncManager {
    pub(super) fn import_acquire(
        &mut self,
        device: &Device,
        surface: SurfaceId,
        fd: OwnedFd,
    ) -> Result<(), ClientSyncError> {
        if self.ledger.contains(surface) {
            return Err(ClientSyncError::AcquireAlreadyPending(surface));
        }

        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let semaphore = unsafe { device.create_semaphore(&semaphore_info, None) }
            .map_err(ClientSyncError::CreateSemaphore)?;
        let import_info = vk::ImportSemaphoreFdInfoKHR::builder()
            .semaphore(semaphore)
            .flags(vk::SemaphoreImportFlags::TEMPORARY)
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD)
            .fd(fd.as_raw_fd());

        if let Err(error) = unsafe { device.import_semaphore_fd_khr(&import_info) } {
            unsafe { device.destroy_semaphore(semaphore, None) };
            return Err(ClientSyncError::ImportSemaphore(error));
        }

        // Successful Vulkan import transfers ownership of the fd to Vulkan.
        let _ = fd.into_raw_fd();
        self.ledger
            .insert_acquire(surface, semaphore)
            .expect("surface acquire was checked before semaphore import");
        Ok(())
    }

    pub(super) fn tracked_surface_ids(&self, frame: &FrameSubmission) -> Vec<SurfaceId> {
        unique_surface_ids(frame, |surface| self.ledger.contains(surface))
    }

    pub(super) fn wait_semaphores(&self, surfaces: &[SurfaceId]) -> Vec<vk::Semaphore> {
        surfaces
            .iter()
            .filter_map(|surface| self.ledger.acquire(*surface).copied())
            .collect()
    }

    /// Commit state only after `queue_submit2` accepted the command buffer.
    /// A failed submit leaves the imported semaphore in the ledger so the
    /// caller can retry without re-exporting the client's acquire point.
    pub(super) fn mark_submitted(
        &mut self,
        surfaces: &[SurfaceId],
        timeline: u64,
        completion: Option<Arc<OwnedFd>>,
    ) {
        let retired = self
            .ledger
            .mark_submitted(surfaces.iter().copied(), timeline, completion);
        for semaphore in retired {
            self.retired.push((semaphore, timeline));
        }
    }

    pub(super) fn finish(
        &mut self,
        device: &Device,
        surface: SurfaceId,
        completed_timeline: u64,
    ) -> ClientReleaseFence {
        let Some(entry) = self.ledger.finish(surface, completed_timeline) else {
            return ClientReleaseFence::Ready;
        };
        if let Some(semaphore) = entry.acquire {
            // A pending acquire was never submitted.  It is safe to discard
            // its imported payload instead of waiting on it.
            unsafe { device.destroy_semaphore(semaphore, None) };
        }
        match entry.release {
            ReleaseState::Ready => ClientReleaseFence::Ready,
            ReleaseState::SyncFile(fence) => ClientReleaseFence::SyncFile(fence),
            ReleaseState::PendingTimeline(value) => ClientReleaseFence::PendingTimeline(value),
        }
    }

    pub(super) fn retire_completed(&mut self, device: &Device, completed_timeline: u64) {
        let mut retained = Vec::with_capacity(self.retired.len());
        for (semaphore, retire_value) in self.retired.drain(..) {
            if retire_value <= completed_timeline {
                unsafe { device.destroy_semaphore(semaphore, None) };
            } else {
                retained.push((semaphore, retire_value));
            }
        }
        self.retired = retained;
    }

    pub(super) unsafe fn destroy(&mut self, device: &Device) {
        for (_, semaphore) in self.ledger.drain() {
            unsafe { device.destroy_semaphore(semaphore, None) };
        }
        for (semaphore, _) in self.retired.drain(..) {
            unsafe { device.destroy_semaphore(semaphore, None) };
        }
    }
}

fn unique_surface_ids(
    frame: &FrameSubmission,
    mut predicate: impl FnMut(SurfaceId) -> bool,
) -> Vec<SurfaceId> {
    let mut seen = HashSet::new();
    frame
        .draw_plan
        .draws()
        .iter()
        .map(|draw| draw.surface_id)
        .filter(|surface| predicate(*surface) && seen.insert(*surface))
        .collect()
}

#[derive(Debug, Error)]
pub(super) enum ClientSyncError {
    #[error("surface {0:?} already has an imported client acquire semaphore")]
    AcquireAlreadyPending(SurfaceId),
    #[error("failed to create a client acquire semaphore: {0:?}")]
    CreateSemaphore(vk::ErrorCode),
    #[error("failed to import client acquire SYNC_FD: {0:?}")]
    ImportSemaphore(vk::ErrorCode),
}

#[derive(Debug)]
struct SyncLedger<A, F> {
    entries: HashMap<SurfaceId, SyncEntry<A, F>>,
}

impl<A, F> Default for SyncLedger<A, F> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<A, F> SyncLedger<A, F> {
    fn contains(&self, surface: SurfaceId) -> bool {
        self.entries.contains_key(&surface)
    }

    fn insert_acquire(&mut self, surface: SurfaceId, acquire: A) -> Result<(), A> {
        if self.entries.contains_key(&surface) {
            return Err(acquire);
        }
        self.entries.insert(
            surface,
            SyncEntry {
                acquire: Some(acquire),
                last_use: 0,
                completion: None,
            },
        );
        Ok(())
    }

    fn acquire(&self, surface: SurfaceId) -> Option<&A> {
        self.entries.get(&surface)?.acquire.as_ref()
    }

    fn mark_submitted(
        &mut self,
        surfaces: impl IntoIterator<Item = SurfaceId>,
        timeline: u64,
        completion: Option<F>,
    ) -> Vec<A>
    where
        F: Clone,
    {
        let mut retired = Vec::new();
        for surface in surfaces {
            let Some(entry) = self.entries.get_mut(&surface) else {
                continue;
            };
            entry.last_use = entry.last_use.max(timeline);
            entry.completion = completion.clone();
            if let Some(acquire) = entry.acquire.take() {
                retired.push(acquire);
            }
        }
        retired
    }

    fn finish(
        &mut self,
        surface: SurfaceId,
        completed_timeline: u64,
    ) -> Option<FinishedEntry<A, F>> {
        let entry = self.entries.remove(&surface)?;
        let release = match entry.completion {
            Some(fence) => ReleaseState::SyncFile(fence),
            None if entry.last_use == 0 || entry.last_use <= completed_timeline => {
                ReleaseState::Ready
            }
            None => ReleaseState::PendingTimeline(entry.last_use),
        };
        Some(FinishedEntry {
            acquire: entry.acquire,
            release,
        })
    }

    fn drain(&mut self) -> impl Iterator<Item = (SurfaceId, A)> + '_ {
        self.entries
            .drain()
            .filter_map(|(surface, entry)| entry.acquire.map(|acquire| (surface, acquire)))
    }
}

#[derive(Debug)]
struct SyncEntry<A, F> {
    acquire: Option<A>,
    last_use: u64,
    completion: Option<F>,
}

#[derive(Debug)]
struct FinishedEntry<A, F> {
    acquire: Option<A>,
    release: ReleaseState<F>,
}

#[derive(Debug)]
enum ReleaseState<F> {
    Ready,
    SyncFile(F),
    PendingTimeline(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    const SURFACE: SurfaceId = SurfaceId::new(7);

    #[test]
    fn failed_submission_keeps_acquire_and_never_advances_release() {
        let mut ledger = SyncLedger::<u32, u64>::default();
        ledger.insert_acquire(SURFACE, 11).unwrap();
        assert!(ledger.acquire(SURFACE).is_some());
        let finished = ledger.finish(SURFACE, 0).unwrap();
        assert_eq!(finished.acquire, Some(11));
        assert!(matches!(finished.release, ReleaseState::Ready));
    }

    #[test]
    fn accepted_submission_retires_acquire_and_keeps_completion_fence() {
        let mut ledger = SyncLedger::<u32, u64>::default();
        ledger.insert_acquire(SURFACE, 11).unwrap();
        assert_eq!(ledger.mark_submitted([SURFACE], 5, Some(99)), vec![11]);
        let finished = ledger.finish(SURFACE, 0).unwrap();
        assert!(finished.acquire.is_none());
        assert!(matches!(finished.release, ReleaseState::SyncFile(99)));
    }

    #[test]
    fn repeated_repaint_releases_against_the_latest_gpu_read() {
        let mut ledger = SyncLedger::<u32, u64>::default();
        ledger.insert_acquire(SURFACE, 11).unwrap();
        assert_eq!(ledger.mark_submitted([SURFACE], 5, Some(90)), vec![11]);
        assert!(ledger.mark_submitted([SURFACE], 8, Some(99)).is_empty());
        let finished = ledger.finish(SURFACE, 0).unwrap();
        assert!(matches!(finished.release, ReleaseState::SyncFile(99)));
    }

    #[test]
    fn export_failure_exposes_timeline_without_signalling_early() {
        let mut ledger = SyncLedger::<u32, u64>::default();
        ledger.insert_acquire(SURFACE, 11).unwrap();
        assert_eq!(ledger.mark_submitted([SURFACE], 8, None), vec![11]);
        let pending = ledger.finish(SURFACE, 7).unwrap();
        assert!(matches!(pending.release, ReleaseState::PendingTimeline(8)));
        let mut next = SyncLedger::<u32, u64>::default();
        next.insert_acquire(SURFACE, 12).unwrap();
        assert_eq!(next.mark_submitted([SURFACE], 8, None), vec![12]);
        let ready = next.finish(SURFACE, 8).unwrap();
        assert!(matches!(ready.release, ReleaseState::Ready));
    }

    #[test]
    fn replacement_is_rejected_until_protocol_finishes_old_surface() {
        let mut ledger = SyncLedger::<u32, u64>::default();
        ledger.insert_acquire(SURFACE, 11).unwrap();
        assert_eq!(ledger.insert_acquire(SURFACE, 12), Err(12));
        let finished = ledger.finish(SURFACE, 0).unwrap();
        assert_eq!(finished.acquire, Some(11));
        ledger.insert_acquire(SURFACE, 12).unwrap();
        assert_eq!(ledger.entries.len(), 1);
    }
}
