//! NativeShell public methods.

use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{
    wl_compositor, wl_data_device_manager, wl_output, wl_seat, wl_shm,
};
use wayland_client::Proxy;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape as CursorShape;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_manager_v1;
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1;
use wayland_protocols::wp::viewporter::client::wp_viewporter;
use wayland_protocols::xdg::shell::client::xdg_wm_base;

use super::handle::NativeSurfaceHandle;
use super::types::{
    LayerRecord, NativeCapabilities, NativePopupPositioner, NativeShellEvent, NativeShellState,
    NativeSurfaceId, PopupRecord,
};
use crate::layer_shell::{LayerAnchor, LayerKeyboardInteractivity, LayerSurfaceLayer};
use crate::surface::{ConstraintAdjustments, Gravity, PopupAnchor};
use wayland_protocols::wp::pointer_gestures::zv1::client::zwp_pointer_gestures_v1;
use wayland_protocols::wp::relative_pointer::zv1::client::zwp_relative_pointer_manager_v1;
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1;
use wayland_protocols::xdg::shell::client::xdg_positioner;
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};
use crate::display_io::DisplayReadiness;
use crate::native::connection::{NativeConnection, NativeError};
use crate::native::display_readiness_from_conn;
use crate::native::protocols::core::shm;
use wayland_client::globals::GlobalList;
use wayland_client::EventQueue;

/// SCTK-free shell runtime.
pub struct NativeShell {
    pub(crate) connection: NativeConnection,
    pub(crate) readiness: DisplayReadiness,
    #[allow(dead_code)]
    pub(crate) globals: GlobalList,
    pub(crate) queue: EventQueue<NativeShellState>,
    pub(crate) state: NativeShellState,
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
        if let Ok(ddm) =
            globals.bind::<wl_data_device_manager::WlDataDeviceManager, _, _>(&qh, 1..=3, ())
        {
            state.data_device_manager = Some(ddm);
        }
        // Bind every advertised wl_output (multi-instance).
        for global in globals.contents().clone_list() {
            if global.interface == "wl_output" {
                let version = global.version.min(4).max(1);
                let output = globals
                    .registry()
                    .bind::<wl_output::WlOutput, _, _>(global.name, version, &qh, ());
                state.output_objects.insert(output.id().protocol_id(), global.name);
                state.outputs.insert(
                    global.name,
                    super::types::OutputRecord {
                        scale: 1,
                        make: String::new(),
                        model: String::new(),
                    },
                );
            }
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
        if let Ok(tim) = globals.bind::<
            wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3,
            _,
            _,
        >(&qh, 1..=1, ())
        {
            state.text_input_manager = Some(tim);
        }
        if let Ok(layer) = globals.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(
            &qh,
            1..=4,
            (),
        ) {
            state.layer_shell = Some(layer);
        }
        if let Ok(act) =
            globals.bind::<xdg_activation_v1::XdgActivationV1, _, _>(&qh, 1..=1, ())
        {
            state.activation = Some(act);
        }
        if let Ok(gestures) =
            globals.bind::<zwp_pointer_gestures_v1::ZwpPointerGesturesV1, _, _>(&qh, 1..=3, ())
        {
            state.pointer_gestures = Some(gestures);
        }
        if let Ok(rel) = globals
            .bind::<zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1, _, _>(
                &qh,
                1..=1,
                (),
            )
        {
            state.relative_pointer_manager = Some(rel);
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

        if let (Some(manager), Some(seat)) = (
            state.data_device_manager.as_ref(),
            state.seat.as_ref(),
        ) {
            state.data_device = Some(manager.get_data_device(seat, &qh, ()));
        }
        if let (Some(tim), Some(seat)) = (state.text_input_manager.as_ref(), state.seat.as_ref()) {
            state.text_input = Some(tim.get_text_input(seat, &qh, ()));
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

    pub fn capabilities(&self) -> NativeCapabilities {
        NativeCapabilities {
            fractional_scale: self.state.fractional_manager.is_some(),
            viewporter: self.state.viewporter.is_some(),
            cursor_shape: self.state.cursor_shape_manager.is_some(),
            seat: self.state.seat.is_some(),
            pointer: self.state.pointer.is_some()
                || self
                    .state
                    .seat_capabilities
                    .contains(wayland_client::protocol::wl_seat::Capability::Pointer),
            keyboard: self.state.keyboard.is_some()
                || self
                    .state
                    .seat_capabilities
                    .contains(wayland_client::protocol::wl_seat::Capability::Keyboard),
            touch: self.state.touch.is_some()
                || self
                    .state
                    .seat_capabilities
                    .contains(wayland_client::protocol::wl_seat::Capability::Touch),
            output_count: self.state.outputs.len() as u32,
            data_device: self.state.data_device.is_some(),
            xkb: self.state.xkb.is_some(),
            text_input: self.state.text_input.is_some(),
            layer_shell: self.state.layer_shell.is_some(),
            activation: self.state.activation.is_some(),
            pointer_gestures: self.state.pointer_gestures.is_some(),
            pointer_gesture_hold: self
                .state
                .pointer_gestures
                .as_ref()
                .is_some_and(|g| g.version() >= 3),
            relative_pointer: self.state.relative_pointer_manager.is_some(),
        }
    }

    /// Enable `zwp_relative_pointer_v1` for the seat pointer (unaccelerated motion).
    pub fn enable_relative_pointer(&mut self) -> Result<(), NativeError> {
        if self.state.relative_pointer.is_some() {
            return Ok(());
        }
        let manager = self
            .state
            .relative_pointer_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("relative_pointer_manager missing".into()))?;
        let pointer = self
            .state
            .pointer
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("no pointer".into()))?;
        let qh = self.queue.handle();
        self.state.relative_pointer = Some(manager.get_relative_pointer(pointer, &qh, ()));
        self.connection.flush()?;
        Ok(())
    }

    pub fn disable_relative_pointer(&mut self) -> Result<(), NativeError> {
        if let Some(rel) = self.state.relative_pointer.take() {
            rel.destroy();
            self.connection.flush()?;
        }
        Ok(())
    }

    pub fn has_layer_shell(&self) -> bool {
        self.state.layer_shell.is_some()
    }

    pub fn has_activation(&self) -> bool {
        self.state.activation.is_some()
    }

    /// Request an `xdg_activation_v1` token for `surface`.
    ///
    /// Completes with [`NativeShellEvent::ActivationToken`].
    pub fn request_activation_token(
        &mut self,
        surface: NativeSurfaceId,
        app_id: Option<&str>,
    ) -> Result<(), NativeError> {
        let activation = self
            .state
            .activation
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("xdg_activation_v1 missing".into()))?;
        let wl = self
            .state
            .toplevels
            .get(&surface)
            .map(|t| t.wl.clone())
            .or_else(|| self.state.popups.get(&surface).map(|p| p.wl.clone()))
            .or_else(|| self.state.layers.get(&surface).map(|l| l.wl.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {surface:?}")))?;
        let qh = self.queue.handle();
        let token = activation.get_activation_token(&qh, ());
        if let Some(app_id) = app_id {
            token.set_app_id(app_id.to_string());
        }
        if let (Some(serial), Some(seat)) =
            (self.state.last_input_serial, self.state.seat.as_ref())
        {
            token.set_serial(serial, seat);
        }
        token.set_surface(&wl);
        token.commit();
        let obj_id = token.id().protocol_id();
        self.state
            .activation_tokens
            .insert(obj_id, (surface, token));
        self.connection.flush()?;
        Ok(())
    }

    /// Activate `surface` with a previously obtained token string.
    pub fn activate_with_token(
        &mut self,
        surface: NativeSurfaceId,
        token: impl Into<String>,
    ) -> Result<(), NativeError> {
        let activation = self
            .state
            .activation
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("xdg_activation_v1 missing".into()))?;
        let wl = self
            .state
            .toplevels
            .get(&surface)
            .map(|t| t.wl.clone())
            .or_else(|| self.state.popups.get(&surface).map(|p| p.wl.clone()))
            .or_else(|| self.state.layers.get(&surface).map(|l| l.wl.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {surface:?}")))?;
        activation.activate(token.into(), &wl);
        self.connection.flush()?;
        Ok(())
    }

    /// Create a `zwlr_layer_surface_v1` (panel / bar / overlay).
    pub fn create_layer_surface(
        &mut self,
        namespace: impl Into<String>,
        layer: LayerSurfaceLayer,
        width: u32,
        height: u32,
        anchor: LayerAnchor,
        exclusive_zone: i32,
        keyboard: LayerKeyboardInteractivity,
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
        let shell = self
            .state
            .layer_shell
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("zwlr_layer_shell_v1 missing".into()))?;

        let wl = compositor.create_surface(&qh, ());
        wl.set_buffer_scale(1);
        let layer_surface = shell.get_layer_surface(
            &wl,
            None,
            layer.into(),
            namespace.into(),
            &qh,
            (),
        );
        layer_surface.set_size(width, height);
        layer_surface.set_anchor(layer_anchor_to_wire(anchor));
        layer_surface.set_exclusive_zone(exclusive_zone);
        layer_surface.set_keyboard_interactivity(keyboard.into());
        wl.commit();

        // Placeholder buffer; size may be 0xH for stretched panels — use at least 1×1.
        let bw = width.max(1);
        let bh = height.max(1);
        let (file, pool, buffer) =
            shm::create_solid_buffer(shm, &qh, bw, bh, [0xff, 0x18, 0x18, 0x22])
                .map_err(|e| NativeError::Io(e.to_string()))?;

        let id = self.state.alloc_id();
        self.state
            .layer_objects
            .insert(layer_surface.id().protocol_id(), id);
        self.state
            .wl_surface_objects
            .insert(wl.id().protocol_id(), id);
        self.state.layers.insert(
            id,
            LayerRecord {
                wl,
                layer: layer_surface,
                buffer: Some(buffer),
                _pool: Some(pool),
                _file: Some(file),
                configured: false,
                pending_size: Some((width, height)),
                logical_w: width,
                logical_h: height,
            },
        );
        self.connection.flush()?;
        Ok(id)
    }

    pub fn destroy_layer_surface(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let Some(record) = self.state.layers.remove(&id) else {
            return Err(NativeError::Protocol(format!("unknown layer {id:?}")));
        };
        self.state
            .layer_objects
            .remove(&record.layer.id().protocol_id());
        self.state
            .wl_surface_objects
            .remove(&record.wl.id().protocol_id());
        record.layer.destroy();
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

    pub fn is_layer_configured(&self, id: NativeSurfaceId) -> bool {
        self.state.layers.get(&id).is_some_and(|l| l.configured)
    }

    pub fn layer_count(&self) -> usize {
        self.state.layers.len()
    }

    pub fn output_scale_factor(&self, output_name: u32) -> Option<i32> {
        self.state.outputs.get(&output_name).map(|o| o.scale)
    }

    /// Request a `wl_surface.frame` callback; emits [`NativeShellEvent::Frame`].
    pub fn request_frame(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let qh = self.queue.handle();
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        let callback = record.wl.frame(&qh, ());
        self.state
            .frame_callbacks
            .insert(callback.id().protocol_id(), id);
        self.connection.flush()?;
        Ok(())
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

    /// Drain native events mapped to the public [`crate::Event`] model.
    ///
    /// Uses the live seat proxy when present so keyboard/pointer serials are real
    /// `WlSeat` handles. Unmapped events (clipboard, modifiers, …) are dropped;
    /// use [`Self::drain_events`] when those are needed.
    pub fn drain_public_events(
        &mut self,
        surfaces: &mut crate::native::event_map::SurfaceIdMap,
        map_state: &mut crate::native::event_map::NativeEventMapState,
        out: &mut Vec<crate::Event>,
    ) {
        let seat = self.state.seat.clone();
        if let Some(serial) = self.state.last_input_serial {
            map_state.last_serial = serial;
        }
        for event in self.state.events.drain(..) {
            if let Some(mapped) = crate::native::event_map::map_native_event_full(
                event,
                surfaces,
                seat.as_ref(),
                map_state,
            ) {
                out.push(mapped);
            }
        }
    }

    /// Borrow the seat proxy (for building [`crate::InputSerial`] outside the shell).
    pub fn seat(&self) -> Option<&wayland_client::protocol::wl_seat::WlSeat> {
        self.state.seat.as_ref()
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

    /// Borrow a renderer handle for wgpu / Vulkan Wayland surface creation.
    pub fn surface_handle(
        &self,
        id: NativeSurfaceId,
    ) -> Result<NativeSurfaceHandle, NativeError> {
        let record = self
            .state
            .toplevels
            .get(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?;
        Ok(NativeSurfaceHandle::new(
            self.connection.connection().clone(),
            record.wl.clone(),
            id,
        ))
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
        // Destroy child popups first (parent must outlive them for some compositors).
        let child_popups: Vec<_> = self
            .state
            .popups
            .iter()
            .filter(|(_, p)| p.parent == id)
            .map(|(&pid, _)| pid)
            .collect();
        for pid in child_popups {
            let _ = self.destroy_popup(pid);
        }

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

    /// Create an `xdg_popup` child of a configured toplevel (or another popup).
    ///
    /// When `grab` is true, uses the latest pointer/keyboard serial for popup grab.
    pub fn create_popup(
        &mut self,
        parent: NativeSurfaceId,
        positioner: &NativePopupPositioner,
        grab: bool,
    ) -> Result<NativeSurfaceId, NativeError> {
        if positioner.size.width == 0 || positioner.size.height == 0 {
            return Err(NativeError::Protocol("popup size must be non-zero".into()));
        }
        let parent_xdg = self
            .state
            .toplevels
            .get(&parent)
            .map(|t| t.xdg.clone())
            .or_else(|| self.state.popups.get(&parent).map(|p| p.xdg.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown parent {parent:?}")))?;

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

        let pos = wm_base.create_positioner(&qh, ());
        apply_positioner(&pos, positioner);

        let wl = compositor.create_surface(&qh, ());
        wl.set_buffer_scale(1);
        let xdg = wm_base.get_xdg_surface(&wl, &qh, ());
        let popup = xdg.get_popup(Some(&parent_xdg), &pos, &qh, ());
        pos.destroy();

        if grab {
            if let (Some(serial), Some(seat)) =
                (self.state.last_input_serial, self.state.seat.as_ref())
            {
                popup.grab(seat, serial);
            }
        }

        let w = positioner.size.width;
        let h = positioner.size.height;
        let (file, pool, buffer) =
            shm::create_solid_buffer(shm, &qh, w, h, [0xff, 0x33, 0x33, 0x33])
                .map_err(|e| NativeError::Io(e.to_string()))?;
        wl.commit();

        let id = self.state.alloc_id();
        self.state
            .popup_objects
            .insert(popup.id().protocol_id(), id);
        self.state
            .xdg_surface_objects
            .insert(xdg.id().protocol_id(), id);
        self.state
            .wl_surface_objects
            .insert(wl.id().protocol_id(), id);
        self.state.popups.insert(
            id,
            PopupRecord {
                wl,
                xdg,
                popup,
                parent,
                buffer: Some(buffer),
                _pool: Some(pool),
                _file: Some(file),
                configured: false,
                pending_geom: None,
                logical_w: w,
                logical_h: h,
            },
        );
        self.connection.flush()?;
        Ok(id)
    }

    pub fn destroy_popup(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        let Some(record) = self.state.popups.remove(&id) else {
            return Err(NativeError::Protocol(format!("unknown popup {id:?}")));
        };
        self.state
            .popup_objects
            .remove(&record.popup.id().protocol_id());
        self.state
            .xdg_surface_objects
            .remove(&record.xdg.id().protocol_id());
        self.state
            .wl_surface_objects
            .remove(&record.wl.id().protocol_id());
        record.popup.destroy();
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

    pub fn popup_count(&self) -> usize {
        self.state.popups.len()
    }

    pub fn is_popup_configured(&self, id: NativeSurfaceId) -> bool {
        self.state.popups.get(&id).is_some_and(|p| p.configured)
    }
}

fn layer_anchor_to_wire(anchor: LayerAnchor) -> zwlr_layer_surface_v1::Anchor {
    zwlr_layer_surface_v1::Anchor::from_bits_truncate(u32::from(anchor.bits()))
}

fn apply_positioner(pos: &xdg_positioner::XdgPositioner, value: &NativePopupPositioner) {
    pos.set_size(value.size.width as i32, value.size.height as i32);
    pos.set_anchor_rect(
        value.anchor_rect.origin.x,
        value.anchor_rect.origin.y,
        value.anchor_rect.size.width as i32,
        value.anchor_rect.size.height as i32,
    );
    pos.set_anchor(map_anchor(value.anchor));
    pos.set_gravity(map_gravity(value.gravity));
    pos.set_constraint_adjustment(map_constraints(value.constraints));
    pos.set_offset(value.offset.x, value.offset.y);
}

fn map_anchor(value: PopupAnchor) -> xdg_positioner::Anchor {
    match value {
        PopupAnchor::None => xdg_positioner::Anchor::None,
        PopupAnchor::Top => xdg_positioner::Anchor::Top,
        PopupAnchor::Bottom => xdg_positioner::Anchor::Bottom,
        PopupAnchor::Left => xdg_positioner::Anchor::Left,
        PopupAnchor::Right => xdg_positioner::Anchor::Right,
        PopupAnchor::TopLeft => xdg_positioner::Anchor::TopLeft,
        PopupAnchor::BottomLeft => xdg_positioner::Anchor::BottomLeft,
        PopupAnchor::TopRight => xdg_positioner::Anchor::TopRight,
        PopupAnchor::BottomRight => xdg_positioner::Anchor::BottomRight,
    }
}

fn map_gravity(value: Gravity) -> xdg_positioner::Gravity {
    match value {
        Gravity::None => xdg_positioner::Gravity::None,
        Gravity::Top => xdg_positioner::Gravity::Top,
        Gravity::Bottom => xdg_positioner::Gravity::Bottom,
        Gravity::Left => xdg_positioner::Gravity::Left,
        Gravity::Right => xdg_positioner::Gravity::Right,
        Gravity::TopLeft => xdg_positioner::Gravity::TopLeft,
        Gravity::BottomLeft => xdg_positioner::Gravity::BottomLeft,
        Gravity::TopRight => xdg_positioner::Gravity::TopRight,
        Gravity::BottomRight => xdg_positioner::Gravity::BottomRight,
    }
}

fn map_constraints(value: ConstraintAdjustments) -> xdg_positioner::ConstraintAdjustment {
    let mut result = xdg_positioner::ConstraintAdjustment::empty();
    if value.contains(ConstraintAdjustments::SLIDE_X) {
        result |= xdg_positioner::ConstraintAdjustment::SlideX;
    }
    if value.contains(ConstraintAdjustments::SLIDE_Y) {
        result |= xdg_positioner::ConstraintAdjustment::SlideY;
    }
    if value.contains(ConstraintAdjustments::FLIP_X) {
        result |= xdg_positioner::ConstraintAdjustment::FlipX;
    }
    if value.contains(ConstraintAdjustments::FLIP_Y) {
        result |= xdg_positioner::ConstraintAdjustment::FlipY;
    }
    if value.contains(ConstraintAdjustments::RESIZE_X) {
        result |= xdg_positioner::ConstraintAdjustment::ResizeX;
    }
    if value.contains(ConstraintAdjustments::RESIZE_Y) {
        result |= xdg_positioner::ConstraintAdjustment::ResizeY;
    }
    result
}

