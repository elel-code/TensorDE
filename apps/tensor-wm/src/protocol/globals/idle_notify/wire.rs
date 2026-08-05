use wayland_protocols::ext::idle_notify::v1::server::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::{self, ExtIdleNotifierV1},
};
use wayland_server::{Client, DataInit, DisplayHandle, New, backend::ClientId};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

use super::NotificationKind;

#[derive(Debug)]
pub(in crate::protocol) struct IdleNotifyGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct IdleNotifyManagerData;

#[derive(Debug)]
pub(in crate::protocol) struct IdleNotificationData;

impl GlobalDispatchDelegate<ExtIdleNotifierV1, RuntimeState> for IdleNotifyGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ExtIdleNotifierV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        data_init.init(resource, IdleNotifyManagerData);
    }
}

impl DispatchDelegate<ExtIdleNotifierV1, RuntimeState> for IdleNotifyManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        _manager: &ExtIdleNotifierV1,
        request: ext_idle_notifier_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let (id, timeout, seat, kind) = match request {
            ext_idle_notifier_v1::Request::GetIdleNotification { id, timeout, seat } => {
                (id, timeout, seat, NotificationKind::Inhibitable)
            }
            ext_idle_notifier_v1::Request::GetInputIdleNotification { id, timeout, seat } => {
                (id, timeout, seat, NotificationKind::InputOnly)
            }
            ext_idle_notifier_v1::Request::Destroy => return,
            _ => unreachable!(),
        };
        if !state.protocol_globals.seat.owns(&seat) {
            return;
        }
        let notification = data_init.init(id, IdleNotificationData);
        state
            .protocol_globals
            .idle_notify
            .register(notification, timeout, kind);
    }
}

impl DispatchDelegate<ExtIdleNotificationV1, RuntimeState> for IdleNotificationData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _notification: &ExtIdleNotificationV1,
        request: ext_idle_notification_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            ext_idle_notification_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        notification: &ExtIdleNotificationV1,
    ) {
        state.protocol_globals.idle_notify.remove(notification);
    }
}

delegate_global_dispatch!(RuntimeState, ExtIdleNotifierV1, IdleNotifyGlobalData);
delegate_dispatch!(RuntimeState, ExtIdleNotifierV1, IdleNotifyManagerData);
delegate_dispatch!(RuntimeState, ExtIdleNotificationV1, IdleNotificationData);
