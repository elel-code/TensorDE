//! Minimal usable native shell: compositor + shm + xdg toplevel (+ optional seat).
//!
//! No SCTK. Compio drives the display pump. Events are pushed into an owned
//! queue for linear async consumers.

use std::collections::HashMap;
use std::fs::File;

use wayland_client::globals::{registry_queue_init, GlobalList, GlobalListContents};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use super::connection::{NativeConnection, NativeError};
use super::display_readiness_from_conn;
use super::protocols::core::shm;
use crate::display_io::DisplayReadiness;
use crate::geometry::SuggestedSize;

/// Opaque id for a native toplevel surface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeSurfaceId(u32);

/// Events emitted by the native shell (subset of the public crate Event model).
#[derive(Clone, Debug)]
pub enum NativeShellEvent {
    ToplevelConfigure {
        surface: NativeSurfaceId,
        suggested_size: SuggestedSize,
    },
    ToplevelClose {
        surface: NativeSurfaceId,
    },
    SeatKeyboardKey {
        key: u32,
        pressed: bool,
    },
}

struct ToplevelRecord {
    wl: wl_surface::WlSurface,
    /// Kept alive for the surface role lifetime.
    #[allow(dead_code)]
    xdg: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    buffer: Option<wl_buffer::WlBuffer>,
    /// Keep pool/file alive while buffer is attached.
    _pool: Option<wl_shm_pool::WlShmPool>,
    _file: Option<File>,
    configured: bool,
    pending_size: Option<(i32, i32)>,
}

/// Dispatch state for the native shell event queue.
pub struct NativeShellState {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    seat: Option<wl_seat::WlSeat>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    toplevels: HashMap<NativeSurfaceId, ToplevelRecord>,
    /// Map xdg_toplevel object id → surface id for event routing.
    toplevel_objects: HashMap<u32, NativeSurfaceId>,
    xdg_surface_objects: HashMap<u32, NativeSurfaceId>,
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
            toplevels: HashMap::new(),
            toplevel_objects: HashMap::new(),
            xdg_surface_objects: HashMap::new(),
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

/// SCTK-free shell runtime: connect, create toplevels, Compio pump, drain events.
pub struct NativeShell {
    connection: NativeConnection,
    readiness: DisplayReadiness,
    #[allow(dead_code)]
    globals: GlobalList,
    queue: EventQueue<NativeShellState>,
    state: NativeShellState,
}

impl NativeShell {
    /// Connect and bind core + xdg_wm_base (+ seat if present).
    pub fn connect_to_env() -> Result<Self, NativeError> {
        let connection = NativeConnection::connect_to_env()?;
        let readiness = display_readiness_from_conn(connection.connection())?;
        let (globals, queue) = registry_queue_init::<NativeShellState>(connection.connection())
            .map_err(|error| NativeError::Registry(error.to_string()))?;
        let qh = queue.handle();
        let mut state = NativeShellState::default();

        // Bind baseline globals from the initial snapshot.
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
        // Seat capabilities arrive asynchronously; one pending dispatch is enough
        // to request a keyboard if already advertised.
        let _ = shell.dispatch_pending()?;
        Ok(shell)
    }

    pub fn connection(&self) -> &NativeConnection {
        &self.connection
    }

    /// Create an xdg-toplevel with a solid-color buffer (default 640×480).
    pub fn create_toplevel(
        &mut self,
        title: impl Into<String>,
        app_id: impl Into<String>,
    ) -> Result<NativeSurfaceId, NativeError> {
        self.create_toplevel_sized(title, app_id, 640, 480, [0x22, 0x66, 0xcc, 0xff])
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
        self.state.toplevels.insert(
            id,
            ToplevelRecord {
                wl,
                xdg,
                toplevel,
                buffer: Some(buffer),
                _pool: Some(pool),
                _file: Some(file),
                configured: false,
                pending_size: Some((width as i32, height as i32)),
            },
        );
        self.connection.flush()?;
        Ok(id)
    }

    pub fn dispatch_pending(&mut self) -> Result<usize, NativeError> {
        Ok(self.queue.dispatch_pending(&mut self.state)?)
    }

    /// Compio-driven: await readable, read, dispatch.
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
        self.state
            .toplevels
            .get(&id)
            .is_some_and(|t| t.configured)
    }

    pub fn set_title(&mut self, id: NativeSurfaceId, title: impl Into<String>) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or(NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.toplevel.set_title(title.into());
        self.connection.flush()?;
        Ok(())
    }

    pub fn set_app_id(&mut self, id: NativeSurfaceId, app_id: impl Into<String>) -> Result<(), NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or(NativeError::Protocol(format!("unknown surface {id:?}")))?;
        record.toplevel.set_app_id(app_id.into());
        self.connection.flush()?;
        Ok(())
    }

    /// Destroy a toplevel and drop its buffers/roles.
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

// —— Dispatch implementations ——

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for NativeShellState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        // Late multi-instance seats (and anything missed at bootstrap).
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_seat" if state.seat.is_none() => {
                    let v = version.min(9).max(1);
                    state.seat = Some(registry.bind(name, v, qh, ()));
                }
                _ => {}
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
                    let (w, h) = record.pending_size.unwrap_or((0, 0));
                    let suggested = SuggestedSize::new(
                        if w > 0 { Some(w as u32) } else { None },
                        if h > 0 { Some(h as u32) } else { None },
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
        if let wl_keyboard::Event::Key { key, state: key_state, .. } = event {
            let pressed = matches!(key_state, WEnum::Value(wayland_client::protocol::wl_keyboard::KeyState::Pressed));
            state.push(NativeShellEvent::SeatKeyboardKey { key, pressed });
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

        // Pump a few times for configure.
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

        // On a real compositor we expect configure; headless nested may vary.
        let mut events = Vec::new();
        shell.drain_events_into(&mut events);
        let _ = events;
        let _ = shell.destroy_toplevel(id);
    }
}
