use std::{
    collections::{BTreeMap, VecDeque},
    num::NonZeroU64,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NotificationId(u32);

impl NotificationId {
    pub const fn from_raw(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NotificationUrgency {
    Low,
    #[default]
    Normal,
    Critical,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NotificationTimeout {
    #[default]
    Default,
    Never,
    Milliseconds(NonZeroU64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationAction {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NotificationRequest {
    pub replaces: Option<NotificationId>,
    pub app_name: String,
    pub desktop_entry: Option<String>,
    pub summary: String,
    pub body: String,
    pub icon: Option<String>,
    pub urgency: NotificationUrgency,
    pub timeout: NotificationTimeout,
    pub transient: bool,
    pub actions: Vec<NotificationAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Notification {
    pub id: NotificationId,
    pub app_name: String,
    pub desktop_entry: Option<String>,
    pub summary: String,
    pub body: String,
    pub icon: Option<String>,
    pub urgency: NotificationUrgency,
    pub timeout: NotificationTimeout,
    pub transient: bool,
    pub actions: Vec<NotificationAction>,
    pub received_at_ms: u64,
}

impl Notification {
    pub fn group_key(&self) -> &str {
        if !self.app_name.is_empty() {
            &self.app_name
        } else if let Some(entry) = self
            .desktop_entry
            .as_deref()
            .filter(|entry| !entry.is_empty())
        {
            entry
        } else {
            "System"
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseReason {
    Expired,
    DismissedByUser,
    ClosedByApplication,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClosedNotification {
    pub id: NotificationId,
    pub reason: CloseReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotificationStoreConfig {
    pub max_history: usize,
    pub max_visible_popups: usize,
    pub max_queued_popups: usize,
    pub low_timeout_ms: NonZeroU64,
    pub normal_timeout_ms: NonZeroU64,
}

impl Default for NotificationStoreConfig {
    fn default() -> Self {
        Self {
            max_history: 500,
            max_visible_popups: 4,
            max_queued_popups: 32,
            low_timeout_ms: NonZeroU64::new(4_000).unwrap(),
            normal_timeout_ms: NonZeroU64::new(7_000).unwrap(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NotificationStoreError {
    #[error("notification history capacity must be non-zero")]
    EmptyHistory,
    #[error("visible notification popup capacity must be non-zero")]
    EmptyVisiblePopups,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VisiblePopup {
    id: NotificationId,
    deadline_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct QueuedPopup {
    id: NotificationId,
    timeout: NotificationTimeout,
    urgency: NotificationUrgency,
}

#[derive(Debug)]
pub struct NotificationStore {
    config: NotificationStoreConfig,
    next_id: u32,
    revision: u64,
    do_not_disturb: bool,
    active: BTreeMap<NotificationId, Notification>,
    history: VecDeque<Notification>,
    visible: VecDeque<VisiblePopup>,
    queued: VecDeque<QueuedPopup>,
}

impl NotificationStore {
    pub fn new(config: NotificationStoreConfig) -> Result<Self, NotificationStoreError> {
        if config.max_history == 0 {
            return Err(NotificationStoreError::EmptyHistory);
        }
        if config.max_visible_popups == 0 {
            return Err(NotificationStoreError::EmptyVisiblePopups);
        }
        Ok(Self {
            config,
            next_id: 1,
            revision: 0,
            do_not_disturb: false,
            active: BTreeMap::new(),
            history: VecDeque::with_capacity(config.max_history),
            visible: VecDeque::with_capacity(config.max_visible_popups),
            queued: VecDeque::with_capacity(config.max_queued_popups),
        })
    }

    pub fn notify(&mut self, request: NotificationRequest, now_ms: u64) -> NotificationId {
        let replacement = request
            .replaces
            .filter(|candidate| self.active.contains_key(candidate));
        let id = replacement.unwrap_or_else(|| self.allocate_id());
        let notification = Notification {
            id,
            app_name: request.app_name,
            desktop_entry: request.desktop_entry,
            summary: request.summary,
            body: request.body,
            icon: request.icon,
            urgency: request.urgency,
            timeout: request.timeout,
            transient: request.transient,
            actions: request.actions,
            received_at_ms: now_ms,
        };
        self.active.insert(id, notification.clone());
        self.remove_popup(id);
        if !notification.transient {
            self.history.retain(|entry| entry.id != id);
            self.history.push_front(notification.clone());
            self.trim_history();
        }
        if !self.do_not_disturb || notification.urgency == NotificationUrgency::Critical {
            self.enqueue_popup(&notification, now_ms);
        }
        self.bump_revision();
        id
    }

    pub fn close(
        &mut self,
        id: NotificationId,
        reason: CloseReason,
        now_ms: u64,
    ) -> Option<ClosedNotification> {
        self.active.remove(&id)?;
        self.remove_popup(id);
        self.promote_popups(now_ms);
        self.bump_revision();
        Some(ClosedNotification { id, reason })
    }

    pub fn expire(&mut self, now_ms: u64) -> Vec<ClosedNotification> {
        let expired = self
            .visible
            .iter()
            .filter(|popup| popup.deadline_ms.is_some_and(|deadline| deadline <= now_ms))
            .map(|popup| popup.id)
            .collect::<Vec<_>>();
        expired
            .into_iter()
            .filter_map(|id| self.close(id, CloseReason::Expired, now_ms))
            .collect()
    }

    pub fn set_do_not_disturb(&mut self, enabled: bool, now_ms: u64) {
        if self.do_not_disturb == enabled {
            return;
        }
        self.do_not_disturb = enabled;
        if enabled {
            self.visible.retain(|popup| {
                self.active
                    .get(&popup.id)
                    .is_some_and(|entry| entry.urgency == NotificationUrgency::Critical)
            });
            self.queued
                .retain(|popup| popup.urgency == NotificationUrgency::Critical);
        }
        self.promote_popups(now_ms);
        self.bump_revision();
    }

    pub const fn do_not_disturb(&self) -> bool {
        self.do_not_disturb
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn has_active_critical(&self) -> bool {
        self.active
            .values()
            .any(|notification| notification.urgency == NotificationUrgency::Critical)
    }

    pub fn active(&self, id: NotificationId) -> Option<&Notification> {
        self.active.get(&id)
    }

    pub fn history(&self) -> impl Iterator<Item = &Notification> {
        self.history.iter()
    }

    pub fn visible_popups(&self) -> impl Iterator<Item = &Notification> {
        self.visible
            .iter()
            .filter_map(|popup| self.active.get(&popup.id))
    }

    pub fn queued_popup_count(&self) -> usize {
        self.queued.len()
    }

    pub fn groups(&self) -> Vec<NotificationGroup<'_>> {
        let mut indexes = BTreeMap::<&str, usize>::new();
        let mut groups = Vec::<NotificationGroup<'_>>::new();
        for notification in &self.history {
            let key = notification.group_key();
            if let Some(index) = indexes.get(key).copied() {
                groups[index].notifications.push(notification);
            } else {
                indexes.insert(key, groups.len());
                groups.push(NotificationGroup {
                    key,
                    notifications: vec![notification],
                });
            }
        }
        groups
    }

    pub fn dismiss_history(&mut self, id: NotificationId) -> bool {
        let previous_len = self.history.len();
        self.history.retain(|entry| entry.id != id);
        let changed = previous_len != self.history.len();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn clear_history(&mut self) {
        if self.history.is_empty() {
            return;
        }
        self.history.clear();
        self.bump_revision();
    }

    fn allocate_id(&mut self) -> NotificationId {
        loop {
            let candidate = NotificationId(self.next_id.max(1));
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if !self.active.contains_key(&candidate)
                && !self.history.iter().any(|entry| entry.id == candidate)
            {
                return candidate;
            }
        }
    }

    fn enqueue_popup(&mut self, notification: &Notification, now_ms: u64) {
        if self.visible.len() < self.config.max_visible_popups {
            let deadline_ms = self.deadline(notification.timeout, notification.urgency, now_ms);
            self.visible.push_back(VisiblePopup {
                id: notification.id,
                deadline_ms,
            });
            return;
        }
        if self.config.max_queued_popups == 0 {
            return;
        }
        if self.queued.len() == self.config.max_queued_popups {
            let evict = self
                .queued
                .iter()
                .position(|popup| popup.urgency != NotificationUrgency::Critical)
                .unwrap_or(0);
            self.queued.remove(evict);
        }
        self.queued.push_back(QueuedPopup {
            id: notification.id,
            timeout: notification.timeout,
            urgency: notification.urgency,
        });
    }

    fn promote_popups(&mut self, now_ms: u64) {
        while self.visible.len() < self.config.max_visible_popups {
            let Some(popup) = self.queued.pop_front() else {
                break;
            };
            if self.do_not_disturb && popup.urgency != NotificationUrgency::Critical {
                continue;
            }
            if !self.active.contains_key(&popup.id) {
                continue;
            }
            let deadline_ms = self.deadline(popup.timeout, popup.urgency, now_ms);
            self.visible.push_back(VisiblePopup {
                id: popup.id,
                deadline_ms,
            });
        }
    }

    fn remove_popup(&mut self, id: NotificationId) {
        self.visible.retain(|popup| popup.id != id);
        self.queued.retain(|popup| popup.id != id);
    }

    fn deadline(
        &self,
        timeout: NotificationTimeout,
        urgency: NotificationUrgency,
        now_ms: u64,
    ) -> Option<u64> {
        let duration = match (timeout, urgency) {
            (NotificationTimeout::Never, _) | (_, NotificationUrgency::Critical) => return None,
            (NotificationTimeout::Milliseconds(duration), _) => duration,
            (NotificationTimeout::Default, NotificationUrgency::Low) => self.config.low_timeout_ms,
            (NotificationTimeout::Default, NotificationUrgency::Normal) => {
                self.config.normal_timeout_ms
            }
        };
        Some(now_ms.saturating_add(duration.get()))
    }

    fn trim_history(&mut self) {
        while self.history.len() > self.config.max_history {
            self.history.pop_back();
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

impl Default for NotificationStore {
    fn default() -> Self {
        Self::new(NotificationStoreConfig::default()).unwrap()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct NotificationGroup<'a> {
    pub key: &'a str,
    pub notifications: Vec<&'a Notification>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(app: &str, summary: &str) -> NotificationRequest {
        NotificationRequest {
            app_name: app.into(),
            summary: summary.into(),
            ..NotificationRequest::default()
        }
    }

    #[test]
    fn replacement_keeps_id_and_updates_history_and_popup() {
        let mut store = NotificationStore::default();
        let id = store.notify(request("Mail", "First"), 10);
        let mut replacement = request("Mail", "Updated");
        replacement.replaces = Some(id);
        assert_eq!(store.notify(replacement, 20), id);
        assert_eq!(store.active(id).unwrap().summary, "Updated");
        assert_eq!(store.history().count(), 1);
        assert_eq!(store.visible_popups().count(), 1);
    }

    #[test]
    fn history_is_grouped_by_application_in_recency_order() {
        let mut store = NotificationStore::default();
        store.notify(request("Chat", "Old"), 10);
        store.notify(request("Mail", "Message"), 20);
        store.notify(request("Chat", "New"), 30);
        let groups = store.groups();
        assert_eq!(groups[0].key, "Chat");
        assert_eq!(groups[0].notifications.len(), 2);
        assert_eq!(groups[0].notifications[0].summary, "New");
        assert_eq!(groups[1].key, "Mail");
    }

    #[test]
    fn do_not_disturb_suppresses_normal_popups_but_not_history_or_critical() {
        let mut store = NotificationStore::default();
        store.set_do_not_disturb(true, 0);
        store.notify(request("Chat", "Quiet"), 10);
        let mut critical = request("System", "Battery critical");
        critical.urgency = NotificationUrgency::Critical;
        store.notify(critical, 20);
        assert_eq!(store.history().count(), 2);
        let popups = store.visible_popups().collect::<Vec<_>>();
        assert_eq!(popups.len(), 1);
        assert_eq!(popups[0].summary, "Battery critical");
    }

    #[test]
    fn popup_capacity_queues_and_promotes_after_expiry() {
        let config = NotificationStoreConfig {
            max_visible_popups: 1,
            ..NotificationStoreConfig::default()
        };
        let mut store = NotificationStore::new(config).unwrap();
        let first = store.notify(request("Mail", "First"), 0);
        store.notify(request("Chat", "Second"), 1);
        assert_eq!(store.visible_popups().next().unwrap().id, first);
        assert_eq!(store.queued_popup_count(), 1);
        assert_eq!(
            store.expire(7_000),
            vec![ClosedNotification {
                id: first,
                reason: CloseReason::Expired,
            }]
        );
        assert_eq!(store.visible_popups().next().unwrap().summary, "Second");
    }

    #[test]
    fn transient_notification_never_enters_history() {
        let mut store = NotificationStore::default();
        let mut transient = request("Volume", "50%");
        transient.transient = true;
        store.notify(transient, 0);
        assert_eq!(store.history().count(), 0);
        assert_eq!(store.visible_popups().count(), 1);
    }

    #[test]
    fn history_capacity_discards_the_oldest_entry() {
        let config = NotificationStoreConfig {
            max_history: 2,
            ..NotificationStoreConfig::default()
        };
        let mut store = NotificationStore::new(config).unwrap();
        store.notify(request("One", "1"), 1);
        store.notify(request("Two", "2"), 2);
        store.notify(request("Three", "3"), 3);
        assert_eq!(
            store
                .history()
                .map(|entry| entry.summary.as_str())
                .collect::<Vec<_>>(),
            ["3", "2"]
        );
    }

    #[test]
    fn revision_changes_only_when_notification_state_changes() {
        let mut store = NotificationStore::default();
        assert_eq!(store.revision(), 0);
        let id = store.notify(request("Mail", "New"), 0);
        assert_eq!(store.revision(), 1);
        assert!(!store.dismiss_history(NotificationId::from_raw(99).unwrap()));
        assert_eq!(store.revision(), 1);
        assert!(store.dismiss_history(id));
        assert_eq!(store.revision(), 2);
        assert!(store.close(id, CloseReason::DismissedByUser, 0).is_some());
        assert_eq!(store.revision(), 3);
    }
}
