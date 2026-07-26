//! NativeShell public methods.

use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_compositor, wl_seat, wl_shm};
use wayland_client::Proxy;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape as CursorShape;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1;
use wayland_protocols::wp::viewporter::client::wp_viewporter;
use wayland_protocols::xdg::shell::client::xdg_wm_base;

use super::types::{NativeShellEvent, NativeShellState, NativeSurfaceId, ToplevelRecord};
use crate::display_io::DisplayReadiness;
use crate::native::connection::{NativeConnection, NativeError};
use crate::native::display_readiness_from_conn;
use crate::native::protocols::core::shm;
use wayland_client::globals::GlobalList;
use wayland_client::EventQueue;

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
        if let Ok(cursor) = globals.bind::<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1, _, _>(
            &qh,
            1..=1,
            (),
        ) {
            state.cursor_shape_manager = Some(cursor);
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

    pub fn has_cursor_shape(&self) -> bool {
        self.state.cursor_shape_manager.is_some()
    }

    /// Set the pointer cursor via `wp_cursor_shape` when available.
    pub fn set_cursor_shape(&mut self, shape: CursorShape) -> Result<(), NativeError> {
        let serial = self
            .state
            .pointer_enter_serial
            .ok_or_else(|| NativeError::Protocol("no pointer enter serial".into()))?;
        let pointer = self
            .state
            .pointer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no pointer".into()))?;
        let manager = self
            .state
            .cursor_shape_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("wp_cursor_shape_manager_v1 missing".into()))?;
        let qh = self.queue.handle();
        let device = manager.get_pointer(pointer, &qh, ());
        device.set_shape(serial, shape);
        device.destroy();
        self.connection.flush()?;
        Ok(())
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

