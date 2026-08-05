//! Exclusive per-output DPMS controls for desktop idle policy.

mod wire;

use std::collections::HashMap;

use tensor_host::ConnectorId;
use wayland_protocols_wlr::output_power_management::v1::server::{
    zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1,
    zwlr_output_power_v1::{Mode, ZwlrOutputPowerV1},
};
use wayland_server::{Client, DisplayHandle, backend::GlobalId};

use crate::protocol::state::RuntimeState;

pub(crate) struct OutputPowerProtocol {
    _global: GlobalId,
    controls: HashMap<ConnectorId, ZwlrOutputPowerV1>,
}

impl OutputPowerProtocol {
    pub(crate) fn new<F>(display: &DisplayHandle, filter: F) -> Self
    where
        F: for<'client> Fn(&'client Client) -> bool + Send + Sync + 'static,
    {
        Self {
            _global: display.create_global::<RuntimeState, ZwlrOutputPowerManagerV1, _>(
                1,
                wire::OutputPowerGlobalData::new(filter),
            ),
            controls: HashMap::new(),
        }
    }

    pub(crate) fn output_removed(&mut self, output: ConnectorId) {
        if let Some(control) = self.controls.remove(&output) {
            control.failed();
        }
    }

    fn is_active(&self, output: ConnectorId, control: &ZwlrOutputPowerV1) -> bool {
        self.controls
            .get(&output)
            .is_some_and(|active| active == control)
    }

    fn mode_changed(&self, output: ConnectorId, powered: bool) {
        if let Some(control) = self.controls.get(&output) {
            control.mode(if powered { Mode::On } else { Mode::Off });
        }
    }

    fn remove(&mut self, output: ConnectorId, control: &ZwlrOutputPowerV1) {
        if self.is_active(output, control) {
            self.controls.remove(&output);
        }
    }

    #[cfg(test)]
    pub(crate) fn control_count(&self) -> usize {
        self.controls.len()
    }
}
