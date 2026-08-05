//! Tensor-owned `wp_single_pixel_buffer_manager_v1` wire adapter.

use wayland_protocols::wp::single_pixel_buffer::v1::server::wp_single_pixel_buffer_manager_v1::{
    self, WpSinglePixelBufferManagerV1,
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource,
    backend::{ClientId, GlobalId},
    protocol::wl_buffer::{self, WlBuffer},
};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

pub(crate) struct SinglePixelBufferProtocol {
    _global: GlobalId,
}

impl SinglePixelBufferProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        let global = display.create_global::<RuntimeState, WpSinglePixelBufferManagerV1, _>(
            1,
            SinglePixelBufferGlobalData,
        );
        Self { _global: global }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct SinglePixelBufferGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct SinglePixelBufferManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct SinglePixelBufferData {
    rgba: [u32; 4],
}

impl SinglePixelBufferData {
    fn rgba(&self) -> &[u32; 4] {
        &self.rgba
    }
}

pub(in crate::protocol) fn single_pixel_rgba(buffer: &WlBuffer) -> Option<&[u32; 4]> {
    Some(buffer.data::<SinglePixelBufferData>()?.rgba())
}

pub(in crate::protocol) trait SinglePixelBufferHandler: 'static {
    fn single_pixel_buffer_destroyed(&mut self, buffer: &WlBuffer);
}

impl SinglePixelBufferHandler for RuntimeState {
    fn single_pixel_buffer_destroyed(&mut self, buffer: &WlBuffer) {
        #[cfg(feature = "tty")]
        self.buffer_destroyed(&buffer.id());
        #[cfg(not(feature = "tty"))]
        let _ = buffer;
    }
}

impl<D> GlobalDispatchDelegate<WpSinglePixelBufferManagerV1, D> for SinglePixelBufferGlobalData
where
    D: Dispatch<WpSinglePixelBufferManagerV1, SinglePixelBufferManagerData>,
    D: 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpSinglePixelBufferManagerV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, SinglePixelBufferManagerData);
    }
}

impl<D> DispatchDelegate<WpSinglePixelBufferManagerV1, D> for SinglePixelBufferManagerData
where
    D: Dispatch<WlBuffer, SinglePixelBufferData>,
    D: SinglePixelBufferHandler,
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _manager: &WpSinglePixelBufferManagerV1,
        request: wp_single_pixel_buffer_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wp_single_pixel_buffer_manager_v1::Request::Destroy => {}
            wp_single_pixel_buffer_manager_v1::Request::CreateU32RgbaBuffer { id, r, g, b, a } => {
                data_init.init(id, SinglePixelBufferData { rgba: [r, g, b, a] });
            }
            _ => unreachable!(),
        }
    }
}

impl<D> DispatchDelegate<WlBuffer, D> for SinglePixelBufferData
where
    D: SinglePixelBufferHandler,
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _buffer: &WlBuffer,
        request: wl_buffer::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wl_buffer::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, buffer: &WlBuffer) {
        state.single_pixel_buffer_destroyed(buffer);
    }
}

delegate_global_dispatch!(
    RuntimeState,
    WpSinglePixelBufferManagerV1,
    SinglePixelBufferGlobalData
);
delegate_dispatch!(
    RuntimeState,
    WpSinglePixelBufferManagerV1,
    SinglePixelBufferManagerData
);
delegate_dispatch!(RuntimeState, WlBuffer, SinglePixelBufferData);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_pixel_userdata_is_four_inline_u32_channels() {
        assert_eq!(
            std::mem::size_of::<SinglePixelBufferData>(),
            4 * std::mem::size_of::<u32>()
        );
    }
}
