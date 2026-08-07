use vulkan_renderer::{Extent2D, Rect2D};
use wayland_client_runtime::{LogicalRect, LogicalSize};

use crate::panel::PanelDraw;
use crate::{Notification, NotificationId, NotificationStore, NotificationUrgency};

const INSET: u32 = 16;
const GAP: u32 = 8;
const CARD_HEIGHT: u32 = 88;
const MAX_SCENE_CARDS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NotificationSceneKind {
    Center,
    Popups,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NotificationHit {
    Dismiss(NotificationId),
    Action(NotificationId),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NotificationInteraction {
    pub hovered: Option<NotificationHit>,
    pub pressed: Option<NotificationHit>,
    pub focused: Option<NotificationHit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NotificationCard {
    id: NotificationId,
    bounds: LogicalRect,
    dismiss: LogicalRect,
    action_key: Option<String>,
    urgency: NotificationUrgency,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NotificationScene {
    extent: LogicalSize,
    kind: NotificationSceneKind,
    cards: Vec<NotificationCard>,
}

impl NotificationScene {
    pub(crate) fn from_store(
        extent: LogicalSize,
        store: &NotificationStore,
        kind: NotificationSceneKind,
    ) -> Self {
        let notifications = match kind {
            NotificationSceneKind::Center => store
                .history()
                .map(|notification| (notification, store.active(notification.id).is_some()))
                .collect::<Vec<_>>(),
            NotificationSceneKind::Popups => store
                .visible_popups()
                .map(|notification| (notification, true))
                .collect::<Vec<_>>(),
        };
        Self::build(extent, kind, notifications)
    }

    fn build(
        extent: LogicalSize,
        kind: NotificationSceneKind,
        notifications: Vec<(&Notification, bool)>,
    ) -> Self {
        let mut cards = Vec::with_capacity(MAX_SCENE_CARDS.min(notifications.len()));
        let width = extent.width.saturating_sub(INSET.saturating_mul(2));
        let card_width = width.max(1);
        let mut y = INSET;
        for (notification, actions_enabled) in notifications.into_iter().take(MAX_SCENE_CARDS) {
            if y >= extent.height || card_width == 0 {
                break;
            }
            let height = CARD_HEIGHT.min(extent.height.saturating_sub(y));
            if height == 0 {
                break;
            }
            let bounds = LogicalRect::new(
                i32::try_from(INSET).unwrap_or(i32::MAX),
                i32::try_from(y).unwrap_or(i32::MAX),
                card_width,
                height,
            );
            let dismiss_width = 28.min(card_width);
            let dismiss = LogicalRect::new(
                i32::try_from(INSET.saturating_add(card_width.saturating_sub(dismiss_width)))
                    .unwrap_or(i32::MAX),
                i32::try_from(y).unwrap_or(i32::MAX),
                dismiss_width,
                height,
            );
            cards.push(NotificationCard {
                id: notification.id,
                bounds,
                dismiss,
                action_key: if actions_enabled {
                    notification
                        .actions
                        .first()
                        .map(|action| action.key.clone())
                } else {
                    None
                },
                urgency: notification.urgency,
            });
            y = y.saturating_add(CARD_HEIGHT).saturating_add(GAP);
        }
        Self {
            extent,
            kind,
            cards,
        }
    }

    pub(crate) fn hit_test(&self, position: (f64, f64)) -> Option<NotificationHit> {
        if !position.0.is_finite() || !position.1.is_finite() {
            return None;
        }
        for card in &self.cards {
            if contains(card.dismiss, position) {
                return Some(NotificationHit::Dismiss(card.id));
            }
            if card.action_key.is_some() && contains(card.bounds, position) {
                return Some(NotificationHit::Action(card.id));
            }
        }
        None
    }

    pub(crate) fn action_key(&self, id: NotificationId) -> Option<&str> {
        self.cards
            .iter()
            .find(|card| card.id == id)
            .and_then(|card| card.action_key.as_deref())
    }

    pub(crate) fn first_focus(&self) -> Option<NotificationHit> {
        self.focus_at(0)
    }

    pub(crate) fn navigate_focus(
        &self,
        current: Option<NotificationHit>,
        forward: bool,
    ) -> Option<NotificationHit> {
        let count = self.focus_count();
        if count == 0 {
            return None;
        }
        let index = current
            .and_then(|hit| self.focus_index(hit))
            .map(|index| {
                if forward {
                    (index + 1) % count
                } else {
                    (index + count - 1) % count
                }
            })
            .unwrap_or(if forward { 0 } else { count - 1 });
        self.focus_at(index)
    }

    pub(crate) fn has_focus(&self, focus: NotificationHit) -> bool {
        self.focus_index(focus).is_some()
    }

    fn focus_count(&self) -> usize {
        self.cards
            .iter()
            .map(|card| usize::from(card.action_key.is_some()) + 1)
            .sum()
    }

    fn focus_at(&self, mut index: usize) -> Option<NotificationHit> {
        for card in &self.cards {
            if card.action_key.is_some() {
                if index == 0 {
                    return Some(NotificationHit::Action(card.id));
                }
                index -= 1;
            }
            if index == 0 {
                return Some(NotificationHit::Dismiss(card.id));
            }
            index -= 1;
        }
        None
    }

    fn focus_index(&self, focus: NotificationHit) -> Option<usize> {
        let mut index = 0;
        for card in &self.cards {
            if card.action_key.is_some() {
                if focus == NotificationHit::Action(card.id) {
                    return Some(index);
                }
                index += 1;
            }
            if focus == NotificationHit::Dismiss(card.id) {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    pub(crate) fn physical_draws(
        &self,
        physical_extent: Extent2D,
        interaction: NotificationInteraction,
    ) -> Vec<PanelDraw> {
        let mut draws = Vec::with_capacity(self.cards.len().saturating_mul(2));
        for card in &self.cards {
            if let Some(rect) = physical_rect(card.bounds, self.extent, physical_extent) {
                draws.push(PanelDraw {
                    rect,
                    color: card_color(card, interaction),
                });
            }
            if let Some(rect) = physical_rect(card.dismiss, self.extent, physical_extent) {
                draws.push(PanelDraw {
                    rect,
                    color: dismiss_color(card.id, interaction),
                });
            }
        }
        if draws.is_empty()
            && self.kind == NotificationSceneKind::Center
            && let Some(rect) = empty_rect(self.extent, physical_extent)
        {
            draws.push(PanelDraw {
                rect,
                color: [0.10, 0.11, 0.13, 0.92],
            });
        }
        draws
    }
}

fn contains(bounds: LogicalRect, position: (f64, f64)) -> bool {
    let left = f64::from(bounds.origin.x);
    let top = f64::from(bounds.origin.y);
    let right = left + f64::from(bounds.size.width);
    let bottom = top + f64::from(bounds.size.height);
    position.0 >= left && position.0 < right && position.1 >= top && position.1 < bottom
}

fn card_color(card: &NotificationCard, interaction: NotificationInteraction) -> [f32; 4] {
    if interaction
        .pressed
        .is_some_and(|hit| hit_id(hit) == card.id)
    {
        return [0.20, 0.36, 0.40, 0.98];
    }
    if interaction
        .hovered
        .is_some_and(|hit| hit_id(hit) == card.id)
    {
        return [0.15, 0.25, 0.29, 0.98];
    }
    if interaction.focused == Some(NotificationHit::Action(card.id)) {
        return [0.18, 0.30, 0.34, 0.98];
    }
    match card.urgency {
        NotificationUrgency::Low => [0.12, 0.13, 0.16, 0.94],
        NotificationUrgency::Normal => [0.14, 0.15, 0.18, 0.96],
        NotificationUrgency::Critical => [0.38, 0.10, 0.12, 0.98],
    }
}

fn dismiss_color(id: NotificationId, interaction: NotificationInteraction) -> [f32; 4] {
    if interaction.pressed == Some(NotificationHit::Dismiss(id)) {
        [0.48, 0.13, 0.15, 0.98]
    } else if interaction.hovered == Some(NotificationHit::Dismiss(id)) {
        [0.30, 0.12, 0.14, 0.98]
    } else if interaction.focused == Some(NotificationHit::Dismiss(id)) {
        [0.40, 0.16, 0.18, 0.96]
    } else {
        [0.20, 0.10, 0.12, 0.82]
    }
}

const fn hit_id(hit: NotificationHit) -> NotificationId {
    match hit {
        NotificationHit::Dismiss(id) | NotificationHit::Action(id) => id,
    }
}

fn empty_rect(logical: LogicalSize, physical: Extent2D) -> Option<Rect2D> {
    let width = logical.width.min(240);
    let height = logical.height.min(32);
    let rect = LogicalRect::new(
        ((logical.width - width) / 2) as i32,
        ((logical.height - height) / 2) as i32,
        width,
        height,
    );
    physical_rect(rect, logical, physical)
}

fn physical_rect(
    logical: LogicalRect,
    logical_extent: LogicalSize,
    physical_extent: Extent2D,
) -> Option<Rect2D> {
    if logical_extent.is_empty() || physical_extent.is_empty() {
        return None;
    }
    let left = scale_edge(
        logical.origin.x.max(0) as u32,
        logical_extent.width,
        physical_extent.width,
    );
    let top = scale_edge(
        logical.origin.y.max(0) as u32,
        logical_extent.height,
        physical_extent.height,
    );
    let right = scale_edge(
        logical.origin.x.max(0) as u32 + logical.size.width,
        logical_extent.width,
        physical_extent.width,
    );
    let bottom = scale_edge(
        logical.origin.y.max(0) as u32 + logical.size.height,
        logical_extent.height,
        physical_extent.height,
    );
    (right > left && bottom > top).then(|| {
        Rect2D::new(
            i32::try_from(left).unwrap_or(i32::MAX),
            i32::try_from(top).unwrap_or(i32::MAX),
            right - left,
            bottom - top,
        )
    })
}

fn scale_edge(value: u32, logical: u32, physical: u32) -> u32 {
    let scaled = u64::from(value) * u64::from(physical) / u64::from(logical.max(1));
    u32::try_from(scaled).unwrap_or(u32::MAX).min(physical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NotificationAction, NotificationRequest};

    fn notification(summary: &str, urgency: NotificationUrgency) -> NotificationRequest {
        NotificationRequest {
            app_name: "demo".into(),
            summary: summary.into(),
            urgency,
            actions: vec![NotificationAction {
                key: "open".into(),
                label: "Open".into(),
            }],
            ..NotificationRequest::default()
        }
    }

    #[test]
    fn history_scene_has_action_and_dismiss_hit_regions() {
        let mut store = NotificationStore::default();
        let id = store.notify(notification("Ready", NotificationUrgency::Normal), 0);
        let scene = NotificationScene::from_store(
            LogicalSize::new(420, 560),
            &store,
            NotificationSceneKind::Center,
        );
        assert_eq!(
            scene.hit_test((30.0, 30.0)),
            Some(NotificationHit::Action(id))
        );
        assert_eq!(
            scene.hit_test((400.0, 30.0)),
            Some(NotificationHit::Dismiss(id))
        );
        assert_eq!(scene.action_key(id), Some("open"));
    }

    #[test]
    fn popup_scene_is_bounded_and_empty_center_has_a_draw() {
        let store = NotificationStore::default();
        let scene = NotificationScene::from_store(
            LogicalSize::new(320, 96),
            &store,
            NotificationSceneKind::Popups,
        );
        assert!(
            scene
                .physical_draws(Extent2D::new(320, 96), Default::default())
                .is_empty()
        );
        let center = NotificationScene::from_store(
            LogicalSize::new(420, 560),
            &store,
            NotificationSceneKind::Center,
        );
        assert_eq!(
            center
                .physical_draws(Extent2D::new(420, 560), Default::default())
                .len(),
            1
        );
    }

    #[test]
    fn closed_history_disables_stale_actions_but_keeps_dismiss() {
        let mut store = NotificationStore::default();
        let id = store.notify(notification("Done", NotificationUrgency::Normal), 0);
        assert!(store.close(id, crate::CloseReason::Expired, 1).is_some());
        let scene = NotificationScene::from_store(
            LogicalSize::new(420, 560),
            &store,
            NotificationSceneKind::Center,
        );
        assert_eq!(scene.action_key(id), None);
        assert_eq!(scene.hit_test((30.0, 30.0)), None);
        assert_eq!(
            scene.hit_test((400.0, 30.0)),
            Some(NotificationHit::Dismiss(id))
        );
        assert_eq!(scene.first_focus(), Some(NotificationHit::Dismiss(id)));
    }

    #[test]
    fn focus_navigation_orders_actions_before_dismiss_and_wraps() {
        let mut store = NotificationStore::default();
        let older = store.notify(notification("First", NotificationUrgency::Normal), 0);
        let newest = store.notify(notification("Second", NotificationUrgency::Low), 1);
        let scene = NotificationScene::from_store(
            LogicalSize::new(420, 560),
            &store,
            NotificationSceneKind::Center,
        );
        let newest_action = Some(NotificationHit::Action(newest));
        let newest_dismiss = Some(NotificationHit::Dismiss(newest));
        let older_action = Some(NotificationHit::Action(older));
        let older_dismiss = Some(NotificationHit::Dismiss(older));
        assert_eq!(scene.first_focus(), newest_action);
        assert_eq!(scene.navigate_focus(newest_action, true), newest_dismiss);
        assert_eq!(scene.navigate_focus(newest_dismiss, true), older_action);
        assert_eq!(scene.navigate_focus(older_action, true), older_dismiss);
        assert_eq!(scene.navigate_focus(older_dismiss, true), newest_action);
        assert_eq!(scene.navigate_focus(newest_action, false), older_dismiss);
        assert_eq!(scene.navigate_focus(None, false), older_dismiss);
        assert!(scene.has_focus(NotificationHit::Action(newest)));
        assert!(!scene.has_focus(NotificationHit::Action(
            NotificationId::from_raw(99).unwrap()
        )));
    }

    #[test]
    fn focus_navigation_is_empty_for_a_scene_without_notifications() {
        let store = NotificationStore::default();
        let scene = NotificationScene::from_store(
            LogicalSize::new(420, 560),
            &store,
            NotificationSceneKind::Center,
        );
        assert_eq!(scene.first_focus(), None);
        assert_eq!(scene.navigate_focus(None, true), None);
        assert_eq!(scene.navigate_focus(None, false), None);
    }
}
