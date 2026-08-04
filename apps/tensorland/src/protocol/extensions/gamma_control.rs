//! `zwlr_gamma_control_manager_v1` protocol adapter.
//!
//! Ported from Niri/Hyprland-style wlr gamma control. The compositor supplies
//! LUT size and apply/reset through [`GammaControlHandler`].

use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read},
};

use tensor_host::ConnectorId;
use tracing::{trace, warn};
use wayland_protocols_wlr::gamma_control::v1::server::{
    zwlr_gamma_control_manager_v1::{self, ZwlrGammaControlManagerV1},
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource, backend::ClientId,
    protocol::wl_output::WlOutput,
};

use crate::protocol::dispatch::{
    DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
};
use crate::protocol::state::RuntimeState;

const VERSION: u32 = 1;

pub struct GammaControlManagerState {
    gamma_controls: HashMap<ConnectorId, ZwlrGammaControlV1>,
}

pub struct GammaControlManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

pub trait GammaControlHandler: 'static {
    fn gamma_control_manager_state(&mut self) -> &mut GammaControlManagerState;
    fn gamma_output_id(&self, output: &WlOutput) -> Option<ConnectorId>;
    fn get_gamma_size(&mut self, output: ConnectorId) -> Option<u32>;
    fn set_gamma(&mut self, output: ConnectorId, ramp: Option<Vec<u16>>) -> Option<()>;
}

pub struct GammaControlState {
    output: Option<ConnectorId>,
    gamma_size: u32,
}

#[derive(Debug)]
enum GammaRampReadError {
    InvalidSize,
    Io(io::Error),
    TrailingData,
}

fn read_gamma_ramp(
    reader: &mut impl Read,
    gamma_size: u32,
) -> Result<Vec<u16>, GammaRampReadError> {
    let values = usize::try_from(gamma_size)
        .ok()
        .and_then(|size| size.checked_mul(3))
        .ok_or(GammaRampReadError::InvalidSize)?;
    let mut ramp = vec![0u16; values];
    // Every u16 bit pattern is valid, and the protocol sends native-endian
    // values. Reading into the initialized allocation avoids a second buffer
    // and a full-ramp copy on this control path.
    let bytes = bytemuck::cast_slice_mut(&mut ramp);
    reader.read_exact(bytes).map_err(GammaRampReadError::Io)?;
    match reader.read(&mut [0]) {
        Ok(0) => Ok(ramp),
        Ok(_) => Err(GammaRampReadError::TrailingData),
        Err(error) => Err(GammaRampReadError::Io(error)),
    }
}

impl GammaControlManagerState {
    pub fn new<D, F>(display: &DisplayHandle, filter: F) -> Self
    where
        D: wayland_server::GlobalDispatch<ZwlrGammaControlManagerV1, GammaControlManagerGlobalData>,
        D: Dispatch<ZwlrGammaControlManagerV1, GammaControlManagerGlobalData>,
        D: Dispatch<ZwlrGammaControlV1, GammaControlState>,
        D: GammaControlHandler,
        D: 'static,
        F: for<'c> Fn(&'c Client) -> bool + Send + Sync + 'static,
    {
        let global_data = GammaControlManagerGlobalData {
            filter: Box::new(filter),
        };
        display.create_global::<D, ZwlrGammaControlManagerV1, _>(VERSION, global_data);
        Self {
            gamma_controls: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn output_removed(&mut self, output: ConnectorId) {
        if let Some(gamma_control) = self.gamma_controls.remove(&output) {
            gamma_control.failed();
        }
    }
}

impl<D> GlobalDispatchDelegate<ZwlrGammaControlManagerV1, D> for GammaControlManagerGlobalData
where
    D: Dispatch<ZwlrGammaControlManagerV1, GammaControlManagerGlobalData>,
    D: 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrGammaControlManagerV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(
            resource,
            GammaControlManagerGlobalData {
                filter: Box::new(|_| true),
            },
        );
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

impl<D> DispatchDelegate<ZwlrGammaControlManagerV1, D> for GammaControlManagerGlobalData
where
    D: Dispatch<ZwlrGammaControlV1, GammaControlState>,
    D: GammaControlHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        _resource: &ZwlrGammaControlManagerV1,
        request: <ZwlrGammaControlManagerV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_gamma_control_manager_v1::Request::GetGammaControl { id, output } => {
                if let Some(output) = state.gamma_output_id(&output)
                    && !state
                        .gamma_control_manager_state()
                        .gamma_controls
                        .contains_key(&output)
                    && let Some(gamma_size) = state.get_gamma_size(output)
                {
                    let zwlr_gamma_control = data_init.init(
                        id,
                        GammaControlState {
                            output: Some(output),
                            gamma_size,
                        },
                    );
                    zwlr_gamma_control.gamma_size(gamma_size);
                    state
                        .gamma_control_manager_state()
                        .gamma_controls
                        .insert(output, zwlr_gamma_control);
                    return;
                }
                data_init
                    .init(
                        id,
                        GammaControlState {
                            output: None,
                            gamma_size: 0,
                        },
                    )
                    .failed();
            }
            zwlr_gamma_control_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl<D> DispatchDelegate<ZwlrGammaControlV1, D> for GammaControlState
where
    D: GammaControlHandler,
    D: 'static,
{
    fn request(
        &self,
        state: &mut D,
        _client: &Client,
        resource: &ZwlrGammaControlV1,
        request: <ZwlrGammaControlV1 as Resource>::Request,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwlr_gamma_control_v1::Request::SetGamma { fd } => {
                let Some(output) = self.output else {
                    return;
                };
                let is_active = state
                    .gamma_control_manager_state()
                    .gamma_controls
                    .get(&output)
                    .is_some_and(|control| control == resource);
                if !is_active {
                    return;
                }
                trace!(?output, "setting gamma for output");

                let mut file = File::from(fd);
                let gamma = match read_gamma_ramp(&mut file, self.gamma_size) {
                    Ok(gamma) => gamma,
                    Err(error) => {
                        match &error {
                            GammaRampReadError::Io(error) => {
                                warn!(?error, "failed to read gamma data");
                            }
                            GammaRampReadError::InvalidSize => {
                                warn!("gamma ramp size is not representable");
                            }
                            GammaRampReadError::TrailingData => {
                                warn!("gamma data is too large");
                            }
                        }
                        resource.failed();
                        state
                            .gamma_control_manager_state()
                            .gamma_controls
                            .remove(&output);
                        let _ = state.set_gamma(output, None);
                        return;
                    }
                };
                if state.set_gamma(output, Some(gamma)).is_none() {
                    resource.failed();
                    state
                        .gamma_control_manager_state()
                        .gamma_controls
                        .remove(&output);
                    let _ = state.set_gamma(output, None);
                }
            }
            zwlr_gamma_control_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, resource: &ZwlrGammaControlV1) {
        let Some(output) = self.output else {
            return;
        };
        let is_active = state
            .gamma_control_manager_state()
            .gamma_controls
            .get(&output)
            .is_some_and(|control| control == resource);
        if is_active {
            state
                .gamma_control_manager_state()
                .gamma_controls
                .remove(&output);
            let _ = state.set_gamma(output, None);
        }
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ZwlrGammaControlManagerV1,
    GammaControlManagerGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ZwlrGammaControlManagerV1,
    GammaControlManagerGlobalData
);
delegate_dispatch!(RuntimeState, ZwlrGammaControlV1, GammaControlState);

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn gamma_ramp_reads_native_endian_values_without_staging() {
        let values = [0u16, 1, 258, u16::MAX, 17, 19];
        let bytes = values
            .into_iter()
            .flat_map(u16::to_ne_bytes)
            .collect::<Vec<_>>();

        let ramp = read_gamma_ramp(&mut Cursor::new(bytes), 2).unwrap();
        assert_eq!(ramp, values);
    }

    #[test]
    fn gamma_ramp_rejects_short_and_trailing_payloads() {
        assert!(matches!(
            read_gamma_ramp(&mut Cursor::new(vec![0; 5]), 1),
            Err(GammaRampReadError::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof
        ));
        assert!(matches!(
            read_gamma_ramp(&mut Cursor::new(vec![0; 7]), 1),
            Err(GammaRampReadError::TrailingData)
        ));
    }
}
