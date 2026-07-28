use std::sync::{Arc, Mutex, atomic::AtomicBool};

use tensor_util::{Point, Rect};
use wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, WEnum,
    backend::ClientId,
    protocol::{
        wl_callback::{self, WlCallback},
        wl_compositor::{self, WlCompositor},
        wl_region::{self, WlRegion},
        wl_subcompositor::{self, WlSubcompositor},
        wl_subsurface::{self, WlSubsurface},
        wl_surface::{self, WlSurface},
    },
};

use super::{
    BufferAssignment, Cacheable, Damage, RectangleKind, RegionAttributes, SurfaceAttributes,
    tree::{Location, PrivateSurfaceData, SurfaceUserData},
};
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

#[derive(Debug)]
pub(in crate::protocol) struct CompositorGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct SubcompositorGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct CallbackData;

#[derive(Debug)]
pub(super) struct RegionData {
    pub(super) attributes: Mutex<RegionAttributes>,
}

#[derive(Debug)]
pub(in crate::protocol) struct SubsurfaceData {
    surface: WlSurface,
}

#[derive(Debug)]
pub(super) struct SubsurfaceState {
    pub(super) sync: AtomicBool,
}

impl Default for SubsurfaceState {
    fn default() -> Self {
        Self {
            sync: AtomicBool::new(true),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::protocol) struct SubsurfaceCachedState {
    pub(in crate::protocol) location: Point,
}

impl Cacheable for SubsurfaceCachedState {
    fn commit(&mut self, _display: &DisplayHandle) -> Self {
        *self
    }

    fn merge_into(self, current: &mut Self, _display: &DisplayHandle) {
        *current = self;
    }
}

impl Cacheable for SurfaceAttributes {
    fn commit(&mut self, _display: &DisplayHandle) -> Self {
        Self {
            buffer: self.buffer.take(),
            buffer_delta: self.buffer_delta.take(),
            buffer_scale: self.buffer_scale,
            buffer_transform: self.buffer_transform,
            opaque_region: self.opaque_region.clone(),
            input_region: self.input_region.clone(),
            damage: std::mem::take(&mut self.damage),
            frame_callbacks: std::mem::take(&mut self.frame_callbacks),
            client_scale: self.client_scale,
        }
    }

    fn merge_into(self, current: &mut Self, _display: &DisplayHandle) {
        if self.buffer.is_some()
            && let Some(BufferAssignment::NewBuffer(previous)) =
                std::mem::replace(&mut current.buffer, self.buffer)
        {
            let replacement = current
                .buffer
                .as_ref()
                .and_then(|assignment| match assignment {
                    BufferAssignment::Removed => None,
                    BufferAssignment::NewBuffer(buffer) => Some(buffer),
                });
            if replacement != Some(&previous) {
                previous.release();
            }
        }
        current.buffer_delta = self.buffer_delta;
        current.buffer_scale = self.buffer_scale;
        current.buffer_transform = self.buffer_transform;
        current.opaque_region = self.opaque_region;
        current.input_region = self.input_region;
        current.damage.extend(self.damage);
        current.frame_callbacks.extend(self.frame_callbacks);
        current.client_scale = self.client_scale;
    }
}

impl GlobalDispatchDelegate<WlCompositor, RuntimeState> for CompositorGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WlCompositor>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, CompositorGlobalData);
    }
}

impl DispatchDelegate<WlCompositor, RuntimeState> for CompositorGlobalData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _resource: &WlCompositor,
        request: wl_compositor::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_compositor::Request::CreateSurface { id } => {
                let client_state = state.compositor_state.track_surface(client.id());
                let surface = data_init.init(id, SurfaceUserData::new(client.id(), client_state));
                PrivateSurfaceData::init(&surface);
            }
            wl_compositor::Request::CreateRegion { id } => {
                data_init.init(
                    id,
                    RegionData {
                        attributes: Mutex::new(RegionAttributes::default()),
                    },
                );
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WlSurface, RuntimeState> for SurfaceUserData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        surface: &WlSurface,
        request: wl_surface::Request,
        display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_surface::Request::Attach { buffer, x, y } => {
                let offset = (x != 0 || y != 0).then_some((x, y));
                let offset = if surface.version() < 5 {
                    offset.map(|(x, y)| scale_point(x, y, self.client_state.client_scale()))
                } else {
                    if offset.is_some() {
                        surface.post_error(
                            wl_surface::Error::InvalidOffset,
                            "non-zero attach offsets are invalid since wl_surface v5",
                        );
                    }
                    None
                };
                PrivateSurfaceData::with_states(surface, |states| {
                    let mut attributes = states.cached_state.get::<SurfaceAttributes>();
                    let pending = attributes.pending();
                    if offset.is_some() {
                        pending.buffer_delta = offset;
                    }
                    pending.buffer = Some(match buffer {
                        Some(buffer) => BufferAssignment::NewBuffer(buffer),
                        None => BufferAssignment::Removed,
                    });
                });
            }
            wl_surface::Request::Damage {
                x,
                y,
                width,
                height,
            } => {
                if let Some(rect) =
                    scale_rect(x, y, width, height, self.client_state.client_scale())
                {
                    PrivateSurfaceData::with_states(surface, |states| {
                        states
                            .cached_state
                            .get::<SurfaceAttributes>()
                            .pending()
                            .damage
                            .push(Damage::Surface(rect));
                    });
                }
            }
            wl_surface::Request::Frame { callback } => {
                let callback = data_init.init(callback, CallbackData);
                PrivateSurfaceData::with_states(surface, |states| {
                    states
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .pending()
                        .frame_callbacks
                        .push(callback);
                });
            }
            wl_surface::Request::SetOpaqueRegion { region } => {
                let region = region.map(|region| {
                    Arc::new(
                        region
                            .data::<RegionData>()
                            .expect("wl_region was not created by Tensor")
                            .attributes
                            .lock()
                            .unwrap()
                            .clone(),
                    )
                });
                PrivateSurfaceData::with_states(surface, |states| {
                    states
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .pending()
                        .opaque_region = region;
                });
            }
            wl_surface::Request::SetInputRegion { region } => {
                let region = region.map(|region| {
                    Arc::new(
                        region
                            .data::<RegionData>()
                            .expect("wl_region was not created by Tensor")
                            .attributes
                            .lock()
                            .unwrap()
                            .clone(),
                    )
                });
                PrivateSurfaceData::with_states(surface, |states| {
                    states
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .pending()
                        .input_region = region;
                });
            }
            wl_surface::Request::Commit => {
                PrivateSurfaceData::with_states(surface, |states| {
                    states
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .pending()
                        .client_scale = self.client_state.client_scale();
                });
                PrivateSurfaceData::invoke_pre_commit_hooks(state, display, surface);
                PrivateSurfaceData::commit(surface, display, state);
            }
            wl_surface::Request::SetBufferTransform { transform } => match transform {
                WEnum::Value(transform) => {
                    PrivateSurfaceData::with_states(surface, |states| {
                        states
                            .cached_state
                            .get::<SurfaceAttributes>()
                            .pending()
                            .buffer_transform = transform;
                    });
                }
                WEnum::Unknown(_) => surface.post_error(
                    wl_surface::Error::InvalidTransform,
                    "unknown wl_output transform",
                ),
            },
            wl_surface::Request::SetBufferScale { scale } => {
                if scale < 1 {
                    surface.post_error(wl_surface::Error::InvalidScale, "scale must be positive");
                } else {
                    PrivateSurfaceData::with_states(surface, |states| {
                        states
                            .cached_state
                            .get::<SurfaceAttributes>()
                            .pending()
                            .buffer_scale = scale;
                    });
                }
            }
            wl_surface::Request::DamageBuffer {
                x,
                y,
                width,
                height,
            } => {
                if let Some(rect) = wire_rect(x, y, width, height) {
                    PrivateSurfaceData::with_states(surface, |states| {
                        states
                            .cached_state
                            .get::<SurfaceAttributes>()
                            .pending()
                            .damage
                            .push(Damage::Buffer(rect));
                    });
                }
            }
            wl_surface::Request::Offset { x, y } => {
                let offset = scale_point(x, y, self.client_state.client_scale());
                PrivateSurfaceData::with_states(surface, |states| {
                    states
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .pending()
                        .buffer_delta = Some(offset);
                });
            }
            wl_surface::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, surface: &WlSurface) {
        state.surface_destroyed_applied(surface);
        PrivateSurfaceData::cleanup(state, self, surface);
        state.compositor_state.untrack_surface(&self.client);
    }
}

impl DispatchDelegate<WlRegion, RuntimeState> for RegionData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _region: &WlRegion,
        request: wl_region::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let scale = state.compositor_state.client_scale(client);
        let (kind, rect) = match request {
            wl_region::Request::Add {
                x,
                y,
                width,
                height,
            } => (RectangleKind::Add, scale_rect(x, y, width, height, scale)),
            wl_region::Request::Subtract {
                x,
                y,
                width,
                height,
            } => (
                RectangleKind::Subtract,
                scale_rect(x, y, width, height, scale),
            ),
            wl_region::Request::Destroy => return,
            _ => unreachable!(),
        };
        if let Some(rect) = rect {
            self.attributes.lock().unwrap().rects.push((kind, rect));
        }
    }
}

impl GlobalDispatchDelegate<WlSubcompositor, RuntimeState> for SubcompositorGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WlSubcompositor>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, SubcompositorGlobalData);
    }
}

impl DispatchDelegate<WlSubcompositor, RuntimeState> for SubcompositorGlobalData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        resource: &WlSubcompositor,
        request: wl_subcompositor::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_subcompositor::Request::GetSubsurface {
                id,
                surface,
                parent,
            } => {
                if PrivateSurfaceData::set_parent(&surface, &parent).is_err() {
                    resource.post_error(
                        wl_subcompositor::Error::BadSurface,
                        "surface already has a role, parent, or cyclic ancestry",
                    );
                    return;
                }
                data_init.init(
                    id,
                    SubsurfaceData {
                        surface: surface.clone(),
                    },
                );
                PrivateSurfaceData::with_states(&surface, |states| {
                    states
                        .data_map
                        .insert_if_missing_threadsafe(SubsurfaceState::default);
                });
            }
            wl_subcompositor::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WlSubsurface, RuntimeState> for SubsurfaceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        resource: &WlSubsurface,
        request: wl_subsurface::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_subsurface::Request::SetPosition { x, y } => {
                let point = scale_point(x, y, state.compositor_state.client_scale(client));
                PrivateSurfaceData::with_states(&self.surface, |states| {
                    states
                        .cached_state
                        .get::<SubsurfaceCachedState>()
                        .pending()
                        .location = point;
                });
            }
            wl_subsurface::Request::PlaceAbove { sibling } => {
                if PrivateSurfaceData::reorder(&self.surface, Location::After, &sibling).is_err() {
                    resource.post_error(
                        wl_subsurface::Error::BadSurface,
                        "surface is not a sibling or parent",
                    );
                }
            }
            wl_subsurface::Request::PlaceBelow { sibling } => {
                if PrivateSurfaceData::reorder(&self.surface, Location::Before, &sibling).is_err() {
                    resource.post_error(
                        wl_subsurface::Error::BadSurface,
                        "surface is not a sibling or parent",
                    );
                }
            }
            wl_subsurface::Request::SetSync => {
                PrivateSurfaceData::with_states(&self.surface, |states| {
                    states
                        .data_map
                        .get::<SubsurfaceState>()
                        .unwrap()
                        .sync
                        .store(true, std::sync::atomic::Ordering::Release);
                });
            }
            wl_subsurface::Request::SetDesync => {
                PrivateSurfaceData::with_states(&self.surface, |states| {
                    states
                        .data_map
                        .get::<SubsurfaceState>()
                        .unwrap()
                        .sync
                        .store(false, std::sync::atomic::Ordering::Release);
                });
            }
            wl_subsurface::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, _state: &mut RuntimeState, _client: ClientId, _resource: &WlSubsurface) {
        PrivateSurfaceData::unset_parent(&self.surface);
        PrivateSurfaceData::with_states(&self.surface, |states| {
            states
                .data_map
                .get::<SubsurfaceState>()
                .unwrap()
                .sync
                .store(true, std::sync::atomic::Ordering::Release);
            let mut cached = states.cached_state.get::<SubsurfaceCachedState>();
            *cached.pending() = SubsurfaceCachedState::default();
            *cached.current() = SubsurfaceCachedState::default();
        });
    }
}

impl DispatchDelegate<WlCallback, RuntimeState> for CallbackData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &WlCallback,
        _request: wl_callback::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
    }
}

fn scale_point(x: i32, y: i32, scale: f64) -> Point {
    Point::new(scale_axis(x, scale), scale_axis(y, scale))
}

fn scale_rect(x: i32, y: i32, width: i32, height: i32, scale: f64) -> Option<Rect> {
    let width = scale_extent(width, scale)?;
    let height = scale_extent(height, scale)?;
    Some(Rect::new(
        scale_axis(x, scale),
        scale_axis(y, scale),
        width,
        height,
    ))
}

fn wire_rect(x: i32, y: i32, width: i32, height: i32) -> Option<Rect> {
    Some(Rect::new(
        x,
        y,
        u32::try_from(width).ok().filter(|width| *width > 0)?,
        u32::try_from(height).ok().filter(|height| *height > 0)?,
    ))
}

fn scale_axis(value: i32, scale: f64) -> i32 {
    (f64::from(value) / scale)
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn scale_extent(value: i32, scale: f64) -> Option<u32> {
    if value <= 0 {
        return None;
    }
    let value = (f64::from(value) / scale)
        .round()
        .clamp(1.0, f64::from(u32::MAX));
    Some(value as u32)
}

delegate_global_dispatch!(RuntimeState, WlCompositor, CompositorGlobalData);
delegate_dispatch!(RuntimeState, WlCompositor, CompositorGlobalData);
delegate_dispatch!(RuntimeState, WlSurface, SurfaceUserData);
delegate_dispatch!(RuntimeState, WlRegion, RegionData);
delegate_global_dispatch!(RuntimeState, WlSubcompositor, SubcompositorGlobalData);
delegate_dispatch!(RuntimeState, WlSubcompositor, SubcompositorGlobalData);
delegate_dispatch!(RuntimeState, WlSubsurface, SubsurfaceData);
delegate_dispatch!(RuntimeState, WlCallback, CallbackData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_geometry_scaling_is_checked_and_rounded_once() {
        assert_eq!(scale_point(9, -9, 1.5), Point::new(6, -6));
        assert_eq!(scale_rect(3, 6, 9, 12, 1.5), Some(Rect::new(2, 4, 6, 8)));
        assert_eq!(scale_rect(0, 0, -1, 4, 1.0), None);
    }
}
