use crate::windowing::PhysicalSize;
use fika_core::{Entry, ViewRect};

use crate::shell::icon_roles::{FILE_ICON_CORNER_RADIUS_RATIO, FOLDER_ICON_CORNER_RADIUS_RATIO};
use crate::shell::render::quad::{QuadVertex, push_clipped_rect, push_clipped_rounded_rect};
use crate::shell::theme::{NEUTRAL_ICON_COLOR, ShellScrollbarColors, ShellTheme, UiColor};

pub(crate) fn push_scrollbar(
    vertices: &mut Vec<QuadVertex>,
    track: ViewRect,
    thumb: ViewRect,
    clip: ViewRect,
    colors: ShellScrollbarColors,
    size: PhysicalSize<u32>,
) {
    let track_radius = track.width.min(track.height) / 2.0;
    let thumb_radius = thumb.width.min(thumb.height) / 2.0;
    push_clipped_rounded_rect(vertices, track, clip, track_radius, colors.track, size);
    push_clipped_rounded_rect(vertices, thumb, clip, thumb_radius, colors.thumb, size);
}

pub(crate) fn push_location_bar_icon(
    vertices: &mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    active: bool,
    theme: ShellTheme,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let colors = theme.toolbar_button(active);
    push_clipped_rounded_rect(
        vertices,
        bounds,
        clip,
        (5.0 * scale_factor).round().max(1.0),
        colors.fill,
        size,
    );
    let s = |value: f32| {
        (value * bounds.width.min(bounds.height) / 18.0)
            .round()
            .max(1.0)
    };
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(5.0),
            y: bounds.y + s(6.0),
            width: s(7.0),
            height: s(3.0),
        },
        clip,
        s(1.0),
        colors.icon,
        size,
    );
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(4.0),
            y: bounds.y + s(8.0),
            width: bounds.width - s(8.0),
            height: bounds.height - s(11.0),
        },
        clip,
        s(2.0),
        colors.icon,
        size,
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaceIconShape {
    Folder,
    Drive,
    Trash,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaceIconColorRole {
    Folder,
    Trash,
    Network,
    Root,
    Editable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlaceIconPaint {
    pub(crate) shape: PlaceIconShape,
    pub(crate) color_role: PlaceIconColorRole,
}

impl PlaceIconPaint {
    pub(crate) fn from_flags(
        trash: bool,
        network: bool,
        root: bool,
        editable: bool,
        drive_like: bool,
    ) -> Self {
        let shape = if trash {
            PlaceIconShape::Trash
        } else if root || network || drive_like {
            PlaceIconShape::Drive
        } else {
            PlaceIconShape::Folder
        };
        let color_role = if trash {
            PlaceIconColorRole::Trash
        } else if network {
            PlaceIconColorRole::Network
        } else if root {
            PlaceIconColorRole::Root
        } else if editable {
            PlaceIconColorRole::Editable
        } else {
            PlaceIconColorRole::Folder
        };
        Self { shape, color_role }
    }
}

pub(crate) fn push_place_icon(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    paint: PlaceIconPaint,
    theme: ShellTheme,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let (fg, bg) = place_icon_colors(paint, theme);
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        (6.0 * scale_factor).round().max(1.0),
        bg,
        size,
    );
    match paint.shape {
        PlaceIconShape::Folder => {
            push_place_folder_icon(vertices, rect, clip, fg, scale_factor, size)
        }
        PlaceIconShape::Drive => {
            push_place_drive_icon(vertices, rect, clip, fg, scale_factor, size)
        }
        PlaceIconShape::Trash => {
            push_place_trash_icon(vertices, rect, clip, fg, scale_factor, size)
        }
    }
}

fn place_icon_colors(paint: PlaceIconPaint, theme: ShellTheme) -> (UiColor, UiColor) {
    if theme.is_dark() {
        return match paint.color_role {
            PlaceIconColorRole::Trash => ([0.973, 0.444, 0.444, 1.0], [0.286, 0.102, 0.102, 1.0]),
            PlaceIconColorRole::Network => (theme.accent(), theme.toolbar_button(true).fill),
            PlaceIconColorRole::Root => ([0.580, 0.639, 0.718, 1.0], theme.field()),
            PlaceIconColorRole::Editable => {
                ([0.188, 0.839, 0.514, 1.0], [0.063, 0.220, 0.145, 1.0])
            }
            PlaceIconColorRole::Folder => ([0.953, 0.612, 0.071, 1.0], [0.286, 0.196, 0.102, 1.0]),
        };
    }
    match paint.color_role {
        PlaceIconColorRole::Trash => ([0.690, 0.282, 0.282, 1.0], [1.000, 0.922, 0.922, 1.0]),
        PlaceIconColorRole::Network => ([0.184, 0.435, 0.929, 1.0], [0.918, 0.945, 1.000, 1.0]),
        PlaceIconColorRole::Root => (NEUTRAL_ICON_COLOR, [0.902, 0.922, 0.945, 1.0]),
        PlaceIconColorRole::Editable => ([0.192, 0.486, 0.310, 1.0], [0.910, 0.973, 0.925, 1.0]),
        PlaceIconColorRole::Folder => ([0.749, 0.435, 0.047, 1.0], [1.000, 0.953, 0.855, 1.0]),
    }
}

fn place_icon_metric(value: f32, scale_factor: f32) -> f32 {
    (value * scale_factor).round().max(1.0)
}

fn push_place_folder_icon(
    vertices: &mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    fg: UiColor,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let s = |value| place_icon_metric(value, scale_factor);
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(5.0),
            y: bounds.y + s(6.0),
            width: s(7.0),
            height: s(3.0),
        },
        clip,
        s(1.0),
        fg,
        size,
    );
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(4.0),
            y: bounds.y + s(8.0),
            width: bounds.width - s(8.0),
            height: bounds.height - s(11.0),
        },
        clip,
        s(2.0),
        fg,
        size,
    );
}

fn push_place_drive_icon(
    vertices: &mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    fg: UiColor,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let s = |value| place_icon_metric(value, scale_factor);
    let body = ViewRect {
        x: bounds.x + s(4.0),
        y: bounds.y + s(5.0),
        width: bounds.width - s(8.0),
        height: bounds.height - s(10.0),
    };
    push_clipped_rounded_rect(vertices, body, clip, s(2.0), fg, size);
    push_clipped_rect(
        vertices,
        ViewRect {
            x: body.x + s(3.0),
            y: body.bottom() - s(4.0),
            width: body.width - s(6.0),
            height: s(1.0),
        },
        clip,
        [1.000, 1.000, 1.000, 0.75],
        size,
    );
}

fn push_place_trash_icon(
    vertices: &mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    fg: UiColor,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let s = |value| place_icon_metric(value, scale_factor);
    push_clipped_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(6.0),
            y: bounds.y + s(5.0),
            width: bounds.width - s(12.0),
            height: s(2.0),
        },
        clip,
        fg,
        size,
    );
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: bounds.x + s(5.0),
            y: bounds.y + s(8.0),
            width: bounds.width - s(10.0),
            height: bounds.height - s(12.0),
        },
        clip,
        s(2.0),
        fg,
        size,
    );
}

pub(crate) fn push_fallback_file_icon(
    vertices: &mut Vec<QuadVertex>,
    entry: &Entry,
    icon_rect: ViewRect,
    content_clip: ViewRect,
    theme: ShellTheme,
    size: PhysicalSize<u32>,
) {
    let palette = FallbackIconPalette::from_shell_theme(theme);
    if entry.is_dir {
        let tab = ViewRect {
            x: icon_rect.x + icon_rect.width * 0.12,
            y: icon_rect.y + icon_rect.height * 0.16,
            width: icon_rect.width * 0.42,
            height: icon_rect.height * 0.18,
        };
        let body = ViewRect {
            x: icon_rect.x + icon_rect.width * 0.08,
            y: icon_rect.y + icon_rect.height * 0.28,
            width: icon_rect.width * 0.84,
            height: icon_rect.height * 0.56,
        };
        let radius =
            (icon_rect.width.min(icon_rect.height) * FOLDER_ICON_CORNER_RADIUS_RATIO * 0.8)
                .max(1.0);
        push_clipped_rounded_rect(
            vertices,
            tab,
            content_clip,
            radius,
            palette.folder_tab,
            size,
        );
        push_clipped_rounded_rect(
            vertices,
            body,
            content_clip,
            radius,
            palette.folder_body,
            size,
        );
        let highlight_inset = radius * 0.55;
        let highlight = ViewRect {
            x: body.x + highlight_inset,
            y: body.y + radius * 0.45,
            width: (body.width - highlight_inset * 2.0).max(1.0),
            height: (body.height * 0.10).max(1.0),
        };
        push_clipped_rounded_rect(
            vertices,
            highlight,
            content_clip,
            highlight.height / 2.0,
            palette.folder_highlight,
            size,
        );
    } else {
        let body = ViewRect {
            x: icon_rect.x + icon_rect.width * 0.18,
            y: icon_rect.y + icon_rect.height * 0.10,
            width: icon_rect.width * 0.64,
            height: icon_rect.height * 0.78,
        };
        let radius = (body.width.min(body.height) * FILE_ICON_CORNER_RADIUS_RATIO).max(1.0);
        push_clipped_rounded_rect(
            vertices,
            body,
            content_clip,
            radius,
            fallback_file_color(entry, palette),
            size,
        );
        let fold = icon_rect.width.min(icon_rect.height) * 0.18;
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: body.right() - fold,
                y: body.y,
                width: fold,
                height: fold,
            },
            content_clip,
            radius * 0.65,
            palette.file_fold,
            size,
        );
        push_fallback_file_glyph(vertices, entry, body, content_clip, palette, size);
    }
}

fn push_fallback_file_glyph(
    vertices: &mut Vec<QuadVertex>,
    entry: &Entry,
    body: ViewRect,
    content_clip: ViewRect,
    palette: FallbackIconPalette,
    size: PhysicalSize<u32>,
) {
    let mime = entry.mime_type.as_deref().unwrap_or_default();
    if mime.starts_with("image/") {
        let dot = body.width.min(body.height) * 0.12;
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: body.x + body.width * 0.60,
                y: body.y + body.height * 0.36,
                width: dot,
                height: dot,
            },
            content_clip,
            dot / 2.0,
            palette.file_stripe,
            size,
        );
        for step in 0..3 {
            push_clipped_rounded_rect(
                vertices,
                ViewRect {
                    x: body.x + body.width * (0.18 + step as f32 * 0.12),
                    y: body.y + body.height * (0.68 - step as f32 * 0.08),
                    width: body.width * 0.18,
                    height: (body.height * 0.055).max(1.0),
                },
                content_clip,
                (body.height * 0.025).max(1.0),
                palette.file_stripe,
                size,
            );
        }
        return;
    }
    if mime.starts_with("video/") || mime.starts_with("audio/") {
        for step in 0..3 {
            push_clipped_rounded_rect(
                vertices,
                ViewRect {
                    x: body.x + body.width * (0.24 + step as f32 * 0.12),
                    y: body.y + body.height * (0.42 + step as f32 * 0.08),
                    width: body.width * 0.18,
                    height: (body.height * 0.08).max(1.0),
                },
                content_clip,
                (body.height * 0.035).max(1.0),
                palette.file_stripe,
                size,
            );
        }
        return;
    }
    for row in 0..3 {
        let line_width = body.width * if row == 2 { 0.36 } else { 0.52 };
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: body.x + body.width * 0.18,
                y: body.y + body.height * 0.42 + row as f32 * body.height * 0.14,
                width: line_width,
                height: (body.height * 0.045).max(1.0),
            },
            content_clip,
            (body.height * 0.025).max(1.0),
            palette.file_stripe,
            size,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FallbackIconPalette {
    folder_tab: UiColor,
    folder_body: UiColor,
    folder_highlight: UiColor,
    file_stripe: UiColor,
    file_fold: UiColor,
    image_file: UiColor,
    media_file: UiColor,
    text_file: UiColor,
    generic_file: UiColor,
}

impl FallbackIconPalette {
    pub(crate) fn from_shell_theme(theme: ShellTheme) -> Self {
        if theme.is_dark() {
            Self {
                folder_tab: [0.953, 0.612, 0.071, 1.0],
                folder_body: [0.749, 0.435, 0.047, 1.0],
                folder_highlight: [1.000, 0.773, 0.204, 0.48],
                file_stripe: theme.field_separator(),
                file_fold: [0.580, 0.639, 0.718, 0.50],
                image_file: [0.302, 0.741, 0.514, 1.0],
                media_file: [0.678, 0.560, 0.871, 1.0],
                text_file: [0.376, 0.647, 0.980, 1.0],
                generic_file: [0.420, 0.466, 0.545, 1.0],
            }
        } else {
            Self {
                folder_tab: [0.960, 0.700, 0.260, 1.0],
                folder_body: [0.900, 0.580, 0.180, 1.0],
                folder_highlight: [1.000, 0.820, 0.420, 0.50],
                file_stripe: [0.760, 0.800, 0.860, 1.0],
                file_fold: [0.902, 0.922, 0.945, 0.92],
                image_file: [0.500, 0.700, 0.560, 1.0],
                media_file: [0.690, 0.550, 0.820, 1.0],
                text_file: [0.380, 0.600, 0.840, 1.0],
                generic_file: [0.550, 0.600, 0.680, 1.0],
            }
        }
    }
}

pub(crate) fn fallback_file_color(entry: &Entry, palette: FallbackIconPalette) -> UiColor {
    let mime = entry.mime_type.as_deref().unwrap_or_default();
    fallback_file_color_for_mime(mime, palette)
}

fn fallback_file_color_for_mime(mime: &str, palette: FallbackIconPalette) -> UiColor {
    if mime.starts_with("image/") {
        palette.image_file
    } else if mime.starts_with("video/") || mime.starts_with("audio/") {
        palette.media_file
    } else if mime.contains("text") || mime.contains("json") || mime.contains("xml") {
        palette.text_file
    } else {
        palette.generic_file
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_icon_palette_follows_shell_theme() {
        let light = FallbackIconPalette::from_shell_theme(ShellTheme::for_dark_mode(false));
        let dark_theme = ShellTheme::for_dark_mode(true);
        let dark = FallbackIconPalette::from_shell_theme(dark_theme);

        assert_eq!(light.file_stripe, [0.760, 0.800, 0.860, 1.0]);
        assert_eq!(dark.file_stripe, dark_theme.field_separator());
        assert_ne!(light.generic_file, dark.generic_file);
        assert_eq!(
            fallback_file_color_for_mime("text/plain", dark),
            dark.text_file
        );
        assert_eq!(
            fallback_file_color_for_mime("video/mp4", dark),
            dark.media_file
        );
    }

    #[test]
    fn place_icon_paint_uses_semantic_shape_and_theme_colors() {
        let paint = PlaceIconPaint::from_flags(false, true, false, false, false);
        assert_eq!(paint.shape, PlaceIconShape::Drive);
        assert_eq!(paint.color_role, PlaceIconColorRole::Network);

        let dark = ShellTheme::for_dark_mode(true);
        assert_eq!(
            place_icon_colors(paint, dark),
            (dark.accent(), dark.toolbar_button(true).fill)
        );

        let folder = PlaceIconPaint::from_flags(false, false, false, false, false);
        assert_eq!(
            place_icon_colors(folder, dark),
            ([0.953, 0.612, 0.071, 1.0], [0.286, 0.196, 0.102, 1.0])
        );
    }
}
