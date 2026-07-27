use std::cell::RefCell;

use smithay::wayland::compositor::{BufferAssignment, SurfaceAttributes, with_states};
use wayland_protocols::wp::linux_drm_syncobj::v1::server::wp_linux_drm_syncobj_surface_v1::{
    self, WpLinuxDrmSyncobjSurfaceV1,
};
use wayland_server::{
    Resource,
    protocol::{wl_buffer::WlBuffer, wl_surface::WlSurface},
};

use crate::protocol::{
    globals::{
        DrmSyncobjCachedState, DrmSyncobjHandler, DrmSyncobjState,
        dmabuf::{DmabufBuffer, DmabufImportHandler, is_dmabuf_buffer},
    },
    state::{ExplicitSyncPoints, RuntimeState},
};

pub(super) enum ExplicitSyncCommit {
    None,
    Points(ExplicitSyncPoints),
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExplicitSyncShape {
    None,
    Points,
    MissingPoints,
    Rejected,
}

impl DrmSyncobjHandler for RuntimeState {
    fn drm_syncobj_state(&mut self) -> Option<&mut DrmSyncobjState> {
        self.protocol_globals.drm_syncobj_state()
    }
}

pub(super) fn take_explicit_sync_points(surface: &WlSurface) -> ExplicitSyncCommit {
    with_states(surface, |states| {
        let syncobj_surface = states
            .data_map
            .get::<RefCell<Option<WpLinuxDrmSyncobjSurfaceV1>>>()
            .and_then(|surface| surface.borrow().clone());
        let new_buffer = {
            let mut cached = states.cached_state.get::<SurfaceAttributes>();
            cached
                .current()
                .buffer
                .as_ref()
                .and_then(|assignment| match assignment {
                    BufferAssignment::NewBuffer(buffer) => Some(buffer.clone()),
                    _ => None,
                })
        };
        let mut cached = states.cached_state.get::<DrmSyncobjCachedState>();
        let current = cached.current();
        let acquire = current.acquire_point.take();
        let release = current.release_point.take();

        match explicit_sync_shape(
            syncobj_surface.is_some(),
            new_buffer.is_some(),
            acquire.is_some(),
            release.is_some(),
        ) {
            ExplicitSyncShape::None => ExplicitSyncCommit::None,
            ExplicitSyncShape::Points => {
                let buffer = new_buffer.expect("shape checked a new buffer");
                let acquire = acquire.expect("shape checked an acquire point");
                let release = release.expect("shape checked a release point");
                let conflicting =
                    acquire.timeline() == release.timeline() && release.point() <= acquire.point();
                if conflicting || !is_dmabuf_buffer(&buffer) {
                    ExplicitSyncCommit::Rejected
                } else {
                    ExplicitSyncCommit::Points(ExplicitSyncPoints { acquire, release })
                }
            }
            ExplicitSyncShape::MissingPoints => {
                syncobj_surface
                    .expect("shape checked a syncobj surface")
                    .post_error(
                        wp_linux_drm_syncobj_surface_v1::Error::NoAcquirePoint,
                        "buffer commit did not provide explicit acquire/release points".to_owned(),
                    );
                ExplicitSyncCommit::Rejected
            }
            ExplicitSyncShape::Rejected => ExplicitSyncCommit::Rejected,
        }
    })
}

fn explicit_sync_shape(
    has_surface: bool,
    has_buffer: bool,
    has_acquire: bool,
    has_release: bool,
) -> ExplicitSyncShape {
    match (has_surface, has_buffer, has_acquire, has_release) {
        (false, _, false, false) | (true, false, false, false) => ExplicitSyncShape::None,
        (true, true, true, true) => ExplicitSyncShape::Points,
        (true, true, false, false) => ExplicitSyncShape::MissingPoints,
        _ => ExplicitSyncShape::Rejected,
    }
}

impl DmabufImportHandler for RuntimeState {
    fn import_dmabuf(
        &mut self,
        buffer: &DmabufBuffer,
    ) -> Result<crate::ecs::SurfaceBufferId, String> {
        if buffer.flags() != 0 {
            return Err(format!(
                "dma-buf flags {:#x} require unsupported image transforms",
                buffer.flags()
            ));
        }
        if self.renderer.is_none() {
            return Err("renderer is unavailable".to_owned());
        }
        let Some(buffer_id) = self.allocate_client_buffer_id() else {
            return Err("client buffer identity space is exhausted".to_owned());
        };
        self.renderer
            .as_mut()
            .expect("renderer existence was checked above")
            .import_client_dmabuf(buffer_id, buffer.descriptor())
            .map_err(|error| error.to_string())?;
        Ok(buffer_id)
    }

    fn register_dmabuf_buffer(
        &mut self,
        buffer: &WlBuffer,
        id: crate::ecs::SurfaceBufferId,
        size: tensor_util::Size,
    ) -> bool {
        self.register_imported_client_buffer(buffer.id(), id, size)
    }

    fn release_dmabuf_import(&mut self, id: crate::ecs::SurfaceBufferId) {
        self.release_client_buffers([id]);
    }

    fn dmabuf_buffer_destroyed(&mut self, buffer: &WlBuffer) {
        self.buffer_destroyed(&buffer.id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syncobj_surface_requires_both_points_for_every_buffer_attach() {
        assert_eq!(
            explicit_sync_shape(true, true, false, false),
            ExplicitSyncShape::MissingPoints
        );
        assert_eq!(
            explicit_sync_shape(true, true, true, false),
            ExplicitSyncShape::Rejected
        );
        assert_eq!(
            explicit_sync_shape(true, true, false, true),
            ExplicitSyncShape::Rejected
        );
        assert_eq!(
            explicit_sync_shape(true, true, true, true),
            ExplicitSyncShape::Points
        );
    }

    #[test]
    fn damage_only_commit_does_not_require_new_points() {
        assert_eq!(
            explicit_sync_shape(true, false, false, false),
            ExplicitSyncShape::None
        );
        assert_eq!(
            explicit_sync_shape(true, false, true, true),
            ExplicitSyncShape::Rejected
        );
    }
}
