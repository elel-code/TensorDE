use wayland_server::Display;

use super::RuntimeState;

impl RuntimeState {
    pub(crate) fn display(&self) -> &Display<Self> {
        self.display
            .as_ref()
            .expect("Wayland display must remain installed")
    }

    pub(crate) fn dispatch_wayland_clients(&mut self) -> std::io::Result<usize> {
        let mut display = self
            .display
            .take()
            .expect("Wayland display must remain installed");
        let result = display.dispatch_clients(self);
        self.display = Some(display);
        result
    }
}
