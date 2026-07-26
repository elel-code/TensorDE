//! Protocol pump helpers.
//!
//! The core path is **executor-agnostic**: flush + read + dispatch.
//! Async waiting on the display fd lives behind `feature = "compio"`.

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

/// Owns connection + registry and advances protocol I/O.
///
/// Without the `compio` feature, use [`Self::pump_pending`] and register
/// [`NativeConnection::as_fd`] in your own event loop.
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

    /// Non-blocking: flush + dispatch only what is already available.
    pub fn pump_pending(&mut self) -> Result<usize, NativeError> {
        self.connection.flush()?;
        self.registry.dispatch_pending()
    }

    /// Non-blocking read attempt + dispatch (for external readiness loops).
    pub fn try_read_and_dispatch(&mut self) -> Result<PumpStep, NativeError> {
        self.connection.flush()?;
        let mut dispatched = self.registry.dispatch_pending()?;
        let mut did_read = false;
        match self.connection.connection().prepare_read() {
            None => {
                dispatched += self.registry.dispatch_pending()?;
            }
            Some(guard) => match guard.read() {
                Ok(_) => {
                    did_read = true;
                    dispatched += self.registry.dispatch_pending()?;
                }
                Err(WaylandError::Io(err)) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.into()),
            },
        }
        Ok(PumpStep {
            dispatched,
            did_read,
        })
    }

    /// Flush, wait for display data if needed, read, and dispatch pending.
    ///
    /// Must run on a Compio executor (io_uring completion waits).
    #[cfg(feature = "compio")]
    pub async fn pump_once(&mut self) -> Result<PumpStep, NativeError> {
        use crate::display_io::DisplayReadiness;

        self.connection.flush()?;
        let mut dispatched = self.registry.dispatch_pending()?;
        let mut did_read = false;
        match self.connection.connection().prepare_read() {
            None => {
                dispatched += self.registry.dispatch_pending()?;
            }
            Some(guard) => {
                let readiness = DisplayReadiness::from_as_fd(self.connection.as_fd())?;
                readiness.wait_readable().await?;
                match guard.read() {
                    Ok(_) => {
                        did_read = true;
                    }
                    Err(WaylandError::Io(err))
                        if err.kind() == std::io::ErrorKind::WouldBlock => {}
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pump_connect_lists_core_globals_when_compositor_present() {
        let Ok(mut pump) = NativePump::connect_to_env() else {
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
        let _ = pump.pump_pending().expect("pump_pending");
        let _ = pump.try_read_and_dispatch().expect("try_read");
    }
}
