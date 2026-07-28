use std::collections::HashMap;

use crate::{SurfaceBufferId, SurfaceId};

/// Explicit-sync points associated with active surface attachments.
///
/// `P` is adapter-owned. The registry never inspects or signals a point; it
/// only enforces attachment replacement and acquire-consumption ordering.
#[derive(Debug)]
pub struct SurfaceSyncRegistry<P> {
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
    pub fn replace(
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

    /// Retain explicit points for a damage-only commit to the same buffer.
    pub fn reconcile_implicit(
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

    pub fn pending_acquire(&self, surface: SurfaceId, buffer: SurfaceBufferId) -> Option<&P> {
        self.active
            .get(&surface)
            .filter(|sync| sync.buffer == buffer)
            .and_then(|sync| sync.acquire.as_ref())
    }

    /// Consume an acquire only after the renderer successfully imported it.
    pub fn mark_acquire_imported(&mut self, surface: SurfaceId) -> bool {
        self.active
            .get_mut(&surface)
            .and_then(|sync| sync.acquire.take())
            .is_some()
    }

    pub fn remove(&mut self, surface: SurfaceId) -> Option<SurfaceSync<P>> {
        self.active.remove(&surface)
    }

    pub fn active_buffer(&self, surface: SurfaceId) -> Option<SurfaceBufferId> {
        self.active.get(&surface).map(|sync| sync.buffer)
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
}

#[derive(Debug)]
pub struct SurfaceSync<P> {
    pub buffer: SurfaceBufferId,
    pub acquire: Option<P>,
    pub release: P,
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
        assert!(registry.is_empty());
    }
}
