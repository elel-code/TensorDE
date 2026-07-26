//! Registry snapshot without SCTK.
//!
//! Uses `wayland_client::globals::registry_queue_init` with a minimal dispatch
//! state so global discovery does not pull in SeatState/XdgShell/etc.

use wayland_client::globals::{Global, GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};

use super::connection::{NativeConnection, NativeError};

/// One compositor-advertised global (name / interface / version).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalAdvertisement {
    pub name: u32,
    pub interface: String,
    pub version: u32,
}

impl From<&Global> for GlobalAdvertisement {
    fn from(global: &Global) -> Self {
        Self {
            name: global.name,
            interface: global.interface.clone(),
            version: global.version,
        }
    }
}

/// Minimal state required by `registry_queue_init`.
///
/// Multi-instance globals (seat/output) that appear after the initial roundtrip
/// are recorded here so the native path can observe runtime changes later.
#[derive(Debug, Default)]
pub struct NativeRegistryState {
    pub late_globals: Vec<GlobalAdvertisement>,
    pub removed: Vec<u32>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for NativeRegistryState {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                state.late_globals.push(GlobalAdvertisement {
                    name,
                    interface,
                    version,
                });
            }
            wl_registry::Event::GlobalRemove { name } => {
                state.removed.push(name);
                state.late_globals.retain(|g| g.name != name);
            }
            _ => {}
        }
    }
}

/// Initial registry contents plus the event queue used for further native work.
pub struct NativeRegistry {
    globals: GlobalList,
    queue: EventQueue<NativeRegistryState>,
    state: NativeRegistryState,
}

impl NativeRegistry {
    /// Perform registry bind + initial roundtrip and snapshot globals.
    pub fn bootstrap(connection: &NativeConnection) -> Result<Self, NativeError> {
        let (globals, queue) = registry_queue_init::<NativeRegistryState>(connection.connection())
            .map_err(|error| NativeError::Registry(error.to_string()))?;
        Ok(Self {
            globals,
            queue,
            state: NativeRegistryState::default(),
        })
    }

    pub fn queue_handle(&self) -> QueueHandle<NativeRegistryState> {
        self.queue.handle()
    }

    pub fn global_list(&self) -> &GlobalList {
        &self.globals
    }

    pub fn state_mut(&mut self) -> &mut NativeRegistryState {
        &mut self.state
    }

    pub fn queue_mut(&mut self) -> &mut EventQueue<NativeRegistryState> {
        &mut self.queue
    }

    /// Snapshot of globals known after the bootstrap roundtrip.
    pub fn advertisements(&self) -> Vec<GlobalAdvertisement> {
        self.globals
            .contents()
            .clone_list()
            .iter()
            .map(GlobalAdvertisement::from)
            .collect()
    }

    pub fn has_interface(&self, interface: &str) -> bool {
        self.globals
            .contents()
            .with_list(|list| list.iter().any(|g| g.interface == interface))
    }

    pub fn interface_version(&self, interface: &str) -> Option<u32> {
        self.globals.contents().with_list(|list| {
            list.iter()
                .filter(|g| g.interface == interface)
                .map(|g| g.version)
                .max()
        })
    }

    /// Dispatch pending registry events into [`NativeRegistryState`].
    pub fn dispatch_pending(&mut self) -> Result<usize, NativeError> {
        Ok(self.queue.dispatch_pending(&mut self.state)?)
    }
}

/// Convenience: connect and list interfaces (sync). Useful for smoke/tests.
pub fn list_env_globals() -> Result<Vec<GlobalAdvertisement>, NativeError> {
    let connection = NativeConnection::connect_to_env()?;
    let registry = NativeRegistry::bootstrap(&connection)?;
    Ok(registry.advertisements())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_env_globals_finds_wl_display_stack_when_available() {
        let Ok(globals) = list_env_globals() else {
            return;
        };
        assert!(globals.iter().any(|g| g.interface == "wl_compositor"));
    }
}
