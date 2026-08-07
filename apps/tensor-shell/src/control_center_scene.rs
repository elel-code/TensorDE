use vulkan_renderer::{Extent2D, Rect2D};
use wayland_client_runtime::{LogicalRect, LogicalSize};

use tensor_dbus::freedesktop::mpris::MprisAction;

use crate::media::{MediaActionState, MediaServiceSnapshot};
use crate::network::{NetworkActionState, NetworkServiceSnapshot};
use crate::panel::{PanelAppletAvailability, PanelAppletEmphasis, PanelAppletState, PanelDraw};
use crate::session_lock_service::{SessionAction, SessionActionState};
use crate::system_status::PowerServiceSnapshot;

const INSET: u32 = 16;
const GAP: u32 = 8;
const ACTION_HEIGHT: u32 = 72;
const METER_HEIGHT: u32 = 18;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ControlCenterHit {
    Network,
    Lock,
    Suspend,
    DoNotDisturb,
    Previous,
    PlayPause,
    Next,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ControlCenterInteraction {
    pub hovered: Option<ControlCenterHit>,
    pub pressed: Option<ControlCenterHit>,
    pub focused: Option<ControlCenterHit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ControlCenterScene {
    extent: LogicalSize,
    actions: [ActionCard; 7],
    power: PanelAppletState,
    media: PanelAppletState,
    media_action: MediaActionState,
    network: PanelAppletState,
    network_action: NetworkActionState,
    action_state: SessionActionState,
    do_not_disturb: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActionCard {
    hit: ControlCenterHit,
    bounds: LogicalRect,
    enabled: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct ControlCenterSnapshot<'a> {
    pub(crate) power: &'a PowerServiceSnapshot,
    pub(crate) media: &'a MediaServiceSnapshot,
    pub(crate) media_action: MediaActionState,
    pub(crate) network: &'a NetworkServiceSnapshot,
    pub(crate) network_action: NetworkActionState,
    pub(crate) do_not_disturb: bool,
    pub(crate) session_action: SessionActionState,
}

impl ControlCenterScene {
    pub(crate) fn build(extent: LogicalSize, snapshot: ControlCenterSnapshot<'_>) -> Self {
        let power = snapshot.power.panel_state();
        let media_state = snapshot.media.panel_state();
        let network_state = snapshot.network.panel_state();
        let width = extent.width.saturating_sub(INSET.saturating_mul(2));
        let card_width = width.saturating_sub(GAP.saturating_mul(2)) / 3;
        let actions = [
            ControlCenterHit::Network,
            ControlCenterHit::Lock,
            ControlCenterHit::Suspend,
            ControlCenterHit::DoNotDisturb,
            ControlCenterHit::Previous,
            ControlCenterHit::PlayPause,
            ControlCenterHit::Next,
        ]
        .map(|hit| {
            let top = action_top(hit);
            ActionCard {
                hit,
                bounds: LogicalRect::new(
                    i32::try_from(
                        INSET.saturating_add(
                            (card_width.saturating_add(GAP))
                                .saturating_mul((action_index(hit) % 3) as u32),
                        ),
                    )
                    .unwrap_or(i32::MAX),
                    i32::try_from(top).unwrap_or(i32::MAX),
                    card_width,
                    ACTION_HEIGHT.min(extent.height.saturating_sub(top)),
                ),
                enabled: match hit {
                    ControlCenterHit::Network => {
                        snapshot.network.supports_wireless_toggle()
                            && !matches!(snapshot.network_action, NetworkActionState::Pending(_))
                    }
                    _ => media_action(hit).is_none_or(|action| {
                        snapshot.media.supports(action)
                            && !matches!(snapshot.media_action, MediaActionState::Pending(_))
                    }),
                },
            }
        });
        Self {
            extent,
            actions,
            power,
            media: media_state,
            media_action: snapshot.media_action,
            network: network_state,
            network_action: snapshot.network_action,
            action_state: snapshot.session_action,
            do_not_disturb: snapshot.do_not_disturb,
        }
    }

    pub(crate) fn hit_test(&self, position: (f64, f64)) -> Option<ControlCenterHit> {
        if !position.0.is_finite() || !position.1.is_finite() {
            return None;
        }
        self.actions
            .iter()
            .find(|card| card.enabled && contains(card.bounds, position))
            .map(|card| card.hit)
    }

    pub(crate) fn first_focus(&self) -> Option<ControlCenterHit> {
        self.actions
            .iter()
            .find(|card| card.enabled)
            .map(|card| card.hit)
    }

    pub(crate) fn navigate_focus(
        &self,
        current: Option<ControlCenterHit>,
        forward: bool,
    ) -> Option<ControlCenterHit> {
        let count = self.actions.iter().filter(|card| card.enabled).count();
        if count == 0 {
            return None;
        }
        let current = current.and_then(|hit| {
            self.actions
                .iter()
                .filter(|card| card.enabled)
                .position(|card| card.hit == hit)
        });
        let index = current
            .map(|index| {
                if forward {
                    (index + 1) % count
                } else {
                    (index + count - 1) % count
                }
            })
            .unwrap_or(if forward { 0 } else { count - 1 });
        self.actions
            .iter()
            .filter(|card| card.enabled)
            .nth(index)
            .map(|card| card.hit)
    }

    pub(crate) fn has_focus(&self, focus: ControlCenterHit) -> bool {
        self.actions
            .iter()
            .any(|card| card.enabled && card.hit == focus)
    }

    pub(crate) fn physical_draws(
        &self,
        physical_extent: Extent2D,
        interaction: ControlCenterInteraction,
    ) -> Vec<PanelDraw> {
        let mut draws = Vec::with_capacity(9);
        for card in self.actions {
            if let Some(rect) = physical_rect(card.bounds, self.extent, physical_extent) {
                draws.push(PanelDraw {
                    rect,
                    color: action_color(self, card.hit, card.enabled, interaction),
                });
            }
        }
        if let Some(meter) = self.power.meter() {
            let meter_rect = LogicalRect::new(
                i32::try_from(INSET).unwrap_or(i32::MAX),
                i32::try_from(status_top()).unwrap_or(i32::MAX),
                self.extent.width.saturating_sub(INSET.saturating_mul(2)),
                METER_HEIGHT.min(self.extent.height),
            );
            if let Some(rect) = physical_rect(meter_rect, self.extent, physical_extent) {
                draws.push(PanelDraw {
                    rect,
                    color: meter_color(self.power, meter),
                });
            }
        } else if self.power.availability() != PanelAppletAvailability::Ready {
            let status_rect = LogicalRect::new(
                i32::try_from(INSET).unwrap_or(i32::MAX),
                i32::try_from(status_top()).unwrap_or(i32::MAX),
                self.extent.width.saturating_sub(INSET.saturating_mul(2)),
                METER_HEIGHT.min(self.extent.height),
            );
            if let Some(rect) = physical_rect(status_rect, self.extent, physical_extent) {
                draws.push(PanelDraw {
                    rect,
                    color: status_color(self.power),
                });
            }
        }
        if let Some(meter) = self.network.meter() {
            let available = self.extent.width.saturating_sub(INSET.saturating_mul(2));
            let width = (available.saturating_mul(u32::from(meter)) / 100).max(1);
            let network_rect = LogicalRect::new(
                i32::try_from(INSET).unwrap_or(i32::MAX),
                i32::try_from(status_top().saturating_add(METER_HEIGHT + GAP)).unwrap_or(i32::MAX),
                width,
                METER_HEIGHT.min(self.extent.height),
            );
            if let Some(rect) = physical_rect(network_rect, self.extent, physical_extent) {
                draws.push(PanelDraw {
                    rect,
                    color: meter_color(self.network, meter),
                });
            }
        }
        draws
    }
}

const fn action_index(hit: ControlCenterHit) -> usize {
    match hit {
        ControlCenterHit::Network => 0,
        ControlCenterHit::Lock => 1,
        ControlCenterHit::Suspend => 2,
        ControlCenterHit::DoNotDisturb => 3,
        ControlCenterHit::Previous => 4,
        ControlCenterHit::PlayPause => 5,
        ControlCenterHit::Next => 6,
    }
}

const fn media_action(hit: ControlCenterHit) -> Option<MprisAction> {
    match hit {
        ControlCenterHit::Previous => Some(MprisAction::Previous),
        ControlCenterHit::PlayPause => Some(MprisAction::PlayPause),
        ControlCenterHit::Next => Some(MprisAction::Next),
        ControlCenterHit::Network
        | ControlCenterHit::Lock
        | ControlCenterHit::Suspend
        | ControlCenterHit::DoNotDisturb => None,
    }
}

const fn action_top(hit: ControlCenterHit) -> u32 {
    INSET.saturating_add(
        ACTION_HEIGHT
            .saturating_add(GAP)
            .saturating_mul((action_index(hit) / 3) as u32),
    )
}

const fn status_top() -> u32 {
    INSET.saturating_add(ACTION_HEIGHT.saturating_add(GAP).saturating_mul(3))
}

fn contains(bounds: LogicalRect, position: (f64, f64)) -> bool {
    let left = f64::from(bounds.origin.x);
    let top = f64::from(bounds.origin.y);
    let right = left + f64::from(bounds.size.width);
    let bottom = top + f64::from(bounds.size.height);
    position.0 >= left && position.0 < right && position.1 >= top && position.1 < bottom
}

fn action_color(
    scene: &ControlCenterScene,
    hit: ControlCenterHit,
    enabled: bool,
    interaction: ControlCenterInteraction,
) -> [f32; 4] {
    if hit == ControlCenterHit::Network
        && matches!(scene.network_action, NetworkActionState::Pending(_))
    {
        return [0.34, 0.25, 0.08, 0.98];
    }
    if media_action(hit).is_some_and(|action| {
        matches!(scene.media_action, MediaActionState::Pending(candidate) if candidate == action)
    }) {
        return [0.34, 0.25, 0.08, 0.98];
    }
    if !enabled {
        let availability = match hit {
            ControlCenterHit::Network => scene.network.availability(),
            _ => scene.media.availability(),
        };
        return match availability {
            PanelAppletAvailability::Pending => [0.10, 0.11, 0.12, 0.72],
            PanelAppletAvailability::Unavailable => [0.18, 0.15, 0.08, 0.76],
            PanelAppletAvailability::Failed => [0.28, 0.08, 0.09, 0.80],
            PanelAppletAvailability::Ready => [0.07, 0.08, 0.09, 0.72],
        };
    }
    if interaction.pressed == Some(hit) {
        return [0.21, 0.45, 0.48, 0.98];
    }
    if interaction.hovered == Some(hit) {
        return [0.15, 0.30, 0.34, 0.98];
    }
    if interaction.focused == Some(hit) {
        return [0.18, 0.34, 0.38, 0.98];
    }
    let action = match hit {
        ControlCenterHit::Network => {
            return match scene.network_action {
                NetworkActionState::Pending(_) => [0.34, 0.25, 0.08, 0.98],
                NetworkActionState::Failed(_) => [0.42, 0.09, 0.11, 0.98],
                NetworkActionState::Succeeded(true) => [0.08, 0.30, 0.19, 0.98],
                NetworkActionState::Succeeded(false) => [0.12, 0.18, 0.20, 0.98],
                NetworkActionState::Idle => match scene.network.emphasis() {
                    PanelAppletEmphasis::Critical => [0.42, 0.09, 0.11, 0.98],
                    PanelAppletEmphasis::Attention => [0.34, 0.25, 0.08, 0.98],
                    PanelAppletEmphasis::Active => [0.08, 0.30, 0.19, 0.98],
                    PanelAppletEmphasis::Normal => [0.12, 0.18, 0.20, 0.98],
                },
            };
        }
        ControlCenterHit::Lock => SessionAction::Lock,
        ControlCenterHit::Suspend => SessionAction::Suspend,
        ControlCenterHit::DoNotDisturb if scene.do_not_disturb => {
            return [0.08, 0.30, 0.19, 0.98];
        }
        ControlCenterHit::DoNotDisturb => return [0.12, 0.18, 0.20, 0.98],
        ControlCenterHit::Previous | ControlCenterHit::PlayPause | ControlCenterHit::Next => {
            let media_action = media_action(hit).expect("media hit maps to an MPRIS action");
            return match scene.media_action {
                MediaActionState::Pending(candidate) if candidate == media_action => {
                    [0.34, 0.25, 0.08, 0.98]
                }
                MediaActionState::Succeeded(candidate) if candidate == media_action => {
                    [0.08, 0.30, 0.19, 0.98]
                }
                MediaActionState::Failed(candidate) if candidate == media_action => {
                    [0.42, 0.09, 0.11, 0.98]
                }
                _ if hit == ControlCenterHit::PlayPause
                    && scene.media.emphasis() == PanelAppletEmphasis::Active =>
                {
                    [0.08, 0.30, 0.19, 0.98]
                }
                _ => [0.10, 0.13, 0.16, 0.98],
            };
        }
    };
    match scene.action_state {
        SessionActionState::Pending(candidate) if candidate == action => [0.34, 0.25, 0.08, 0.98],
        SessionActionState::Succeeded(candidate) if candidate == action => [0.08, 0.30, 0.19, 0.98],
        SessionActionState::Failed(candidate) if candidate == action => [0.42, 0.09, 0.11, 0.98],
        _ => [0.10, 0.13, 0.16, 0.98],
    }
}

fn meter_color(state: PanelAppletState, meter: u8) -> [f32; 4] {
    let width = f32::from(meter) / 100.0;
    match state.emphasis() {
        PanelAppletEmphasis::Critical => [0.55 * width, 0.08, 0.10, 0.98],
        PanelAppletEmphasis::Attention => [0.45 * width, 0.28, 0.06, 0.98],
        PanelAppletEmphasis::Active => [0.08, 0.40 * width, 0.20, 0.98],
        PanelAppletEmphasis::Normal => [0.10, 0.27 * width, 0.33, 0.98],
    }
}

const fn status_color(state: PanelAppletState) -> [f32; 4] {
    match state.availability() {
        PanelAppletAvailability::Pending => [0.16, 0.17, 0.19, 0.88],
        PanelAppletAvailability::Unavailable => [0.25, 0.20, 0.08, 0.88],
        PanelAppletAvailability::Failed => [0.42, 0.09, 0.11, 0.96],
        PanelAppletAvailability::Ready => [0.10, 0.13, 0.16, 0.88],
    }
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
    use tensor_dbus::freedesktop::network_manager::{
        Connectivity, NetworkManagerSnapshot, NetworkState, PrimaryConnectionKind,
        wifi::NetworkManagerDetailsSnapshot,
    };

    fn network() -> NetworkServiceSnapshot {
        NetworkServiceSnapshot::Ready(NetworkManagerDetailsSnapshot::from_parts(
            NetworkManagerSnapshot::from_parts(
                true,
                true,
                true,
                NetworkState::ConnectedGlobal,
                Connectivity::Full,
                PrimaryConnectionKind::Wifi,
            ),
            Default::default(),
        ))
    }

    #[test]
    fn control_cards_have_stable_hit_regions() {
        let scene = ControlCenterScene::build(
            LogicalSize::new(420, 560),
            ControlCenterSnapshot {
                power: &PowerServiceSnapshot::Unavailable,
                media: &MediaServiceSnapshot::Ready(None),
                media_action: MediaActionState::Idle,
                network: &network(),
                network_action: NetworkActionState::Idle,
                do_not_disturb: false,
                session_action: SessionActionState::Idle,
            },
        );
        assert_eq!(
            scene.hit_test((20.0, 20.0)),
            Some(ControlCenterHit::Network)
        );
        assert_eq!(scene.hit_test((160.0, 20.0)), Some(ControlCenterHit::Lock));
        assert_eq!(
            scene.hit_test((300.0, 20.0)),
            Some(ControlCenterHit::Suspend)
        );
        assert_eq!(scene.hit_test((f64::NAN, 20.0)), None);
        assert_eq!(
            scene.hit_test((20.0, 100.0)),
            Some(ControlCenterHit::DoNotDisturb)
        );
        assert_eq!(scene.hit_test((300.0, 180.0)), None);
    }

    #[test]
    fn action_and_power_statuses_change_draws_without_changing_geometry() {
        let pending = ControlCenterScene::build(
            LogicalSize::new(420, 560),
            ControlCenterSnapshot {
                power: &PowerServiceSnapshot::Pending,
                media: &MediaServiceSnapshot::Pending,
                media_action: MediaActionState::Pending(MprisAction::PlayPause),
                network: &NetworkServiceSnapshot::Pending,
                network_action: NetworkActionState::Pending(false),
                do_not_disturb: false,
                session_action: SessionActionState::Pending(SessionAction::Lock),
            },
        );
        let failed = ControlCenterScene::build(
            LogicalSize::new(420, 560),
            ControlCenterSnapshot {
                power: &PowerServiceSnapshot::Failed,
                media: &MediaServiceSnapshot::Failed,
                media_action: MediaActionState::Failed(MprisAction::PlayPause),
                network: &NetworkServiceSnapshot::Failed,
                network_action: NetworkActionState::Failed(false),
                do_not_disturb: false,
                session_action: SessionActionState::Failed(SessionAction::Lock),
            },
        );
        let pending_draws = pending.physical_draws(Extent2D::new(420, 560), Default::default());
        let failed_draws = failed.physical_draws(Extent2D::new(420, 560), Default::default());
        assert_eq!(
            pending_draws
                .iter()
                .map(|draw| draw.rect)
                .collect::<Vec<_>>(),
            failed_draws
                .iter()
                .map(|draw| draw.rect)
                .collect::<Vec<_>>()
        );
        assert_ne!(pending_draws, failed_draws);
    }

    #[test]
    fn retained_network_signal_adds_a_bounded_meter_draw() {
        let mut scene = ControlCenterScene::build(
            LogicalSize::new(420, 560),
            ControlCenterSnapshot {
                power: &PowerServiceSnapshot::Unavailable,
                media: &MediaServiceSnapshot::Ready(None),
                media_action: MediaActionState::Idle,
                network: &network(),
                network_action: NetworkActionState::Idle,
                do_not_disturb: false,
                session_action: SessionActionState::Idle,
            },
        );
        scene.network = PanelAppletState::ready().with_meter(64);
        let draws = scene.physical_draws(Extent2D::new(420, 560), Default::default());
        let meter = draws.last().expect("network signal meter is retained");
        assert_eq!(meter.rect.origin.x, 16);
        assert!(meter.rect.extent.width > 1);
        assert!(meter.rect.extent.width < 420);
    }

    #[test]
    fn focus_navigation_skips_disabled_media_and_wraps() {
        let scene = ControlCenterScene::build(
            LogicalSize::new(420, 560),
            ControlCenterSnapshot {
                power: &PowerServiceSnapshot::Unavailable,
                media: &MediaServiceSnapshot::Ready(None),
                media_action: MediaActionState::Idle,
                network: &network(),
                network_action: NetworkActionState::Idle,
                do_not_disturb: false,
                session_action: SessionActionState::Idle,
            },
        );
        assert_eq!(scene.first_focus(), Some(ControlCenterHit::Network));
        assert_eq!(
            scene.navigate_focus(Some(ControlCenterHit::Network), true),
            Some(ControlCenterHit::Lock)
        );
        assert_eq!(
            scene.navigate_focus(Some(ControlCenterHit::DoNotDisturb), true),
            Some(ControlCenterHit::Network)
        );
        assert_eq!(
            scene.navigate_focus(Some(ControlCenterHit::Network), false),
            Some(ControlCenterHit::DoNotDisturb)
        );
        assert!(scene.has_focus(ControlCenterHit::Lock));
        assert!(!scene.has_focus(ControlCenterHit::PlayPause));
    }
}
