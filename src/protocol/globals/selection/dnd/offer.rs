use std::sync::{Arc, Mutex};

use smithay::input::dnd::{DndAction, Source};
use wayland_server::{
    Resource,
    protocol::{
        wl_data_device::WlDataDevice,
        wl_data_device_manager::DndAction as WlDndAction,
        wl_data_offer::{self, WlDataOffer},
    },
};

use super::{DndOfferObject, DndOfferState, SurfaceDndOffer};

pub(super) fn local<S: Source>(source: Arc<S>, devices: Vec<WlDataDevice>) -> SurfaceDndOffer<S> {
    SurfaceDndOffer {
        state: Arc::new(Mutex::new(DndOfferState {
            active: true,
            dropped: false,
            accepted: true,
            finished: false,
            requires_accept: false,
            requires_action: false,
            source_actions: WlDndAction::empty(),
            chosen_action: WlDndAction::empty(),
        })),
        source: Arc::new(Mutex::new(Some(source))),
        devices,
        _offers: Vec::new(),
    }
}

pub(super) fn handle_request<S: Source>(
    offer: &WlDataOffer,
    request: wl_data_offer::Request,
    data: &DndOfferObject<S>,
) {
    match request {
        wl_data_offer::Request::Accept { mime_type, .. } => {
            let mut state = data.state.lock().unwrap();
            if !state.active || state.finished {
                offer.post_error(
                    wl_data_offer::Error::InvalidFinish,
                    "accept sent to an inactive drag offer",
                );
                return;
            }
            let accepted_mime =
                mime_type.filter(|mime| data.mime_types.iter().any(|known| known == mime));
            state.accepted = accepted_mime.is_some();
            drop(state);
            if let Some(source) = data
                .target_source
                .as_ref()
                .and_then(|source| source.upgrade().ok())
            {
                source.target(accepted_mime);
            }
        }
        wl_data_offer::Request::Receive { mime_type, fd } => {
            let active = data.state.lock().unwrap().active;
            if active
                && data.mime_types.iter().any(|known| known == &mime_type)
                && let Some(source) = data.source.lock().unwrap().as_ref()
                && source.alive()
            {
                source.send(&mime_type, fd);
            }
        }
        wl_data_offer::Request::Destroy => {}
        wl_data_offer::Request::Finish => {
            let mut state = data.state.lock().unwrap();
            if !state.ready_to_finish() {
                offer.post_error(
                    wl_data_offer::Error::InvalidFinish,
                    "drag offer is not ready to finish",
                );
                return;
            }
            state.active = false;
            state.finished = true;
            if let Some(source) = data.source.lock().unwrap().take() {
                source.finished();
            }
        }
        wl_data_offer::Request::SetActions {
            dnd_actions,
            preferred_action,
        } => {
            let Ok(dnd_actions) = dnd_actions.into_result() else {
                offer.post_error(
                    wl_data_offer::Error::InvalidActionMask,
                    "drag action mask contains unknown bits",
                );
                return;
            };
            let Ok(preferred_action) = preferred_action.into_result() else {
                offer.post_error(
                    wl_data_offer::Error::InvalidAction,
                    "preferred drag action is unknown",
                );
                return;
            };
            let mut state = data.state.lock().unwrap();
            if !state.active || state.finished {
                offer.post_error(
                    wl_data_offer::Error::InvalidFinish,
                    "actions sent to an inactive drag offer",
                );
                return;
            }
            let Ok(chosen) = negotiate_action(state.source_actions, dnd_actions, preferred_action)
            else {
                offer.post_error(
                    wl_data_offer::Error::InvalidAction,
                    "preferred action must be one source and destination action",
                );
                return;
            };
            if chosen != state.chosen_action {
                state.chosen_action = chosen;
                if offer.version() >= wl_data_offer::EVT_ACTION_SINCE {
                    offer.action(chosen);
                }
                if let Some(source) = data.source.lock().unwrap().as_ref() {
                    source.choose_action(from_wire_action(chosen));
                }
            }
        }
        _ => unreachable!(),
    }
}

fn from_wire_action(action: WlDndAction) -> DndAction {
    match action {
        WlDndAction::Copy => DndAction::Copy,
        WlDndAction::Move => DndAction::Move,
        WlDndAction::Ask => DndAction::Ask,
        _ => DndAction::None,
    }
}

fn valid_single_action(action: WlDndAction) -> bool {
    action.is_empty()
        || action == WlDndAction::Copy
        || action == WlDndAction::Move
        || action == WlDndAction::Ask
}

fn negotiate_action(
    source: WlDndAction,
    destination: WlDndAction,
    preferred: WlDndAction,
) -> Result<WlDndAction, ()> {
    if !valid_single_action(preferred)
        || (!preferred.is_empty()
            && (!source.contains(preferred) || !destination.contains(preferred)))
    {
        return Err(());
    }
    Ok(choose_action(source & destination, preferred))
}

fn choose_action(possible: WlDndAction, preferred: WlDndAction) -> WlDndAction {
    if !preferred.is_empty() && possible.contains(preferred) {
        preferred
    } else if possible.contains(WlDndAction::Copy) {
        WlDndAction::Copy
    } else if possible.contains(WlDndAction::Move) {
        WlDndAction::Move
    } else if possible.contains(WlDndAction::Ask) {
        WlDndAction::Ask
    } else {
        WlDndAction::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finishable_state() -> DndOfferState {
        DndOfferState {
            active: true,
            dropped: true,
            accepted: true,
            finished: false,
            requires_accept: true,
            requires_action: true,
            source_actions: WlDndAction::Copy | WlDndAction::Move,
            chosen_action: WlDndAction::Copy,
        }
    }

    #[test]
    fn finish_requires_active_drop_accept_and_action() {
        assert!(finishable_state().ready_to_finish());

        let mut state = finishable_state();
        state.active = false;
        assert!(!state.ready_to_finish());
        let mut state = finishable_state();
        state.dropped = false;
        assert!(!state.ready_to_finish());
        let mut state = finishable_state();
        state.accepted = false;
        assert!(!state.ready_to_finish());
        let mut state = finishable_state();
        state.chosen_action = WlDndAction::empty();
        assert!(!state.ready_to_finish());
        let mut state = finishable_state();
        state.finished = true;
        assert!(!state.ready_to_finish());
    }

    #[test]
    fn action_negotiation_rejects_unadvertised_preference() {
        let source = WlDndAction::Copy | WlDndAction::Move;
        let destination = WlDndAction::Copy | WlDndAction::Ask;

        assert_eq!(
            negotiate_action(source, destination, WlDndAction::Copy),
            Ok(WlDndAction::Copy)
        );
        assert!(negotiate_action(source, destination, WlDndAction::Move).is_err());
        assert!(negotiate_action(source, destination, WlDndAction::Ask).is_err());
        assert!(
            negotiate_action(source, destination, WlDndAction::Copy | WlDndAction::Move,).is_err()
        );
    }
}
