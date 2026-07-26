use super::RuntimeState;

impl RuntimeState {
    #[cfg(feature = "xwayland")]
    pub(super) fn xwayland_dnd_pointer_grab_active(&self) -> bool {
        use smithay::input::pointer::ClickGrab;

        use crate::protocol::state::popup::PopupPointerGrab;

        let Some(pointer) = self.seat.get_pointer() else {
            return false;
        };
        pointer
            .with_grab(|_, grab| {
                // The current pointer-grab set is closed: Smithay installs
                // ClickGrab, Tensor installs PopupPointerGrab, and XWM is the
                // only remaining producer. Wayland DnD is rejected by the
                // current handler, so the remaining grab is the XDND bridge.
                !grab.is::<ClickGrab<Self>>() && !grab.is::<PopupPointerGrab<Self>>()
            })
            .unwrap_or(false)
    }

    #[cfg(not(feature = "xwayland"))]
    pub(super) fn xwayland_dnd_pointer_grab_active(&self) -> bool {
        false
    }
}
