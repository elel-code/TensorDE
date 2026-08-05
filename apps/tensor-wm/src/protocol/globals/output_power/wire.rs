use tensor_host::ConnectorId;
use wayland_protocols_wlr::output_power_management::v1::server::{
    zwlr_output_power_manager_v1::{self, ZwlrOutputPowerManagerV1},
    zwlr_output_power_v1::{self, Mode, ZwlrOutputPowerV1},
};
use wayland_server::{Client, DataInit, DisplayHandle, New, Resource, WEnum, backend::ClientId};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::output::Output,
    state::RuntimeState,
};

pub(super) struct OutputPowerGlobalData {
    filter: Box<dyn for<'client> Fn(&'client Client) -> bool + Send + Sync>,
}

impl OutputPowerGlobalData {
    pub(super) fn new<F>(filter: F) -> Self
    where
        F: for<'client> Fn(&'client Client) -> bool + Send + Sync + 'static,
    {
        Self {
            filter: Box::new(filter),
        }
    }
}

pub(super) struct OutputPowerManagerData;

pub(super) struct OutputPowerControlData {
    output: Option<ConnectorId>,
}

impl GlobalDispatchDelegate<ZwlrOutputPowerManagerV1, RuntimeState> for OutputPowerGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrOutputPowerManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, OutputPowerManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

impl DispatchDelegate<ZwlrOutputPowerManagerV1, RuntimeState> for OutputPowerManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _manager: &ZwlrOutputPowerManagerV1,
        request: zwlr_output_power_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwlr_output_power_manager_v1::Request::GetOutputPower { id, output } => {
                let output = Output::from_resource(&output).map(|output| output.id());
                let available = output.and_then(|output| {
                    state
                        .output_power_mode(output)
                        .map(|powered| (output, powered))
                });
                let available = available.filter(|(output, _)| {
                    !state
                        .protocol_globals
                        .output_power
                        .controls
                        .contains_key(output)
                });
                let control = data_init.init(
                    id,
                    OutputPowerControlData {
                        output: available.map(|(output, _)| output),
                    },
                );
                let Some((output, powered)) = available else {
                    control.failed();
                    return;
                };
                control.mode(if powered { Mode::On } else { Mode::Off });
                state
                    .protocol_globals
                    .output_power
                    .controls
                    .insert(output, control);
            }
            zwlr_output_power_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwlrOutputPowerV1, RuntimeState> for OutputPowerControlData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        control: &ZwlrOutputPowerV1,
        request: zwlr_output_power_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwlr_output_power_v1::Request::SetMode { mode } => {
                let Some(output) = self.output.filter(|output| {
                    state
                        .protocol_globals
                        .output_power
                        .is_active(*output, control)
                }) else {
                    return;
                };
                let powered = match mode {
                    WEnum::Value(Mode::Off) => false,
                    WEnum::Value(Mode::On) => true,
                    WEnum::Unknown(mode) => {
                        control.post_error(
                            zwlr_output_power_v1::Error::InvalidMode,
                            format!("invalid output power mode {mode}"),
                        );
                        return;
                    }
                    _ => unreachable!(),
                };
                if state.set_output_power_mode(output, powered).is_err() {
                    control.failed();
                    state.protocol_globals.output_power.remove(output, control);
                    return;
                }
                state
                    .protocol_globals
                    .output_power
                    .mode_changed(output, powered);
            }
            zwlr_output_power_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut RuntimeState, _client: ClientId, control: &ZwlrOutputPowerV1) {
        if let Some(output) = self.output {
            state.protocol_globals.output_power.remove(output, control);
        }
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ZwlrOutputPowerManagerV1,
    OutputPowerGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ZwlrOutputPowerManagerV1,
    OutputPowerManagerData
);
delegate_dispatch!(RuntimeState, ZwlrOutputPowerV1, OutputPowerControlData);
