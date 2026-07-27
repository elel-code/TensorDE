//! Tensor-owned zero-cost delegation for generic protocol implementations.
//!
//! Adapted from Smithay's `Dispatch2` bridge at commit c0aa71d. Smithay's
//! copyright notice and MIT terms are in `LICENSES/Smithay-MIT.txt`.

use wayland_server::{Client, DataInit, DisplayHandle, New, Resource, backend::ClientId};

pub(crate) trait DispatchDelegate<I: Resource, State> {
    fn request(
        &self,
        state: &mut State,
        client: &Client,
        resource: &I,
        request: I::Request,
        display: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    );

    fn destroyed(&self, _state: &mut State, _client: ClientId, _resource: &I) {}
}

pub(crate) trait GlobalDispatchDelegate<I: Resource, State> {
    fn bind(
        &self,
        state: &mut State,
        display: &DisplayHandle,
        client: &Client,
        resource: New<I>,
        data_init: &mut DataInit<'_, State>,
    );

    fn can_view(&self, _client: &Client) -> bool {
        true
    }
}

macro_rules! delegate_dispatch {
    ($state:ty, $interface:ty, $data:ty) => {
        impl wayland_server::Dispatch<$interface, $data> for $state {
            fn request(
                state: &mut Self,
                client: &wayland_server::Client,
                resource: &$interface,
                request: <$interface as wayland_server::Resource>::Request,
                data: &$data,
                display: &wayland_server::DisplayHandle,
                data_init: &mut wayland_server::DataInit<'_, Self>,
            ) {
                <$data as $crate::protocol::dispatch::DispatchDelegate<$interface, $state>>::request(
                    data, state, client, resource, request, display, data_init,
                );
            }

            fn destroyed(
                state: &mut Self,
                client: wayland_server::backend::ClientId,
                resource: &$interface,
                data: &$data,
            ) {
                <$data as $crate::protocol::dispatch::DispatchDelegate<$interface, $state>>::destroyed(
                    data, state, client, resource,
                );
            }
        }
    };
}

macro_rules! delegate_global_dispatch {
    ($state:ty, $interface:ty, $data:ty) => {
        impl wayland_server::GlobalDispatch<$interface, $data> for $state {
            fn bind(
                state: &mut Self,
                display: &wayland_server::DisplayHandle,
                client: &wayland_server::Client,
                resource: wayland_server::New<$interface>,
                data: &$data,
                data_init: &mut wayland_server::DataInit<'_, Self>,
            ) {
                <$data as $crate::protocol::dispatch::GlobalDispatchDelegate<
                                    $interface,
                                    $state,
                                >>::bind(
                                    data, state, display, client, resource, data_init,
                                );
            }

            fn can_view(client: wayland_server::Client, data: &$data) -> bool {
                <$data as $crate::protocol::dispatch::GlobalDispatchDelegate<
                                    $interface,
                                    $state,
                                >>::can_view(data, &client)
            }
        }
    };
}

pub(crate) use {delegate_dispatch, delegate_global_dispatch};
