use std::{collections::HashMap, hash::Hash};

use smithay::{
    backend::renderer::utils::CommitCounter, reexports::wayland_server::backend::ObjectId,
};
use tensor_util::{Rect, Size};

use crate::{
    ecs::{SurfaceBufferId, SurfaceId},
    scene::{ContentRevision, SurfaceContent, SurfaceTransform},
};

/// Protocol-owned mapping between live Wayland resources and value-only scene
/// identities.  Renderer handles and dma-buf file descriptors never enter this
/// table.
#[derive(Debug)]
pub(super) struct SurfaceBufferRegistry<K = ObjectId> {
    next_surface_id: u64,
    next_buffer_id: u64,
    surfaces: HashMap<K, SurfaceState<K>>,
    buffers: HashMap<K, BufferState>,
}

impl<K> Default for SurfaceBufferRegistry<K> {
    fn default() -> Self {
        Self {
            next_surface_id: 1,
            next_buffer_id: 1,
            surfaces: HashMap::new(),
            buffers: HashMap::new(),
        }
    }
}

#[derive(Debug)]
struct SurfaceState<K> {
    id: SurfaceId,
    current: Option<AttachedBuffer<K>>,
    revision: ContentRevision,
    last_commit: Option<CommitCounter>,
    buffer_scale: u32,
    transform: SurfaceTransform,
}

#[derive(Clone, Debug)]
struct AttachedBuffer<K> {
    object: K,
    id: SurfaceBufferId,
    local_geometry: Rect,
}

#[derive(Debug)]
struct BufferState {
    id: SurfaceBufferId,
    size: Size,
    attachments: usize,
    destroyed: bool,
}

#[derive(Debug, Default)]
pub(super) struct SurfaceUpdate {
    pub(super) changed: bool,
    pub(super) content: Option<SurfaceContent>,
    pub(super) released_buffers: Vec<SurfaceBufferId>,
}

/// Value snapshot collected after Smithay applies one surface commit.
#[derive(Clone, Debug)]
pub(super) struct SurfaceCommit<K = ObjectId> {
    pub(super) buffer: Option<K>,
    pub(super) logical_size: Option<Size>,
    pub(super) local_offset: (i32, i32),
    pub(super) commit: CommitCounter,
    pub(super) buffer_scale: u32,
    pub(super) transform: SurfaceTransform,
}

impl<K> SurfaceBufferRegistry<K>
where
    K: Clone + Eq + Hash,
{
    pub(super) fn register_surface(&mut self, object: K) -> Option<SurfaceId> {
        if let Some(state) = self.surfaces.get(&object) {
            return Some(state.id);
        }
        let id = SurfaceId::new(self.allocate_surface_id()?);
        self.surfaces.insert(
            object,
            SurfaceState {
                id,
                current: None,
                revision: ContentRevision::default(),
                last_commit: None,
                buffer_scale: 1,
                transform: SurfaceTransform::Normal,
            },
        );
        Some(id)
    }

    pub(super) fn allocate_buffer_id_for_import(&mut self) -> Option<SurfaceBufferId> {
        Some(SurfaceBufferId::new(self.allocate_buffer_id()?))
    }

    pub(super) fn register_imported_buffer(
        &mut self,
        object: K,
        id: SurfaceBufferId,
        size: Size,
    ) -> bool {
        if let Some(state) = self.buffers.get(&object) {
            let same_import = !state.destroyed && state.id == id && state.size == size;
            return same_import;
        }
        self.buffers.insert(
            object,
            BufferState {
                id,
                size,
                attachments: 0,
                destroyed: false,
            },
        );
        true
    }

    pub(super) fn update_surface(
        &mut self,
        surface: &K,
        snapshot: &SurfaceCommit<K>,
    ) -> SurfaceUpdate {
        let Some(mut state) = self.surfaces.remove(surface) else {
            return SurfaceUpdate::default();
        };
        let buffer_scale = snapshot.buffer_scale.max(1);

        let current_object = state.current.as_ref().map(|current| &current.object);
        let next = snapshot.buffer.as_ref().and_then(|object| {
            let record = self.buffers.get(object)?;
            // Destroying wl_buffer does not invalidate an attachment already
            // committed to a surface.  It must, however, never create a new
            // attachment after destruction.
            if record.destroyed && current_object != Some(object) {
                return None;
            }
            let logical_size = snapshot
                .logical_size
                .filter(|size| size.width > 0 && size.height > 0)
                .unwrap_or(record.size);
            Some(AttachedBuffer {
                object: object.clone(),
                id: record.id,
                local_geometry: Rect::new(
                    snapshot.local_offset.0,
                    snapshot.local_offset.1,
                    logical_size.width,
                    logical_size.height,
                ),
            })
        });
        let changed = state.current.as_ref().map(|current| current.id)
            != next.as_ref().map(|current| current.id)
            || state
                .current
                .as_ref()
                .zip(next.as_ref())
                .is_some_and(|(old, new)| old.local_geometry != new.local_geometry)
            || state.last_commit != Some(snapshot.commit)
            || state.buffer_scale != buffer_scale
            || state.transform != snapshot.transform;

        let mut released_buffers = Vec::new();
        if state.current.as_ref().map(|current| &current.object)
            != next.as_ref().map(|current| &current.object)
        {
            if let Some(previous) = state.current.take() {
                self.detach_buffer(&previous.object, &mut released_buffers);
            }
            if let Some(current) = &next
                && let Some(record) = self.buffers.get_mut(&current.object)
            {
                record.attachments = record.attachments.saturating_add(1);
            }
        }

        if changed {
            state.revision = state.revision.next();
            state.current = next;
        }
        state.last_commit = Some(snapshot.commit);
        state.buffer_scale = buffer_scale;
        state.transform = snapshot.transform;
        let content = state.current.as_ref().map(|current| SurfaceContent {
            surface_id: state.id,
            buffer_id: current.id,
            revision: state.revision,
            buffer_size: self
                .buffers
                .get(&current.object)
                .map(|record| record.size)
                .unwrap_or(current.local_geometry.size()),
            local_geometry: current.local_geometry,
            buffer_scale: state.buffer_scale,
            transform: state.transform,
        });
        self.surfaces.insert(surface.clone(), state);

        SurfaceUpdate {
            changed,
            content,
            released_buffers,
        }
    }

    pub(super) fn current_content(&self, surface: &K) -> Option<SurfaceContent> {
        let state = self.surfaces.get(surface)?;
        let current = state.current.as_ref()?;
        Some(SurfaceContent {
            surface_id: state.id,
            buffer_id: current.id,
            revision: state.revision,
            buffer_size: self
                .buffers
                .get(&current.object)
                .map(|record| record.size)
                .unwrap_or(current.local_geometry.size()),
            local_geometry: current.local_geometry,
            buffer_scale: state.buffer_scale,
            transform: state.transform,
        })
    }

    pub(super) fn surface_id(&self, surface: &K) -> Option<SurfaceId> {
        self.surfaces.get(surface).map(|state| state.id)
    }

    pub(super) fn remove_surface(&mut self, surface: &K) -> Vec<SurfaceBufferId> {
        let Some(state) = self.surfaces.remove(surface) else {
            return Vec::new();
        };
        let mut released = Vec::new();
        if let Some(current) = state.current {
            self.detach_buffer(&current.object, &mut released);
        }
        released
    }

    /// Mark a Wayland buffer object as destroyed.  An attached buffer remains
    /// renderer-live until every surface has stopped referring to it.
    pub(super) fn buffer_destroyed(&mut self, object: &K) -> Vec<SurfaceBufferId> {
        let Some(record) = self.buffers.get_mut(object) else {
            return Vec::new();
        };
        record.destroyed = true;
        self.collect_destroyed(object).into_iter().collect()
    }

    fn detach_buffer(&mut self, object: &K, released: &mut Vec<SurfaceBufferId>) {
        if let Some(record) = self.buffers.get_mut(object) {
            record.attachments = record.attachments.saturating_sub(1);
        }
        if let Some(id) = self.collect_destroyed(object) {
            released.push(id);
        }
    }

    fn collect_destroyed(&mut self, object: &K) -> Option<SurfaceBufferId> {
        let removable = self
            .buffers
            .get(object)
            .is_some_and(|record| record.destroyed && record.attachments == 0);
        removable.then(|| self.buffers.remove(object).expect("buffer was checked").id)
    }

    fn allocate_surface_id(&mut self) -> Option<u64> {
        let id = self.next_surface_id;
        self.next_surface_id = id.checked_add(1)?;
        Some(id)
    }

    fn allocate_buffer_id(&mut self) -> Option<u64> {
        let id = self.next_buffer_id;
        self.next_buffer_id = id.checked_add(1)?;
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SURFACE_A: u64 = 1;
    const SURFACE_B: u64 = 2;
    const BUFFER_A: u64 = 10;
    const BUFFER_B: u64 = 11;

    fn commit(buffer: Option<u64>, counter: usize) -> SurfaceCommit<u64> {
        SurfaceCommit {
            buffer,
            logical_size: Some(Size::new(80, 60)),
            local_offset: (0, 0),
            commit: CommitCounter::from(counter),
            buffer_scale: 1,
            transform: SurfaceTransform::Normal,
        }
    }

    #[test]
    fn generated_ids_never_start_at_zero() {
        let mut registry = SurfaceBufferRegistry::<u64>::default();
        assert_eq!(registry.allocate_surface_id(), Some(1));
        assert_eq!(registry.allocate_buffer_id(), Some(1));
    }

    #[test]
    fn generated_ids_fail_closed_instead_of_wrapping() {
        let mut registry = SurfaceBufferRegistry::<u64> {
            next_surface_id: u64::MAX,
            next_buffer_id: u64::MAX,
            ..Default::default()
        };

        assert_eq!(registry.allocate_surface_id(), None);
        assert_eq!(registry.allocate_buffer_id(), None);
        assert!(registry.register_surface(SURFACE_A).is_none());
        assert!(registry.allocate_buffer_id_for_import().is_none());
    }

    #[test]
    fn destroyed_attached_buffer_retires_only_after_surface_detach() {
        let mut registry = SurfaceBufferRegistry::<u64>::default();
        registry.register_surface(SURFACE_A).unwrap();
        registry.register_imported_buffer(BUFFER_A, SurfaceBufferId::new(7), Size::new(80, 60));

        let attached = registry.update_surface(&SURFACE_A, &commit(Some(BUFFER_A), 1));
        assert_eq!(attached.content.unwrap().buffer_id, SurfaceBufferId::new(7));
        assert!(registry.buffer_destroyed(&BUFFER_A).is_empty());

        let recommitted = registry.update_surface(&SURFACE_A, &commit(Some(BUFFER_A), 2));
        assert_eq!(
            recommitted.content.unwrap().buffer_id,
            SurfaceBufferId::new(7)
        );
        assert!(recommitted.released_buffers.is_empty());
        assert_eq!(
            registry.remove_surface(&SURFACE_A),
            [SurfaceBufferId::new(7)]
        );
    }

    #[test]
    fn destroyed_buffer_cannot_gain_a_new_surface_attachment() {
        let mut registry = SurfaceBufferRegistry::<u64>::default();
        registry.register_surface(SURFACE_A).unwrap();
        registry.register_surface(SURFACE_B).unwrap();
        registry.register_imported_buffer(BUFFER_A, SurfaceBufferId::new(7), Size::new(80, 60));
        registry.update_surface(&SURFACE_A, &commit(Some(BUFFER_A), 1));
        assert!(registry.buffer_destroyed(&BUFFER_A).is_empty());

        let rejected = registry.update_surface(&SURFACE_B, &commit(Some(BUFFER_A), 1));
        assert!(rejected.content.is_none());
        assert_eq!(registry.buffers[&BUFFER_A].attachments, 1);
        assert_eq!(
            registry.remove_surface(&SURFACE_A),
            [SurfaceBufferId::new(7)]
        );
    }

    #[test]
    fn replacing_a_destroyed_attachment_releases_only_the_old_image() {
        let mut registry = SurfaceBufferRegistry::<u64>::default();
        registry.register_surface(SURFACE_A).unwrap();
        registry.register_imported_buffer(BUFFER_A, SurfaceBufferId::new(7), Size::new(80, 60));
        registry.register_imported_buffer(BUFFER_B, SurfaceBufferId::new(8), Size::new(80, 60));
        registry.update_surface(&SURFACE_A, &commit(Some(BUFFER_A), 1));
        assert!(registry.buffer_destroyed(&BUFFER_A).is_empty());

        let replaced = registry.update_surface(&SURFACE_A, &commit(Some(BUFFER_B), 2));
        assert_eq!(replaced.released_buffers, [SurfaceBufferId::new(7)]);
        assert_eq!(replaced.content.unwrap().buffer_id, SurfaceBufferId::new(8));
        assert!(!registry.buffers.contains_key(&BUFFER_A));
        assert!(registry.remove_surface(&SURFACE_A).is_empty());
        assert_eq!(
            registry.buffer_destroyed(&BUFFER_B),
            [SurfaceBufferId::new(8)]
        );
    }

    #[test]
    fn a_destroyed_attached_object_cannot_be_registered_as_a_new_buffer() {
        let mut registry = SurfaceBufferRegistry::<u64>::default();
        registry.register_surface(SURFACE_A).unwrap();
        assert!(registry.register_imported_buffer(
            BUFFER_A,
            SurfaceBufferId::new(7),
            Size::new(80, 60),
        ));
        registry.update_surface(&SURFACE_A, &commit(Some(BUFFER_A), 1));
        assert!(registry.buffer_destroyed(&BUFFER_A).is_empty());

        assert!(!registry.register_imported_buffer(
            BUFFER_A,
            SurfaceBufferId::new(8),
            Size::new(80, 60),
        ));
        assert_eq!(registry.buffers[&BUFFER_A].id, SurfaceBufferId::new(7));
    }
}
