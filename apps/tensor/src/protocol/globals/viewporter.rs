//! Tensor-owned `wp_viewporter` wire adapter and double-buffered values.
//!
//! Protocol validation follows Smithay's implementation. See
//! `LICENSES/Smithay-MIT.txt`.

use std::sync::Mutex;

use super::compositor::{self, Cacheable, SurfaceData, with_states};
use tensor_protocol::SurfaceSourceRect;
use wayland_protocols::wp::viewporter::server::{
    wp_viewport::{self, WpViewport},
    wp_viewporter::{self, WpViewporter},
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct ViewporterProtocol {
    _global: GlobalId,
}

impl ViewporterProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        let global =
            display.create_global::<RuntimeState, WpViewporter, _>(1, ViewporterGlobalData);
        Self { _global: global }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct ViewporterGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct ViewporterData;

#[derive(Debug)]
pub(in crate::protocol) struct ViewportData {
    surface: Weak<WlSurface>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ViewportCachedState {
    source: Option<SurfaceSourceRect>,
    destination: Option<(i32, i32)>,
}

impl Cacheable for ViewportCachedState {
    fn commit(&mut self, _display: &DisplayHandle) -> Self {
        *self
    }

    fn merge_into(self, current: &mut Self, _display: &DisplayHandle) {
        *current = self;
    }
}

struct ViewportMarker {
    id: ObjectId,
    resource: Weak<WpViewport>,
}

impl ViewportMarker {
    fn new(viewport: &WpViewport) -> Self {
        Self {
            id: viewport.id(),
            resource: viewport.downgrade(),
        }
    }

    fn matches(&self, viewport: &WpViewport) -> bool {
        self.id == viewport.id()
    }
}

type ViewportSurfaceState = Mutex<Option<ViewportMarker>>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::protocol) struct CommittedViewport {
    pub(in crate::protocol) source: Option<SurfaceSourceRect>,
    destination: Option<(i32, i32)>,
}

impl CommittedViewport {
    pub(in crate::protocol) fn size(self) -> Option<(i32, i32)> {
        self.destination
            .or_else(|| self.source.map(SurfaceSourceRect::integer_size))
    }
}

pub(in crate::protocol) fn committed_viewport(
    states: &SurfaceData,
    buffer_size: (i32, i32),
) -> CommittedViewport {
    let viewport = {
        let mut cached = states.cached_state.get::<ViewportCachedState>();
        *cached.current()
    };

    if let Some(source) = viewport.source
        && !source.fits_within(buffer_size.0, buffer_size.1)
        && let Some(marker) = states.data_map.get::<ViewportSurfaceState>()
        && let Some(marker) = marker.lock().unwrap().as_ref()
        && let Ok(resource) = marker.resource.upgrade()
    {
        let [x, y, width, height] = source.as_f64();
        resource.post_error(
            wp_viewport::Error::OutOfBuffer,
            format!(
                "source rectangle x={},y={},w={},h={} extends outside buffer {}x{}",
                x, y, width, height, buffer_size.0, buffer_size.1
            ),
        );
    }

    CommittedViewport {
        source: viewport.source,
        destination: viewport.destination,
    }
}

pub(in crate::protocol) fn pending_viewport_size(states: &SurfaceData) -> Option<(i32, i32)> {
    let viewport = {
        let mut cached = states.cached_state.get::<ViewportCachedState>();
        *cached.pending()
    };
    viewport
        .destination
        .or_else(|| viewport.source.map(SurfaceSourceRect::integer_size))
}

pub(in crate::protocol) trait ViewporterHandler: 'static {
    fn viewport_client_scale(&self, client: &Client) -> f64;
}

impl ViewporterHandler for RuntimeState {
    fn viewport_client_scale(&self, client: &Client) -> f64 {
        self.client_scale(client)
    }
}

impl<D> GlobalDispatchDelegate<WpViewporter, D> for ViewporterGlobalData
where
    D: Dispatch<WpViewporter, ViewporterData>,
    D: 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpViewporter>,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, ViewporterData);
    }
}

impl<D> DispatchDelegate<WpViewporter, D> for ViewporterData
where
    D: Dispatch<WpViewport, ViewportData>,
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        viewporter: &WpViewporter,
        request: wp_viewporter::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wp_viewporter::Request::Destroy => {}
            wp_viewporter::Request::GetViewport { id, surface } => {
                let already_exists = with_states(&surface, |states| {
                    states
                        .data_map
                        .get::<ViewportSurfaceState>()
                        .is_some_and(|marker| marker.lock().unwrap().is_some())
                });
                if already_exists {
                    viewporter.post_error(
                        wp_viewporter::Error::ViewportExists,
                        "the surface already has a viewport object associated",
                    );
                    return;
                }

                let viewport = data_init.init(
                    id,
                    ViewportData {
                        surface: surface.downgrade(),
                    },
                );
                let mut marker = Some(ViewportMarker::new(&viewport));
                let first_viewport = with_states(&surface, |states| {
                    let inserted = states
                        .data_map
                        .insert_if_missing_threadsafe::<ViewportSurfaceState, _>(|| {
                            Mutex::new(marker.take())
                        });
                    if !inserted {
                        *states
                            .data_map
                            .get::<ViewportSurfaceState>()
                            .expect("viewport marker exists after failed insertion")
                            .lock()
                            .unwrap() = marker.take();
                    }
                    inserted
                });
                if first_viewport {
                    compositor::add_pre_commit_hook::<D, _>(&surface, viewport_pre_commit);
                }
            }
            _ => unreachable!(),
        }
    }
}

impl<D> DispatchDelegate<WpViewport, D> for ViewportData
where
    D: ViewporterHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        client: &Client,
        viewport: &WpViewport,
        request: wp_viewport::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wp_viewport::Request::Destroy => self.clear(viewport),
            wp_viewport::Request::SetSource {
                x,
                y,
                width,
                height,
            } => {
                let unset = x == -1.0 && y == -1.0 && width == -1.0 && height == -1.0;
                if !unset && !(x >= 0.0 && y >= 0.0 && width > 0.0 && height > 0.0) {
                    viewport.post_error(
                        wp_viewport::Error::BadValue,
                        "source position must be non-negative and source size must be positive",
                    );
                    return;
                }
                let Some(surface) = self.surface(viewport) else {
                    return;
                };
                with_states(&surface, |states| {
                    states
                        .cached_state
                        .get::<ViewportCachedState>()
                        .pending()
                        .source = (!unset).then_some(SurfaceSourceRect::from_raw_fixed(
                        wire_fixed(x),
                        wire_fixed(y),
                        wire_fixed(width),
                        wire_fixed(height),
                    ));
                });
            }
            wp_viewport::Request::SetDestination { width, height } => {
                let unset = width == -1 && height == -1;
                if !unset && !(width > 0 && height > 0) {
                    viewport.post_error(
                        wp_viewport::Error::BadValue,
                        "destination size must be positive or exactly -1x-1",
                    );
                    return;
                }
                let Some(surface) = self.surface(viewport) else {
                    return;
                };
                let client_scale = state.viewport_client_scale(client);
                with_states(&surface, |states| {
                    states
                        .cached_state
                        .get::<ViewportCachedState>()
                        .pending()
                        .destination = (!unset).then_some((
                        scale_destination(width, client_scale),
                        scale_destination(height, client_scale),
                    ));
                });
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, _state: &mut D, _client: ClientId, viewport: &WpViewport) {
        self.clear(viewport);
    }
}

impl ViewportData {
    fn surface(&self, viewport: &WpViewport) -> Option<WlSurface> {
        match self.surface.upgrade() {
            Ok(surface) => Some(surface),
            Err(_) => {
                viewport.post_error(
                    wp_viewport::Error::NoSurface,
                    "the associated wl_surface was destroyed",
                );
                None
            }
        }
    }

    fn clear(&self, viewport: &WpViewport) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        with_states(&surface, |states| {
            let removed = states
                .data_map
                .get::<ViewportSurfaceState>()
                .is_some_and(|marker| {
                    let mut marker = marker.lock().unwrap();
                    if marker
                        .as_ref()
                        .is_some_and(|marker| marker.matches(viewport))
                    {
                        marker.take();
                        true
                    } else {
                        false
                    }
                });
            if removed {
                *states.cached_state.get::<ViewportCachedState>().pending() =
                    ViewportCachedState::default();
            }
        });
    }
}

fn viewport_pre_commit<D: 'static>(_state: &mut D, _display: &DisplayHandle, surface: &WlSurface) {
    with_states(surface, |states| {
        let invalid_size = {
            let mut cached = states.cached_state.get::<ViewportCachedState>();
            let viewport = *cached.pending();
            viewport.destination.is_none()
                && viewport
                    .source
                    .is_some_and(|source| !source.has_integer_size())
        };
        if invalid_size
            && let Some(marker) = states.data_map.get::<ViewportSurfaceState>()
            && let Some(marker) = marker.lock().unwrap().as_ref()
            && let Ok(resource) = marker.resource.upgrade()
        {
            resource.post_error(
                wp_viewport::Error::BadSize,
                "source size must be integer when destination size is unset",
            );
        }
    });
}

fn scale_destination(value: i32, client_scale: f64) -> i32 {
    (f64::from(value) / client_scale).round() as i32
}

fn wire_fixed(value: f64) -> i32 {
    (value * SurfaceSourceRect::FIXED_SCALE as f64).round() as i32
}

delegate_global_dispatch!(RuntimeState, WpViewporter, ViewporterGlobalData);
delegate_dispatch!(RuntimeState, WpViewporter, ViewporterData);
delegate_dispatch!(RuntimeState, WpViewport, ViewportData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_validation_checks_fractional_size_and_full_bounds() {
        let source = SurfaceSourceRect::from_raw_fixed(
            wire_fixed(0.5),
            wire_fixed(1.0),
            wire_fixed(9.0),
            wire_fixed(7.5),
        );
        assert!(!source.has_integer_size());
        assert!(source.fits_within(10, 9));
        assert!(!source.fits_within(9, 9));
        assert_eq!(source.raw_fixed(), [128, 256, 2304, 1920]);
    }

    #[test]
    fn destination_scaling_matches_compositor_client_rounding() {
        assert_eq!(scale_destination(9, 1.5), 6);
        assert_eq!(scale_destination(10, 4.0), 3);
    }
}
