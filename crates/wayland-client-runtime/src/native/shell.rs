//! Usable native shell: core + stable xdg + staging scale + seat pointer/keyboard.
//!
//! No SCTK. Compio drives the display pump. Events land in an owned queue for
//! linear async consumers.

use std::collections::HashMap;
use std::fs::File;

use wayland_client::globals::{registry_queue_init, GlobalList, GlobalListContents};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_surface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1, wp_fractional_scale_v1,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use super::connection::{NativeConnection, NativeError};
use super::display_readiness_from_conn;
use super::protocols::core::shm;
use crate::display_io::DisplayReadiness;
use crate::geometry::SuggestedSize;

/// Opaque id for a native toplevel surface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeSurfaceId(u32);

/// Events emitted by the native shell (grows toward the public crate Event model).
#[derive(Clone, Debug)]
pub enum NativeShellEvent {
    ToplevelConfigure {
        surface: NativeSurfaceId,
        suggested_size: SuggestedSize,
    },
    ToplevelClose {
        surface: NativeSurfaceId,
    },
    /// Preferred scale from `wp_fractional_scale_v1` (decoded: protocol / 120).
    ScaleFactorChanged {
        surface: NativeSurfaceId,
        factor: f64,
    },
    PointerEnter {
        surface: NativeSurfaceId,
        x: f64,
        y: f64,
    },
    PointerLeave {
        surface: NativeSurfaceId,
    },
    PointerMotion {
        surface: NativeSurfaceId,
        x: f64,
        y: f64,
    },
    PointerButton {
        surface: Option<NativeSurfaceId>,
        button: u32,
        pressed: bool,
    },
    PointerAxis {
        surface: Option<NativeSurfaceId>,
        horizontal: f64,
        vertical: f64,
    },
    SeatKeyboardKey {
        key: u32,
        pressed: bool,
    },
}

struct ToplevelRecord {
    wl: wl_surface::WlSurface,
    #[allow(dead_code)]
    xdg: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    buffer: Option<wl_buffer::WlBuffer>,
    _pool: Option<wl_shm_pool::WlShmPool>,
    _file: Option<File>,
    viewport: Option<wp_viewport::WpViewport>,
    fractional: Option<wp_fractional_scale_v1::WpFractionalScaleV1>,
    configured: bool,
    pending_size: Option<(i32, i32)>,
    /// Logical destination size for viewporter (surface-local).
    logical_w: u32,
    logical_h: u32,
    scale_factor: f64,
}

/// Dispatch state for the native shell event queue.
pub struct NativeShellState {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    seat: Option<wl_seat::WlSeat>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    viewporter: Option<wp_viewporter::WpViewporter>,
    fractional_manager: Option<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1>,
    toplevels: HashMap<NativeSurfaceId, ToplevelRecord>,
    toplevel_objects: HashMap<u32, NativeSurfaceId>,
    xdg_surface_objects: HashMap<u32, NativeSurfaceId>,
    wl_surface_objects: HashMap<u32, NativeSurfaceId>,
    fractional_objects: HashMap<u32, NativeSurfaceId>,
    pointer_focus: Option<NativeSurfaceId>,
    /// Accumulated axis values until frame (or immediate emit if no frame).
    axis_h: f64,
    axis_v: f64,
    next_id: u32,
    events: Vec<NativeShellEvent>,
    seat_capabilities: wl_seat::Capability,
}

impl Default for NativeShellState {
    fn default() -> Self {
        Self {
            compositor: None,
            shm: None,
            wm_base: None,
            seat: None,
            keyboard: None,
            pointer: None,
            viewporter: None,
            fractional_manager: None,
            toplevels: HashMap::new(),
            toplevel_objects: HashMap::new(),
            xdg_surface_objects: HashMap::new(),
            wl_surface_objects: HashMap::new(),
            fractional_objects: HashMap::new(),
            pointer_focus: None,
            axis_h: 0.0,
            axis_v: 0.0,
            next_id: 1,
            events: Vec::new(),
            seat_capabilities: wl_seat::Capability::empty(),
        }
    }
}

impl NativeShellState {
    fn alloc_id(&mut self) -> NativeSurfaceId {
        let id = NativeSurfaceId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn push(&mut self, event: NativeShellEvent) {
        self.events.push(event);
    }
}

/// SCTK-free shell runtime.
pub struct NativeShell {
    connection: NativeConnection,
    readiness: DisplayReadiness,
    #[allow(dead_code)]
    globals: GlobalList,
    queue: EventQueue<NativeShellState>,
    state: NativeShellState,
}

impl NativeShell {
    /// Connect and bind core + xdg + optional seat/scale globals.
    pub fn connect_to_env() -> Result<Self, NativeError> {
        let connection = NativeConnection::connect_to_env()?;
        let readiness = display_readiness_from_conn(connection.connection())?;
        let (globals, queue) = registry_queue_init::<NativeShellState>(connection.connection())
            .map_err(|error| NativeError::Registry(error.to_string()))?;
        let qh = queue.handle();
        let mut state = NativeShellState::default();

        if let Ok(compositor) = globals.bind::<wl_compositor::WlCompositor, _, _>(&qh, 1..=6, ()) {
            state.compositor = Some(compositor);
        }
        if let Ok(shm) = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ()) {
            state.shm = Some(shm);
        }
        if let Ok(wm_base) = globals.bind::<xdg_wm_base::XdgWmBase, _, _>(&qh, 1..=6, ()) {
            state.wm_base = Some(wm_base);
        }
        if let Ok(seat) = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ()) {
            state.seat = Some(seat);
        }
        if let Ok(viewporter) = globals.bind::<wp_viewporter::WpViewporter, _, _>(&qh, 1..=1, ()) {
            state.viewporter = Some(viewporter);
        }
        if let Ok(frac) = globals
            .bind::<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1, _, _>(
                &qh,
                1..=1,
                (),
            )
        {
            state.fractional_manager = Some(frac);
        }

        if state.compositor.is_none() {
            return Err(NativeError::Registry("wl_compositor missing".into()));
        }
        if state.shm.is_none() {
            return Err(NativeError::Registry("wl_shm missing".into()));
        }
        if state.wm_base.is_none() {
            return Err(NativeError::Registry("xdg_wm_base missing".into()));
        }

        let mut shell = Self {
            connection,
            readiness,
            globals,
            queue,
            state,
        };
        let _ = shell.dispatch_pending()?;
        Ok(shell)
    }

    pub fn connection(&self) -> &NativeConnection {
        &self.connection
    }

    pub fn has_fractional_scale(&self) -> bool {
        self.state.fractional_manager.is_some() && self.state.viewporter.is_some()
    }

    pub fn create_toplevel(
        &mut self,
        title: impl Into<String>,
        app_id: impl Into<String>,
    ) -> Result<NativeSurfaceId, NativeError> {
        self.create_toplevel_sized(title, app_id, 640, 480, [0xff, 0x22, 0x66, 0xcc])
    }

    pub fn create_toplevel_sized(
        &mut self,
        title: impl Into<String>,
        app_id: impl Into<String>,
        width: u32,
        height: u32,
        argb: [u8; 4],
    ) -> Result<NativeSurfaceId, NativeError> {
        let qh = self.queue.handle();
        let compositor = self
            .state
            .compositor
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?;
        let shm = self
            .state
            .shm
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_shm".into()))?;
        let wm_base = self
            .state
            .wm_base
            .as_ref()
            .ok_or_else(|| NativeError::Registry("xdg_wm_base".into()))?;

        let wl = compositor.create_surface(&qh, ());
        // Fractional-scale clients keep buffer_scale at 1.
        wl.set_buffer_scale(1);

        let viewport = self
            .state
            .viewporter
            .as_ref()
            .map(|vp| vp.get_viewport(&wl, &qh, ()));
        if let Some(vp) = viewport.as_ref() {
            vp.set_destination(width as i32, height as i32);
        }

        let fractional = self
            .state
            .fractional_manager
            .as_ref()
            .map(|mgr| mgr.get_fractional_scale(&wl, &qh, ()));

        let (file, pool, buffer) = shm::create_solid_buffer(shm, &qh, width, height, argb)
            .map_err(|e| NativeError::Io(e.to_string()))?;
        let xdg = wm_base.get_xdg_surface(&wl, &qh, ());
        let toplevel = xdg.get_toplevel(&qh, ());
        toplevel.set_title(title.into());
        toplevel.set_app_id(app_id.into());
        wl.commit();

        let id = self.state.alloc_id();
        self.state
            .toplevel_objects
            .insert(toplevel.id().protocol_id(), id);
        self.state
            .xdg_surface_objects
            .insert(xdg.id().protocol_id(), id);
        self.state
            .wl_surface_objects
            .insert(wl.id().protocol_id(), id);
        if let Some(ref frac) = fractional {
            self.state
                .fractional_objects
                .insert(frac.id().protocol_id(), id);
        }
        self.state.toplevels.insert(
            id,
            ToplevelRecord {
                wl,
                xdg,
                toplevel,
                buffer: Some(buffer),
                _pool: Some(pool),
                _file: Some(file),
                viewport,
                fractional,
                configured: false,
                pending_size: Some((width as i32, height as i32)),
                logical_w: width,
                logical_h: height,
                scale_factor: 1.0,
            },
        );
        self.connection.flush()?;
        Ok(id)
    }

    pub fn dispatch_pending(&mut self) -> Result<usize, NativeError> {
        Ok(self.queue.dispatch_pending(&mut self.state)?)
    }

    pub async fn pump_once(&mut self) -> Result<usize, NativeError> {
        self.connection.flush()?;
        let mut n = self.dispatch_pending()?;

        match self.connection.connection().prepare_read() {
            None => {
                n += self.dispatch_pending()?;
            }
            Some(guard) => {
                self.readiness.wait_readable().await?;
                match guard.read() {
                    Ok(_) => {}
                    Err(wayland_client::backend::WaylandError::Io(err))
                        if err.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error.into()),
                }
                n += self.dispatch_pending()?;
            }
        }
        Ok(n)
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = NativeShellEvent> + '_ {
        self.state.events.drain(..)
    }

    pub fn drain_events_into(&mut self, target: &mut Vec<NativeShellEvent>) {
        target.append(&mut self.state.events);
    }

    pub fn toplevel_count(&self) -> usize {
        self.state.toplevels.len()
    }

    pub fn is_configured(&self, id: NativeSurfaceId) -> bool {
        self.state.toplevels.get(&id).is_some_and(|t| t.configured)
    }

    pub fn scale_factor(&self, id: NativeSurfaceId) -> Option<f64> {
        self.state.toplevels.get(&id).map(|t| t.scale_factor)
    }

    pub fn set_title(
        &mut self,
        id: NativeSurfaceId,
        title: impl Into<String>,
    ) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or(NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.toplevel.set_title(title.into());
        self.connection.flush()?;
        Ok(())
    }

    pub fn set_app_id(
        &mut self,
        id: NativeSurfaceId,
        app_id: impl Into<String>,
    ) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or(NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.toplevel.set_app_id(app_id.into());
        self.connection.flush()?;
        Ok(())
    }

    pub fn destroy_toplevel(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let Some(record) = self.state.toplevels.remove(&id) else {
            return Err(NativeError::Protocol(format!("unknown surface {id:?}")));
        };
        self.state
            .toplevel_objects
            .remove(&record.toplevel.id().protocol_id());
        self.state
            .xdg_surface_objects
            .remove(&record.xdg.id().protocol_id());
        self.state
            .wl_surface_objects
            .remove(&record.wl.id().protocol_id());
        if let Some(ref frac) = record.fractional {
            self.state
                .fractional_objects
                .remove(&frac.id().protocol_id());
            frac.destroy();
        }
        if let Some(vp) = record.viewport {
            vp.destroy();
        }
        record.toplevel.destroy();
        record.xdg.destroy();
        if let Some(buffer) = record.buffer {
            buffer.destroy();
        }
        if let Some(pool) = record._pool {
            pool.destroy();
        }
        record.wl.destroy();
        self.connection.flush()?;
        Ok(())
    }
}

// —— Dispatch ——

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for NativeShellState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if interface == "wl_seat" && state.seat.is_none() {
                let v = version.min(9).max(1);
                state.seat = Some(registry.bind(name, v, qh, ()));
            }
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
            let id = state
                .xdg_surface_objects
                .get(&xdg_surface.id().protocol_id())
                .copied();
            if let Some(id) = id {
                if let Some(record) = state.toplevels.get_mut(&id) {
                    record.configured = true;
                    if let Some((w, h)) = record.pending_size {
                        if w > 0 && h > 0 {
                            record.logical_w = w as u32;
                            record.logical_h = h as u32;
                            if let Some(vp) = record.viewport.as_ref() {
                                vp.set_destination(w, h);
                            }
                        }
                    }
                    let suggested = SuggestedSize::new(
                        Some(record.logical_w).filter(|&w| w > 0),
                        Some(record.logical_h).filter(|&h| h > 0),
                    );
                    if let Some(buffer) = record.buffer.as_ref() {
                        record.wl.attach(Some(buffer), 0, 0);
                        record.wl.damage_buffer(0, 0, i32::MAX, i32::MAX);
                        record.wl.commit();
                    }
                    state.push(NativeShellEvent::ToplevelConfigure {
                        surface: id,
                        suggested_size: suggested,
                    });
                }
            }
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        toplevel: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = state
            .toplevel_objects
            .get(&toplevel.id().protocol_id())
            .copied();
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if let Some(id) = id {
                    if let Some(record) = state.toplevels.get_mut(&id) {
                        if width > 0 && height > 0 {
                            record.pending_size = Some((width, height));
                        }
                    }
                }
            }
            xdg_toplevel::Event::Close => {
                if let Some(id) = id {
                    state.push(NativeShellEvent::ToplevelClose { surface: id });
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        {
            state.seat_capabilities = capabilities;
            if capabilities.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
            if capabilities.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key {
            key,
            state: key_state,
            ..
        } = event
        {
            let pressed = matches!(
                key_state,
                WEnum::Value(wl_keyboard::KeyState::Pressed)
            );
            state.push(NativeShellEvent::SeatKeyboardKey { key, pressed });
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface, surface_x, surface_y, ..
            } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                if let Some(id) = id {
                    state.pointer_focus = Some(id);
                    state.push(NativeShellEvent::PointerEnter {
                        surface: id,
                        x: surface_x,
                        y: surface_y,
                    });
                }
            }
            wl_pointer::Event::Leave { surface, .. } => {
                let id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied()
                    .or(state.pointer_focus);
                state.pointer_focus = None;
                if let Some(id) = id {
                    state.push(NativeShellEvent::PointerLeave { surface: id });
                }
            }
            wl_pointer::Event::Motion {
                surface_x, surface_y, ..
            } => {
                if let Some(id) = state.pointer_focus {
                    state.push(NativeShellEvent::PointerMotion {
                        surface: id,
                        x: surface_x,
                        y: surface_y,
                    });
                }
            }
            wl_pointer::Event::Button {
                button,
                state: btn_state,
                ..
            } => {
                let pressed = matches!(btn_state, WEnum::Value(wl_pointer::ButtonState::Pressed));
                state.push(NativeShellEvent::PointerButton {
                    surface: state.pointer_focus,
                    button,
                    pressed,
                });
            }
            wl_pointer::Event::Axis { axis, value, .. } => match axis {
                WEnum::Value(wl_pointer::Axis::VerticalScroll) => state.axis_v += value,
                WEnum::Value(wl_pointer::Axis::HorizontalScroll) => state.axis_h += value,
                _ => {}
            },
            wl_pointer::Event::Frame => {
                if state.axis_h != 0.0 || state.axis_v != 0.0 {
                    state.push(NativeShellEvent::PointerAxis {
                        surface: state.pointer_focus,
                        horizontal: state.axis_h,
                        vertical: state.axis_v,
                    });
                    state.axis_h = 0.0;
                    state.axis_v = 0.0;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wp_viewporter::WpViewporter, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wp_viewporter::WpViewporter,
        _: wp_viewporter::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_viewport::WpViewport, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wp_viewport::WpViewport,
        _: wp_viewport::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        _: wp_fractional_scale_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        fractional: &wp_fractional_scale_v1::WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            let id = state
                .fractional_objects
                .get(&fractional.id().protocol_id())
                .copied();
            if let Some(id) = id {
                let factor = f64::from(scale) / 120.0;
                if let Some(record) = state.toplevels.get_mut(&id) {
                    record.scale_factor = factor;
                }
                state.push(NativeShellEvent::ScaleFactorChanged {
                    surface: id,
                    factor,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_shell_creates_toplevel_when_compositor_present() {
        let Ok(mut shell) = NativeShell::connect_to_env() else {
            return;
        };
        let id = shell
            .create_toplevel("fika-native-smoke", "dev.fika.NativeSmoke")
            .expect("create toplevel");
        assert_eq!(shell.toplevel_count(), 1);

        compio::runtime::Runtime::new()
            .expect("compio")
            .block_on(async {
                for _ in 0..32 {
                    let _ = shell.pump_once().await;
                    if shell.is_configured(id) {
                        break;
                    }
                }
            });

        let mut events = Vec::new();
        shell.drain_events_into(&mut events);
        let _ = shell.destroy_toplevel(id);
    }
}
