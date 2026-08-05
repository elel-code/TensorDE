//! Server-created selection offers.

use wayland_protocols::{
    ext::data_control::v1::server::ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
    wp::primary_selection::zv1::server::zwp_primary_selection_offer_v1::{
        self, ZwpPrimarySelectionOfferV1,
    },
};
use wayland_protocols_wlr::data_control::v1::server::zwlr_data_control_offer_v1::{
    self, ZwlrDataControlOfferV1,
};
use wayland_server::{
    Client, DataInit, DisplayHandle, Resource,
    protocol::wl_data_offer::{self, WlDataOffer},
};

use super::{SelectionTarget, SourceToken};
use crate::protocol::{
    dispatch::{DispatchDelegate, delegate_dispatch},
    state::RuntimeState,
};

pub(in crate::protocol) struct CoreOfferData {
    source: SourceToken,
}

impl CoreOfferData {
    pub(super) fn selection(source: SourceToken) -> Self {
        Self { source }
    }
}

pub(in crate::protocol) struct SelectionOfferData {
    source: SourceToken,
    target: SelectionTarget,
    focused_only: bool,
}

impl SelectionOfferData {
    pub(super) fn focused(source: SourceToken, target: SelectionTarget) -> Self {
        Self {
            source,
            target,
            focused_only: true,
        }
    }

    pub(super) fn control(source: SourceToken, target: SelectionTarget) -> Self {
        Self {
            source,
            target,
            focused_only: false,
        }
    }
}

impl DispatchDelegate<WlDataOffer, RuntimeState> for CoreOfferData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        offer: &WlDataOffer,
        request: wl_data_offer::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wl_data_offer::Request::Receive { mime_type, fd } => {
                state.protocol_globals.selection.receive(
                    &client.id(),
                    self.source,
                    SelectionTarget::Clipboard,
                    true,
                    mime_type,
                    fd,
                )
            }
            wl_data_offer::Request::Destroy => {}
            wl_data_offer::Request::Accept { .. }
            | wl_data_offer::Request::Finish
            | wl_data_offer::Request::SetActions { .. } => offer.post_error(
                wl_data_offer::Error::InvalidOffer,
                "drag-and-drop request sent to a clipboard offer",
            ),
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwpPrimarySelectionOfferV1, RuntimeState> for SelectionOfferData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _offer: &ZwpPrimarySelectionOfferV1,
        request: zwp_primary_selection_offer_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwp_primary_selection_offer_v1::Request::Receive { mime_type, fd } => {
                state.protocol_globals.selection.receive(
                    &client.id(),
                    self.source,
                    self.target,
                    self.focused_only,
                    mime_type,
                    fd,
                )
            }
            zwp_primary_selection_offer_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ZwlrDataControlOfferV1, RuntimeState> for SelectionOfferData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _offer: &ZwlrDataControlOfferV1,
        request: zwlr_data_control_offer_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            zwlr_data_control_offer_v1::Request::Receive { mime_type, fd } => {
                state.protocol_globals.selection.receive(
                    &client.id(),
                    self.source,
                    self.target,
                    self.focused_only,
                    mime_type,
                    fd,
                )
            }
            zwlr_data_control_offer_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<ExtDataControlOfferV1, RuntimeState> for SelectionOfferData {
    fn request(
        &self,
        state: &mut RuntimeState,
        client: &Client,
        _offer: &ExtDataControlOfferV1,
        request: ext_data_control_offer_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_data_control_offer_v1::Request::Receive { mime_type, fd } => {
                state.protocol_globals.selection.receive(
                    &client.id(),
                    self.source,
                    self.target,
                    self.focused_only,
                    mime_type,
                    fd,
                )
            }
            ext_data_control_offer_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

delegate_dispatch!(RuntimeState, WlDataOffer, CoreOfferData);
delegate_dispatch!(RuntimeState, ZwpPrimarySelectionOfferV1, SelectionOfferData);
delegate_dispatch!(RuntimeState, ZwlrDataControlOfferV1, SelectionOfferData);
delegate_dispatch!(RuntimeState, ExtDataControlOfferV1, SelectionOfferData);
