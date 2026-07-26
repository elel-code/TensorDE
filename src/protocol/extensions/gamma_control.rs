//! `zwlr_gamma_control_manager_v1` (Dispatch2).
//!
//! Ported from Niri/Hyprland-style wlr gamma control. The compositor supplies
//! LUT size and apply/reset through [`GammaControlHandler`].

use std::{collections::HashMap, fs::File, io::Read};

use smithay::{
    output::Output,
    wayland::{Dispatch2, GlobalDispatch2},
};
use tracing::{trace, warn};
use wayland_protocols_wlr::gamma_control::v1::server::{
    zwlr_gamma_control_manager_v1::{self, ZwlrGammaControlManagerV1},
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, New, Resource, backend::ClientId};

const VERSION: u32 = 1;

pub struct GammaControlManagerState {
    gamma_controls: HashMap<Output, ZwlrGammaControlV1>,
}

pub struct GammaControlManagerGlobalData {
    filter: Box<dyn for<'c> Fn(&'c Client) -> bool + Send + Sync>,
}

pub trait GammaControlHandler: 'static {
    fn gamma_control_manager_state(&mut self) -> &mut GammaControlManagerState;
    fn get_gamma_size(&mut self, output: &Output) -> Option<u32>;
    fn set_gamma(&mut self, output: &Output, ramp: Option<Vec<u16>>) -> Option<()>;
}

pub struct GammaControlState {
    gamma_size: u32,
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
    pub fn output_removed(&mut self, output: &Output) {
        if let Some(gamma_control) = self.gamma_controls.remove(output) {
            gamma_control.failed();
        }
    }
}

impl<D> GlobalDispatch2<ZwlrGammaControlManagerV1, D> for GammaControlManagerGlobalData
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

impl<D> Dispatch2<ZwlrGammaControlManagerV1, D> for GammaControlManagerGlobalData
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
                if let Some(output) = Output::from_resource(&output) {
                    #[allow(clippy::map_entry)]
                    if !state
                        .gamma_control_manager_state()
                        .gamma_controls
                        .contains_key(&output)
                        && let Some(gamma_size) = state.get_gamma_size(&output)
                    {
                        let zwlr_gamma_control =
                            data_init.init(id, GammaControlState { gamma_size });
                        zwlr_gamma_control.gamma_size(gamma_size);
                        state
                            .gamma_control_manager_state()
                            .gamma_controls
                            .insert(output, zwlr_gamma_control);
                        return;
                    }
                }
                data_init
                    .init(id, GammaControlState { gamma_size: 0 })
                    .failed();
            }
            zwlr_gamma_control_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

// Smithay `Output` is the protocol adapter's lifetime key. Tensor-owned policy
// uses stable connector IDs and never inherits this internally mutable key.
#[allow(clippy::mutable_key_type)]
impl<D> Dispatch2<ZwlrGammaControlV1, D> for GammaControlState
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
                let gamma_controls = &mut state.gamma_control_manager_state().gamma_controls;
                let Some((output, _)) = gamma_controls.iter().find(|(_, x)| *x == resource) else {
                    return;
                };
                let output = output.clone();
                trace!("setting gamma for output {}", output.name());

                let expected = self.gamma_size as usize * 3 * 2;
                let mut file = File::from(fd);
                let mut bytes = vec![0u8; expected];
                if let Err(err) = file.read_exact(&mut bytes) {
                    warn!("failed to read gamma data: {err:?}");
                    resource.failed();
                    gamma_controls.remove(&output);
                    let _ = state.set_gamma(&output, None);
                    return;
                }
                match file.read(&mut [0]) {
                    Ok(0) => {}
                    Ok(_) => {
                        warn!("gamma data is too large");
                        resource.failed();
                        gamma_controls.remove(&output);
                        let _ = state.set_gamma(&output, None);
                        return;
                    }
                    Err(err) => {
                        warn!("error reading gamma data: {err:?}");
                        resource.failed();
                        gamma_controls.remove(&output);
                        let _ = state.set_gamma(&output, None);
                        return;
                    }
                }
                let gamma = bytes
                    .chunks_exact(2)
                    .map(|chunk| u16::from_ne_bytes([chunk[0], chunk[1]]))
                    .collect::<Vec<_>>();
                if state.set_gamma(&output, Some(gamma)).is_none() {
                    resource.failed();
                    let gamma_controls = &mut state.gamma_control_manager_state().gamma_controls;
                    gamma_controls.remove(&output);
                    let _ = state.set_gamma(&output, None);
                }
            }
            zwlr_gamma_control_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, resource: &ZwlrGammaControlV1) {
        let gamma_controls = &mut state.gamma_control_manager_state().gamma_controls;
        let Some((output, _)) = gamma_controls.iter().find(|(_, x)| *x == resource) else {
            return;
        };
        let output = output.clone();
        gamma_controls.remove(&output);
        let _ = state.set_gamma(&output, None);
    }
}
