// Shared types live in `runtime_common` so native builds need no SCTK.
pub use crate::runtime_common::{
    RuntimeCapabilities, RuntimeError, RuntimeOptions, WakeHandle,
};

impl From<GlobalError> for RuntimeError {
    fn from(error: GlobalError) -> Self {
        Self::Protocol(error.to_string())
    }
}

/// The Wayland connection, protocol object graph, calloop dispatcher and owned event queue.
///
/// Display-fd readiness is also tracked via Compio ([`DisplayReadiness`]) so
/// callers can `await` readability without calloop. Protocol dispatch still
/// uses calloop until the native backend replaces SCTK handlers.
pub struct Runtime {
    connection: Connection,
    queue_handle: QueueHandle<RuntimeState>,
    event_loop: CalloopEventLoop<'static, RuntimeState>,
    state: RuntimeState,
    wake: WakeHandle,
    capabilities: RuntimeCapabilities,
    /// Compio poll on a dup of the display fd (does not own the connection).
    display_readiness: crate::DisplayReadiness,
}

