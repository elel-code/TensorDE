//! Native Wayland layer-shell host for future non-GTK renderers.
//!
//! This module owns only the Wayland surface lifecycle. Content renderers
//! such as video, web, shader, or scene runtimes should be layered on top of
//! the surface host here; raw Wayland handle export is the next integration
//! step before GPU or GStreamer overlay content is attached.

use serde::Serialize;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use std::fmt;
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_seat, wl_surface},
};
use wayland_protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeWaylandHostOptions {
    pub namespace: String,
}

impl Default for NativeWaylandHostOptions {
    fn default() -> Self {
        Self {
            namespace: "gilder-wallpaper-native".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeWaylandCapabilities {
    pub built: bool,
    pub experimental: bool,
    pub owns_wlr_layer_shell_surface: bool,
    pub exports_raw_wayland_handles: bool,
    pub raw_wayland_handles_planned: bool,
    pub supports_fractional_scale_protocol: bool,
    pub supports_viewporter_protocol: bool,
    pub consumes_render_sync: bool,
    pub unsafe_policy: &'static str,
}

pub fn capabilities() -> NativeWaylandCapabilities {
    NativeWaylandCapabilities {
        built: true,
        experimental: true,
        owns_wlr_layer_shell_surface: true,
        exports_raw_wayland_handles: false,
        raw_wayland_handles_planned: true,
        supports_fractional_scale_protocol: true,
        supports_viewporter_protocol: true,
        consumes_render_sync: false,
        unsafe_policy: "unsafe is allowed but must stay behind audited native Wayland/GPU boundaries",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeWaylandSurfaceSnapshot {
    pub logical_size: Option<(u32, u32)>,
    pub scale_num: u32,
    pub scale_den: u32,
    pub configured: bool,
    pub surface_protocol_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeWaylandError {
    Wayland(String),
}

impl fmt::Display for NativeWaylandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(err) => write!(f, "wayland error: {err}"),
        }
    }
}

impl std::error::Error for NativeWaylandError {}

pub struct NativeWaylandHost {
    event_queue: EventQueue<NativeWaylandState>,
    state: NativeWaylandState,
}

impl NativeWaylandHost {
    pub fn connect(options: NativeWaylandHostOptions) -> Result<Self, NativeWaylandError> {
        let connection = Connection::connect_to_env()
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let (globals, event_queue) = registry_queue_init(&connection)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let qh = event_queue.handle();

        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let surface = compositor.create_surface(&qh);

        let fractional_manager: Option<WpFractionalScaleManagerV1> =
            globals.bind(&qh, 1..=1, NativeWaylandProtocolData).ok();
        let fractional_scale = fractional_manager
            .as_ref()
            .map(|manager| manager.get_fractional_scale(&surface, &qh, NativeWaylandProtocolData));
        let viewporter: Option<WpViewporter> =
            globals.bind(&qh, 1..=1, NativeWaylandProtocolData).ok();
        let viewport = viewporter
            .as_ref()
            .map(|viewporter| viewporter.get_viewport(&surface, &qh, NativeWaylandProtocolData));

        let layer = layer_shell.create_layer_surface(
            &qh,
            surface,
            Layer::Background,
            Some(options.namespace),
            None,
        );
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(-1);
        layer.set_anchor(Anchor::all());
        layer.set_size(0, 0);
        layer.commit();

        let state = NativeWaylandState {
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            layer,
            scale: NativeScaleState::new(
                fractional_manager,
                fractional_scale,
                viewporter,
                viewport,
            ),
            logical_size: None,
            configured: false,
        };

        Ok(Self { event_queue, state })
    }

    pub fn dispatch_pending(&mut self) -> Result<(), NativeWaylandError> {
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map(|_| ())
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))
    }

    pub fn blocking_dispatch(&mut self) -> Result<(), NativeWaylandError> {
        self.event_queue
            .blocking_dispatch(&mut self.state)
            .map(|_| ())
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))
    }

    pub fn snapshot(&self) -> NativeWaylandSurfaceSnapshot {
        self.state.snapshot()
    }
}

struct NativeWaylandState {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    layer: LayerSurface,
    scale: NativeScaleState,
    logical_size: Option<(u32, u32)>,
    configured: bool,
}

impl NativeWaylandState {
    fn reconfigure(&mut self) {
        let Some((width, height)) = self.logical_size else {
            return;
        };

        if !self.scale.received {
            let outputs: Vec<wl_output::WlOutput> = self.output_state.outputs().collect();
            for output in &outputs {
                if self
                    .scale
                    .compute_from_output(&self.output_state, output, self.logical_size)
                {
                    break;
                }
            }
        }

        if let Some(viewport) = &self.scale.viewport {
            viewport.set_destination(width as i32, height as i32);
        }

        self.layer.set_size(width, height);
        let _ = self.layer.set_buffer_scale(1);
        self.layer.commit();
        self.configured = true;
    }

    fn snapshot(&self) -> NativeWaylandSurfaceSnapshot {
        NativeWaylandSurfaceSnapshot {
            logical_size: self.logical_size,
            scale_num: self.scale.num,
            scale_den: NativeScaleState::DENOMINATOR,
            configured: self.configured,
            surface_protocol_id: self.layer.wl_surface().id().protocol_id(),
        }
    }
}

impl CompositorHandler for NativeWaylandState {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for NativeWaylandState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.reconfigure();
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.reconfigure();
    }

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl SeatHandler for NativeWaylandState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl LayerShellHandler for NativeWaylandState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {}

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let (width, height) = configure.new_size;
        if width == 0 || height == 0 {
            return;
        }
        self.logical_size = Some((width, height));
        self.reconfigure();
    }
}

delegate_compositor!(NativeWaylandState);
delegate_output!(NativeWaylandState);
delegate_seat!(NativeWaylandState);
delegate_layer!(NativeWaylandState);
delegate_registry!(NativeWaylandState);

impl ProvidesRegistryState for NativeWaylandState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

#[derive(Debug)]
struct NativeWaylandProtocolData;

impl Dispatch<WpFractionalScaleManagerV1, NativeWaylandProtocolData, NativeWaylandState>
    for NativeWaylandState
{
    fn event(
        _: &mut NativeWaylandState,
        _: &WpFractionalScaleManagerV1,
        _: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _: &NativeWaylandProtocolData,
        _: &Connection,
        _: &QueueHandle<NativeWaylandState>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleV1, NativeWaylandProtocolData, NativeWaylandState>
    for NativeWaylandState
{
    fn event(
        state: &mut NativeWaylandState,
        _: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as Proxy>::Event,
        _: &NativeWaylandProtocolData,
        _: &Connection,
        _: &QueueHandle<NativeWaylandState>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.scale.handle_preferred_scale(scale);
            state.reconfigure();
        }
    }
}

impl Dispatch<WpViewporter, NativeWaylandProtocolData, NativeWaylandState> for NativeWaylandState {
    fn event(
        _: &mut NativeWaylandState,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &NativeWaylandProtocolData,
        _: &Connection,
        _: &QueueHandle<NativeWaylandState>,
    ) {
    }
}

impl Dispatch<WpViewport, NativeWaylandProtocolData, NativeWaylandState> for NativeWaylandState {
    fn event(
        _: &mut NativeWaylandState,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &NativeWaylandProtocolData,
        _: &Connection,
        _: &QueueHandle<NativeWaylandState>,
    ) {
    }
}

struct NativeScaleState {
    #[allow(dead_code)]
    fractional_manager: Option<WpFractionalScaleManagerV1>,
    #[allow(dead_code)]
    fractional_scale: Option<WpFractionalScaleV1>,
    #[allow(dead_code)]
    viewporter: Option<WpViewporter>,
    viewport: Option<WpViewport>,
    num: u32,
    received: bool,
}

impl NativeScaleState {
    const DENOMINATOR: u32 = 120;

    fn new(
        fractional_manager: Option<WpFractionalScaleManagerV1>,
        fractional_scale: Option<WpFractionalScaleV1>,
        viewporter: Option<WpViewporter>,
        viewport: Option<WpViewport>,
    ) -> Self {
        Self {
            fractional_manager,
            fractional_scale,
            viewporter,
            viewport,
            num: Self::DENOMINATOR,
            received: false,
        }
    }

    fn handle_preferred_scale(&mut self, scale: u32) {
        self.num = scale;
        self.received = true;
    }

    fn compute_from_output(
        &mut self,
        output_state: &OutputState,
        output: &wl_output::WlOutput,
        fallback_logical: Option<(u32, u32)>,
    ) -> bool {
        if self.received {
            return false;
        }
        let Some(info) = output_state.info(output) else {
            return false;
        };
        let Some(mode) = info.modes.iter().find(|mode| mode.current) else {
            return false;
        };
        let (logical_width, logical_height) = match info.logical_size {
            Some((width, height)) if width > 0 && height > 0 => (width, height),
            _ => match fallback_logical {
                Some((width, height)) => (width as i32, height as i32),
                None => return false,
            },
        };
        if logical_width <= 0 || logical_height <= 0 {
            return false;
        }

        let width_scale = mode.dimensions.0 as f64 / logical_width as f64;
        let height_scale = mode.dimensions.1 as f64 / logical_height as f64;
        let computed = ((width_scale + height_scale) / 2.0 * Self::DENOMINATOR as f64).round();
        let computed = computed.max(Self::DENOMINATOR as f64) as u32;
        self.num = computed;
        self.received = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_experimental_native_capabilities() {
        let capabilities = capabilities();
        assert!(capabilities.built);
        assert!(capabilities.experimental);
        assert!(capabilities.owns_wlr_layer_shell_surface);
        assert!(!capabilities.exports_raw_wayland_handles);
        assert!(capabilities.raw_wayland_handles_planned);
        assert!(!capabilities.consumes_render_sync);
    }
}
