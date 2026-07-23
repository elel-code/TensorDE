use std::{
    collections::{HashMap, HashSet},
    os::fd::{AsFd, OwnedFd},
    sync::Arc,
};

use smithay::{
    reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface},
    wayland::drm_syncobj::DrmSyncPoint,
};
use tracing::warn;

use crate::{
    ecs::{SurfaceBufferId, SurfaceId},
    render::ClientReleaseFence,
};

use super::RuntimeState;

/// Value-independent state for one rendered surface attachment.  The point
/// handles remain in the protocol owner; only the surface and buffer IDs cross
/// into the renderer/ECS boundary.
#[derive(Debug)]
pub(super) struct SurfaceSyncRegistry<P = DrmSyncPoint> {
    active: HashMap<SurfaceId, SurfaceSync<P>>,
}

impl<P> Default for SurfaceSyncRegistry<P> {
    fn default() -> Self {
        Self {
            active: HashMap::new(),
        }
    }
}

impl<P> SurfaceSyncRegistry<P> {
    pub(super) fn replace(
        &mut self,
        surface: SurfaceId,
        buffer: SurfaceBufferId,
        acquire: P,
        release: P,
    ) -> Option<SurfaceSync<P>> {
        self.active.insert(
            surface,
            SurfaceSync {
                buffer,
                acquire: Some(acquire),
                release,
            },
        )
    }

    /// Keep an explicit point while a surface merely commits damage to the
    /// same attachment.  A buffer replacement/removal retires its old point.
    pub(super) fn reconcile_implicit(
        &mut self,
        surface: SurfaceId,
        current_buffer: Option<SurfaceBufferId>,
    ) -> Option<SurfaceSync<P>> {
        let same_buffer = self
            .active
            .get(&surface)
            .is_some_and(|sync| Some(sync.buffer) == current_buffer);
        (!same_buffer)
            .then(|| self.active.remove(&surface))
            .flatten()
    }

    pub(super) fn pending_acquire(
        &self,
        surface: SurfaceId,
        buffer: SurfaceBufferId,
    ) -> Option<&P> {
        self.active
            .get(&surface)
            .filter(|sync| sync.buffer == buffer)
            .and_then(|sync| sync.acquire.as_ref())
    }

    pub(super) fn mark_acquire_imported(&mut self, surface: SurfaceId) -> bool {
        self.active
            .get_mut(&surface)
            .and_then(|sync| sync.acquire.take())
            .is_some()
    }

    pub(super) fn remove(&mut self, surface: SurfaceId) -> Option<SurfaceSync<P>> {
        self.active.remove(&surface)
    }

    #[cfg(test)]
    pub(super) fn active_buffer(&self, surface: SurfaceId) -> Option<SurfaceBufferId> {
        self.active.get(&surface).map(|sync| sync.buffer)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.active.len()
    }
}

#[derive(Debug)]
pub(super) struct SurfaceSync<P> {
    pub(super) buffer: SurfaceBufferId,
    pub(super) acquire: Option<P>,
    pub(super) release: P,
}

#[derive(Debug)]
pub(crate) struct ExplicitSyncPoints {
    pub(crate) acquire: DrmSyncPoint,
    pub(crate) release: DrmSyncPoint,
}

#[derive(Debug)]
pub(super) enum PendingClientRelease {
    Signal(DrmSyncPoint),
    SyncFile {
        point: DrmSyncPoint,
        fence: Arc<OwnedFd>,
    },
    Timeline {
        point: DrmSyncPoint,
        value: u64,
    },
}

impl RuntimeState {
    pub(crate) fn finish_unused_explicit_sync(&mut self, points: ExplicitSyncPoints) {
        self.queue_client_release(points.release, ClientReleaseFence::Ready);
    }

    pub(crate) fn reconcile_surface_sync(
        &mut self,
        surface: &WlSurface,
        points: Option<ExplicitSyncPoints>,
    ) {
        let Some(surface_id) = self.surface_buffers.surface_id(&surface.id()) else {
            if let Some(points) = points {
                self.queue_client_release(points.release, ClientReleaseFence::Ready);
            }
            return;
        };
        let current = self.surface_buffers.current_content(&surface.id());
        let Some(current) = current else {
            if let Some(old) = self.surface_sync.remove(surface_id) {
                self.finish_surface_sync(surface_id, old.release);
            }
            if let Some(points) = points {
                self.queue_client_release(points.release, ClientReleaseFence::Ready);
            }
            return;
        };

        if let Some(points) = points {
            if let Some(old) = self.surface_sync.replace(
                surface_id,
                current.buffer_id,
                points.acquire,
                points.release,
            ) {
                self.finish_surface_sync(surface_id, old.release);
            }
        } else if let Some(old) = self
            .surface_sync
            .reconcile_implicit(surface_id, Some(current.buffer_id))
        {
            self.finish_surface_sync(surface_id, old.release);
        }
        self.flush_client_releases();
    }

    pub(super) fn finish_surface_sync(&mut self, surface: SurfaceId, release: DrmSyncPoint) {
        let completed = self
            .renderer
            .as_ref()
            .and_then(|renderer| renderer.completed_timeline().ok())
            .unwrap_or(0);
        let fence = self
            .renderer
            .as_mut()
            .map(|renderer| renderer.finish_client_sync(surface, completed))
            .unwrap_or(ClientReleaseFence::Ready);
        self.queue_client_release(release, fence);
    }

    fn queue_client_release(&mut self, point: DrmSyncPoint, fence: ClientReleaseFence) {
        match fence {
            ClientReleaseFence::Ready => {
                if let Err(error) = point.signal() {
                    warn!(%error, "failed to signal an unused client release point");
                    self.pending_client_releases
                        .push(PendingClientRelease::Signal(point));
                }
            }
            ClientReleaseFence::SyncFile(fence) => {
                if let Err(error) = point.import_sync_file(fence.as_fd()) {
                    warn!(%error, "failed to import renderer completion into client release point");
                    self.pending_client_releases
                        .push(PendingClientRelease::SyncFile { point, fence });
                }
            }
            ClientReleaseFence::PendingTimeline(value) => {
                self.pending_client_releases
                    .push(PendingClientRelease::Timeline { point, value });
            }
        }
    }

    pub(crate) fn flush_client_releases(&mut self) {
        if self.pending_client_releases.is_empty() {
            return;
        }
        let completed = self
            .renderer
            .as_ref()
            .and_then(|renderer| renderer.completed_timeline().ok());
        let pending = std::mem::take(&mut self.pending_client_releases);
        for release in pending {
            match release {
                PendingClientRelease::Signal(point) => {
                    if let Err(error) = point.signal() {
                        warn!(%error, "failed to retry client release point signal");
                        self.pending_client_releases
                            .push(PendingClientRelease::Signal(point));
                    }
                }
                PendingClientRelease::SyncFile { point, fence } => {
                    if let Err(error) = point.import_sync_file(fence.as_fd()) {
                        warn!(%error, "failed to retry renderer completion import");
                        self.pending_client_releases
                            .push(PendingClientRelease::SyncFile { point, fence });
                    }
                }
                PendingClientRelease::Timeline { point, value } => {
                    if completed.is_some_and(|completed| completed >= value) {
                        if let Err(error) = point.signal() {
                            warn!(%error, "failed to signal retired timeline client release");
                            self.pending_client_releases
                                .push(PendingClientRelease::Signal(point));
                        }
                    } else {
                        self.pending_client_releases
                            .push(PendingClientRelease::Timeline { point, value });
                    }
                }
            }
        }
    }

    pub(crate) fn prepare_surface_acquires(
        &mut self,
        scene: &crate::scene::SceneSnapshot,
    ) -> Result<(), String> {
        let mut seen = HashSet::new();
        let pending = scene
            .contents()
            .iter()
            .filter_map(|content| {
                if !seen.insert(content.surface_id) {
                    return None;
                }
                self.surface_sync
                    .pending_acquire(content.surface_id, content.buffer_id)
                    .cloned()
                    .map(|point| (content.surface_id, point))
            })
            .collect::<Vec<_>>();
        for (surface, point) in pending {
            let fd = point
                .export_sync_file()
                .map_err(|error| format!("failed to export client acquire point: {error}"))?;
            let renderer = self
                .renderer
                .as_mut()
                .ok_or_else(|| "renderer is unavailable for explicit sync".to_owned())?;
            renderer
                .import_client_acquire(surface, fd)
                .map_err(|error| error.to_string())?;
            self.surface_sync.mark_acquire_imported(surface);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SURFACE: SurfaceId = SurfaceId::new(1);
    const BUFFER_A: SurfaceBufferId = SurfaceBufferId::new(10);
    const BUFFER_B: SurfaceBufferId = SurfaceBufferId::new(11);

    #[test]
    fn damage_commit_keeps_the_active_explicit_attachment() {
        let mut registry = SurfaceSyncRegistry::<u32>::default();
        registry.replace(SURFACE, BUFFER_A, 3, 4);
        assert!(
            registry
                .reconcile_implicit(SURFACE, Some(BUFFER_A))
                .is_none()
        );
        assert_eq!(registry.active_buffer(SURFACE), Some(BUFFER_A));
        assert_eq!(registry.pending_acquire(SURFACE, BUFFER_A), Some(&3));
    }

    #[test]
    fn replacing_a_buffer_returns_the_old_release_point() {
        let mut registry = SurfaceSyncRegistry::<u32>::default();
        assert!(registry.replace(SURFACE, BUFFER_A, 3, 4).is_none());
        let old = registry.replace(SURFACE, BUFFER_B, 5, 6).unwrap();
        assert_eq!(old.buffer, BUFFER_A);
        assert_eq!(old.release, 4);
        assert_eq!(registry.active_buffer(SURFACE), Some(BUFFER_B));
    }

    #[test]
    fn acquire_is_consumed_only_after_renderer_import_succeeds() {
        let mut registry = SurfaceSyncRegistry::<u32>::default();
        registry.replace(SURFACE, BUFFER_A, 3, 4);
        assert_eq!(registry.pending_acquire(SURFACE, BUFFER_A), Some(&3));
        assert!(registry.mark_acquire_imported(SURFACE));
        assert!(registry.pending_acquire(SURFACE, BUFFER_A).is_none());
        assert!(!registry.mark_acquire_imported(SURFACE));
    }

    #[test]
    fn detach_retires_the_point_and_clears_registry() {
        let mut registry = SurfaceSyncRegistry::<u32>::default();
        registry.replace(SURFACE, BUFFER_A, 3, 4);
        let old = registry.reconcile_implicit(SURFACE, None).unwrap();
        assert_eq!(old.release, 4);
        assert_eq!(registry.len(), 0);
    }
}
