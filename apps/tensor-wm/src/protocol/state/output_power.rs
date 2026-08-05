use tensor_host::ConnectorId;

#[cfg(feature = "tty")]
use super::OutputRedrawState;
use super::RuntimeState;

impl RuntimeState {
    pub(in crate::protocol) fn output_power_mode(&self, output: ConnectorId) -> Option<bool> {
        #[cfg(feature = "tty")]
        {
            self.outputs.contains_key(&output).then_some(())?;
            self.backend.as_ref()?.output_powered(output)
        }
        #[cfg(not(feature = "tty"))]
        {
            let _ = output;
            None
        }
    }

    pub(in crate::protocol) fn set_output_power_mode(
        &mut self,
        output: ConnectorId,
        powered: bool,
    ) -> Result<(), String> {
        #[cfg(feature = "tty")]
        {
            if !self.outputs.contains_key(&output) {
                return Err(format!("output {output:?} is no longer live"));
            }
            let changed = self
                .backend
                .as_mut()
                .ok_or_else(|| "TTY backend is unavailable".to_owned())?
                .set_output_powered(output, powered)
                .map_err(|error| error.to_string())?;
            if changed {
                if powered {
                    self.set_redraw_state(output, OutputRedrawState::Queued);
                    self.flush_queued_redraws();
                } else {
                    self.set_redraw_state(output, OutputRedrawState::Idle);
                }
            }
            Ok(())
        }
        #[cfg(not(feature = "tty"))]
        {
            let _ = (output, powered);
            Err("native output power is unavailable in a headless compositor".to_owned())
        }
    }
}
