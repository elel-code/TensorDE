//! Per-output DPMS control through `wlr-output-power-management-unstable-v1`.

use wayland_client::Proxy;
use wayland_protocols_wlr::output_power_management::v1::client::zwlr_output_power_v1;

use super::api::NativeShell;
use super::types::OutputPowerRecord;
use crate::native::connection::NativeError;
use crate::output::{OutputId, OutputPowerMode};

impl NativeShell {
    pub fn has_output_power(&self) -> bool {
        self.state.output_power_manager.is_some()
    }

    /// Create an exclusive power control for one currently advertised output.
    pub fn create_output_power(&mut self, output: OutputId) -> Result<(), NativeError> {
        let output_id = output.get();
        let manager = self
            .state
            .output_power_manager
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("zwlr_output_power_manager_v1 missing".into()))?
            .clone();
        let wl_output = self
            .state
            .output_proxies
            .get(&output_id)
            .cloned()
            .ok_or_else(|| NativeError::Protocol(format!("unknown output {output_id}")))?;
        if self.state.output_powers.contains_key(&output_id) {
            return Err(NativeError::Protocol(format!(
                "output power control already exists for output {output_id}"
            )));
        }

        let power = manager.get_output_power(&wl_output, &self.queue.handle(), ());
        self.state
            .output_power_objects
            .insert(power.id().protocol_id(), output_id);
        self.state.output_powers.insert(
            output_id,
            OutputPowerRecord {
                power: Some(power),
                mode: None,
                failed: false,
            },
        );
        self.connection.mark_dirty();
        Ok(())
    }

    /// Request DPMS power state without changing output topology.
    pub fn set_output_power(
        &mut self,
        output: OutputId,
        mode: OutputPowerMode,
    ) -> Result<(), NativeError> {
        let output_id = output.get();
        let record = self.state.output_powers.get(&output_id).ok_or_else(|| {
            NativeError::Protocol(format!("no output power control for output {output_id}"))
        })?;
        if record.failed {
            return Err(NativeError::Protocol(format!(
                "output power control failed for output {output_id}"
            )));
        }
        let power = record.power.as_ref().ok_or_else(|| {
            NativeError::Protocol(format!(
                "output power control is no longer live for output {output_id}"
            ))
        })?;
        let wire_mode = match mode {
            OutputPowerMode::Off => zwlr_output_power_v1::Mode::Off,
            OutputPowerMode::On => zwlr_output_power_v1::Mode::On,
        };
        power.set_mode(wire_mode);
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn output_power_mode(&self, output: OutputId) -> Option<OutputPowerMode> {
        self.state
            .output_powers
            .get(&output.get())
            .and_then(|record| record.mode)
    }

    pub fn output_power_failed(&self, output: OutputId) -> bool {
        self.state
            .output_powers
            .get(&output.get())
            .is_some_and(|record| record.failed)
    }

    pub fn destroy_output_power(&mut self, output: OutputId) -> Result<(), NativeError> {
        let output_id = output.get();
        let Some(record) = self.state.output_powers.remove(&output_id) else {
            return Err(NativeError::Protocol(format!(
                "no output power control for output {output_id}"
            )));
        };
        if let Some(power) = record.power {
            self.state
                .output_power_objects
                .remove(&power.id().protocol_id());
            power.destroy();
            self.connection.mark_dirty();
        }
        self.state
            .pending_output_power_destroy
            .retain(|(pending, _)| *pending != output_id);
        Ok(())
    }

    pub(crate) fn destroy_pending_output_powers(&mut self) {
        if self.state.pending_output_power_destroy.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.state.pending_output_power_destroy);
        let mut destroyed = false;
        for (output, retain_failed) in pending {
            let Some(record) = self.state.output_powers.get_mut(&output) else {
                continue;
            };
            if let Some(power) = record.power.take() {
                self.state
                    .output_power_objects
                    .remove(&power.id().protocol_id());
                power.destroy();
                destroyed = true;
            }
            if !retain_failed {
                self.state.output_powers.remove(&output);
            }
        }
        if destroyed {
            self.connection.mark_dirty();
        }
    }
}
