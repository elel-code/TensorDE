use cosmic_text::Color as TextColor;

use crate::ui::tasks::ShellTaskStatusKind;
use crate::ui::theme::{ShellScrollbarColors, ShellTheme, UiColor};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PopupTheme {
    pub(crate) backdrop: UiColor,
    pub(crate) surface: UiColor,
    pub(crate) header: UiColor,
    pub(crate) panel: UiColor,
    pub(crate) input: UiColor,
    pub(crate) border: UiColor,
    pub(crate) divider: UiColor,
    pub(crate) row_alt: UiColor,
    pub(crate) button_secondary: UiColor,
    pub(crate) button_primary: UiColor,
    pub(crate) button_primary_soft: UiColor,
    pub(crate) button_warning: UiColor,
    pub(crate) button_danger: UiColor,
    pub(crate) field_focus: UiColor,
    pub(crate) marker_neutral: UiColor,
    pub(crate) warning_header: UiColor,
    pub(crate) warning_divider: UiColor,
    pub(crate) selection_fill: UiColor,
    pub(crate) scrollbar: ShellScrollbarColors,
    pub(crate) title_text: TextColor,
    pub(crate) body_text: TextColor,
    pub(crate) muted_text: TextColor,
    pub(crate) soft_text: TextColor,
    pub(crate) inverse_text: TextColor,
    pub(crate) error_text: TextColor,
    pub(crate) warning_text: TextColor,
    status_running: UiColor,
    status_completed: UiColor,
    status_failed: UiColor,
    status_cancelled: UiColor,
    status_running_text: TextColor,
    status_completed_text: TextColor,
}

impl PopupTheme {
    pub(crate) fn from_shell_theme(shell: ShellTheme) -> Self {
        if shell.is_dark() {
            Self::dark(shell)
        } else {
            Self::light(shell)
        }
    }

    pub(crate) fn status_fill(self, kind: ShellTaskStatusKind) -> UiColor {
        match kind {
            ShellTaskStatusKind::Running => self.status_running,
            ShellTaskStatusKind::Completed => self.status_completed,
            ShellTaskStatusKind::Failed => self.status_failed,
            ShellTaskStatusKind::Cancelled => self.status_cancelled,
        }
    }

    pub(crate) fn status_text(self, kind: ShellTaskStatusKind) -> TextColor {
        match kind {
            ShellTaskStatusKind::Running => self.status_running_text,
            ShellTaskStatusKind::Completed => self.status_completed_text,
            ShellTaskStatusKind::Failed => self.error_text,
            ShellTaskStatusKind::Cancelled => self.muted_text,
        }
    }

    pub(crate) fn list_row_background(self, alternate: bool) -> UiColor {
        if alternate { self.row_alt } else { self.input }
    }

    fn light(shell: ShellTheme) -> Self {
        Self {
            backdrop: [0.047, 0.051, 0.056, 0.30],
            surface: [0.990, 0.991, 0.988, 0.99],
            header: [0.965, 0.969, 0.973, 1.0],
            panel: [0.976, 0.979, 0.978, 1.0],
            input: [1.000, 1.000, 1.000, 1.0],
            border: [0.706, 0.722, 0.741, 1.0],
            divider: [0.855, 0.863, 0.873, 1.0],
            row_alt: [0.974, 0.976, 0.972, 1.0],
            button_secondary: [0.941, 0.944, 0.941, 1.0],
            button_primary: [0.119, 0.392, 0.635, 1.0],
            button_primary_soft: [0.176, 0.459, 0.620, 1.0],
            button_warning: [0.718, 0.404, 0.133, 1.0],
            button_danger: [0.700, 0.235, 0.188, 1.0],
            field_focus: [0.239, 0.502, 0.710, 1.0],
            marker_neutral: [0.608, 0.674, 0.753, 1.0],
            warning_header: [1.000, 0.957, 0.890, 1.0],
            warning_divider: [0.906, 0.737, 0.490, 1.0],
            selection_fill: [0.918, 0.945, 1.000, 1.0],
            scrollbar: shell.scrollbar(),
            title_text: TextColor::rgb(31, 37, 45),
            body_text: TextColor::rgb(48, 56, 66),
            muted_text: TextColor::rgb(100, 113, 128),
            soft_text: TextColor::rgb(74, 86, 100),
            inverse_text: TextColor::rgb(255, 255, 255),
            error_text: TextColor::rgb(177, 54, 43),
            warning_text: TextColor::rgb(92, 55, 18),
            status_running: shell.task_status_color(ShellTaskStatusKind::Running),
            status_completed: shell.task_status_color(ShellTaskStatusKind::Completed),
            status_failed: shell.task_status_color(ShellTaskStatusKind::Failed),
            status_cancelled: shell.task_status_color(ShellTaskStatusKind::Cancelled),
            status_running_text: TextColor::rgb(46, 103, 168),
            status_completed_text: TextColor::rgb(31, 132, 79),
        }
    }

    fn dark(shell: ShellTheme) -> Self {
        Self {
            backdrop: [0.000, 0.000, 0.000, 0.45],
            surface: [0.118, 0.129, 0.145, 0.99],
            header: [0.145, 0.157, 0.176, 1.0],
            panel: [0.102, 0.112, 0.126, 1.0],
            input: [0.078, 0.086, 0.098, 1.0],
            border: shell.divider(),
            divider: shell.divider(),
            row_alt: [0.125, 0.137, 0.153, 1.0],
            button_secondary: [0.145, 0.157, 0.176, 1.0],
            button_primary: shell.accent(),
            button_primary_soft: [0.102, 0.173, 0.286, 1.0],
            button_warning: [0.749, 0.435, 0.047, 1.0],
            button_danger: shell.task_status_color(ShellTaskStatusKind::Failed),
            field_focus: shell.accent(),
            marker_neutral: [0.580, 0.639, 0.718, 1.0],
            warning_header: [0.286, 0.196, 0.102, 1.0],
            warning_divider: [0.588, 0.361, 0.090, 1.0],
            selection_fill: [0.102, 0.173, 0.286, 1.0],
            scrollbar: shell.scrollbar(),
            title_text: shell.primary_text(),
            body_text: shell.primary_text(),
            muted_text: shell.muted_text(),
            soft_text: shell.section_text(),
            inverse_text: TextColor::rgb(255, 255, 255),
            error_text: TextColor::rgb(248, 113, 113),
            warning_text: TextColor::rgb(253, 186, 116),
            status_running: shell.task_status_color(ShellTaskStatusKind::Running),
            status_completed: shell.task_status_color(ShellTaskStatusKind::Completed),
            status_failed: shell.task_status_color(ShellTaskStatusKind::Failed),
            status_cancelled: shell.task_status_color(ShellTaskStatusKind::Cancelled),
            status_running_text: TextColor::rgb(147, 197, 253),
            status_completed_text: TextColor::rgb(134, 239, 172),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_theme_follows_shell_theme_mode() {
        let light_shell = ShellTheme::for_dark_mode(false);
        let dark_shell = ShellTheme::for_dark_mode(true);
        let light = PopupTheme::from_shell_theme(light_shell);
        let dark = PopupTheme::from_shell_theme(dark_shell);

        assert_ne!(light.surface, dark.surface);
        assert_eq!(
            light.status_fill(ShellTaskStatusKind::Failed),
            light_shell.task_status_color(ShellTaskStatusKind::Failed)
        );
        assert_eq!(
            dark.status_fill(ShellTaskStatusKind::Failed),
            dark_shell.task_status_color(ShellTaskStatusKind::Failed)
        );
        assert_eq!(dark.border, dark_shell.divider());
        assert_ne!(light.selection_fill, dark.selection_fill);
        assert_eq!(light.list_row_background(false), light.input);
        assert_eq!(dark.list_row_background(true), dark.row_alt);
        assert_eq!(dark.scrollbar, dark_shell.scrollbar());
    }
}
