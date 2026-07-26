mod adapter;
mod tablet;
#[cfg(feature = "tty")]
mod virtual_pointer;

mod pointer_geometry;
#[cfg(test)]
mod tests;

use pointer_geometry::{
    center_pointer_location, replace_non_finite_pointer_location, sanitize_relative_pointer_delta,
    virtual_terminal_for_keysym, workspace_index_for_keysym,
};

pub(crate) use pointer_geometry::constrain_pointer_location;

use smithay::{
    backend::{
        input::{
            AbsolutePositionEvent, ButtonState, Device, DeviceCapability, Event as InputEventTrait,
            InputEvent, KeyState, KeyboardKeyEvent, PointerButtonEvent, PointerMotionEvent,
        },
        libinput::LibinputInputBackend,
    },
    input::{
        keyboard::{FilterResult, keysyms},
        pointer::{ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
    wayland::seat::WaylandFocus,
};
use tracing::{debug, warn};

use super::{
    focus::KeyboardFocusTarget,
    state::{InputDeviceCapabilities, RuntimeState},
};

impl RuntimeState {
    pub(crate) fn process_input_event(&mut self, event: InputEvent<LibinputInputBackend>) {
        // Session lock captures the seat: only VT recovery remains compositor-owned.
        if self.session_is_locked() {
            if let InputEvent::Keyboard { event } = &event {
                let key_state = event.state();
                if let Some(keyboard) = self.seat.get_keyboard() {
                    keyboard.input::<(), _>(
                        self,
                        event.key_code(),
                        event.state(),
                        SERIAL_COUNTER.next_serial(),
                        event.time_msec(),
                        move |state, _, handle| {
                            let Some(vt) = virtual_terminal_for_keysym(handle.modified_sym().raw())
                            else {
                                return FilterResult::Intercept(());
                            };
                            if key_state == KeyState::Pressed {
                                state.request_virtual_terminal(vt);
                            }
                            FilterResult::Intercept(())
                        },
                    );
                }
            }
            return;
        }
        let activity = matches!(
            event,
            InputEvent::Keyboard { .. }
                | InputEvent::PointerMotion { .. }
                | InputEvent::PointerMotionAbsolute { .. }
                | InputEvent::PointerButton { .. }
                | InputEvent::PointerAxis { .. }
                | InputEvent::TouchDown { .. }
                | InputEvent::TouchMotion { .. }
                | InputEvent::TouchUp { .. }
        );
        match event {
            InputEvent::DeviceAdded { ref device } => {
                let capabilities = InputDeviceCapabilities {
                    keyboard: Device::has_capability(device, DeviceCapability::Keyboard),
                    pointer: Device::has_capability(device, DeviceCapability::Pointer),
                    touch: Device::has_capability(device, DeviceCapability::Touch),
                    tablet: Device::has_capability(device, DeviceCapability::TabletTool),
                };
                self.input_devices.insert(device.id(), capabilities);
                self.reconcile_seat_capabilities();
                if capabilities.tablet {
                    self.process_tablet_event(event);
                }
            }
            InputEvent::DeviceRemoved { ref device } => {
                if Device::has_capability(device, DeviceCapability::TabletTool) {
                    self.process_tablet_event(InputEvent::DeviceRemoved {
                        device: device.clone(),
                    });
                }
                self.input_devices.remove(&device.id());
                self.reconcile_seat_capabilities();
            }
            InputEvent::Keyboard { event } => self.forward_keyboard(event),
            InputEvent::PointerMotion { event } => self.forward_pointer_motion(event),
            InputEvent::PointerMotionAbsolute { event } => {
                self.forward_pointer_motion_absolute(event)
            }
            InputEvent::PointerButton { event } => self.forward_pointer_button(event),
            InputEvent::PointerAxis { event } => self.forward_pointer_axis(event),
            InputEvent::TabletToolAxis { .. }
            | InputEvent::TabletToolProximity { .. }
            | InputEvent::TabletToolTip { .. }
            | InputEvent::TabletToolButton { .. } => self.process_tablet_event(event),
            _ => {}
        }
        if activity {
            self.protocol_globals
                .idle_notifier()
                .notify_activity(&self.seat);
            self.refresh_idle_inhibition();
        }
    }

    /// Publish the aggregate libinput capabilities on the single Wayland
    /// seat. A keyboard can arrive after an application has mapped, so a
    /// successful keyboard creation must also restore the compositor-selected
    /// focus to the new Smithay keyboard handle.
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

        if keyboard_count > 0 && self.seat.get_keyboard().is_none() {
            match self.seat.add_keyboard(Default::default(), 200, 25) {
                Ok(_) => self.restore_keyboard_focus(),
                Err(error) => warn!(%error, "failed to publish keyboard capability"),
            }
        } else if keyboard_count == 0 && self.seat.get_keyboard().is_some() {
            self.seat.remove_keyboard();
        }

        if pointer_count > 0 && self.seat.get_pointer().is_none() {
            self.seat.add_pointer();
            // A pointer can be discovered after the first output frame. Draw
            // the default software cursor immediately instead of waiting for
            // the user's first motion event to make it visible.
            self.request_redraw_all();
        } else if pointer_count == 0 && self.seat.get_pointer().is_some() {
            self.seat.remove_pointer();
            // The next frame has no overlay, which damages the previous
            // cursor bounds and clears the last visible arrow.
            self.request_redraw_all();
        }

        debug!(
            keyboard_count,
            pointer_count, touch_count, "libinput seat capabilities reconciled"
        );
    }

    fn forward_keyboard(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::KeyboardKeyEvent,
    ) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let key_state = event.state();
        if key_state == KeyState::Pressed && self.cursor.note_keyboard_activity() {
            // Typing hid the software cursor; repaint the pointer head.
            if let Some(pointer) = self.seat.get_pointer() {
                self.request_redraw_at(pointer.current_location());
            } else {
                self.request_redraw_all();
            }
        }
        keyboard.input::<(), _>(
            self,
            event.key_code(),
            event.state(),
            SERIAL_COUNTER.next_serial(),
            event.time_msec(),
            move |state, modifiers, handle| {
                let keysym = handle.modified_sym().raw();
                if let Some(vt) = virtual_terminal_for_keysym(keysym) {
                    if key_state == KeyState::Pressed {
                        state.request_virtual_terminal(vt);
                    }
                    // A VT switch can prevent a key release from reaching us.
                    return FilterResult::Intercept(());
                }
                // Super+digit → workspace 1..9; Super+Shift+digit moves focused view
                // and follows; Super+Page_Up/Down cycles.
                if key_state == KeyState::Pressed && modifiers.logo {
                    if let Some(index) = workspace_index_for_keysym(keysym) {
                        if modifiers.shift {
                            if let Some(view) = state.world.focused_view(state.active_workspace()) {
                                let _ = state.move_view_to_workspace(
                                    view,
                                    crate::ecs::WorkspaceId::new(index),
                                );
                                let _ = state.activate_workspace_index(index);
                            }
                        } else {
                            let _ = state.activate_workspace_index(index);
                        }
                        return FilterResult::Intercept(());
                    }
                    if keysym == keysyms::KEY_Page_Down || keysym == keysyms::KEY_Right {
                        let _ = state.cycle_workspace(1);
                        return FilterResult::Intercept(());
                    }
                    if keysym == keysyms::KEY_Page_Up || keysym == keysyms::KEY_Left {
                        let _ = state.cycle_workspace(-1);
                        return FilterResult::Intercept(());
                    }
                }
                FilterResult::Forward
            },
        );
        // Value bus: keycode-level sample (not keysym — keymap stays seat-side).
        self.push_key_sample(adapter::key_sample(&event));
    }

    fn request_virtual_terminal(&mut self, vt: i32) {
        let Some(backend) = self.backend.as_mut() else {
            warn!(vt, "ignored virtual terminal request without a tty backend");
            return;
        };
        backend.change_vt(vt);
    }

    fn forward_pointer_motion(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::PointerMotionEvent,
    ) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let location = self.relative_pointer_location(pointer.current_location(), event.delta());
        let Some(location) = location else {
            return;
        };
        self.forward_pointer_location(location, event.time_msec());
    }

    /// Follow the same absolute-coordinate conversion used by Niri: libinput
    /// maps the device into the compositor's logical output bounds, then the
    /// seat receives the resulting global location and a redraw is queued.
    fn forward_pointer_motion_absolute(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::PointerMotionAbsoluteEvent,
    ) {
        let Some(bounds) = self.pointer_coordinate_space() else {
            return;
        };
        let current = self
            .seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
            .unwrap_or_else(|| center_pointer_location(bounds));
        let location = event.position_transformed(bounds.size) + bounds.loc.to_f64();
        let location = replace_non_finite_pointer_location(location, current);
        self.forward_pointer_location(
            constrain_pointer_location(location, bounds),
            event.time_msec(),
        );
    }

    fn forward_pointer_location(&mut self, location: Point<f64, Logical>, time: u32) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let _ = self.cursor.note_pointer_activity();
        let focus = self.pointer_focus_under(location);
        pointer.motion(
            self,
            focus,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
        self.maybe_activate_pointer_constraint();
        // Value bus: coalesce motion samples for the event layer (device Hz
        // must not expand the queue). Seat path already applied the sample.
        // Value bus via tensor-input sample (adapter-free payload).
        let _ = self.push_event(adapter::motion_sample(location.x, location.y, time).into_event());
        // The cursor is a compositor-owned overlay, so pointer motion must
        // request a presentation even when no client surface changed. Target
        // only the head under the pointer so dual high-refresh outputs do not
        // both resubmit on every relative move. Immediate redraw keeps pointer
        // latency off the idle-turn path; bus coalescing still records intent.
        self.request_redraw_at(location);
    }

    /// Match Niri's relative-pointer behavior: movement can cross directly
    /// into a neighboring output, but motion through a gap is clipped to the
    /// current output. This keeps a compositor-owned cursor renderable on a
    /// real output at all times.
    pub(super) fn relative_pointer_location(
        &self,
        previous: Point<f64, Logical>,
        delta: Point<f64, Logical>,
    ) -> Option<Point<f64, Logical>> {
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
    fn initial_pointer_location(&self) -> Option<Point<f64, Logical>> {
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
    /// rectangle. This is the union of every mapped Smithay output, so tablet
    /// and remote-pointer events do not depend on HashMap iteration order or
    /// an independently selected renderer device.
    pub(crate) fn pointer_coordinate_space(&self) -> Option<Rectangle<i32, Logical>> {
        self.space
            .outputs()
            .filter_map(|output| self.space.output_geometry(output))
            .filter(|geometry| geometry.size.w > 0 && geometry.size.h > 0)
            .reduce(Rectangle::merge)
    }

    fn forward_pointer_button(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::PointerButtonEvent,
    ) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        if event.state() == ButtonState::Pressed && !pointer.is_grabbed() {
            self.focus_window_at(pointer.current_location(), serial);
        }
        pointer.button(
            self,
            &ButtonEvent {
                serial,
                time: event.time_msec(),
                button: event.button_code(),
                state: event.state(),
            },
        );
        pointer.frame(self);
        let _ = self.push_event(adapter::button_sample(&event).into_event());
    }

    fn forward_pointer_axis(
        &mut self,
        event: <LibinputInputBackend as smithay::backend::input::InputBackend>::PointerAxisEvent,
    ) {
        use smithay::backend::input::{Axis, PointerAxisEvent};
        use smithay::input::pointer::AxisFrame;

        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let mut frame = AxisFrame::new(event.time_msec())
            .source(event.source())
            .relative_direction(Axis::Horizontal, event.relative_direction(Axis::Horizontal))
            .relative_direction(Axis::Vertical, event.relative_direction(Axis::Vertical));
        let mut horizontal = 0.0;
        let mut vertical = 0.0;
        for axis in [Axis::Horizontal, Axis::Vertical] {
            if let Some(amount) = event.amount(axis) {
                frame = frame.value(axis, amount);
                match axis {
                    Axis::Horizontal => horizontal = amount,
                    Axis::Vertical => vertical = amount,
                }
            }
            if let Some(steps) = event.amount_v120(axis) {
                frame = frame.v120(axis, steps.round() as i32);
            }
        }
        pointer.axis(self, frame);
        if let Some(sample) =
            adapter::axis_sample_if_nonzero(horizontal, vertical, event.time_msec())
        {
            let _ = self.push_event(sample.into_event());
        }
    }

    /// Resolve pointer input in compositor logical coordinates. Overlay and
    /// top layer surfaces sit above windows; bottom/background sit below.
    /// XWayland surfaces remain ordinary Wayland pointer targets.
    fn pointer_focus_under(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.layer_or_window_pointer_focus(location)
    }

    /// Focus the keyboard-capable target under the pointer: layer shells with
    /// exclusive/on-demand interactivity, otherwise the toplevel/root window.
    fn focus_window_at(&mut self, location: Point<f64, Logical>, serial: smithay::utils::Serial) {
        self.focus_at_pointer(location, serial);
    }

    /// Reapply the ECS-selected root when a keyboard capability becomes
    /// available after its window mapped. The focus method intentionally does
    /// not reflow here: the window already has its configure, and only a
    /// `wl_keyboard.enter` is missing.
    pub(crate) fn restore_keyboard_focus(&mut self) {
        let Some(view_id) = self.world.focused_view(self.active_workspace()) else {
            return;
        };
        let Some(window) = self.mapped_window_for_view(view_id) else {
            return;
        };
        let _ = self.focus_mapped_window(window, SERIAL_COUNTER.next_serial());
    }

    pub(crate) fn focus_mapped_window(
        &mut self,
        window: smithay::desktop::Window,
        serial: smithay::utils::Serial,
    ) -> bool {
        let Some(surface) = window.wl_surface().map(std::borrow::Cow::into_owned) else {
            return false;
        };
        let Some(view_id) = self.view_for_surface(&surface) else {
            return false;
        };
        let keyboard = self.seat.get_keyboard();
        if keyboard
            .as_ref()
            .is_some_and(|keyboard| keyboard.is_grabbed())
        {
            return false;
        }

        #[cfg(feature = "xwayland")]
        let focus = window
            .x11_surface()
            .cloned()
            .map(KeyboardFocusTarget::from)
            .unwrap_or_else(|| KeyboardFocusTarget::from(surface.clone()));
        #[cfg(not(feature = "xwayland"))]
        let focus = KeyboardFocusTarget::from(surface);

        let focus_changed = !self.world.is_focused(view_id);
        // Niri and Hyprland make the state transition idempotent before they
        // notify clients. Smithay performs the same equality check internally,
        // but keeping it explicit here means a focus repair cannot reach a
        // future keyboard-grab implementation as a redundant enter/leave.
        let seat_focus_changed = keyboard
            .as_ref()
            .is_some_and(|keyboard| keyboard.current_focus().as_ref() != Some(&focus));
        if let Err(error) = self.world.focus_view(view_id) {
            warn!(%error, view_id = view_id.get(), "failed to focus mapped view");
            return false;
        }
        // Window activation supersedes on-demand layer keyboard memory.
        self.layer_shell_on_demand_focus = None;
        self.publish_window_activation(Some(&window));
        // Match Niri and Hyprland's central focus-state early return: a seat
        // focus repair must not silently reorder Smithay's hit-test space
        // after ECS intentionally kept its scene order unchanged. Only a real
        // active-view transition raises the complete attachment family.
        if focus_changed {
            self.raise_view_family_in_space(view_id, &window);
            #[cfg(feature = "xwayland")]
            self.raise_x11_popups_for_root(&surface);
            #[cfg(feature = "xwayland")]
            if let KeyboardFocusTarget::X11(x11) = &focus
                && let Some(xwm) = self.xwm.as_mut()
                && let Err(error) = xwm.raise_window(x11.as_ref())
            {
                warn!(%error, window = x11.window_id(), "failed to synchronize XWayland stacking");
            }
        }
        // XDG clients must observe their first configure before a keyboard
        // enter. The initial commit handler sends that configure and then
        // re-enters this focus path; X11 has no XDG configure gate.
        let keyboard_ready = window
            .toplevel()
            .is_none_or(|toplevel| toplevel.is_initial_configure_sent());
        if let Some(keyboard) = keyboard
            && keyboard_ready
            && seat_focus_changed
        {
            keyboard.set_focus(self, Some(focus), serial);
        }
        focus_changed
    }

    /// Keep the three focus contracts in lockstep: ECS owns the selected
    /// view, Smithay's seat owns keyboard delivery, and xdg-toplevel clients
    /// observe `Activated`. In particular, terminals such as Ghostty use the
    /// latter to decide whether their text cursor should blink.
    ///
    /// Initial xdg configure publication remains in the commit handler. A
    /// toplevel that has not made its first commit only receives this pending
    /// state there, as required by xdg-shell's initial-configure ordering.
    pub(crate) fn publish_window_activation(
        &mut self,
        focused_window: Option<&smithay::desktop::Window>,
    ) {
        let windows = self.space.elements().cloned().collect::<Vec<_>>();
        for window in windows {
            let active = focused_window.is_some_and(|focused| window == *focused);
            if !window.set_activated(active) {
                continue;
            }
            if let Some(toplevel) = window.toplevel()
                && toplevel.is_initial_configure_sent()
            {
                toplevel.send_pending_configure();
            }
        }
    }

    /// Keep Smithay's input-space stacking aligned with the ECS scene when a
    /// focused dialog is attached to a tiled owner. Rendering order is still
    /// value-only ECS state; this only ensures pointer hit-testing sees the
    /// same family above unrelated views.
    fn raise_view_family_in_space(
        &mut self,
        focused: crate::ecs::ViewId,
        focused_window: &smithay::desktop::Window,
    ) {
        let Some(root) = self.world.tiled_ancestor(focused) else {
            self.space.raise_element(focused_window, true);
            return;
        };
        let mut family = self.view_attachment_family(root);
        if focused != root {
            let focused_subtree = self.view_attachment_family(focused);
            family.retain(|view_id| !focused_subtree.contains(view_id));
            family.extend(focused_subtree);
        }
        for view_id in family {
            let window = (view_id == focused)
                .then(|| focused_window.clone())
                .or_else(|| self.mapped_window_for_view(view_id));
            if let Some(window) = window {
                self.space.raise_element(&window, view_id == focused);
            }
        }
    }

    fn view_attachment_family(&self, root: crate::ecs::ViewId) -> Vec<crate::ecs::ViewId> {
        let mut family = vec![root];
        let mut index = 0;
        while let Some(owner) = family.get(index).copied() {
            family.extend(self.world.attached_children(owner));
            index += 1;
        }
        family
    }

    pub(crate) fn mapped_window_for_view(
        &self,
        view_id: crate::ecs::ViewId,
    ) -> Option<smithay::desktop::Window> {
        self.space
            .elements()
            .find(|window| {
                window
                    .wl_surface()
                    .as_deref()
                    .and_then(|surface| self.view_for_surface(surface))
                    == Some(view_id)
            })
            .cloned()
    }
}
