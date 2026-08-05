mod device;
mod pointer_geometry;
mod session_lock;
mod tablet;
#[cfg(test)]
mod tests;

use pointer_geometry::{
    center_pointer_location, replace_non_finite_pointer_location, sanitize_relative_pointer_delta,
    virtual_terminal_for_keysym, workspace_index_for_keysym,
};

pub(crate) use pointer_geometry::constrain_pointer_location;

use tensor_util::{LogicalPoint, LogicalRect};
use tracing::{debug, warn};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};
use xkbcommon::xkb::keysyms;

use tensor_event::{
    AbsoluteMotionEvent, BackendInputEvent, KeyboardEvent, PointerAxisEvent, PointerButtonEvent,
    PointerGestureEvent, RelativeMotionEvent,
};

use crate::backend::LibinputEvent;

use super::{
    globals::pointer_constraints::ConstraintMotion,
    serial::{Serial, next_serial},
    state::{RuntimeState, surface_tree_under},
};

impl RuntimeState {
    pub(crate) fn process_input_event(&mut self, event: LibinputEvent) {
        if let LibinputEvent::Device(event) = event {
            self.process_input_device_change(event);
            return;
        }
        // A confirmed or pending lock owns client input. Compositor shortcuts
        // are disabled except VT recovery, and normal surface hit-testing is
        // never consulted.
        if self.session_is_locked() {
            self.process_session_lock_input(event);
            return;
        }
        let cursor_revealed = self.note_cursor_device_activity(&event);
        let activity = matches!(&event, LibinputEvent::Input(event) if event.is_activity());
        match event {
            LibinputEvent::Device(_) => unreachable!("device changes returned above"),
            LibinputEvent::Input(BackendInputEvent::Keyboard(event)) => {
                self.forward_keyboard(event)
            }
            LibinputEvent::Input(BackendInputEvent::PointerMotion(event)) => {
                self.forward_pointer_motion(event)
            }
            LibinputEvent::Input(BackendInputEvent::PointerMotionAbsolute(event)) => {
                self.forward_pointer_motion_absolute(event)
            }
            LibinputEvent::Input(BackendInputEvent::PointerButton(event)) => {
                self.forward_pointer_button(event)
            }
            LibinputEvent::Input(BackendInputEvent::PointerAxis(event)) => {
                self.forward_pointer_axis(event)
            }
            LibinputEvent::Input(BackendInputEvent::PointerGesture(event)) => {
                self.forward_pointer_gesture(event)
            }
            LibinputEvent::Input(BackendInputEvent::TabletToolAdded(event)) => self
                .protocol_globals
                .tablet
                .add_tool(&self.display_handle, event),
            LibinputEvent::Input(BackendInputEvent::TabletToolProximity(event)) => {
                self.forward_tablet_proximity(event)
            }
            LibinputEvent::Input(BackendInputEvent::TabletToolAxes(event)) => {
                self.forward_tablet_axes(event)
            }
            LibinputEvent::Input(BackendInputEvent::TabletToolTip(event)) => {
                self.protocol_globals.tablet.tool_tip(event)
            }
            LibinputEvent::Input(BackendInputEvent::TabletToolButton(event)) => {
                self.protocol_globals.tablet.tool_button(event)
            }
            LibinputEvent::Input(BackendInputEvent::TabletPad(event)) => self
                .protocol_globals
                .tablet
                .pad_event(&self.display_handle, event),
            LibinputEvent::Input(BackendInputEvent::Activity) => {}
        }
        if activity {
            self.notify_idle_activity();
        }
        if cursor_revealed {
            self.flush_queued_redraws();
        }
    }

    fn note_cursor_device_activity(&mut self, event: &LibinputEvent) -> bool {
        let activity = matches!(
            event,
            LibinputEvent::Input(
                BackendInputEvent::PointerMotion(_)
                    | BackendInputEvent::PointerMotionAbsolute(_)
                    | BackendInputEvent::PointerButton(_)
                    | BackendInputEvent::PointerAxis(_)
                    | BackendInputEvent::TabletToolProximity(_)
                    | BackendInputEvent::TabletToolAxes(_)
                    | BackendInputEvent::TabletToolTip(_)
                    | BackendInputEvent::TabletToolButton(_)
            )
        );
        if !activity {
            return false;
        }
        let revealed = self.cursor.note_pointer_activity(std::time::Instant::now());
        if revealed {
            self.queue_all_cursor_extents();
        }
        revealed
    }

    fn hide_cursor_for_keyboard_activity(&mut self) {
        if !self.cursor.will_hide_for_keyboard_activity() {
            return;
        }
        let location = self.input_seat.pointer_location();
        if let Some(location) = location {
            self.queue_cursor_redraw_between(0, location, location);
        }
        assert!(self.cursor.note_keyboard_activity());
        if location.is_some() {
            self.flush_queued_redraws();
        } else {
            self.request_redraw_all();
        }
    }

    /// Publish the aggregate libinput capabilities on the single Wayland
    /// seat. A keyboard can arrive after an application has mapped, so a
    /// successful keyboard creation must also restore the compositor-selected
    /// focus to the new keyboard state.
    pub(crate) fn reconcile_seat_capabilities(&mut self) {
        let keyboard_count = self
            .input_devices
            .values()
            .filter(|capabilities| capabilities.keyboard)
            .count();
        let pointer_count = self
            .input_devices
            .values()
            .filter(|capabilities| capabilities.pointer)
            .count();
        let touch_count = self
            .input_devices
            .values()
            .filter(|capabilities| capabilities.touch)
            .count();

        let keyboard_present =
            keyboard_count > 0 || self.protocol_globals.virtual_keyboard.is_active();
        if keyboard_present && !self.input_seat.keyboard_enabled() {
            match self
                .input_seat
                .enable_keyboard()
                .map(ToOwned::to_owned)
                .and_then(|keymap| {
                    self.protocol_globals
                        .seat
                        .set_keyboard_enabled(true, Some(&keymap))
                }) {
                Ok(()) => {
                    if let Some(grab) = self.protocol_globals.input_method.keyboard_grab_resource()
                    {
                        self.protocol_globals
                            .seat
                            .initialize_input_method_grab(&grab);
                        let modifiers = self.input_seat.keyboard_modifiers().serialized;
                        grab.modifiers(
                            next_serial().into(),
                            modifiers.depressed,
                            modifiers.latched,
                            modifiers.locked,
                            modifiers.layout,
                        );
                    }
                    if self.session_is_locked() {
                        if let Some(surface) =
                            self.protocol_globals.session_lock.first_active_surface()
                        {
                            self.focus_session_lock_surface(&surface);
                        }
                    } else {
                        self.restore_keyboard_focus();
                    }
                }
                Err(error) => {
                    self.input_seat.disable_keyboard();
                    warn!(%error, "failed to publish keyboard capability");
                }
            }
        } else if !keyboard_present && self.input_seat.keyboard_enabled() {
            self.set_keyboard_focus(None, next_serial());
            self.input_seat.disable_keyboard();
            let _ = self.protocol_globals.seat.set_keyboard_enabled(false, None);
            self.protocol_globals.activation.sync_keyboard_focus(None);
        }

        if pointer_count > 0 && !self.input_seat.pointer_enabled() {
            self.input_seat.enable_pointer();
            self.protocol_globals.seat.set_pointer_enabled(true);
            // A pointer can be discovered after the first output frame. Draw
            // the default software cursor immediately instead of waiting for
            // the user's first motion event to make it visible.
            self.request_redraw_all();
        } else if pointer_count == 0 && self.input_seat.pointer_enabled() {
            self.clear_pointer_focus(next_serial(), 0);
            self.input_seat.disable_pointer();
            self.protocol_globals.seat.set_pointer_enabled(false);
            self.protocol_globals.activation.sync_pointer_focus(None);
            // The next frame has no overlay, which damages the previous
            // cursor bounds and clears the last visible arrow.
            self.request_redraw_all();
        }

        if touch_count > 0 && !self.input_seat.touch_enabled() {
            self.input_seat.set_touch_enabled(true);
            self.protocol_globals.seat.set_touch_enabled(true);
        } else if touch_count == 0 && self.input_seat.touch_enabled() {
            self.input_seat.set_touch_enabled(false);
            self.protocol_globals.seat.set_touch_enabled(false);
        }

        debug!(
            keyboard_count,
            pointer_count, touch_count, "libinput seat capabilities reconciled"
        );
    }

    fn forward_keyboard(&mut self, event: KeyboardEvent) {
        if !self.input_seat.keyboard_enabled() {
            return;
        }
        let serial = next_serial();
        if self.protocol_globals.seat.activate_default_keymap() {
            if let Some(grab) = self.protocol_globals.input_method.keyboard_grab_resource() {
                self.protocol_globals
                    .seat
                    .initialize_input_method_grab(&grab);
            }
            self.protocol_globals
                .seat
                .modifiers(self.input_seat.keyboard_modifiers(), serial);
        }
        if event.pressed {
            self.hide_cursor_for_keyboard_activity();
        }
        let shortcuts_inhibited = self
            .input_seat
            .keyboard_focus()
            .is_some_and(|surface| self.shortcuts_inhibited_for(surface));
        let Some(update) = self.input_seat.update_key(event.key, event.pressed, serial) else {
            return;
        };
        if !update.transition {
            return;
        }
        let mut intercepted = false;
        if let Some(vt) = virtual_terminal_for_keysym(update.keysym) {
            if update.pressed {
                self.request_virtual_terminal(vt);
            }
            // A VT switch can prevent a key release from reaching us.
            intercepted = true;
        } else if !shortcuts_inhibited && update.pressed && update.modifiers.logo {
            if let Some(index) = workspace_index_for_keysym(update.keysym) {
                if update.modifiers.shift {
                    if let Some(view) = self.world.focused_view(self.active_workspace()) {
                        let _ =
                            self.move_view_to_workspace(view, crate::ecs::WorkspaceId::new(index));
                        let _ = self.activate_workspace_index(index);
                    }
                } else {
                    let _ = self.activate_workspace_index(index);
                }
                intercepted = true;
            } else if update.keysym == keysyms::KEY_Page_Down || update.keysym == keysyms::KEY_Right
            {
                let _ = self.cycle_workspace(1);
                intercepted = true;
            } else if update.keysym == keysyms::KEY_Page_Up || update.keysym == keysyms::KEY_Left {
                let _ = self.cycle_workspace(-1);
                intercepted = true;
            }
        }
        let application_route = self.input_seat.key_was_forwarded(update.evdev_key);
        let input_method_route = self
            .input_seat
            .key_was_forwarded_to_input_method(update.evdev_key);
        if !update.pressed && !application_route && !input_method_route {
            intercepted = true;
        }
        if !intercepted {
            let start_input_method_route = update.pressed
                && !application_route
                && self.protocol_globals.input_method.keyboard_grab_active();
            if input_method_route || start_input_method_route {
                let _ = self.protocol_globals.input_method.forward_key(
                    update.evdev_key,
                    update.pressed,
                    serial,
                    event.time_msec(),
                    update.modifiers_changed.then_some(update.modifiers),
                );
                self.input_seat
                    .set_key_forwarded_to_input_method(update.evdev_key, update.pressed);
                intercepted = true;
            } else {
                if update.modifiers_changed {
                    self.protocol_globals
                        .seat
                        .modifiers(update.modifiers, serial);
                }
                self.protocol_globals.seat.key(
                    update.evdev_key,
                    update.pressed,
                    serial,
                    event.time_msec(),
                );
                self.input_seat
                    .set_key_forwarded(update.evdev_key, update.pressed);
            }
        }
        if update.pressed && !intercepted {
            self.protocol_globals
                .activation
                .note_keyboard_interaction(serial.into());
        }
        // Value bus: keycode-level sample (not keysym — keymap stays seat-side).
        self.push_key_sample(event.sample());
    }

    fn request_virtual_terminal(&mut self, vt: i32) {
        let Some(backend) = self.backend.as_mut() else {
            warn!(vt, "ignored virtual terminal request without a tty backend");
            return;
        };
        backend.change_vt(vt);
    }

    pub(crate) fn forward_pointer_motion(&mut self, event: RelativeMotionEvent) {
        let Some(current) = self.input_seat.pointer_location() else {
            return;
        };
        let location =
            self.relative_pointer_location(current, (event.delta_x, event.delta_y).into());
        let Some(location) = location else {
            return;
        };
        self.forward_pointer_location(location, event.time_ns, Some(event));
    }

    /// Follow the same absolute-coordinate conversion used by Niri: libinput
    /// maps the device into the compositor's logical output bounds, then the
    /// seat receives the resulting global location and a redraw is queued.
    pub(crate) fn forward_pointer_motion_absolute(&mut self, event: AbsoluteMotionEvent) {
        let Some(bounds) = self.pointer_coordinate_space() else {
            return;
        };
        let current = self
            .input_seat
            .pointer_location()
            .unwrap_or_else(|| center_pointer_location(bounds));
        let location = LogicalPoint::from((
            event.x * f64::from(bounds.size.w),
            event.y * f64::from(bounds.size.h),
        )) + bounds.loc.to_f64();
        let location = replace_non_finite_pointer_location(location, current);
        self.forward_pointer_location(
            constrain_pointer_location(location, bounds),
            event.time_ns,
            None,
        );
    }

    fn forward_pointer_location(
        &mut self,
        proposed_location: LogicalPoint<f64>,
        time_ns: u64,
        relative: Option<RelativeMotionEvent>,
    ) {
        let Some(previous_location) = self.input_seat.pointer_location() else {
            return;
        };
        let time = (time_ns / 1_000_000) as u32;
        let serial = next_serial();
        let constraint = self
            .protocol_globals
            .pointer_constraints
            .constrain_motion(previous_location, proposed_location);
        let planned_location = match constraint {
            ConstraintMotion::Free(location)
            | ConstraintMotion::Confined(location)
            | ConstraintMotion::Locked(location) => location,
        };
        if self.protocol_globals.selection.dnd_active() {
            self.input_seat.set_pointer_location(planned_location);
            self.move_active_xdg_toplevel_drag(planned_location);
            let excluded = self.active_xdg_toplevel_drag_surface();
            let target = self
                .dnd_pointer_focus_under(planned_location, excluded.as_ref())
                .map(|(surface, origin)| {
                    let scale = surface
                        .client()
                        .map(|client| self.client_scale(&client))
                        .unwrap_or(1.0);
                    (
                        surface,
                        (
                            (planned_location.x - origin.x) * scale,
                            (planned_location.y - origin.y) * scale,
                        ),
                    )
                });
            self.protocol_globals
                .selection
                .dnd_motion(target, serial, time);
            let _ = self.push_event(
                tensor_event::Sample::pointer_motion(
                    planned_location.x,
                    planned_location.y,
                    time_ns,
                )
                .into_event(),
            );
            self.refresh_cursor_surface_outputs();
            self.refresh_dnd_icon_outputs();
            self.request_cursor_redraw_between(0, previous_location, planned_location);
            return;
        }
        self.reconcile_popup_grab(serial);
        let hit = (!matches!(constraint, ConstraintMotion::Locked(_)))
            .then(|| self.pointer_focus_under(planned_location))
            .flatten();
        let focus = if let Some(grab) = self.popup_grab.as_ref() {
            hit.filter(|(surface, _)| grab.allows(surface))
        } else if self.input_seat.pointer_is_grabbed() {
            self.input_seat
                .pointer_grab_start()
                .and_then(|start| start.focus.clone().map(|surface| (surface, start.origin)))
        } else {
            hit
        };
        let focus_identity = focus
            .as_ref()
            .map(|(surface, origin)| (surface.id(), *origin));
        let confined_target_matches = self
            .protocol_globals
            .pointer_constraints
            .active_matches(focus_identity.as_ref().map(|(surface, _)| surface));
        let emit_motion = match constraint {
            ConstraintMotion::Free(_) => true,
            ConstraintMotion::Confined(_) => {
                confined_target_matches && planned_location != previous_location
            }
            ConstraintMotion::Locked(_) => false,
        };
        let location = if emit_motion {
            self.deliver_pointer_motion(focus, planned_location, serial, time);
            planned_location
        } else {
            previous_location
        };
        let current_focus = self.input_seat.pointer_focus_owned();
        self.protocol_globals
            .activation
            .sync_pointer_focus(current_focus.as_ref());
        if emit_motion {
            let constraint_focus = current_focus.as_ref().and_then(|surface| {
                focus_identity
                    .as_ref()
                    .filter(|(id, _)| *id == surface.id())
                    .map(|(_, origin)| (surface, *origin))
            });
            let warp = self
                .protocol_globals
                .pointer_constraints
                .focus_changed(constraint_focus, location);
            self.apply_pointer_constraint_hint(warp);
        }
        self.protocol_globals
            .pointer_gestures
            .focus_changed(current_focus.as_ref(), serial, time);
        if let Some(event) = relative
            && let Some(client) = current_focus.as_ref().and_then(Resource::client)
        {
            let client_scale = self.client_scale(&client);
            self.protocol_globals
                .relative_pointer
                .motion(&client.id(), client_scale, event);
        }
        if emit_motion {
            self.protocol_globals.seat.pointer_frame();
        }
        // Value bus: coalesce motion samples for the event layer (device Hz
        // must not expand the queue). Seat path already applied the sample.
        let _ = self.push_event(
            tensor_event::Sample::pointer_motion(location.x, location.y, time_ns).into_event(),
        );
        self.refresh_cursor_surface_outputs();
        self.refresh_dnd_icon_outputs();
        // The cursor is a compositor-owned overlay, so pointer motion must
        // request a presentation even when no client surface changed. Target
        // only the head under the pointer so dual high-refresh outputs do not
        // both resubmit on every relative move. Immediate redraw keeps pointer
        // latency off the idle-turn path; bus coalescing still records intent.
        self.request_cursor_redraw_between(0, previous_location, location);
    }

    pub(crate) fn forward_pointer_gesture(&mut self, event: PointerGestureEvent) {
        let target = if matches!(
            event,
            PointerGestureEvent::SwipeBegin { .. }
                | PointerGestureEvent::PinchBegin { .. }
                | PointerGestureEvent::HoldBegin { .. }
        ) {
            self.input_seat.pointer_focus_owned().and_then(|surface| {
                let client = surface.client()?;
                let client_scale = self.client_scale(&client);
                Some((surface, client_scale))
            })
        } else {
            None
        };
        self.protocol_globals.pointer_gestures.event(
            target.as_ref().map(|(surface, scale)| (surface, *scale)),
            event,
        );
    }

    /// Match Niri's relative-pointer behavior: movement can cross directly
    /// into a neighboring output, but motion through a gap is clipped to the
    /// current output. This keeps a compositor-owned cursor renderable on a
    /// real output at all times.
    pub(super) fn relative_pointer_location(
        &self,
        previous: LogicalPoint<f64>,
        delta: LogicalPoint<f64>,
    ) -> Option<LogicalPoint<f64>> {
        let proposed = previous + sanitize_relative_pointer_delta(delta);
        if self.space.output_under(proposed).next().is_some() {
            return Some(proposed);
        }
        if let Some(output) = self.space.output_under(previous).next()
            && let Some(bounds) = self.space.output_geometry(output)
        {
            return Some(constrain_pointer_location(proposed, bounds));
        }
        self.initial_pointer_location()
    }

    /// A reset seat or an output-layout change can leave the pointer outside
    /// every live output. Niri restores it to an actual output rather than a
    /// bounding-box gap. Tensor picks the top-left output deterministically so
    /// hotplug order cannot influence the next pointer event.
    fn initial_pointer_location(&self) -> Option<LogicalPoint<f64>> {
        self.space
            .outputs()
            .filter_map(|output| self.space.output_geometry(output))
            .filter(|geometry| geometry.size.w > 0 && geometry.size.h > 0)
            .min_by_key(|geometry| {
                (
                    geometry.loc.x,
                    geometry.loc.y,
                    geometry.size.w,
                    geometry.size.h,
                )
            })
            .map(center_pointer_location)
    }

    /// Absolute devices are described in one compositor-wide coordinate
    /// rectangle. This is the union of every mapped Tensor output, so tablet
    /// and remote-pointer events do not depend on HashMap iteration order or
    /// an independently selected renderer device.
    pub(crate) fn pointer_coordinate_space(&self) -> Option<LogicalRect<i32>> {
        self.space
            .outputs()
            .filter_map(|output| self.space.output_geometry(output))
            .filter(|geometry| geometry.size.w > 0 && geometry.size.h > 0)
            .reduce(LogicalRect::union)
    }

    pub(crate) fn forward_pointer_button(&mut self, event: PointerButtonEvent) {
        let Some(location) = self.input_seat.pointer_location() else {
            return;
        };
        let serial = next_serial();
        if self.protocol_globals.selection.dnd_active() {
            let transitioned = self
                .input_seat
                .set_button(event.button, event.pressed, serial);
            if transitioned && !event.pressed && !self.input_seat.pointer_is_grabbed() {
                self.finish_selection_dnd();
            }
            let _ = self.push_event(event.sample().into_event());
            return;
        }
        self.reconcile_popup_grab(serial);
        if event.pressed && self.popup_grab.is_some() && self.input_seat.pointer_focus().is_none() {
            self.dismiss_popup_grab(serial, true);
            if let Some(focus) = self.pointer_focus_under(location) {
                self.deliver_pointer_motion(focus.into(), location, serial, event.time_msec());
                self.protocol_globals.seat.pointer_frame();
            }
        }
        let grabbed = self.input_seat.pointer_is_grabbed();
        if event.pressed && !grabbed {
            self.focus_window_at(location, serial);
        }
        if event.pressed && !grabbed {
            self.protocol_globals
                .activation
                .note_pointer_interaction(serial.into());
        }
        if self
            .input_seat
            .set_button(event.button, event.pressed, serial)
        {
            self.protocol_globals.seat.pointer_button(
                serial,
                event.time_msec(),
                event.button,
                event.pressed,
            );
            self.protocol_globals.seat.pointer_frame();
        }
        let _ = self.push_event(event.sample().into_event());
    }

    pub(crate) fn forward_pointer_axis(&mut self, event: PointerAxisEvent) {
        if !self.input_seat.pointer_enabled() {
            return;
        }
        let scale = self
            .input_seat
            .pointer_focus()
            .and_then(Resource::client)
            .map(|client| self.client_scale(&client))
            .unwrap_or(1.0);
        self.protocol_globals.seat.pointer_axis(event, scale);
        self.protocol_globals.seat.pointer_frame();
        if let Some(sample) = event.sample() {
            let _ = self.push_event(sample.into_event());
        }
    }

    /// Resolve pointer input in compositor logical coordinates. Overlay and
    /// top layer surfaces sit above windows; bottom/background sit below.
    /// XWayland surfaces remain ordinary Wayland pointer targets.
    pub(in crate::protocol) fn pointer_focus_under(
        &self,
        location: LogicalPoint<f64>,
    ) -> Option<(WlSurface, LogicalPoint<f64>)> {
        self.layer_or_window_pointer_focus(location)
    }

    /// Focus the keyboard-capable target under the pointer: layer shells with
    /// exclusive/on-demand interactivity, otherwise the toplevel/root window.
    fn focus_window_at(&mut self, location: LogicalPoint<f64>, serial: Serial) {
        if self.popup_grab.is_some() {
            return;
        }
        self.focus_at_pointer(location, serial);
    }

    fn reconcile_popup_grab(&mut self, serial: Serial) {
        if self
            .popup_grab
            .as_ref()
            .is_some_and(|grab| grab.has_ended())
        {
            self.dismiss_popup_grab(serial, true);
        }
    }

    pub(crate) fn dismiss_popup_grab(&mut self, serial: Serial, restore_focus: bool) {
        let Some(grab) = self.popup_grab.take() else {
            return;
        };
        let mut restore = grab.current_grab();
        if let Some((tree_root, dismissed)) = grab.ungrab() {
            if let Some(popup) = self.popups.find_popup(&dismissed) {
                let _ = self.popups.dismiss_popup(&tree_root, &popup);
            }
            restore = tree_root;
        }
        if restore_focus && restore.is_alive() {
            self.set_keyboard_focus(Some(restore), serial);
        }
    }

    fn deliver_pointer_motion(
        &mut self,
        focus: Option<(WlSurface, LogicalPoint<f64>)>,
        location: LogicalPoint<f64>,
        serial: Serial,
        time: u32,
    ) {
        self.input_seat.set_pointer_location(location);
        let focus = focus.filter(|(surface, _)| surface.is_alive());
        let old = self.input_seat.pointer_focus_owned();
        let changed =
            old.as_ref().map(Resource::id) != focus.as_ref().map(|(surface, _)| surface.id());
        if changed {
            if let Some(old) = old {
                self.protocol_globals.seat.pointer_leave(&old, serial);
            }
            self.input_seat.replace_pointer_focus(focus.clone());
            if let Some((surface, origin)) = focus {
                let scale = surface
                    .client()
                    .map(|client| self.client_scale(&client))
                    .unwrap_or(1.0);
                self.protocol_globals.seat.pointer_enter(
                    &surface,
                    serial,
                    (location.x - origin.x, location.y - origin.y),
                    scale,
                );
            }
            return;
        }
        let Some((surface, origin)) = focus else {
            return;
        };
        self.input_seat.update_pointer_origin(origin);
        let scale = surface
            .client()
            .map(|client| self.client_scale(&client))
            .unwrap_or(1.0);
        self.protocol_globals.seat.pointer_motion(
            time,
            (location.x - origin.x, location.y - origin.y),
            scale,
        );
    }

    pub(crate) fn clear_pointer_focus(&mut self, serial: Serial, time: u32) {
        let Some(location) = self.input_seat.pointer_location() else {
            return;
        };
        self.input_seat.clear_pointer_grab();
        self.deliver_pointer_motion(None, location, serial, time);
        self.protocol_globals.seat.pointer_frame();
        self.protocol_globals.activation.sync_pointer_focus(None);
        self.protocol_globals
            .pointer_gestures
            .focus_changed(None, serial, time);
        let _ = self
            .protocol_globals
            .pointer_constraints
            .focus_changed(None, location);
    }
}
