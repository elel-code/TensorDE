use std::sync::Arc;

use smithay::wayland::compositor::CompositorClientState;
use wayland_server::backend::{ClientData, ClientId, DisconnectReason};

#[derive(Debug, Default)]
pub(crate) struct WaylandClientState {
    pub(crate) compositor_state: CompositorClientState,
    /// Immutable sandbox identity for clients accepted through `wp_security_context`.
    pub(crate) security_context: Option<Arc<tensor_protocol::SecurityContextMetadata>>,
}

impl ClientData for WaylandClientState {
    fn initialized(&self, _client_id: ClientId) {}

    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
