//! Additional Wayland protocol handlers beyond the core shell path.

use smithay::{
    input::pointer::PointerHandle,
    reexports::wayland_server::{
        Resource,
        protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    },
    utils::{IsAlive, Logical, Point, Serial},
    wayland::{
        foreign_toplevel_list::ForeignToplevelListHandler,
        idle_inhibit::IdleInhibitHandler,
        input_method::{InputMethodHandler, PopupSurface as ImPopupSurface},
        keyboard_shortcuts_inhibit::{
            KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState,
            KeyboardShortcutsInhibitor,
        },
        pointer_constraints::{PointerConstraintsHandler, with_pointer_constraint},
        pointer_warp::PointerWarpHandler,
        seat::WaylandFocus,
        security_context::{
            SecurityContext, SecurityContextHandler, SecurityContextListenerSource,
        },
        session_lock::{LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker},
        xdg_foreign::XdgForeignHandler,
        xdg_system_bell::XdgSystemBellHandler,
    },
};
use tracing::{debug, info, warn};

use crate::protocol::state::{ObjectKey, RuntimeState, SessionLockState};

impl RuntimeState {
    pub(crate) fn refresh_idle_inhibition(&mut self) {
        self.protocol_side
            .idle_inhibitors
            .retain(|surface| surface.alive());
        let inhibited = !self.protocol_side.idle_inhibitors.is_empty();
        self.protocol_globals
            .idle_notifier()
            .set_is_inhibited(inhibited);
    }

    pub(crate) fn publish_foreign_toplevel_from_surface(&mut self, surface: &WlSurface) {
        let (title, app_id) = toplevel_metadata(surface);
        self.publish_foreign_toplevel(
            surface,
            title.unwrap_or_default(),
            app_id.unwrap_or_default(),
        );
    }

    pub(crate) fn refresh_foreign_toplevel_metadata(
        &mut self,
        surface: &smithay::wayland::shell::xdg::ToplevelSurface,
    ) {
        let (title, app_id) = toplevel_metadata(surface.wl_surface());
        self.update_foreign_toplevel(surface.wl_surface(), title.as_deref(), app_id.as_deref());
    }

    /// Activate a pointer constraint when focus matches the constrained surface.
    pub(crate) fn maybe_activate_pointer_constraint(&mut self) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let Some(focus) = pointer.current_focus() else {
            return;
        };
        with_pointer_constraint(&focus, &pointer, |constraint| {
            if let Some(constraint) = constraint {
                constraint.activate();
            }
        });
    }

    pub(crate) fn publish_foreign_toplevel(
        &mut self,
        surface: &WlSurface,
        title: impl Into<String>,
        app_id: impl Into<String>,
    ) {
        let key = ObjectKey::from_surface(surface);
        if self.protocol_side.foreign_toplevels.contains_key(&key) {
            return;
        }
        let handle = self
            .protocol_globals
            .foreign_toplevel_list()
            .new_toplevel::<RuntimeState>(title, app_id);
        self.protocol_side.foreign_toplevels.insert(key, handle);
    }

    pub(crate) fn update_foreign_toplevel(
        &mut self,
        surface: &WlSurface,
        title: Option<&str>,
        app_id: Option<&str>,
    ) {
        let key = ObjectKey::from_surface(surface);
        let Some(handle) = self.protocol_side.foreign_toplevels.get(&key) else {
            return;
        };
        if let Some(title) = title {
            handle.send_title(title);
        }
        if let Some(app_id) = app_id {
            handle.send_app_id(app_id);
        }
        handle.send_done();
    }

    pub(crate) fn close_foreign_toplevel(&mut self, surface: &WlSurface) {
        let key = ObjectKey::from_surface(surface);
        if let Some(handle) = self.protocol_side.foreign_toplevels.remove(&key) {
            handle.send_closed();
        }
    }

    pub(crate) fn session_is_locked(&self) -> bool {
        self.protocol_side.session_lock.is_some()
    }

    /// Whether compositor keybinds (except VT recovery) should be suppressed.
    #[allow(dead_code)] // reserved for the upcoming bind path
    pub(crate) fn shortcuts_inhibited_for(&self, surface: &WlSurface) -> bool {
        self.protocol_side
            .shortcut_inhibitors
            .get(&ObjectKey::from_surface(surface))
            .is_some_and(KeyboardShortcutsInhibitor::is_active)
    }
}

impl PointerConstraintsHandler for RuntimeState {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        let focused = pointer
            .current_focus()
            .is_some_and(|focus| focus.id() == surface.id());
        if focused {
            with_pointer_constraint(surface, pointer, |constraint| {
                if let Some(constraint) = constraint {
                    constraint.activate();
                }
            });
        }
    }

    fn remove_constraint(&mut self, _surface: &WlSurface, _pointer: &PointerHandle<Self>) {}

    fn cursor_position_hint(
        &mut self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
        location: Point<f64, Logical>,
    ) {
        let active = with_pointer_constraint(surface, pointer, |constraint| {
            constraint.is_some_and(|c| c.is_active())
        });
        if !active {
            return;
        }
        let Some(window) = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(surface))
        else {
            return;
        };
        let Some(geometry) = self.space.element_geometry(window) else {
            return;
        };
        let target = geometry.loc.to_f64() + location;
        #[cfg(feature = "tty")]
        if let Some(bounds) = self.pointer_coordinate_space() {
            pointer.set_location(crate::protocol::input::constrain_pointer_location(
                target, bounds,
            ));
        } else {
            pointer.set_location(target);
        }
        #[cfg(not(feature = "tty"))]
        pointer.set_location(target);
        #[cfg(feature = "tty")]
        self.request_redraw_at(pointer.current_location());
    }
}

impl IdleInhibitHandler for RuntimeState {
    fn inhibit(&mut self, surface: WlSurface) {
        self.protocol_side.idle_inhibitors.insert(surface);
        self.refresh_idle_inhibition();
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        self.protocol_side.idle_inhibitors.remove(&surface);
        self.refresh_idle_inhibition();
    }
}

impl KeyboardShortcutsInhibitHandler for RuntimeState {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        self.protocol_globals.keyboard_shortcuts_inhibit()
    }

    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        inhibitor.activate();
        self.protocol_side
            .shortcut_inhibitors
            .insert(ObjectKey::from_surface(inhibitor.wl_surface()), inhibitor);
    }

    fn inhibitor_destroyed(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        self.protocol_side
            .shortcut_inhibitors
            .remove(&ObjectKey::from_surface(inhibitor.wl_surface()));
    }
}

impl XdgSystemBellHandler for RuntimeState {
    fn ring(&mut self, surface: Option<WlSurface>) {
        info!(
            surface = surface.as_ref().map(|s| s.id().protocol_id()),
            "xdg-system-bell ring"
        );
    }
}

impl PointerWarpHandler for RuntimeState {
    fn warp_pointer(
        &mut self,
        surface: WlSurface,
        _pointer: smithay::reexports::wayland_server::protocol::wl_pointer::WlPointer,
        pos: Point<f64, Logical>,
        _serial: Serial,
    ) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let origin = self
            .space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(&surface))
            .and_then(|window| self.space.element_geometry(window))
            .map(|geo| geo.loc.to_f64());
        #[cfg(feature = "tty")]
        let origin = origin.or_else(|| {
            use smithay::desktop::{WindowSurfaceType, layer_map_for_output};
            self.space.outputs().find_map(|output| {
                let map = layer_map_for_output(output);
                let layer = map.layer_for_surface(&surface, WindowSurfaceType::TOPLEVEL)?;
                let layer_geo = map.layer_geometry(layer)?;
                let output_geo = self.space.output_geometry(output)?;
                Some((output_geo.loc + layer_geo.loc).to_f64())
            })
        });
        let Some(origin) = origin else {
            return;
        };
        let target = origin + pos;
        #[cfg(feature = "tty")]
        if let Some(bounds) = self.pointer_coordinate_space() {
            pointer.set_location(crate::protocol::input::constrain_pointer_location(
                target, bounds,
            ));
        } else {
            pointer.set_location(target);
        }
        #[cfg(not(feature = "tty"))]
        pointer.set_location(target);
        #[cfg(feature = "tty")]
        self.request_redraw_at(pointer.current_location());
    }
}

impl ForeignToplevelListHandler for RuntimeState {
    fn foreign_toplevel_list_state(
        &mut self,
    ) -> &mut smithay::wayland::foreign_toplevel_list::ForeignToplevelListState {
        self.protocol_globals.foreign_toplevel_list()
    }
}

impl XdgForeignHandler for RuntimeState {
    fn xdg_foreign_state(&mut self) -> &mut smithay::wayland::xdg_foreign::XdgForeignState {
        self.protocol_globals.xdg_foreign()
    }
}

impl SessionLockHandler for RuntimeState {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        self.protocol_globals.session_lock()
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        if self.protocol_side.session_lock.is_some() {
            return;
        }
        confirmation.lock();
        self.protocol_side.session_lock = Some(SessionLockState {
            surfaces: std::collections::HashMap::new(),
        });
        info!("session locked");
        #[cfg(feature = "tty")]
        self.request_redraw_all();
    }

    fn unlock(&mut self) {
        self.protocol_side.session_lock = None;
        info!("session unlocked");
        #[cfg(feature = "tty")]
        {
            self.restore_keyboard_focus();
            self.request_redraw_all();
        }
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        let Some(lock) = self.protocol_side.session_lock.as_mut() else {
            return;
        };
        let name = self
            .space
            .outputs()
            .find(|candidate| candidate.owns(&output))
            .map(|output| output.name())
            .unwrap_or_else(|| "unknown".to_owned());
        if let Some(output_obj) = self.space.outputs().find(|o| o.owns(&output))
            && let Some(geometry) = self.space.output_geometry(output_obj)
        {
            surface.with_pending_state(|state| {
                state.size = Some(
                    (
                        u32::try_from(geometry.size.w).unwrap_or(1).max(1),
                        u32::try_from(geometry.size.h).unwrap_or(1).max(1),
                    )
                        .into(),
                );
            });
            surface.send_configure();
        }
        lock.surfaces.insert(name, surface);
        #[cfg(feature = "tty")]
        self.request_redraw_all();
    }
}

impl SecurityContextHandler for RuntimeState {
    fn context_created(&mut self, source: SecurityContextListenerSource, context: SecurityContext) {
        debug!(
            sandbox = ?context.sandbox_engine,
            app_id = ?context.app_id,
            "security context listener ready"
        );
        use std::sync::Arc;

        use crate::protocol::state::WaylandClientState;
        if let Err(error) = self
            .loop_handle
            .insert_source(source, move |stream, _, state| {
                let client_data = WaylandClientState {
                    from_security_context: true,
                    ..Default::default()
                };
                if let Err(error) = state
                    .display_handle
                    .insert_client(stream, Arc::new(client_data))
                {
                    warn!(%error, ?context, "failed to insert sandboxed Wayland client");
                }
            })
        {
            warn!(%error, "failed to register security-context listener");
        }
    }
}

impl InputMethodHandler for RuntimeState {
    fn new_popup(&mut self, surface: ImPopupSurface) {
        use smithay::desktop::PopupKind;
        if let Err(error) = self.popups.track_popup(PopupKind::InputMethod(surface)) {
            warn!(%error, "failed to track input-method popup");
        }
        #[cfg(feature = "tty")]
        self.request_redraw_workspace();
    }

    fn dismiss_popup(&mut self, surface: ImPopupSurface) {
        use smithay::desktop::{PopupKind, PopupManager};
        let parent = surface.get_parent().map(|parent| parent.surface.clone());
        if let Some(parent) = parent {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::InputMethod(surface));
        }
        #[cfg(feature = "tty")]
        self.request_redraw_workspace();
    }

    fn popup_repositioned(&mut self, _surface: ImPopupSurface) {
        #[cfg(feature = "tty")]
        self.request_redraw_workspace();
    }

    fn parent_geometry(&self, parent: &WlSurface) -> smithay::utils::Rectangle<i32, Logical> {
        self.space
            .elements()
            .find(|window| window.wl_surface().as_deref() == Some(parent))
            .and_then(|window| self.space.element_geometry(window))
            .unwrap_or_default()
    }
}

fn toplevel_metadata(surface: &WlSurface) -> (Option<String>, Option<String>) {
    use smithay::wayland::compositor::with_states;
    use smithay::wayland::shell::xdg::XdgToplevelSurfaceData;
    with_states(surface, |states| {
        let data = states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .map(|d| d.lock().unwrap());
        (
            data.as_ref().and_then(|d| d.title.clone()),
            data.as_ref().and_then(|d| d.app_id.clone()),
        )
    })
}

impl smithay::wayland::xdg_toplevel_icon::XdgToplevelIconHandler for RuntimeState {}
impl smithay::wayland::xdg_toplevel_tag::XdgToplevelTagHandler for RuntimeState {}

impl smithay::wayland::selection::wlr_data_control::DataControlHandler for RuntimeState {
    fn data_control_state(
        &mut self,
    ) -> &mut smithay::wayland::selection::wlr_data_control::DataControlState {
        self.protocol_globals.wlr_data_control()
    }
}

impl smithay::wayland::selection::ext_data_control::DataControlHandler for RuntimeState {
    fn data_control_state(
        &mut self,
    ) -> &mut smithay::wayland::selection::ext_data_control::DataControlState {
        self.protocol_globals.ext_data_control()
    }
}

// Blur regions live on surface cached state until the Vulkan frame path
// samples them; advertising the global matches Niri-class clients.
impl smithay::wayland::background_effect::ExtBackgroundEffectHandler for RuntimeState {}

impl smithay::wayland::xwayland_keyboard_grab::XWaylandKeyboardGrabHandler for RuntimeState {
    fn keyboard_focus_for_xsurface(
        &self,
        surface: &WlSurface,
    ) -> Option<crate::protocol::focus::KeyboardFocusTarget> {
        #[cfg(feature = "xwayland")]
        {
            use smithay::desktop::Window;
            use smithay::wayland::seat::WaylandFocus;
            self.space
                .elements()
                .find(|window| window.wl_surface().as_deref() == Some(surface))
                .and_then(Window::x11_surface)
                .cloned()
                .map(crate::protocol::focus::KeyboardFocusTarget::from)
                .or_else(|| {
                    Some(crate::protocol::focus::KeyboardFocusTarget::from(
                        surface.clone(),
                    ))
                })
        }
        #[cfg(not(feature = "xwayland"))]
        {
            Some(crate::protocol::focus::KeyboardFocusTarget::from(
                surface.clone(),
            ))
        }
    }
}
