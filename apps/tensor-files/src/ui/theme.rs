use cosmic_text::Color as TextColor;

use crate::ui::options::ShellViewMode;
use crate::ui::tasks::ShellTaskStatusKind;

pub(crate) type UiColor = [f32; 4];

pub(crate) const NEUTRAL_ICON_COLOR: UiColor = ui_color_rgb8(75, 81, 91);
pub(crate) const PROPERTIES_ICON_COLOR: UiColor = ui_color_rgb8(55, 65, 81);

const fn ui_color_rgb8(red: u8, green: u8, blue: u8) -> UiColor {
    [
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
        1.0,
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellThemeMode {
    Light,
    Dark,
}

impl ShellThemeMode {
    pub(crate) fn from_dark_mode(dark_mode: bool) -> Self {
        if dark_mode { Self::Dark } else { Self::Light }
    }

    pub(crate) fn is_dark(self) -> bool {
        self == Self::Dark
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShellTheme {
    mode: ShellThemeMode,
    background_opacity: Option<f32>,
    view_surface: UiColor,
    view_content: UiColor,
    divider: UiColor,
    field: UiColor,
    field_separator: UiColor,
    details_header: UiColor,
    primary_text: TextColor,
    muted_text: TextColor,
    section_text: TextColor,
    accent_text: TextColor,
    accent: UiColor,
    task_running: UiColor,
    task_completed: UiColor,
    task_failed: UiColor,
    task_cancelled: UiColor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellToolbarButtonColors {
    pub(crate) border: UiColor,
    pub(crate) fill: UiColor,
    pub(crate) icon: UiColor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellScrollbarColors {
    pub(crate) track: UiColor,
    pub(crate) thumb: UiColor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellRubberBandColors {
    pub(crate) fill: UiColor,
    pub(crate) border: UiColor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellDropTargetColors {
    pub(crate) fill: UiColor,
    pub(crate) border: UiColor,
    pub(crate) marker: UiColor,
}

impl ShellTheme {
    pub(crate) fn for_dark_mode(dark_mode: bool) -> Self {
        match ShellThemeMode::from_dark_mode(dark_mode) {
            ShellThemeMode::Light => Self::light(),
            ShellThemeMode::Dark => Self::dark(),
        }
    }

    pub(crate) fn for_transparent_background(dark_mode: bool, background_opacity: f32) -> Self {
        let mut theme = Self::for_dark_mode(dark_mode);
        theme.background_opacity = Some(background_opacity.clamp(0.0, 1.0));
        theme
    }

    #[cfg(test)]
    pub(crate) fn mode(self) -> ShellThemeMode {
        self.mode
    }

    pub(crate) fn is_dark(self) -> bool {
        self.mode.is_dark()
    }

    pub(crate) fn uses_transparent_background(self) -> bool {
        self.background_opacity.is_some()
    }

    pub(crate) fn view_mode_surface(self, _view_mode: ShellViewMode) -> UiColor {
        match self.background_opacity {
            Some(opacity) => with_alpha(self.view_surface, opacity),
            None => self.view_surface,
        }
    }

    pub(crate) fn view_mode_content(self, _view_mode: ShellViewMode) -> UiColor {
        self.background_overlay(self.view_content)
    }

    pub(crate) fn divider(self) -> UiColor {
        self.divider
    }

    pub(crate) fn field(self) -> UiColor {
        self.field
    }

    pub(crate) fn field_separator(self) -> UiColor {
        self.field_separator
    }

    pub(crate) fn details_header(self) -> UiColor {
        self.background_overlay(self.details_header)
    }

    pub(crate) fn primary_text(self) -> TextColor {
        self.primary_text
    }

    pub(crate) fn muted_text(self) -> TextColor {
        self.muted_text
    }

    pub(crate) fn section_text(self) -> TextColor {
        self.section_text
    }

    pub(crate) fn accent_text(self) -> TextColor {
        self.accent_text
    }

    pub(crate) fn accent(self) -> UiColor {
        self.accent
    }

    pub(crate) fn toolbar_button(self, active: bool) -> ShellToolbarButtonColors {
        match (self.mode, active) {
            (ShellThemeMode::Light, true) => ShellToolbarButtonColors {
                border: self.accent,
                fill: [0.918, 0.945, 1.000, 1.0],
                icon: [0.122, 0.310, 0.749, 1.0],
            },
            (ShellThemeMode::Light, false) => ShellToolbarButtonColors {
                border: [0.694, 0.729, 0.776, 1.0],
                fill: [0.984, 0.986, 0.990, 1.0],
                icon: [0.420, 0.466, 0.545, 1.0],
            },
            (ShellThemeMode::Dark, true) => ShellToolbarButtonColors {
                border: self.accent,
                fill: [0.102, 0.173, 0.286, 1.0],
                icon: [0.576, 0.773, 0.992, 1.0],
            },
            (ShellThemeMode::Dark, false) => ShellToolbarButtonColors {
                border: self.divider,
                fill: [0.145, 0.157, 0.176, 1.0],
                icon: [0.580, 0.639, 0.718, 1.0],
            },
        }
    }

    pub(crate) fn scrollbar(self) -> ShellScrollbarColors {
        match self.mode {
            ShellThemeMode::Light => ShellScrollbarColors {
                track: [0.902, 0.922, 0.945, 1.0],
                thumb: [0.596, 0.647, 0.714, 1.0],
            },
            ShellThemeMode::Dark => ShellScrollbarColors {
                track: [0.145, 0.157, 0.176, 1.0],
                thumb: [0.420, 0.466, 0.545, 1.0],
            },
        }
    }

    pub(crate) fn rubber_band(self) -> ShellRubberBandColors {
        match self.mode {
            ShellThemeMode::Light => ShellRubberBandColors {
                fill: [0.280, 0.580, 0.920, 0.18],
                border: [0.450, 0.720, 0.980, 0.92],
            },
            ShellThemeMode::Dark => ShellRubberBandColors {
                fill: [0.184, 0.435, 0.929, 0.24],
                border: [0.576, 0.773, 0.992, 0.88],
            },
        }
    }

    pub(crate) fn drop_target(self) -> ShellDropTargetColors {
        match self.mode {
            ShellThemeMode::Light => ShellDropTargetColors {
                fill: [1.000, 0.953, 0.820, 0.82],
                border: [0.924, 0.518, 0.043, 0.98],
                marker: [0.924, 0.518, 0.043, 1.0],
            },
            ShellThemeMode::Dark => ShellDropTargetColors {
                fill: [0.286, 0.196, 0.102, 0.86],
                border: [0.953, 0.612, 0.071, 0.95],
                marker: [0.953, 0.612, 0.071, 1.0],
            },
        }
    }

    pub(crate) fn task_status_color(self, kind: ShellTaskStatusKind) -> UiColor {
        match kind {
            ShellTaskStatusKind::Running => self.task_running,
            ShellTaskStatusKind::Completed => self.task_completed,
            ShellTaskStatusKind::Failed => self.task_failed,
            ShellTaskStatusKind::Cancelled => self.task_cancelled,
        }
    }

    fn background_overlay(self, color: UiColor) -> UiColor {
        if self.background_opacity.is_none() {
            return color;
        }
        with_alpha(color, 0.0)
    }

    fn light() -> Self {
        Self {
            mode: ShellThemeMode::Light,
            background_opacity: None,
            view_surface: [0.973, 0.976, 0.984, 1.0],
            view_content: [0.973, 0.976, 0.984, 1.0],
            divider: [0.784, 0.808, 0.839, 1.0],
            field: [1.000, 1.000, 1.000, 1.0],
            field_separator: [0.835, 0.851, 0.875, 1.0],
            details_header: [0.953, 0.961, 0.973, 1.0],
            primary_text: TextColor::rgb(36, 41, 47),
            muted_text: TextColor::rgb(89, 99, 110),
            section_text: TextColor::rgb(107, 114, 128),
            accent_text: TextColor::rgb(31, 79, 191),
            accent: [0.184, 0.435, 0.929, 1.0],
            task_running: [0.184, 0.435, 0.929, 1.0],
            task_completed: [0.102, 0.514, 0.286, 1.0],
            task_failed: [0.820, 0.184, 0.184, 1.0],
            task_cancelled: [0.475, 0.514, 0.565, 1.0],
        }
    }

    fn dark() -> Self {
        Self {
            mode: ShellThemeMode::Dark,
            background_opacity: None,
            view_surface: [0.102, 0.112, 0.126, 1.0],
            view_content: [0.078, 0.086, 0.098, 1.0],
            divider: [0.255, 0.278, 0.310, 1.0],
            field: [0.145, 0.157, 0.176, 1.0],
            field_separator: [0.255, 0.278, 0.310, 1.0],
            details_header: [0.125, 0.137, 0.153, 1.0],
            primary_text: TextColor::rgb(226, 232, 240),
            muted_text: TextColor::rgb(148, 163, 184),
            section_text: TextColor::rgb(156, 163, 175),
            accent_text: TextColor::rgb(147, 197, 253),
            accent: [0.184, 0.435, 0.929, 1.0],
            task_running: [0.184, 0.435, 0.929, 1.0],
            task_completed: [0.102, 0.514, 0.286, 1.0],
            task_failed: [0.820, 0.184, 0.184, 1.0],
            task_cancelled: [0.475, 0.514, 0.565, 1.0],
        }
    }
}

fn with_alpha(mut color: UiColor, alpha: f32) -> UiColor {
    color[3] = alpha.clamp(0.0, 1.0);
    color
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_mode_selects_light_and_dark_palettes() {
        let light = ShellTheme::for_dark_mode(false);
        let dark = ShellTheme::for_dark_mode(true);

        assert_eq!(light.mode(), ShellThemeMode::Light);
        assert_eq!(dark.mode(), ShellThemeMode::Dark);
        assert!(!light.is_dark());
        assert!(dark.is_dark());
        assert_eq!(
            light.task_status_color(ShellTaskStatusKind::Failed),
            [0.820, 0.184, 0.184, 1.0]
        );
        assert_eq!(
            dark.toolbar_button(false).border,
            [0.255, 0.278, 0.310, 1.0]
        );
        assert_eq!(light.toolbar_button(true).fill, [0.918, 0.945, 1.000, 1.0]);
        assert_eq!(light.scrollbar().track, [0.902, 0.922, 0.945, 1.0]);
        assert_eq!(dark.scrollbar().track, [0.145, 0.157, 0.176, 1.0]);
        assert_eq!(light.rubber_band().fill, [0.280, 0.580, 0.920, 0.18]);
        assert_eq!(dark.drop_target().marker, [0.953, 0.612, 0.071, 1.0]);
    }

    #[test]
    fn transparent_theme_uses_one_background_layer_and_keeps_fields_opaque() {
        let theme = ShellTheme::for_transparent_background(true, 0.65);

        assert_eq!(
            theme.view_mode_surface(ShellViewMode::Icons),
            [0.102, 0.112, 0.126, 0.65]
        );
        assert_eq!(theme.view_mode_content(ShellViewMode::Icons)[3], 0.0);
        assert_eq!(theme.details_header()[3], 0.0);
        assert_eq!(theme.field()[3], 1.0);
        assert_eq!(theme.primary_text(), TextColor::rgb(226, 232, 240));
        assert_eq!(theme.toolbar_button(false).icon[3], 1.0);
    }
}
