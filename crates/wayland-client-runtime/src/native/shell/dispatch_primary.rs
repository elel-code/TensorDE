//! Primary selection (middle-click paste) dispatch.
//!
//! Behavioral reference: smithay-client-toolkit `primary_selection` module
//! (pending offer → selection attach, resist faulty compositors). Source was
//! not copied; only the protocol state machine is mirrored.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols::wp::primary_selection::zv1::client::{
    zwp_primary_selection_device_manager_v1, zwp_primary_selection_device_v1,
    zwp_primary_selection_offer_v1, zwp_primary_selection_source_v1,
};

use super::types::{NativeShellEvent, NativeShellState};
use crate::data_transfer::spawn_write_fd;

impl Dispatch<zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1, ()>
    for NativeShellState
{
    fn event(
        _: &mut Self,
        _: &zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1,
        _: zwp_primary_selection_device_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1, ()>
    for NativeShellState
{
    // Opcode 0 = data_offer creates a new primary selection offer child.
    event_created_child!(NativeShellState, zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1, [
        0 => (zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1, ())
    ]);

    fn event(
        state: &mut Self,
        _: &zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
        event: zwp_primary_selection_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_primary_selection_device_v1::Event::DataOffer { offer } => {
                // Resist faulty compositors that send offer without selection.
                if let Some(pending) = state.primary_pending_offer.take() {
                    let id = pending.id().protocol_id();
                    state.primary_offer_mimes.remove(&id);
                    pending.destroy();
                }
                let offer_id = offer.id().protocol_id();
                state.primary_offer_mimes.entry(offer_id).or_default();
                state.primary_pending_offer = Some(offer);
            }
            zwp_primary_selection_device_v1::Event::Selection { id } => {
                if let Some(old) = state.primary_offer.take() {
                    let old_id = old.id().protocol_id();
                    state.primary_offer_mimes.remove(&old_id);
                    old.destroy();
                }
                state.primary_mimes.clear();
                match id {
                    Some(offer) => {
                        let offer_id = offer.id().protocol_id();
                        // Prefer pending offer if it matches (SCTK pattern).
                        if state
                            .primary_pending_offer
                            .as_ref()
                            .is_some_and(|p| p.id() == offer.id())
                        {
                            let _ = state.primary_pending_offer.take();
                        } else if let Some(pending) = state.primary_pending_offer.take() {
                            let pid = pending.id().protocol_id();
                            state.primary_offer_mimes.remove(&pid);
                            pending.destroy();
                        }
                        let mimes = state
                            .primary_offer_mimes
                            .remove(&offer_id)
                            .unwrap_or_default();
                        state.primary_mimes = mimes.clone();
                        state.primary_offer = Some(offer);
                        state.push(NativeShellEvent::PrimarySelection { mimes });
                    }
                    None => {
                        if let Some(pending) = state.primary_pending_offer.take() {
                            let pid = pending.id().protocol_id();
                            state.primary_offer_mimes.remove(&pid);
                            pending.destroy();
                        }
                        state.push(NativeShellEvent::PrimarySelection { mimes: Vec::new() });
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        offer: &zwp_primary_selection_offer_v1::ZwpPrimarySelectionOfferV1,
        event: zwp_primary_selection_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_primary_selection_offer_v1::Event::Offer { mime_type } = event {
            let offer_id = offer.id().protocol_id();
            let mimes = state.primary_offer_mimes.entry(offer_id).or_default();
            if !mimes.iter().any(|m| m == &mime_type) {
                mimes.push(mime_type.clone());
            }
            if state
                .primary_offer
                .as_ref()
                .is_some_and(|o| o.id() == offer.id())
                && !state.primary_mimes.iter().any(|m| m == &mime_type)
            {
                state.primary_mimes.push(mime_type);
            }
        }
    }
}

impl Dispatch<zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1, ()>
    for NativeShellState
{
    fn event(
        state: &mut Self,
        source: &zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
        event: zwp_primary_selection_source_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_primary_selection_source_v1::Event::Send { mime_type, fd } => {
                if state
                    .primary_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id())
                    && let Some(bytes) = state
                        .primary_content
                        .as_ref()
                        .and_then(|c| c.bytes_for_mime(&mime_type))
                {
                    spawn_write_fd("fika-wl-primary-send", fd, bytes);
                }
            }
            zwp_primary_selection_source_v1::Event::Cancelled
                if state
                    .primary_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id()) =>
            {
                state.primary_source = None;
                state.primary_content = None;
                state.push(NativeShellEvent::PrimarySelectionCancelled);
            }
            _ => {}
        }
    }
}
