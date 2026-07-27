//! `ext-idle-notify-v1` helpers on [`NativeShell`].

use wayland_client::Proxy;

use super::api::NativeShell;
use crate::native::connection::NativeError;

/// How an idle notification treats idle inhibitors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdleNotifyKind {
    /// `get_idle_notification` — respects `zwp_idle_inhibitor_v1`.
    WithInhibitors,
    /// `get_input_idle_notification` (v2) — input activity only; ignores inhibitors.
    InputOnly,
}

impl NativeShell {
    pub fn has_idle_notify(&self) -> bool {
        self.state.idle_notifier.is_some()
    }

    pub fn has_idle_notify_input(&self) -> bool {
        self.state
            .idle_notifier
            .as_ref()
            .is_some_and(|n| n.version() >= 2)
    }

    /// Create a seat-scoped idle notification.
    ///
    /// Emits [`super::types::NativeShellEvent::IdleNotify`] when the seat goes
    /// idle / resumes. Returns a client-local id for [`Self::destroy_idle_notification`].
    pub fn create_idle_notification(
        &mut self,
        timeout_ms: u32,
        seat: Option<crate::SeatId>,
        kind: IdleNotifyKind,
    ) -> Result<u64, NativeError> {
        let notifier = self
            .state
            .idle_notifier
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("ext_idle_notifier_v1 missing".into()))?
            .clone();
        let wl_seat = if let Some(id) = seat {
            self.state
                .seats
                .get(&id.get())
                .map(|r| r.seat.clone())
                .ok_or_else(|| NativeError::Protocol(format!("unknown seat {}", id.get())))?
        } else {
            self.state
                .seat
                .clone()
                .ok_or_else(|| NativeError::Protocol("no seat".into()))?
        };
        let qh = self.queue.handle();
        let notification = match kind {
            IdleNotifyKind::WithInhibitors => {
                notifier.get_idle_notification(timeout_ms, &wl_seat, &qh, ())
            }
            IdleNotifyKind::InputOnly => {
                if notifier.version() < 2 {
                    return Err(NativeError::Protocol(
                        "ext_idle_notifier_v1 v2 required for input-only idle".into(),
                    ));
                }
                notifier.get_input_idle_notification(timeout_ms, &wl_seat, &qh, ())
            }
        };
        let id = self.state.next_idle_notification_id;
        self.state.next_idle_notification_id = id.saturating_add(1);
        self.state
            .idle_notification_objects
            .insert(notification.id().protocol_id(), id);
        self.state.idle_notifications.insert(id, notification);
        self.connection.mark_dirty();
        Ok(id)
    }

    pub fn destroy_idle_notification(&mut self, id: u64) -> Result<(), NativeError> {
        let Some(notification) = self.state.idle_notifications.remove(&id) else {
            return Err(NativeError::Protocol(format!(
                "unknown idle notification {id}"
            )));
        };
        self.state
            .idle_notification_objects
            .remove(&notification.id().protocol_id());
        notification.destroy();
        self.connection.mark_dirty();
        Ok(())
    }
}
