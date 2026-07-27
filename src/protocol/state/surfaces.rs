//! Tensor-owned surface render state at the transitional Smithay compositor edge.

use std::sync::Mutex;

use smithay::wayland::compositor::{
    BufferAssignment, Damage, SUBSURFACE_ROLE, SubsurfaceCachedState, SurfaceAttributes,
    SurfaceData, TraversalAction, is_sync_subsurface, with_states, with_surface_tree_upward,
};
use tensor_protocol::{SurfaceAlpha, SurfaceSourceRect, SurfaceTransform};
use wayland_server::{
    Resource,
    protocol::{wl_buffer::WlBuffer, wl_output, wl_surface::WlSurface},
};

#[cfg(feature = "tty")]
use crate::protocol::globals::dmabuf::dmabuf_buffer;
use crate::protocol::globals::shm::shm_buffer;
use crate::protocol::globals::single_pixel_buffer::single_pixel_rgba;
use crate::protocol::globals::viewporter::{CommittedViewport, committed_viewport};

#[cfg(feature = "tty")]
use wayland_server::backend::ObjectId;

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
    pub(super) transform: SurfaceTransform,
    pub(super) source: Option<SurfaceSourceRect>,
    pub(super) alpha: SurfaceAlpha,
}

#[derive(Debug)]
struct SurfaceState {
    buffer: Option<WlBuffer>,
    buffer_size: Option<(i32, i32)>,
    view: Option<SurfaceViewSnapshot>,
    commit: u64,
    buffer_scale: u32,
    transform: SurfaceTransform,
    source: Option<SurfaceSourceRect>,
    alpha: SurfaceAlpha,
}

impl Default for SurfaceState {
    fn default() -> Self {
        Self {
            buffer: None,
            buffer_size: None,
            view: None,
            commit: 0,
            buffer_scale: 1,
            transform: SurfaceTransform::Normal,
            source: None,
            alpha: SurfaceAlpha::OPAQUE,
        }
    }
}

impl SurfaceState {
    fn update(&mut self, states: &SurfaceData) {
        let mut cached = states.cached_state.get::<SurfaceAttributes>();
        let attributes = cached.current();
        match attributes.buffer.take() {
            Some(BufferAssignment::NewBuffer(buffer)) => {
                let Some(buffer_size) = wire_buffer_size(&buffer) else {
                    if buffer.is_alive() {
                        buffer.release();
                    }
                    self.reset();
                    return;
                };
                self.replace_buffer(buffer);
                self.buffer_size = Some(buffer_size);
            }
            Some(BufferAssignment::Removed) => {
                self.reset();
                return;
            }
            None => {}
        }

        let Some(buffer_size) = self.buffer_size else {
            return;
        };
        self.buffer_scale = u32::try_from(attributes.buffer_scale).unwrap_or(1).max(1);
        self.transform = surface_transform(attributes.buffer_transform);
        let logical_size = logical_buffer_size(buffer_size, self.buffer_scale, self.transform);
        let viewport = committed_viewport(states, logical_size);
        let view = view_snapshot(states, logical_size, viewport);
        let damaged = attributes
            .damage
            .iter()
            .any(|damage| damage_overlaps(damage, buffer_size, view.size));
        attributes.damage.clear();
        if damaged {
            self.commit = self.commit.wrapping_add(1);
        }
        self.view = Some(view);
        self.source = viewport.source;
    }

    fn replace_buffer(&mut self, buffer: WlBuffer) {
        if self.buffer.as_ref() == Some(&buffer) {
            return;
        }
        self.release_buffer();
        self.buffer = Some(buffer);
    }

    fn reset(&mut self) {
        self.release_buffer();
        self.buffer_size = None;
        self.view = None;
        self.buffer_scale = 1;
        self.transform = SurfaceTransform::Normal;
        self.source = None;
        self.commit = self.commit.wrapping_add(1);
    }

    fn release_buffer(&mut self) {
        if let Some(buffer) = self.buffer.take()
            && buffer.is_alive()
        {
            buffer.release();
        }
    }
}

pub(crate) fn on_commit_surface_handler(surface: &WlSurface) {
    if is_sync_subsurface(surface) {
        return;
    }
    with_surface_tree_upward(
        surface,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |_, states, _| {
            states
                .data_map
                .insert_if_missing_threadsafe(|| Mutex::new(SurfaceState::default()));
            states
                .data_map
                .get::<Mutex<SurfaceState>>()
                .expect("surface state was inserted above")
                .lock()
                .unwrap()
                .update(states);
        },
        |_, _, _| true,
    );
}

pub(crate) fn destroy_surface_state(surface: &WlSurface) {
    with_states(surface, |states| {
        if let Some(state) = states.data_map.get::<Mutex<SurfaceState>>() {
            state.lock().unwrap().reset();
        }
    });
}

pub(crate) fn apply_surface_alpha(surface: &WlSurface, alpha: SurfaceAlpha) {
    with_states(surface, |states| {
        states
            .data_map
            .insert_if_missing_threadsafe(|| Mutex::new(SurfaceState::default()));
        let mut state = states
            .data_map
            .get::<Mutex<SurfaceState>>()
            .expect("surface state was inserted above")
            .lock()
            .unwrap();
        state.alpha = alpha;
    });
}

pub(super) fn surface_view(states: &SurfaceData) -> Option<SurfaceViewSnapshot> {
    states
        .data_map
        .get::<Mutex<SurfaceState>>()?
        .lock()
        .unwrap()
        .view
}

pub(in crate::protocol) fn surface_contains_point(surface: &WlSurface, point: (f64, f64)) -> bool {
    if !point.0.is_finite() || !point.1.is_finite() || point.0 < 0.0 || point.1 < 0.0 {
        return false;
    }
    with_states(surface, |states| {
        surface_view(states).is_some_and(|view| {
            point.0 < f64::from(view.size.0) && point.1 < f64::from(view.size.1)
        })
    })
}

#[cfg(feature = "tty")]
pub(super) fn surface_has_buffer(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        states
            .data_map
            .get::<Mutex<SurfaceState>>()
            .is_some_and(|state| state.lock().unwrap().buffer.is_some())
    })
}

pub(super) fn surface_buffer(surface: &WlSurface) -> Option<WlBuffer> {
    with_states(surface, |states| {
        states
            .data_map
            .get::<Mutex<SurfaceState>>()?
            .lock()
            .unwrap()
            .buffer
            .clone()
    })
}

#[cfg(test)]
pub(crate) fn test_surface_buffer(surface: &WlSurface) -> Option<WlBuffer> {
    surface_buffer(surface)
}

#[cfg(feature = "tty")]
pub(super) fn surface_render_snapshot(states: &SurfaceData) -> Option<SurfaceRenderSnapshot> {
    let state = states
        .data_map
        .get::<Mutex<SurfaceState>>()?
        .lock()
        .unwrap();
    let logical_size = state.view.and_then(|view| {
        Some(tensor_util::Size::new(
            u32::try_from(view.size.0).ok()?,
            u32::try_from(view.size.1).ok()?,
        ))
    });
    Some(SurfaceRenderSnapshot {
        buffer: state.buffer.as_ref().map(Resource::id),
        logical_size,
        commit: state.commit,
        buffer_scale: state.buffer_scale,
        transform: state.transform,
        source: state.source,
        alpha: state.alpha,
    })
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TestSurfaceState {
    pub(crate) surface: u32,
    pub(crate) buffer: Option<u32>,
    pub(crate) offset: (i32, i32),
    pub(crate) size: (i32, i32),
    pub(crate) commit: u64,
    pub(crate) buffer_scale: u32,
    pub(crate) transform: SurfaceTransform,
    pub(crate) source: Option<SurfaceSourceRect>,
    pub(crate) alpha: SurfaceAlpha,
}

#[cfg(test)]
pub(crate) fn test_surface_tree_states(root: &WlSurface) -> Vec<TestSurfaceState> {
    let mut snapshots = Vec::new();
    with_surface_tree_upward(
        root,
        (),
        |_, _, _| TraversalAction::DoChildren(()),
        |surface, states, _| {
            let Some(state) = states.data_map.get::<Mutex<SurfaceState>>() else {
                return;
            };
            let state = state.lock().unwrap();
            let Some(view) = state.view else {
                return;
            };
            snapshots.push(TestSurfaceState {
                surface: surface.id().protocol_id(),
                buffer: state
                    .buffer
                    .as_ref()
                    .map(|buffer| buffer.id().protocol_id()),
                offset: view.offset,
                size: view.size,
                commit: state.commit,
                buffer_scale: state.buffer_scale,
                transform: state.transform,
                source: state.source,
                alpha: state.alpha,
            });
        },
        |_, _, _| true,
    );
    snapshots
}

fn wire_buffer_size(buffer: &WlBuffer) -> Option<(i32, i32)> {
    #[cfg(feature = "tty")]
    if let Some(dmabuf) = dmabuf_buffer(buffer) {
        let size = dmabuf.size();
        return valid_size((
            i32::try_from(size.width).ok()?,
            i32::try_from(size.height).ok()?,
        ));
    }
    if single_pixel_rgba(buffer).is_some() {
        return Some((1, 1));
    }
    let metadata = shm_buffer(buffer)?.metadata();
    valid_size((metadata.width, metadata.height))
}

fn logical_buffer_size(
    buffer_size: (i32, i32),
    buffer_scale: u32,
    transform: SurfaceTransform,
) -> (i32, i32) {
    let scale = i32::try_from(buffer_scale).unwrap_or(i32::MAX).max(1);
    let size = (buffer_size.0 / scale, buffer_size.1 / scale);
    match transform {
        SurfaceTransform::Rotate90
        | SurfaceTransform::Rotate270
        | SurfaceTransform::Flipped90
        | SurfaceTransform::Flipped270 => (size.1, size.0),
        _ => size,
    }
}

fn view_snapshot(
    states: &SurfaceData,
    logical_size: (i32, i32),
    viewport: CommittedViewport,
) -> SurfaceViewSnapshot {
    let size = viewport.size().and_then(valid_size).unwrap_or(logical_size);
    let offset = if states.role == Some(SUBSURFACE_ROLE) {
        let mut subsurface = states.cached_state.get::<SubsurfaceCachedState>();
        let location = subsurface.current().location;
        (location.x, location.y)
    } else {
        (0, 0)
    };
    SurfaceViewSnapshot { offset, size }
}

fn damage_overlaps(damage: &Damage, buffer_size: (i32, i32), view_size: (i32, i32)) -> bool {
    match damage {
        Damage::Buffer(rect) => rectangle_overlaps(
            rect.loc.x,
            rect.loc.y,
            rect.size.w,
            rect.size.h,
            buffer_size,
        ),
        Damage::Surface(rect) => {
            rectangle_overlaps(rect.loc.x, rect.loc.y, rect.size.w, rect.size.h, view_size)
        }
    }
}

fn rectangle_overlaps(x: i32, y: i32, width: i32, height: i32, bounds: (i32, i32)) -> bool {
    width > 0
        && height > 0
        && x < bounds.0
        && y < bounds.1
        && x.saturating_add(width) > 0
        && y.saturating_add(height) > 0
}

fn valid_size(size: (i32, i32)) -> Option<(i32, i32)> {
    (size.0 > 0 && size.1 > 0).then_some(size)
}

fn surface_transform(transform: wl_output::Transform) -> SurfaceTransform {
    match transform {
        wl_output::Transform::Normal => SurfaceTransform::Normal,
        wl_output::Transform::_90 => SurfaceTransform::Rotate90,
        wl_output::Transform::_180 => SurfaceTransform::Rotate180,
        wl_output::Transform::_270 => SurfaceTransform::Rotate270,
        wl_output::Transform::Flipped => SurfaceTransform::Flipped,
        wl_output::Transform::Flipped90 => SurfaceTransform::Flipped90,
        wl_output::Transform::Flipped180 => SurfaceTransform::Flipped180,
        wl_output::Transform::Flipped270 => SurfaceTransform::Flipped270,
        _ => SurfaceTransform::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_size_applies_scale_before_axis_transform() {
        assert_eq!(
            logical_buffer_size((3840, 2160), 2, SurfaceTransform::Normal),
            (1920, 1080)
        );
        assert_eq!(
            logical_buffer_size((3840, 2160), 2, SurfaceTransform::Rotate90),
            (1080, 1920)
        );
        assert_eq!(
            logical_buffer_size((3840, 2160), 2, SurfaceTransform::Flipped270),
            (1080, 1920)
        );
    }

    #[test]
    fn damage_overlap_rejects_empty_and_outside_rectangles() {
        assert!(rectangle_overlaps(90, 50, 20, 20, (100, 60)));
        assert!(!rectangle_overlaps(100, 0, 20, 20, (100, 60)));
        assert!(!rectangle_overlaps(0, 60, 20, 20, (100, 60)));
        assert!(!rectangle_overlaps(0, 0, 0, 20, (100, 60)));
    }
}
