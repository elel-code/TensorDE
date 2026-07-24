use smithay::{
    backend::{
        input::{
            ButtonState, Device, DeviceCapability, Event as InputEventTrait, InputEvent, KeyState,
            KeyboardKeyEvent, PointerButtonEvent, PointerMotionEvent,
        },
        libinput::LibinputInputBackend,
    },
    desktop::WindowSurfaceType,
    input::{
        keyboard::{FilterResult, keysyms},
        pointer::{ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, SERIAL_COUNTER},
    wayland::seat::WaylandFocus,
};
use tracing::{debug, warn};

use super::{
    focus::KeyboardFocusTarget,
    state::{DEFAULT_WORKSPACE, InputDeviceCapabilities, RuntimeState},
};

impl RuntimeState {
    pub(crate) fn process_input_event(&mut self, event: InputEvent<LibinputInputBackend>) {
        match event {
            InputEvent::DeviceAdded { device } => {
                let capabilities = InputDeviceCapabilities {
                    keyboard: Device::has_capability(&device, DeviceCapability::Keyboard),
                    pointer: Device::has_capability(&device, DeviceCapability::Pointer),
                    touch: Device::has_capability(&device, DeviceCapability::Touch),
                };
                self.input_devices.insert(device.id(), capabilities);
                self.reconcile_seat_capabilities();
            }
            InputEvent::DeviceRemoved { device } => {
                self.input_devices.remove(&device.id());
                self.reconcile_seat_capabilities();
            }
            InputEvent::Keyboard { event } => self.forward_keyboard(event),
            InputEvent::PointerMotion { event } => self.forward_pointer_motion(event),
            InputEvent::PointerButton { event } => self.forward_pointer_button(event),
            InputEvent::PointerAxis { event } => self.forward_pointer_axis(event),
            _ => {}
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
            self.submit_default_workspace_frame();
        } else if pointer_count == 0 && self.seat.get_pointer().is_some() {
            self.seat.remove_pointer();
            // The next frame has no overlay, which damages the previous
            // cursor bounds and clears the last visible arrow.
            self.submit_default_workspace_frame();
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
        keyboard.input::<(), _>(
            self,
            event.key_code(),
            event.state(),
            SERIAL_COUNTER.next_serial(),
            event.time_msec(),
            move |state, _, handle| {
                let Some(vt) = virtual_terminal_for_keysym(handle.modified_sym().raw()) else {
                    return FilterResult::Forward;
                };

                if key_state == KeyState::Pressed {
                    state.request_virtual_terminal(vt);
                }
                // A VT switch can prevent a key release from reaching us. Keep
                // both sides compositor-owned so a client never sees a lone
                // release for this non-inhibitable recovery action.
                FilterResult::Intercept(())
            },
        );
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
        let location = pointer.current_location() + event.delta();
        let focus = self.pointer_focus_under(location);
        pointer.motion(
            self,
            focus,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: event.time_msec(),
            },
        );
        pointer.frame(self);
        // The cursor is a compositor-owned overlay, so pointer motion must
        // request a presentation even when no client surface changed.
        self.submit_default_workspace_frame();
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
        for axis in [Axis::Horizontal, Axis::Vertical] {
            if let Some(amount) = event.amount(axis) {
                frame = frame.value(axis, amount);
            }
            if let Some(steps) = event.amount_v120(axis) {
                frame = frame.v120(axis, steps.round() as i32);
            }
        }
        pointer.axis(self, frame);
    }

    /// Resolve pointer input in compositor logical coordinates. XWayland
    /// surfaces remain ordinary Wayland pointer targets; their X11 focus is
    /// handled separately when a root window is activated.
    fn pointer_focus_under(
        &self,
        location: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let (window, window_location) = self.space.element_under(location)?;
        window
            .surface_under(location - window_location.to_f64(), WindowSurfaceType::ALL)
            .map(|(surface, surface_location)| {
                (surface, (surface_location + window_location).to_f64())
            })
    }

    /// Focus the toplevel/root rather than a subsurface or popup. This keeps
    /// client popup lifetimes stable and lets X11 windows run their ICCCM
    /// focus handshake through `X11Surface`.
    fn focus_window_at(&mut self, location: Point<f64, Logical>, serial: smithay::utils::Serial) {
        let Some((window, _)) = self
            .space
            .element_under(location)
            .map(|(window, location)| (window.clone(), location))
        else {
            return;
        };
        let Some(surface) = window.wl_surface().map(std::borrow::Cow::into_owned) else {
            return;
        };
        let Some(root) = self.owning_view_root(&surface) else {
            return;
        };
        let Some(root_window) = self
            .space
            .elements()
            .find(|candidate| candidate.wl_surface().as_deref() == Some(&root))
            .cloned()
        else {
            return;
        };
        self.focus_mapped_window(root_window, serial);
        self.reflow_default_workspace();
    }

    /// Reapply the ECS-selected root when a keyboard capability becomes
    /// available after its window mapped. The focus method intentionally does
    /// not reflow here: the window already has its configure, and only a
    /// `wl_keyboard.enter` is missing.
    fn restore_keyboard_focus(&mut self) {
        let Some(view_id) = self.world.focused_view(DEFAULT_WORKSPACE) else {
            return;
        };
        let Some(window) = self.mapped_window_for_view(view_id) else {
            return;
        };
        self.focus_mapped_window(window, SERIAL_COUNTER.next_serial());
    }

    pub(crate) fn focus_mapped_window(
        &mut self,
        window: smithay::desktop::Window,
        serial: smithay::utils::Serial,
    ) {
        let Some(surface) = window.wl_surface().map(std::borrow::Cow::into_owned) else {
            return;
        };
        let Some(view_id) = self.view_for_surface(&surface) else {
            return;
        };
        let keyboard = self.seat.get_keyboard();
        if keyboard
            .as_ref()
            .is_some_and(|keyboard| keyboard.is_grabbed())
        {
            return;
        }

        #[cfg(feature = "xwayland")]
        let focus = window
            .x11_surface()
            .cloned()
            .map(KeyboardFocusTarget::from)
            .unwrap_or_else(|| KeyboardFocusTarget::from(surface.clone()));
        #[cfg(not(feature = "xwayland"))]
        let focus = KeyboardFocusTarget::from(surface);

        if let Err(error) = self.world.focus_view(view_id) {
            warn!(%error, view_id = view_id.get(), "failed to focus mapped view");
        }
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
        if let Some(keyboard) = keyboard {
            keyboard.set_focus(self, Some(focus), serial);
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

    fn mapped_window_for_view(
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

fn virtual_terminal_for_keysym(keysym: u32) -> Option<i32> {
    (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12)
        .contains(&keysym)
        .then(|| (keysym - keysyms::KEY_XF86Switch_VT_1 + 1) as i32)
}

#[cfg(test)]
mod tests {
    use smithay::input::keyboard::keysyms;

    use super::virtual_terminal_for_keysym;

    #[test]
    fn virtual_terminal_recovery_keys_are_complete_and_bounded() {
        for vt in 1..=12 {
            let keysym = keysyms::KEY_XF86Switch_VT_1 + (vt - 1);
            assert_eq!(virtual_terminal_for_keysym(keysym), Some(vt as i32));
        }
        assert_eq!(
            virtual_terminal_for_keysym(keysyms::KEY_XF86Switch_VT_1 - 1),
            None
        );
        assert_eq!(
            virtual_terminal_for_keysym(keysyms::KEY_XF86Switch_VT_12 + 1),
            None
        );
    }
}
