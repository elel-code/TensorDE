pub(crate) use crate::shell::theme::UiColor;

use crate::shell::theme::ShellTheme;

pub(crate) const BREEZE_ITEM_ROUNDNESS: f32 = 5.0;
pub(crate) const BREEZE_FOCUS_PEN_WIDTH: f32 = 1.25;

const BREEZE_HIGHLIGHT: UiColor = [0.239, 0.502, 0.710, 1.0];
const BREEZE_TEXT: UiColor = [0.188, 0.220, 0.259, 1.0];
const VIEW_BASE: UiColor = [0.973, 0.976, 0.984, 1.0];
const VIEW_ALTERNATE_BASE: UiColor = [0.949, 0.957, 0.969, 1.0];
const BREEZE_LIGHT_FOCUS: UiColor = [0.217, 0.456, 0.645, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FileManagerItemPalette {
    view_base: UiColor,
    view_alternate_base: UiColor,
    highlight: UiColor,
    hover: UiColor,
    focus: UiColor,
}

impl FileManagerItemPalette {
    pub(crate) fn from_shell_theme(theme: ShellTheme) -> Self {
        if theme.is_dark() {
            Self {
                view_base: theme.view_mode_content(crate::shell::options::ShellViewMode::Details),
                view_alternate_base: theme.details_header(),
                highlight: theme.accent(),
                hover: [0.580, 0.639, 0.718, 0.10],
                focus: theme.accent(),
            }
        } else if theme.uses_transparent_background() {
            Self {
                view_base: theme.view_mode_content(crate::shell::options::ShellViewMode::Details),
                view_alternate_base: theme.details_header(),
                highlight: BREEZE_HIGHLIGHT,
                hover: with_alpha(BREEZE_TEXT, 0.06),
                focus: BREEZE_LIGHT_FOCUS,
            }
        } else {
            Self::light()
        }
    }

    pub(crate) fn light() -> Self {
        Self {
            view_base: VIEW_BASE,
            view_alternate_base: VIEW_ALTERNATE_BASE,
            highlight: BREEZE_HIGHLIGHT,
            hover: with_alpha(BREEZE_TEXT, 0.06),
            focus: BREEZE_LIGHT_FOCUS,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn item_background_color(selected: bool, hovered: bool) -> UiColor {
    item_background_color_for_palette(selected, hovered, FileManagerItemPalette::light())
}

pub(crate) fn item_background_color_for_palette(
    selected: bool,
    hovered: bool,
    palette: FileManagerItemPalette,
) -> UiColor {
    match (selected, hovered) {
        (true, true) => with_alpha(palette.highlight, 0.40),
        (true, false) => with_alpha(palette.highlight, 0.32),
        (false, true) => palette.hover,
        (false, false) => transparent(),
    }
}

pub(crate) fn item_background_color_for_palette_with_hover_progress(
    selected: bool,
    hovered: bool,
    palette: FileManagerItemPalette,
    hover_progress: f32,
) -> UiColor {
    if !hovered {
        return item_background_color_for_palette(selected, false, palette);
    }
    let progress = hover_progress.clamp(0.0, 1.0);
    let mut target = item_background_color_for_palette(selected, true, palette);
    let base = item_background_color_for_palette(selected, false, palette);
    target[3] = base[3] + (target[3] - base[3]) * progress;
    target
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn details_row_background_color(
    selected: bool,
    hovered: bool,
    alternate: bool,
) -> UiColor {
    details_row_background_color_for_palette(
        selected,
        hovered,
        alternate,
        FileManagerItemPalette::light(),
    )
}

pub(crate) fn details_row_background_color_for_palette(
    selected: bool,
    hovered: bool,
    alternate: bool,
    palette: FileManagerItemPalette,
) -> UiColor {
    match (selected, hovered, alternate) {
        (true, _, _) | (false, true, _) => {
            item_background_color_for_palette(selected, hovered, palette)
        }
        (false, false, true) => palette.view_alternate_base,
        (false, false, false) => palette.view_base,
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn place_row_background_color(active: bool, hovered: bool) -> UiColor {
    place_row_background_color_for_palette(active, hovered, FileManagerItemPalette::light())
}

pub(crate) fn place_row_background_color_for_palette(
    active: bool,
    hovered: bool,
    palette: FileManagerItemPalette,
) -> UiColor {
    item_background_color_for_palette(active, hovered, palette)
}

pub(crate) fn place_row_background_color_for_palette_with_hover_progress(
    active: bool,
    hovered: bool,
    palette: FileManagerItemPalette,
    hover_progress: f32,
) -> UiColor {
    item_background_color_for_palette_with_hover_progress(active, hovered, palette, hover_progress)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn item_focus_color(selected: bool, hovered: bool) -> UiColor {
    item_focus_color_for_palette(selected, hovered, FileManagerItemPalette::light())
}

pub(crate) fn item_focus_color_for_palette(
    selected: bool,
    hovered: bool,
    palette: FileManagerItemPalette,
) -> UiColor {
    with_alpha(palette.focus, if selected || hovered { 1.0 } else { 0.8 })
}

const fn with_alpha(mut color: UiColor, alpha: f32) -> UiColor {
    color[3] = alpha;
    color
}

const fn transparent() -> UiColor {
    [0.0, 0.0, 0.0, 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breeze_item_background_uses_file_manager_alpha_levels() {
        assert_eq!(
            item_background_color(true, false),
            [0.239, 0.502, 0.710, 0.32]
        );
        assert_eq!(
            item_background_color(true, true),
            [0.239, 0.502, 0.710, 0.40]
        );
        assert_eq!(
            item_background_color(false, true),
            [0.188, 0.220, 0.259, 0.06]
        );
        assert_eq!(item_background_color(false, false), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(
            place_row_background_color(true, false),
            item_background_color(true, false)
        );
    }

    #[test]
    fn hover_progress_eases_background_alpha_without_affecting_selection_base() {
        let palette = FileManagerItemPalette::light();
        assert_eq!(
            item_background_color_for_palette_with_hover_progress(false, true, palette, 0.0),
            [0.188, 0.220, 0.259, 0.0]
        );
        assert_eq!(
            item_background_color_for_palette_with_hover_progress(false, true, palette, 1.0),
            [0.188, 0.220, 0.259, 0.06]
        );
        assert_eq!(
            item_background_color_for_palette_with_hover_progress(true, true, palette, 0.0),
            [0.239, 0.502, 0.710, 0.32]
        );
        assert_eq!(
            item_background_color_for_palette_with_hover_progress(true, true, palette, 1.0),
            [0.239, 0.502, 0.710, 0.40]
        );
    }

    #[test]
    fn details_rows_keep_base_colors_when_not_interactive() {
        assert_eq!(details_row_background_color(false, false, false), VIEW_BASE);
        assert_eq!(
            details_row_background_color(false, false, true),
            VIEW_ALTERNATE_BASE
        );
    }

    #[test]
    fn breeze_focus_color_follows_file_manager_active_item_alpha() {
        assert_eq!(item_focus_color(true, false), [0.217, 0.456, 0.645, 1.0]);
        assert_eq!(item_focus_color(false, true), [0.217, 0.456, 0.645, 1.0]);
        assert_eq!(item_focus_color(false, false), [0.217, 0.456, 0.645, 0.8]);
    }

    #[test]
    fn dark_item_palette_uses_shell_theme_tokens() {
        let theme = ShellTheme::for_dark_mode(true);
        let palette = FileManagerItemPalette::from_shell_theme(theme);

        assert_eq!(
            details_row_background_color_for_palette(false, false, true, palette),
            theme.details_header()
        );
        assert_eq!(
            item_background_color_for_palette(true, false, palette),
            [0.184, 0.435, 0.929, 0.32]
        );
        assert_eq!(
            item_focus_color_for_palette(false, false, palette),
            [0.184, 0.435, 0.929, 0.8]
        );
    }

    #[test]
    fn transparent_background_does_not_add_idle_details_row_materials() {
        let theme = ShellTheme::for_transparent_background(false, 0.65);
        let palette = FileManagerItemPalette::from_shell_theme(theme);

        assert_eq!(
            details_row_background_color_for_palette(false, false, false, palette)[3],
            0.0
        );
        assert_eq!(
            details_row_background_color_for_palette(false, false, true, palette)[3],
            0.0
        );
        assert_eq!(
            item_background_color_for_palette(true, false, palette),
            [0.239, 0.502, 0.710, 0.32]
        );
    }
}
