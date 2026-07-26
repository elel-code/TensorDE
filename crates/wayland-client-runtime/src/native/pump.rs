//! Compio-driven read/dispatch pump for the native backend.
//!
//! Implements the non-blocking Wayland client pattern:
//! flush → prepare_read → (await readable) → read → dispatch_pending.

use wayland_client::backend::WaylandError;

use super::connection::{NativeConnection, NativeError};
use super::registry::NativeRegistry;

/// One iteration result of the native pump.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PumpStep {
    /// Events dispatched into the registry (and future shell) state.
    pub dispatched: usize,
    /// Whether a socket read was performed this step.
    pub did_read: bool,
}

/// Owns connection + registry and advances protocol I/O without calloop/SCTK.
pub struct NativePump {
    connection: NativeConnection,
    registry: NativeRegistry,
}

impl NativePump {
    pub fn connect_to_env() -> Result<Self, NativeError> {
        let connection = NativeConnection::connect_to_env()?;
        let registry = NativeRegistry::bootstrap(&connection)?;
        Ok(Self {
            connection,
            registry,
        })
    }

    pub fn connection(&self) -> &NativeConnection {
        &self.connection
    }

    pub fn registry(&self) -> &NativeRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut NativeRegistry {
        &mut self.registry
    }

    /// Flush, wait for display data if needed, read, and dispatch pending.
    ///
    /// Must run on a Compio executor. This is the async shape future Fika main
    /// loops will use once shell objects move off SCTK.
    pub async fn pump_once(&mut self) -> Result<PumpStep, NativeError> {
        self.connection.flush()?;

        // Dispatch anything already queued without blocking.
        let mut dispatched = self.registry.dispatch_pending()?;

        let mut did_read = false;
        match self.connection.connection().prepare_read() {
            None => {
                // Another thread/read path already filled the queue.
                dispatched += self.registry.dispatch_pending()?;
            }
            Some(guard) => {
                self.connection.wait_readable().await?;
                match guard.read() {
                    Ok(_) => {
                        did_read = true;
                    }
                    Err(WaylandError::Io(err))
                        if err.kind() == std::io::ErrorKind::WouldBlock =>
                    {
                        // Spurious wake; fall through to dispatch_pending.
                    }
                    Err(error) => return Err(error.into()),
                }
                dispatched += self.registry.dispatch_pending()?;
            }
        }

        Ok(PumpStep {
            dispatched,
            did_read,
        })
    }

    /// Non-blocking: flush + dispatch only what is already available.
    pub fn pump_pending(&mut self) -> Result<usize, NativeError> {
        self.connection.flush()?;
        self.registry.dispatch_pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pump_connect_lists_core_globals_when_compositor_present() {
        let Ok(mut pump) = NativePump::connect_to_env() else {
            // CI / headless: no Wayland display is fine.
            return;
        };
        let ads = pump.registry().advertisements();
        assert!(
            ads.iter().any(|g| g.interface == "wl_compositor"),
            "expected wl_compositor in {ads:?}"
        );
        assert!(
            ads.iter().any(|g| g.interface == "wl_shm"),
            "expected wl_shm in {ads:?}"
        );

        // Non-blocking pending pump must not hang.
        let _ = pump.pump_pending().expect("pump_pending");

        compio::runtime::Runtime::new()
            .expect("compio")
            .block_on(async {
                // Single async step; may or may not read depending on compositor.
                let step = pump.pump_once().await.expect("pump_once");
                let _ = step;
            });
    }
}
