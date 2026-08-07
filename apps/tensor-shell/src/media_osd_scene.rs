use tensor_dbus::freedesktop::mpris::{MprisAction, PlaybackStatus};
use vulkan_renderer::{Extent2D, Rect2D};
use wayland_client_runtime::{LogicalRect, LogicalSize};

use crate::{media_osd::MediaOsdContent, panel::PanelDraw};

const INSET: u32 = 12;
const GAP: u32 = 6;
const ACTION_SIZE: u32 = 32;
const METADATA_GAP: u32 = 5;
const PROGRESS_GAP: u32 = 4;
const PROGRESS_HEIGHT: u32 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaOsdHit {
    Dismiss,
    Previous,
    PlayPause,
    Next,
}

impl MediaOsdHit {
    pub(crate) const fn action(self) -> Option<MprisAction> {
        match self {
            Self::Dismiss => None,
            Self::Previous => Some(MprisAction::Previous),
            Self::PlayPause => Some(MprisAction::PlayPause),
            Self::Next => Some(MprisAction::Next),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct MediaOsdInteraction {
    pub(crate) hovered: Option<MediaOsdHit>,
    pub(crate) pressed: Option<MediaOsdHit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MediaOsdScene {
    extent: LogicalSize,
    actions: [ActionCard; 3],
    title: LogicalRect,
    detail: LogicalRect,
    progress: LogicalRect,
    content: MediaOsdContent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActionCard {
    hit: MediaOsdHit,
    bounds: LogicalRect,
    enabled: bool,
}

impl MediaOsdScene {
    pub(crate) fn build(extent: LogicalSize, content: &MediaOsdContent) -> Self {
        let inner_width = extent.width.saturating_sub(INSET.saturating_mul(2));
        let inner_height = extent.height.saturating_sub(INSET.saturating_mul(2));
        let controls_width = inner_width.min(
            ACTION_SIZE
                .saturating_mul(3)
                .saturating_add(GAP.saturating_mul(2)),
        );
        let action_size = inner_height
            .min(ACTION_SIZE)
            .min(controls_width.saturating_sub(GAP.saturating_mul(2)) / 3);
        let controls_width = action_size
            .saturating_mul(3)
            .saturating_add(GAP.saturating_mul(2));
        let action_top = INSET.saturating_add(inner_height.saturating_sub(action_size) / 2);
        let actions = [
            MediaOsdHit::Previous,
            MediaOsdHit::PlayPause,
            MediaOsdHit::Next,
        ]
        .map(|hit| {
            let action = hit.action().expect("media action card has an action");
            ActionCard {
                hit,
                bounds: LogicalRect::new(
                    i32::try_from(INSET.saturating_add(
                        (action_size.saturating_add(GAP)).saturating_mul(action_index(hit) as u32),
                    ))
                    .unwrap_or(i32::MAX),
                    i32::try_from(action_top).unwrap_or(i32::MAX),
                    action_size,
                    action_size,
                ),
                enabled: content.supports(action),
            }
        });
        let metadata_left = INSET.saturating_add(controls_width).saturating_add(GAP);
        let metadata_width = extent
            .width
            .saturating_sub(metadata_left)
            .saturating_sub(INSET);
        let text_height = inner_height
            .saturating_sub(METADATA_GAP)
            .saturating_sub(PROGRESS_GAP)
            .saturating_sub(PROGRESS_HEIGHT);
        let line_height = text_height / 2;
        let title = LogicalRect::new(
            i32::try_from(metadata_left).unwrap_or(i32::MAX),
            i32::try_from(INSET).unwrap_or(i32::MAX),
            metadata_width,
            line_height,
        );
        let detail = LogicalRect::new(
            i32::try_from(metadata_left).unwrap_or(i32::MAX),
            i32::try_from(
                INSET
                    .saturating_add(line_height)
                    .saturating_add(METADATA_GAP),
            )
            .unwrap_or(i32::MAX),
            metadata_width,
            line_height,
        );
        let progress = LogicalRect::new(
            i32::try_from(metadata_left).unwrap_or(i32::MAX),
            i32::try_from(
                INSET
                    .saturating_add(line_height.saturating_mul(2))
                    .saturating_add(METADATA_GAP)
                    .saturating_add(PROGRESS_GAP),
            )
            .unwrap_or(i32::MAX),
            metadata_width,
            PROGRESS_HEIGHT,
        );
        Self {
            extent,
            actions,
            title,
            detail,
            progress,
            content: content.clone(),
        }
    }

    pub(crate) fn hit_test(&self, position: (f64, f64)) -> Option<MediaOsdHit> {
        if !position.0.is_finite()
            || !position.1.is_finite()
            || position.0 < 0.0
            || position.1 < 0.0
            || position.0 >= f64::from(self.extent.width)
            || position.1 >= f64::from(self.extent.height)
        {
            return None;
        }
        self.actions
            .iter()
            .find(|card| card.enabled && contains(card.bounds, position))
            .map(|card| card.hit)
            .or(Some(MediaOsdHit::Dismiss))
    }

    /// Updates only the retained progress payload. A false result means a
    /// title, capability, or playback change requires rebuilding the scene.
    pub(crate) fn update_progress(&mut self, content: &MediaOsdContent) -> bool {
        if self.content.title != content.title
            || self.content.artists != content.artists
            || self.content.album != content.album
            || self.content.duration_micros != content.duration_micros
            || self.content.playback != content.playback
            || self.content.previous != content.previous
            || self.content.play_pause != content.play_pause
            || self.content.next != content.next
        {
            return false;
        }
        self.content.position_micros = content.position_micros;
        true
    }

    pub(crate) fn physical_draws(
        &self,
        physical_extent: Extent2D,
        interaction: MediaOsdInteraction,
    ) -> Vec<PanelDraw> {
        let mut draws = Vec::with_capacity(7);
        for card in self.actions {
            if let Some(rect) = physical_rect(card.bounds, self.extent, physical_extent) {
                draws.push(PanelDraw {
                    rect,
                    color: action_color(card, &self.content, interaction),
                });
            }
        }
        if let Some(rect) = physical_rect(self.title, self.extent, physical_extent) {
            draws.push(PanelDraw {
                rect,
                color: [0.20, 0.25, 0.28, 0.94],
            });
        }
        if let Some(rect) = physical_rect(self.detail, self.extent, physical_extent) {
            let populated = !self.content.artists.is_empty() || self.content.album.is_some();
            draws.push(PanelDraw {
                rect,
                color: if populated {
                    [0.14, 0.18, 0.21, 0.90]
                } else {
                    [0.10, 0.12, 0.14, 0.82]
                },
            });
        }
        if let Some(rect) = physical_rect(self.progress, self.extent, physical_extent) {
            draws.push(PanelDraw {
                rect,
                color: [0.08, 0.10, 0.12, 0.92],
            });
            if let Some(value) = progress_value(self.progress, &self.content)
                && let Some(rect) = physical_rect(value, self.extent, physical_extent)
            {
                draws.push(PanelDraw {
                    rect,
                    color: [0.24, 0.62, 0.56, 0.98],
                });
            }
        }
        draws
    }
}

fn progress_value(track: LogicalRect, content: &MediaOsdContent) -> Option<LogicalRect> {
    let duration = content.duration_micros?;
    if duration == 0 {
        return None;
    }
    let position = content.position_micros.unwrap_or(0).min(duration);
    let width = u32::try_from(
        u128::from(track.size.width).saturating_mul(u128::from(position)) / u128::from(duration),
    )
    .unwrap_or(u32::MAX)
    .min(track.size.width);
    (width > 0).then(|| LogicalRect::new(track.origin.x, track.origin.y, width, track.size.height))
}

const fn action_index(hit: MediaOsdHit) -> usize {
    match hit {
        MediaOsdHit::Previous => 0,
        MediaOsdHit::PlayPause => 1,
        MediaOsdHit::Next => 2,
        MediaOsdHit::Dismiss => 0,
    }
}

fn action_color(
    card: ActionCard,
    content: &MediaOsdContent,
    interaction: MediaOsdInteraction,
) -> [f32; 4] {
    if !card.enabled {
        return [0.07, 0.08, 0.09, 0.72];
    }
    if interaction.pressed == Some(card.hit) {
        return [0.21, 0.45, 0.48, 0.98];
    }
    if interaction.hovered == Some(card.hit) {
        return [0.15, 0.30, 0.34, 0.98];
    }
    if card.hit == MediaOsdHit::PlayPause && content.playback == PlaybackStatus::Playing {
        [0.08, 0.30, 0.19, 0.98]
    } else {
        [0.10, 0.13, 0.16, 0.98]
    }
}

fn contains(bounds: LogicalRect, position: (f64, f64)) -> bool {
    let left = f64::from(bounds.origin.x);
    let top = f64::from(bounds.origin.y);
    let right = left + f64::from(bounds.size.width);
    let bottom = top + f64::from(bounds.size.height);
    position.0 >= left && position.0 < right && position.1 >= top && position.1 < bottom
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

    fn content(playback: PlaybackStatus) -> MediaOsdContent {
        MediaOsdContent {
            title: "Track".into(),
            artists: vec!["Artist".into()],
            album: Some("Album".into()),
            position_micros: Some(30),
            duration_micros: Some(100),
            playback,
            previous: true,
            play_pause: true,
            next: false,
        }
    }

    #[test]
    fn enabled_controls_and_dismiss_background_have_stable_hits() {
        let scene =
            MediaOsdScene::build(LogicalSize::new(320, 96), &content(PlaybackStatus::Playing));
        assert_eq!(scene.hit_test((20.0, 40.0)), Some(MediaOsdHit::Previous));
        assert_eq!(scene.hit_test((58.0, 40.0)), Some(MediaOsdHit::PlayPause));
        assert_eq!(scene.hit_test((96.0, 40.0)), Some(MediaOsdHit::Dismiss));
        assert_eq!(scene.hit_test((250.0, 40.0)), Some(MediaOsdHit::Dismiss));
        assert_eq!(scene.hit_test((f64::NAN, 40.0)), None);
    }

    #[test]
    fn playback_changes_color_without_changing_retained_geometry() {
        let playing =
            MediaOsdScene::build(LogicalSize::new(320, 96), &content(PlaybackStatus::Playing));
        let paused =
            MediaOsdScene::build(LogicalSize::new(320, 96), &content(PlaybackStatus::Paused));
        let playing_draws = playing.physical_draws(Extent2D::new(640, 192), Default::default());
        let paused_draws = paused.physical_draws(Extent2D::new(640, 192), Default::default());
        assert_eq!(
            playing_draws
                .iter()
                .map(|draw| draw.rect)
                .collect::<Vec<_>>(),
            paused_draws
                .iter()
                .map(|draw| draw.rect)
                .collect::<Vec<_>>()
        );
        assert_ne!(playing_draws, paused_draws);
    }

    #[test]
    fn progress_bar_is_bounded_and_tracks_position_without_changing_geometry() {
        let half = MediaOsdScene::build(
            LogicalSize::new(320, 96),
            &MediaOsdContent {
                position_micros: Some(50),
                ..content(PlaybackStatus::Playing)
            },
        );
        let complete = MediaOsdScene::build(
            LogicalSize::new(320, 96),
            &MediaOsdContent {
                position_micros: Some(150),
                duration_micros: Some(100),
                ..content(PlaybackStatus::Playing)
            },
        );
        let half_draws = half.physical_draws(Extent2D::new(320, 96), Default::default());
        let complete_draws = complete.physical_draws(Extent2D::new(320, 96), Default::default());
        assert_eq!(half_draws.len(), 7);
        assert_eq!(complete_draws.len(), 7);
        assert_eq!(half_draws[5].rect, complete_draws[5].rect);
        assert!(half_draws[6].rect.extent.width < complete_draws[6].rect.extent.width);
        assert!(complete_draws[6].rect.extent.width <= complete_draws[5].rect.extent.width);
    }
}
