//! Data device (clipboard + drag-and-drop) dispatch for the native shell.

use wayland_client::protocol::{
    wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};

use super::types::{NativeShellEvent, NativeShellState};
use crate::data_transfer::spawn_write_fd;

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &wl_data_device_manager::WlDataDeviceManager,
        _: wl_data_device_manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for NativeShellState {
    // Opcode 0 = data_offer creates a new wl_data_offer child object.
    event_created_child!(NativeShellState, wl_data_device::WlDataDevice, [
        0 => (wl_data_offer::WlDataOffer, ())
    ]);

    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                let offer_id = id.id().protocol_id();
                state.offer_mimes.entry(offer_id).or_default();
            }
            wl_data_device::Event::Selection { id } => {
                if let Some(old) = state.incoming_offer.take() {
                    let old_id = old.id().protocol_id();
                    state.offer_mimes.remove(&old_id);
                    old.destroy();
                }
                state.incoming_mimes.clear();
                if let Some(offer) = id {
                    let offer_id = offer.id().protocol_id();
                    // Move mimes out of the map once; share with event via one clone.
                    let mimes = state.offer_mimes.remove(&offer_id).unwrap_or_default();
                    state.incoming_mimes = mimes.clone();
                    state.incoming_offer = Some(offer);
                    state.push(NativeShellEvent::Selection { mimes });
                } else {
                    state.push(NativeShellEvent::Selection { mimes: Vec::new() });
                }
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => {
                state.dnd_serial = Some(serial);
                // A new enter supersedes any prior offer (including a dropped
                // one that was never finished).
                if let Some(old) = state.dnd_offer.take() {
                    let old_id = old.id().protocol_id();
                    state.offer_mimes.remove(&old_id);
                    old.destroy();
                }
                state.dnd_mimes.clear();
                state.dnd_offer_id = None;
                state.dnd_dropped = false;
                let surface_id = state
                    .wl_surface_objects
                    .get(&surface.id().protocol_id())
                    .copied();
                state.dnd_focus = surface_id;
                if let Some(offer) = id {
                    let offer_obj = offer.id().protocol_id();
                    // Own the mime list once: move into dnd_mimes, clone only for the
                    // public event (and one string for accept if needed).
                    let mimes = state.offer_mimes.remove(&offer_obj).unwrap_or_default();
                    if let Some(mime) = mimes.first().cloned() {
                        offer.accept(serial, Some(mime));
                    }
                    offer.set_actions(
                        wayland_client::protocol::wl_data_device_manager::DndAction::Copy
                            | wayland_client::protocol::wl_data_device_manager::DndAction::Move,
                        wayland_client::protocol::wl_data_device_manager::DndAction::Copy,
                    );
                    let public_id = state.alloc_transfer_id();
                    state.dnd_offer_id = Some(public_id);
                    state.dnd_offer = Some(offer);
                    if let Some(surface) = surface_id {
                        // Event takes ownership; shell retains a clone for receive checks.
                        state.dnd_mimes = mimes.clone();
                        state.push(NativeShellEvent::DndEnter {
                            offer: public_id,
                            surface,
                            x,
                            y,
                            mimes,
                        });
                    } else {
                        state.dnd_mimes = mimes;
                    }
                }
            }
            wl_data_device::Event::Leave => {
                let offer = state.dnd_offer_id.unwrap_or(0);
                let surface = state.dnd_focus;
                if state.dnd_dropped {
                    // Successful drop already happened. Keep the offer alive for
                    // receive/finish; only clear surface focus. Compositors often
                    // send leave after drop even though the transfer continues.
                    state.dnd_focus = None;
                    state.push(NativeShellEvent::DndLeave { offer, surface });
                } else {
                    // Cancel / leave-without-drop: destroy the offer now.
                    if let Some(old) = state.dnd_offer.take() {
                        let old_id = old.id().protocol_id();
                        state.offer_mimes.remove(&old_id);
                        old.destroy();
                    }
                    state.dnd_mimes.clear();
                    state.dnd_focus = None;
                    state.dnd_serial = None;
                    state.dnd_offer_id = None;
                    state.dnd_dropped = false;
                    state.push(NativeShellEvent::DndLeave { offer, surface });
                }
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                let offer = state.dnd_offer_id.unwrap_or(0);
                state.push(NativeShellEvent::DndMotion { offer, x, y });
            }
            wl_data_device::Event::Drop => {
                let offer = state.dnd_offer_id.unwrap_or(0);
                state.dnd_dropped = true;
                state.push(NativeShellEvent::DndDrop { offer });
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_offer::Event::Offer { mime_type } => {
                let offer_id = offer.id().protocol_id();
                let mimes = state.offer_mimes.entry(offer_id).or_default();
                if mimes.iter().any(|m| m == &mime_type) {
                    return;
                }
                let is_incoming = state
                    .incoming_offer
                    .as_ref()
                    .is_some_and(|o| o.id() == offer.id());
                let is_dnd = state
                    .dnd_offer
                    .as_ref()
                    .is_some_and(|o| o.id() == offer.id());
                // Clone only into live mirrors; map entry takes ownership last.
                if is_incoming {
                    state.incoming_mimes.push(mime_type.clone());
                }
                if is_dnd {
                    state.dnd_mimes.push(mime_type.clone());
                }
                mimes.push(mime_type);
            }
            wl_data_offer::Event::SourceActions { .. } | wl_data_offer::Event::Action { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        source: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                let bytes = if state
                    .selection_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id())
                {
                    state
                        .selection_content
                        .as_ref()
                        .and_then(|c| c.bytes_for_mime(&mime_type))
                } else if state
                    .dnd_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id())
                {
                    state
                        .dnd_source_content
                        .as_ref()
                        .and_then(|c| c.bytes_for_mime(&mime_type))
                } else {
                    None
                };
                if let Some(bytes) = bytes {
                    // Never write on the dispatch thread: large payloads or a
                    // peer that is itself blocked on our event loop will hang.
                    spawn_write_fd("fika-wl-data-source-send", fd, bytes);
                }
            }
            wl_data_source::Event::Cancelled => {
                if state
                    .selection_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id())
                {
                    state.selection_source = None;
                    state.selection_content = None;
                    state.push(NativeShellEvent::SelectionCancelled);
                }
                if state
                    .dnd_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id())
                {
                    let source_id = state.dnd_source_id.unwrap_or(0);
                    state.dnd_source = None;
                    state.dnd_source_id = None;
                    state.dnd_source_content = None;
                    state.dnd_icon = None;
                    state.push(NativeShellEvent::DndFinished {
                        source: source_id,
                        cancelled: true,
                    });
                }
            }
            wl_data_source::Event::DndFinished
                if state
                    .dnd_source
                    .as_ref()
                    .is_some_and(|s| s.id() == source.id()) =>
            {
                let source_id = state.dnd_source_id.unwrap_or(0);
                state.dnd_source = None;
                state.dnd_source_id = None;
                state.dnd_source_content = None;
                state.dnd_icon = None;
                state.push(NativeShellEvent::DndFinished {
                    source: source_id,
                    cancelled: false,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "dispatch_data_tests.rs"]
mod tests;
