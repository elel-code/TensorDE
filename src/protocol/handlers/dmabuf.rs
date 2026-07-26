use std::cell::RefCell;

use smithay::{
    backend::allocator::{Buffer, dmabuf::Dmabuf as SmithayDmabuf},
    wayland::{
        compositor::{BufferAssignment, SurfaceAttributes, with_states},
        dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier, get_dmabuf},
        drm_syncobj::{DrmSyncobjCachedState, DrmSyncobjHandler, DrmSyncobjState},
    },
};
use tracing::warn;
use wayland_protocols::wp::linux_drm_syncobj::v1::server::wp_linux_drm_syncobj_surface_v1::{
    self, WpLinuxDrmSyncobjSurfaceV1,
};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::protocol::state::{ExplicitSyncPoints, RuntimeState};

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
                if conflicting || get_dmabuf(&buffer).is_err() {
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

impl DmabufHandler for RuntimeState {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        self.protocol_globals.dmabuf_state()
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: SmithayDmabuf,
        notifier: ImportNotifier,
    ) {
        let Some(size) = dmabuf_size(&dmabuf) else {
            notifier.failed();
            return;
        };
        let Some(_) = self.renderer.as_ref() else {
            notifier.failed();
            return;
        };
        let Some(buffer_id) = self.allocate_client_buffer_id() else {
            warn!("client buffer identity space is exhausted; rejecting linux-dmabuf import");
            notifier.failed();
            return;
        };
        let renderer_dmabuf = renderer_dmabuf(&dmabuf, size);
        let import_result = self
            .renderer
            .as_mut()
            .expect("renderer existence was checked above")
            .import_client_dmabuf(buffer_id, &renderer_dmabuf);
        match import_result {
            Ok(()) => match notifier.successful::<RuntimeState>() {
                Ok(buffer) => {
                    if !self.register_imported_client_buffer(buffer.id(), buffer_id, size) {
                        self.release_client_buffers([buffer_id]);
                        warn!("linux-dmabuf buffer identity was already occupied; released import");
                    }
                }
                Err(error) => {
                    self.release_client_buffers([buffer_id]);
                    warn!(%error, "client disappeared while completing linux-dmabuf import");
                }
            },
            Err(error) => {
                warn!(%error, "client linux-dmabuf import failed");
                notifier.failed();
            }
        }
    }
}

fn dmabuf_size(dmabuf: &SmithayDmabuf) -> Option<tensor_util::Size> {
    let size = dmabuf.size();
    Some(tensor_util::Size::new(
        u32::try_from(size.w).ok()?,
        u32::try_from(size.h).ok()?,
    ))
    .filter(|size| size.width > 0 && size.height > 0)
}

fn renderer_dmabuf<'a>(
    dmabuf: &'a SmithayDmabuf,
    size: tensor_util::Size,
) -> crate::render::Dmabuf<std::os::fd::BorrowedFd<'a>> {
    let planes = dmabuf
        .handles()
        .zip(dmabuf.offsets())
        .zip(dmabuf.strides())
        .map(|((fd, offset), stride)| crate::render::DmabufPlane { fd, offset, stride })
        .collect();
    crate::render::Dmabuf {
        size,
        format: crate::backend::host_drm_format(dmabuf.format()),
        node: None,
        planes,
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
