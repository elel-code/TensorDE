pub(crate) mod paint;

use crate::windowing::PhysicalSize;
use fika_core::{ViewPoint, ViewRect};

use crate::shell::metrics::scaled_dialog_metric;

pub(crate) const BACKGROUND_OPACITY_MIN_PERCENT: u8 = 20;
pub(crate) const BACKGROUND_OPACITY_MAX_PERCENT: u8 = 100;
const BACKGROUND_OPACITY_STEP_PERCENT: u8 = 5;
const SETTINGS_DIALOG_WIDTH: f32 = 420.0;
const SETTINGS_DIALOG_HEIGHT: f32 = 300.0;
const SETTINGS_PADDING: f32 = 18.0;
const SETTINGS_SECTION_TITLE_HEIGHT: f32 = 20.0;
const SETTINGS_SECTION_TITLE_GAP: f32 = 6.0;
const SETTINGS_SECTION_GAP: f32 = 12.0;
const SETTINGS_ROW_HEIGHT: f32 = 40.0;
const SETTINGS_GENERAL_ROW_COUNT: usize = 2;
const SETTINGS_ITEM_COUNT: usize = 5;
const BACKGROUND_OPACITY_ROW: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellSettingsAction {
    ToggleHiddenFiles,
    TogglePlaces,
    ToggleDarkMode,
    ToggleBackgroundBlur,
    SetBackgroundOpacity(u8),
}

impl ShellSettingsAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ToggleHiddenFiles => "toggle-hidden-files",
            Self::TogglePlaces => "toggle-places",
            Self::ToggleDarkMode => "toggle-dark-mode",
            Self::ToggleBackgroundBlur => "toggle-background-blur",
            Self::SetBackgroundOpacity(_) => "set-background-opacity",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShellSettingsItem {
    pub(crate) action: ShellSettingsAction,
    pub(crate) label: &'static str,
    pub(crate) active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellSettingsSnapshot {
    pub(crate) show_hidden: bool,
    pub(crate) places_visible: bool,
    pub(crate) dark_mode: bool,
    pub(crate) background_blur: bool,
    pub(crate) background_opacity: f32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ShellSettingsDialogState {
    pub(crate) hovered_row: Option<usize>,
    pub(crate) opacity_dragging: bool,
}

impl ShellSettingsDialogState {
    pub(crate) fn update_hover(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        scale: f32,
    ) -> bool {
        let hovered_row = settings_dialog_row_at_screen_point(point, size, scale);
        let changed = self.hovered_row != hovered_row;
        self.hovered_row = hovered_row;
        changed
    }

    pub(crate) fn clear_hover(&mut self) -> bool {
        self.hovered_row.take().is_some()
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

pub(crate) fn settings_dialog_items(
    snapshot: ShellSettingsSnapshot,
) -> [ShellSettingsItem; SETTINGS_ITEM_COUNT] {
    [
        ShellSettingsItem {
            action: ShellSettingsAction::ToggleHiddenFiles,
            label: "Show Hidden Files",
            active: snapshot.show_hidden,
        },
        ShellSettingsItem {
            action: ShellSettingsAction::TogglePlaces,
            label: "Places Sidebar",
            active: snapshot.places_visible,
        },
        ShellSettingsItem {
            action: ShellSettingsAction::ToggleDarkMode,
            label: "Dark Appearance",
            active: snapshot.dark_mode,
        },
        ShellSettingsItem {
            action: ShellSettingsAction::ToggleBackgroundBlur,
            label: "Background Blur",
            active: snapshot.background_blur,
        },
        ShellSettingsItem {
            action: ShellSettingsAction::SetBackgroundOpacity(background_opacity_percent(
                snapshot.background_opacity,
            )),
            label: "Background Opacity",
            active: false,
        },
    ]
}

pub(crate) fn settings_dialog_window_size_scaled(scale: f32) -> PhysicalSize<u32> {
    PhysicalSize::new(
        scaled_dialog_metric(SETTINGS_DIALOG_WIDTH, scale)
            .ceil()
            .max(1.0) as u32,
        scaled_dialog_metric(SETTINGS_DIALOG_HEIGHT, scale)
            .ceil()
            .max(1.0) as u32,
    )
}

pub(crate) fn settings_dialog_section_rects(size: PhysicalSize<u32>, scale: f32) -> [ViewRect; 2] {
    let padding = scaled_dialog_metric(SETTINGS_PADDING, scale);
    let title_height = scaled_dialog_metric(SETTINGS_SECTION_TITLE_HEIGHT, scale);
    let title_gap = scaled_dialog_metric(SETTINGS_SECTION_TITLE_GAP, scale);
    let section_gap = scaled_dialog_metric(SETTINGS_SECTION_GAP, scale);
    let row_height = scaled_dialog_metric(SETTINGS_ROW_HEIGHT, scale);
    let width = (size.width.max(1) as f32 - padding * 2.0).max(1.0);
    let general_y = padding + title_height + title_gap;
    let general_height = row_height * SETTINGS_GENERAL_ROW_COUNT as f32;
    let appearance_y = general_y + general_height + section_gap + title_height + title_gap;
    [
        ViewRect {
            x: padding,
            y: general_y,
            width,
            height: general_height,
        },
        ViewRect {
            x: padding,
            y: appearance_y,
            width,
            height: row_height * (SETTINGS_ITEM_COUNT - SETTINGS_GENERAL_ROW_COUNT) as f32,
        },
    ]
}

pub(crate) fn settings_dialog_section_title_rects(
    size: PhysicalSize<u32>,
    scale: f32,
) -> [ViewRect; 2] {
    let padding = scaled_dialog_metric(SETTINGS_PADDING, scale);
    let title_height = scaled_dialog_metric(SETTINGS_SECTION_TITLE_HEIGHT, scale);
    let section_gap = scaled_dialog_metric(SETTINGS_SECTION_GAP, scale);
    let sections = settings_dialog_section_rects(size, scale);
    let width = (size.width.max(1) as f32 - padding * 2.0).max(1.0);
    [
        ViewRect {
            x: padding,
            y: padding,
            width,
            height: title_height,
        },
        ViewRect {
            x: padding,
            y: sections[0].bottom() + section_gap,
            width,
            height: title_height,
        },
    ]
}

pub(crate) fn settings_dialog_row_rect(
    size: PhysicalSize<u32>,
    scale: f32,
    row: usize,
) -> Option<ViewRect> {
    if row >= SETTINGS_ITEM_COUNT {
        return None;
    }
    let sections = settings_dialog_section_rects(size, scale);
    let row_height = scaled_dialog_metric(SETTINGS_ROW_HEIGHT, scale);
    let (section, section_row) = if row < SETTINGS_GENERAL_ROW_COUNT {
        (sections[0], row)
    } else {
        (sections[1], row - SETTINGS_GENERAL_ROW_COUNT)
    };
    Some(ViewRect {
        x: section.x,
        y: section.y + section_row as f32 * row_height,
        width: section.width,
        height: row_height,
    })
}

pub(crate) fn settings_dialog_row_at_screen_point(
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
) -> Option<usize> {
    (0..SETTINGS_ITEM_COUNT).find(|row| {
        settings_dialog_row_rect(size, scale, *row).is_some_and(|rect| rect.contains(point))
    })
}

pub(crate) fn background_opacity_percent(opacity: f32) -> u8 {
    let percent = (opacity.clamp(
        BACKGROUND_OPACITY_MIN_PERCENT as f32 / 100.0,
        BACKGROUND_OPACITY_MAX_PERCENT as f32 / 100.0,
    ) * 100.0)
        .round() as u8;
    ((percent as f32 / BACKGROUND_OPACITY_STEP_PERCENT as f32).round() as u8
        * BACKGROUND_OPACITY_STEP_PERCENT)
        .clamp(
            BACKGROUND_OPACITY_MIN_PERCENT,
            BACKGROUND_OPACITY_MAX_PERCENT,
        )
}

pub(crate) fn settings_dialog_opacity_track_rect(size: PhysicalSize<u32>, scale: f32) -> ViewRect {
    let row = settings_dialog_row_rect(size, scale, BACKGROUND_OPACITY_ROW).unwrap_or(ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    });
    let label_width = scaled_dialog_metric(150.0, scale).min(row.width * 0.48);
    let value_width = scaled_dialog_metric(44.0, scale).min(row.width * 0.18);
    let padding = scaled_dialog_metric(16.0, scale);
    let height = scaled_dialog_metric(4.0, scale);
    ViewRect {
        x: row.x + label_width,
        y: row.y + (row.height - height) / 2.0,
        width: (row.width - label_width - value_width - padding).max(1.0),
        height,
    }
}

pub(crate) fn opacity_percent_at_settings_point(
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
) -> Option<u8> {
    if settings_dialog_row_at_screen_point(point, size, scale) != Some(BACKGROUND_OPACITY_ROW) {
        return None;
    }
    let track = settings_dialog_opacity_track_rect(size, scale);
    let knob_margin = scaled_dialog_metric(8.0, scale);
    if point.x < track.x - knob_margin || point.x > track.right() + knob_margin {
        return None;
    }
    let fraction = ((point.x - track.x) / track.width.max(1.0)).clamp(0.0, 1.0);
    let range = BACKGROUND_OPACITY_MAX_PERCENT - BACKGROUND_OPACITY_MIN_PERCENT;
    let raw = BACKGROUND_OPACITY_MIN_PERCENT as f32 + fraction * range as f32;
    let stepped = (raw / BACKGROUND_OPACITY_STEP_PERCENT as f32).round() as u8
        * BACKGROUND_OPACITY_STEP_PERCENT;
    Some(stepped.clamp(
        BACKGROUND_OPACITY_MIN_PERCENT,
        BACKGROUND_OPACITY_MAX_PERCENT,
    ))
}

pub(crate) fn settings_action_at_screen_point(
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
) -> Option<ShellSettingsAction> {
    match settings_dialog_row_at_screen_point(point, size, scale)? {
        0 => Some(ShellSettingsAction::ToggleHiddenFiles),
        1 => Some(ShellSettingsAction::TogglePlaces),
        2 => Some(ShellSettingsAction::ToggleDarkMode),
        3 => Some(ShellSettingsAction::ToggleBackgroundBlur),
        BACKGROUND_OPACITY_ROW => opacity_percent_at_settings_point(point, size, scale)
            .map(ShellSettingsAction::SetBackgroundOpacity),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_rows_map_to_actions() {
        let size = settings_dialog_window_size_scaled(1.0);
        for (row, action) in [
            ShellSettingsAction::ToggleHiddenFiles,
            ShellSettingsAction::TogglePlaces,
            ShellSettingsAction::ToggleDarkMode,
            ShellSettingsAction::ToggleBackgroundBlur,
        ]
        .into_iter()
        .enumerate()
        {
            let rect = settings_dialog_row_rect(size, 1.0, row).unwrap();
            assert_eq!(
                settings_action_at_screen_point(
                    ViewPoint {
                        x: rect.x + 12.0,
                        y: rect.y + rect.height / 2.0,
                    },
                    size,
                    1.0,
                ),
                Some(action)
            );
        }
    }

    #[test]
    fn settings_items_reflect_current_scene_values() {
        let items = settings_dialog_items(ShellSettingsSnapshot {
            show_hidden: true,
            places_visible: false,
            dark_mode: true,
            background_blur: true,
            background_opacity: 0.8,
        });

        assert!(items[0].active);
        assert!(!items[1].active);
        assert!(items[2].active);
        assert!(items[3].active);
        assert_eq!(
            items[4].action,
            ShellSettingsAction::SetBackgroundOpacity(80)
        );
    }

    #[test]
    fn opacity_track_maps_to_safe_five_percent_steps() {
        let size = settings_dialog_window_size_scaled(1.0);
        let track = settings_dialog_opacity_track_rect(size, 1.0);
        assert_eq!(
            opacity_percent_at_settings_point(
                ViewPoint {
                    x: track.x,
                    y: track.y,
                },
                size,
                1.0,
            ),
            Some(BACKGROUND_OPACITY_MIN_PERCENT)
        );
        assert_eq!(
            opacity_percent_at_settings_point(
                ViewPoint {
                    x: track.right(),
                    y: track.y,
                },
                size,
                1.0,
            ),
            Some(BACKGROUND_OPACITY_MAX_PERCENT)
        );
    }
}
