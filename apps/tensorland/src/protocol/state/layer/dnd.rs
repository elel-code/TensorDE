use super::RuntimeState;

impl RuntimeState {
    #[cfg(feature = "xwayland")]
    pub(super) fn xwayland_dnd_pointer_grab_active(&self) -> bool {
        self.protocol_globals.selection.dnd_active()
    }

    #[cfg(not(feature = "xwayland"))]
    pub(super) fn xwayland_dnd_pointer_grab_active(&self) -> bool {
        false
    }
}
