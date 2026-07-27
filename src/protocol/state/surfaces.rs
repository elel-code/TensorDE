//! Tensor render values extracted at the transitional Smithay adapter edge.

use smithay::backend::renderer::utils::RendererSurfaceStateUserData;
use smithay::wayland::compositor::SurfaceData;

#[cfg(feature = "tty")]
use smithay::backend::renderer::utils::CommitCounter;
#[cfg(feature = "tty")]
use wayland_server::{Resource, backend::ObjectId};

#[cfg(feature = "tty")]
pub(super) type SurfaceBufferRegistry = tensor_protocol::SurfaceBufferRegistry<ObjectId>;
#[cfg(feature = "tty")]
pub(super) type SurfaceCommit<K = ObjectId> = tensor_protocol::SurfaceCommit<K>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SurfaceViewSnapshot {
    pub(super) offset: (i32, i32),
    pub(super) size: (i32, i32),
}

#[cfg(feature = "tty")]
#[derive(Debug)]
pub(super) struct SurfaceRenderSnapshot {
    pub(super) buffer: Option<ObjectId>,
    pub(super) logical_size: Option<tensor_util::Size>,
    pub(super) commit: u64,
    pub(super) buffer_scale: u32,
    pub(super) transform: tensor_protocol::SurfaceTransform,
}

pub(super) fn surface_view(states: &SurfaceData) -> Option<SurfaceViewSnapshot> {
    let renderer = states
        .data_map
        .get::<RendererSurfaceStateUserData>()?
        .lock()
        .unwrap();
    let view = renderer.view()?;
    Some(SurfaceViewSnapshot {
        offset: (view.offset.x, view.offset.y),
        size: (view.dst.w, view.dst.h),
    })
}

#[cfg(feature = "tty")]
pub(super) fn surface_render_snapshot(states: &SurfaceData) -> Option<SurfaceRenderSnapshot> {
    let renderer = states
        .data_map
        .get::<RendererSurfaceStateUserData>()?
        .lock()
        .unwrap();
    let buffer = renderer.buffer().map(|buffer| buffer.id());
    let logical_size = renderer.surface_size().and_then(|size| {
        Some(tensor_util::Size::new(
            u32::try_from(size.w).ok()?,
            u32::try_from(size.h).ok()?,
        ))
    });
    let commit = tensor_commit(renderer.current_commit());
    let buffer_scale = u32::try_from(renderer.buffer_scale()).unwrap_or(1);
    let transform = surface_transform(renderer.buffer_transform());

    Some(SurfaceRenderSnapshot {
        buffer,
        logical_size,
        commit,
        buffer_scale,
        transform,
    })
}

#[cfg(feature = "tty")]
fn tensor_commit(commit: CommitCounter) -> u64 {
    let value = commit
        .distance(Some(CommitCounter::default()))
        .expect("zero is never newer than an unsigned commit counter");
    u64::try_from(value).expect("Rust usize targets fit in u64")
}

#[cfg(feature = "tty")]
fn surface_transform(transform: smithay::utils::Transform) -> tensor_protocol::SurfaceTransform {
    use smithay::utils::Transform;
    use tensor_protocol::SurfaceTransform;

    match transform {
        Transform::Normal => SurfaceTransform::Normal,
        Transform::_90 => SurfaceTransform::Rotate90,
        Transform::_180 => SurfaceTransform::Rotate180,
        Transform::_270 => SurfaceTransform::Rotate270,
        Transform::Flipped => SurfaceTransform::Flipped,
        Transform::Flipped90 => SurfaceTransform::Flipped90,
        Transform::Flipped180 => SurfaceTransform::Flipped180,
        Transform::Flipped270 => SurfaceTransform::Flipped270,
    }
}

#[cfg(all(test, feature = "tty"))]
mod tests {
    use super::*;

    #[test]
    fn smithay_commit_maps_to_a_stable_tensor_value() {
        let commit = CommitCounter::from(41);
        assert_eq!(tensor_commit(commit), 41);
        assert_eq!(tensor_commit(commit), 41);
        assert_eq!(tensor_commit(CommitCounter::from(42)), 42);
    }
}
