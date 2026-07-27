//! ext-idle-notify-v1 dispatch.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};

use super::types::{NativeShellEvent, NativeShellState};

impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for NativeShellState {
    fn event(
        _: &mut Self,
        _: &ext_idle_notifier_v1::ExtIdleNotifierV1,
        _: ext_idle_notifier_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        notification: &ext_idle_notification_v1::ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(id) = state
            .idle_notification_objects
            .get(&notification.id().protocol_id())
            .copied()
        else {
            return;
        };
        match event {
            ext_idle_notification_v1::Event::Idled => {
                state.push(NativeShellEvent::IdleNotify { id, idle: true });
            }
            ext_idle_notification_v1::Event::Resumed => {
                state.push(NativeShellEvent::IdleNotify { id, idle: false });
            }
            _ => {}
        }
    }
}
