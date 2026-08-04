//! Per-output DPMS methods on [`NativeRuntime`].

use crate::output::{OutputId, OutputPowerMode};
use crate::runtime_common::RuntimeError;

use super::runtime_facade::{NativeRuntime, map_native_error};

impl NativeRuntime {
    pub fn has_output_power(&self) -> bool {
        self.shell.has_output_power()
    }

    pub fn create_output_power(&mut self, output: OutputId) -> Result<(), RuntimeError> {
        if !self.shell.has_output_power() {
            return Err(RuntimeError::Unsupported("zwlr_output_power_manager_v1"));
        }
        if self.shell.output_info(output.get()).is_none() {
            return Err(RuntimeError::OutputNotFound(output));
        }
        self.shell
            .create_output_power(output)
            .map_err(map_native_error)
    }

    pub fn set_output_power(
        &mut self,
        output: OutputId,
        mode: OutputPowerMode,
    ) -> Result<(), RuntimeError> {
        self.shell
            .set_output_power(output, mode)
            .map_err(map_native_error)
    }

    pub fn output_power_mode(&self, output: OutputId) -> Option<OutputPowerMode> {
        self.shell.output_power_mode(output)
    }

    pub fn output_power_failed(&self, output: OutputId) -> bool {
        self.shell.output_power_failed(output)
    }

    pub fn destroy_output_power(&mut self, output: OutputId) -> Result<(), RuntimeError> {
        self.shell
            .destroy_output_power(output)
            .map_err(map_native_error)
    }
}
