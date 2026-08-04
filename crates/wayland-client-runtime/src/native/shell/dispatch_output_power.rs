//! `wlr-output-power-management-unstable-v1` dispatch.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols_wlr::output_power_management::v1::client::{
    zwlr_output_power_manager_v1, zwlr_output_power_v1,
};

use super::types::{NativeShellEvent, NativeShellState};
use crate::output::OutputPowerMode;

impl Dispatch<zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &zwlr_output_power_manager_v1::ZwlrOutputPowerManagerV1,
        _: zwlr_output_power_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_output_power_v1::ZwlrOutputPowerV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        power: &zwlr_output_power_v1::ZwlrOutputPowerV1,
        event: zwlr_output_power_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state
            .output_power_objects
            .get(&power.id().protocol_id())
            .copied()
        else {
            return;
        };
        match event {
            zwlr_output_power_v1::Event::Mode {
                mode: WEnum::Value(mode),
            } => {
                let mode = match mode {
                    zwlr_output_power_v1::Mode::Off => OutputPowerMode::Off,
                    zwlr_output_power_v1::Mode::On => OutputPowerMode::On,
                    _ => return,
                };
                if let Some(record) = state.output_powers.get_mut(&output) {
                    record.mode = Some(mode);
                }
                state.push(NativeShellEvent::OutputPowerMode { output, mode });
            }
            zwlr_output_power_v1::Event::Failed => {
                if let Some(record) = state.output_powers.get_mut(&output) {
                    record.failed = true;
                    record.mode = None;
                }
                if !state
                    .pending_output_power_destroy
                    .iter()
                    .any(|(pending, _)| *pending == output)
                {
                    state.pending_output_power_destroy.push((output, true));
                }
                state.push(NativeShellEvent::OutputPowerFailed { output });
            }
            _ => {}
        }
    }
}
