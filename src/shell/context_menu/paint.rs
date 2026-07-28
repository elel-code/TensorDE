use crate::windowing::PhysicalSize;
use cosmic_text::Color as TextColor;
use fika_core::ViewRect;

use crate::shell::context_menu::{
    ShellContextMenu, ShellContextMenuAction, ShellContextMenuCommand, ShellContextMenuIcon,
    ShellContextMenuItem, context_menu_items, context_menu_separator_before,
    context_submenu_actions,
};
use crate::shell::drop_menu::{
    ShellDropMenu, ShellDropMenuCommand, ShellDropMenuIcon, drop_menu_items,
};
use crate::shell::icon_roles::NamedIconFallback;
use crate::shell::menu_geometry::{
    context_menu_rect_scaled, context_menu_submenu_rect, drop_menu_rect_scaled,
    scaled_context_menu_metric,
};
use crate::shell::metrics::{
    CONTEXT_MENU_ICON_SIZE, CONTEXT_MENU_ROW_HEIGHT, CONTEXT_MENU_TEXT_LINE_HEIGHT,
    CONTEXT_MENU_VERTICAL_PADDING,
};
use crate::shell::theme::{NEUTRAL_ICON_COLOR, PROPERTIES_ICON_COLOR, ShellTheme, UiColor};
use crate::{
    IconDrawLayer, IconFrameBuilder, LabelAlignment, QuadVertex, TextFrameBuilder,
    push_clipped_rect, push_clipped_rect_outline, push_clipped_rounded_rect, push_rect,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextMenuGlyph {
    Open,
    OpenWith,
    Pane,
    Hidden,
    Copy,
    Cut,
    Location,
    Rename,
    Trash,
    Restore,
    Delete,
    Place,
    Create,
    Paste,
    Select,
    Refresh,
    Properties,
    Remove,
}

#[derive(Clone, Copy, Debug)]
struct ContextMenuIconColors {
    foreground: [f32; 4],
    background: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
struct ContextMenuPaintTheme {
    surface: UiColor,
    hover: UiColor,
    separator: UiColor,
    border: UiColor,
    text: TextColor,
    hover_text: TextColor,
    muted_text: TextColor,
}

impl ContextMenuPaintTheme {
    fn from_shell_theme(theme: ShellTheme) -> Self {
        if theme.is_dark() {
            Self {
                surface: theme.field(),
                hover: theme.toolbar_button(true).fill,
                separator: theme.divider(),
                border: theme.divider(),
                text: theme.primary_text(),
                hover_text: theme.accent_text(),
                muted_text: theme.muted_text(),
            }
        } else {
            Self {
                surface: [1.000, 1.000, 1.000, 1.0],
                hover: [0.918, 0.945, 1.000, 1.0],
                separator: [0.898, 0.906, 0.922, 1.0],
                border: [0.784, 0.808, 0.839, 1.0],
                text: TextColor::rgb(36, 41, 47),
                hover_text: TextColor::rgb(31, 79, 191),
                muted_text: TextColor::rgb(89, 99, 110),
            }
        }
    }
}

struct ContextMenuOverlayPainter<'frame, 'text, 'icons> {
    theme: ContextMenuPaintTheme,
    scale: f32,
    vertices: &'frame mut Vec<QuadVertex>,
    text: &'frame mut TextFrameBuilder<'text>,
    icons: &'frame mut IconFrameBuilder<'icons>,
    size: PhysicalSize<u32>,
}

#[derive(Clone, Copy)]
pub(crate) struct ContextMenuOverlayConfig {
    pub(crate) show_hidden: bool,
    pub(crate) theme: ShellTheme,
    pub(crate) scale: f32,
    pub(crate) size: PhysicalSize<u32>,
}

pub(crate) fn push_context_menu_overlay(
    menu: &ShellContextMenu,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    icons: &mut IconFrameBuilder<'_>,
    config: ContextMenuOverlayConfig,
) {
    let ContextMenuOverlayConfig {
        show_hidden,
        theme,
        scale,
        size,
    } = config;
    let mut painter = ContextMenuOverlayPainter {
        theme: ContextMenuPaintTheme::from_shell_theme(theme),
        scale,
        vertices,
        text,
        icons,
        size,
    };
    paint_context_menu_root(menu, show_hidden, &mut painter);
    paint_context_submenu_overlay(menu, show_hidden, &mut painter);
}

pub(crate) fn push_drop_menu_overlay(
    menu: &ShellDropMenu,
    shell_theme: ShellTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let theme = ContextMenuPaintTheme::from_shell_theme(shell_theme);
    let rect = drop_menu_rect_scaled(menu, size, scale);
    let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    let row_padding_x = scaled_context_menu_metric(8.0, scale);
    let gap = scaled_context_menu_metric(8.0, scale);
    let icon_size = scaled_context_menu_metric(CONTEXT_MENU_ICON_SIZE, scale);
    let text_height = scaled_context_menu_metric(CONTEXT_MENU_TEXT_LINE_HEIGHT, scale);
    let clip = screen_rect(size);
    push_context_menu_shadow(vertices, rect, clip, scale, size);
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        scaled_context_menu_metric(6.0, scale),
        theme.surface,
        size,
    );

    for (row, item) in drop_menu_items().iter().enumerate() {
        let row_rect = ViewRect {
            x: rect.x,
            y: rect.y + padding_y + row as f32 * row_height,
            width: rect.width,
            height: row_height,
        };
        if menu.hovered_row == Some(row) {
            push_rect(vertices, row_rect, theme.hover, size);
        }
        if matches!(item.command, ShellDropMenuCommand::Cancel) {
            push_clipped_rect(
                vertices,
                ViewRect {
                    x: rect.x + row_padding_x,
                    y: row_rect.y,
                    width: (rect.width - row_padding_x * 2.0).max(1.0),
                    height: scale.round().max(1.0),
                },
                rect,
                theme.separator,
                size,
            );
        }
        let icon = ViewRect {
            x: row_rect.x + row_padding_x,
            y: row_rect.y + (row_rect.height - icon_size) / 2.0,
            width: icon_size,
            height: icon_size,
        };
        let (glyph, foreground, background) = drop_menu_item_icon_style(item.icon);
        push_context_menu_icon(
            vertices,
            icon,
            rect,
            glyph,
            ContextMenuIconColors {
                foreground,
                background,
            },
            scale,
            size,
        );
        let text_x = icon.right() + gap;
        text.push_label_aligned(
            item.label,
            ViewRect {
                x: text_x,
                y: row_rect.y + (row_rect.height - text_height) / 2.0,
                width: (row_rect.right() - text_x - row_padding_x).max(1.0),
                height: text_height,
            },
            rect,
            menu_text_color(menu.hovered_row == Some(row), theme),
            LabelAlignment::Start,
        );
    }
    push_clipped_rect_outline(vertices, rect, clip, 1.0, theme.border, size);
}

fn paint_context_menu_root(
    menu: &ShellContextMenu,
    show_hidden: bool,
    painter: &mut ContextMenuOverlayPainter<'_, '_, '_>,
) {
    let ContextMenuOverlayPainter {
        theme,
        scale,
        vertices,
        text,
        icons,
        size,
    } = painter;
    let theme = *theme;
    let scale = *scale;
    let size = *size;
    let rect = context_menu_rect_scaled(menu, size, scale);
    let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    let row_padding_x = scaled_context_menu_metric(8.0, scale);
    let gap = scaled_context_menu_metric(8.0, scale);
    let icon_size = scaled_context_menu_metric(CONTEXT_MENU_ICON_SIZE, scale);
    let text_height = scaled_context_menu_metric(CONTEXT_MENU_TEXT_LINE_HEIGHT, scale);
    let clip = screen_rect(size);
    push_context_menu_shadow(vertices, rect, clip, scale, size);
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        scaled_context_menu_metric(6.0, scale),
        theme.surface,
        size,
    );

    let items = context_menu_items(menu);
    for (row, item) in items.iter().enumerate() {
        let row_rect = ViewRect {
            x: rect.x,
            y: rect.y + padding_y + row as f32 * row_height,
            width: rect.width,
            height: row_height,
        };
        if menu.hovered_row == Some(row) {
            push_rect(vertices, row_rect, theme.hover, size);
        }
        if context_menu_separator_before(&menu.target, row) {
            push_clipped_rect(
                vertices,
                ViewRect {
                    x: rect.x + row_padding_x,
                    y: row_rect.y,
                    width: (rect.width - row_padding_x * 2.0).max(1.0),
                    height: scale.round().max(1.0),
                },
                rect,
                theme.separator,
                size,
            );
        }
        let icon = ViewRect {
            x: row_rect.x + row_padding_x,
            y: row_rect.y + (row_rect.height - icon_size) / 2.0,
            width: icon_size,
            height: icon_size,
        };
        push_context_menu_item_icon(vertices, icons, item, icon, rect, scale, size);
        let text_x = icon.right() + gap;
        text.push_label_aligned(
            context_menu_item_label(item, show_hidden).as_str(),
            ViewRect {
                x: text_x,
                y: row_rect.y + (row_rect.height - text_height) / 2.0,
                width: (row_rect.right()
                    - text_x
                    - row_padding_x
                    - if item.submenu.is_some() { gap } else { 0.0 })
                .max(1.0),
                height: text_height,
            },
            rect,
            menu_text_color(menu.hovered_row == Some(row), theme),
            LabelAlignment::Start,
        );
        if item.submenu.is_some() {
            text.push_label_aligned(
                ">",
                ViewRect {
                    x: row_rect.right() - row_padding_x - gap,
                    y: row_rect.y + (row_rect.height - text_height) / 2.0,
                    width: gap,
                    height: text_height,
                },
                rect,
                theme.muted_text,
                LabelAlignment::Center,
            );
        }
    }
    push_clipped_rect_outline(vertices, rect, clip, 1.0, theme.border, size);
}

pub(crate) fn context_menu_named_icon_request(
    item: &ShellContextMenuItem,
) -> Option<(&str, NamedIconFallback)> {
    match &item.icon {
        ShellContextMenuIcon::Service(Some(icon)) => {
            let icon = icon.trim();
            (!icon.is_empty()).then_some((icon, NamedIconFallback::Service))
        }
        ShellContextMenuIcon::Service(None) => Some(("system-run", NamedIconFallback::Service)),
        ShellContextMenuIcon::Application(Some(icon)) => {
            let icon = icon.trim();
            (!icon.is_empty()).then_some((icon, NamedIconFallback::Application))
        }
        ShellContextMenuIcon::Application(None) => {
            Some(("application-x-executable", NamedIconFallback::Application))
        }
        _ => None,
    }
}

fn paint_context_submenu_overlay(
    menu: &ShellContextMenu,
    show_hidden: bool,
    painter: &mut ContextMenuOverlayPainter<'_, '_, '_>,
) {
    let ContextMenuOverlayPainter {
        theme,
        scale,
        vertices,
        text,
        icons,
        size,
    } = painter;
    let theme = *theme;
    let scale = *scale;
    let size = *size;
    let Some(submenu) = menu.active_submenu else {
        return;
    };
    let Some(rect) = context_menu_submenu_rect(menu, size, scale) else {
        return;
    };
    let items = context_submenu_actions(submenu, menu);
    let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    let row_padding_x = scaled_context_menu_metric(8.0, scale);
    let gap = scaled_context_menu_metric(8.0, scale);
    let icon_size = scaled_context_menu_metric(CONTEXT_MENU_ICON_SIZE, scale);
    let text_height = scaled_context_menu_metric(CONTEXT_MENU_TEXT_LINE_HEIGHT, scale);
    let clip = screen_rect(size);
    push_context_menu_shadow(vertices, rect, clip, scale, size);
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        scaled_context_menu_metric(6.0, scale),
        theme.surface,
        size,
    );
    for (row, item) in items.iter().enumerate() {
        let row_rect = ViewRect {
            x: rect.x,
            y: rect.y + padding_y + row as f32 * row_height,
            width: rect.width,
            height: row_height,
        };
        if menu.hovered_submenu_row == Some(row) {
            push_rect(vertices, row_rect, theme.hover, size);
        }
        if item.separator_before {
            push_clipped_rect(
                vertices,
                ViewRect {
                    x: rect.x + row_padding_x,
                    y: row_rect.y,
                    width: (rect.width - row_padding_x * 2.0).max(1.0),
                    height: scale.round().max(1.0),
                },
                rect,
                theme.separator,
                size,
            );
        }
        let icon = ViewRect {
            x: row_rect.x + row_padding_x,
            y: row_rect.y + (row_rect.height - icon_size) / 2.0,
            width: icon_size,
            height: icon_size,
        };
        push_context_menu_item_icon(vertices, icons, item, icon, rect, scale, size);
        let text_x = icon.right() + gap;
        text.push_label_aligned(
            context_menu_item_label(item, show_hidden).as_str(),
            ViewRect {
                x: text_x,
                y: row_rect.y + (row_rect.height - text_height) / 2.0,
                width: (row_rect.right() - text_x - row_padding_x).max(1.0),
                height: text_height,
            },
            rect,
            menu_text_color(menu.hovered_submenu_row == Some(row), theme),
            LabelAlignment::Start,
        );
    }
    push_clipped_rect_outline(vertices, rect, clip, 1.0, theme.border, size);
}

fn push_context_menu_item_icon(
    vertices: &mut Vec<QuadVertex>,
    icons: &mut IconFrameBuilder<'_>,
    item: &ShellContextMenuItem,
    icon: ViewRect,
    clip: ViewRect,
    scale: f32,
    size: PhysicalSize<u32>,
) {
    if let Some((icon_name, fallback)) = context_menu_named_icon_request(item)
        && icons.push_named_theme_icon(icon_name, fallback, icon, clip, IconDrawLayer::Overlay)
    {
        return;
    }
    let (glyph, foreground, background) = context_menu_item_icon_style(item);
    push_context_menu_icon(
        vertices,
        icon,
        clip,
        glyph,
        ContextMenuIconColors {
            foreground,
            background,
        },
        scale,
        size,
    );
}

fn context_menu_icon_style(
    action: ShellContextMenuAction,
) -> (ContextMenuGlyph, [f32; 4], [f32; 4]) {
    match action {
        ShellContextMenuAction::OpenWith => (
            ContextMenuGlyph::OpenWith,
            [0.263, 0.220, 0.792, 1.0],
            [0.933, 0.929, 1.000, 1.0],
        ),
        ShellContextMenuAction::OpenInNewPane => (
            ContextMenuGlyph::Pane,
            [0.114, 0.306, 0.847, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::SplitPane => (
            ContextMenuGlyph::Pane,
            [0.114, 0.306, 0.847, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::Copy => (
            ContextMenuGlyph::Copy,
            [0.145, 0.388, 0.922, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::Cut => (
            ContextMenuGlyph::Cut,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
        ShellContextMenuAction::CopyLocation => (
            ContextMenuGlyph::Location,
            [0.200, 0.255, 0.333, 1.0],
            [0.910, 0.933, 0.969, 1.0],
        ),
        ShellContextMenuAction::Rename => (
            ContextMenuGlyph::Rename,
            [0.427, 0.157, 0.851, 1.0],
            [0.949, 0.929, 1.000, 1.0],
        ),
        ShellContextMenuAction::RenameAsAdministrator => (
            ContextMenuGlyph::Rename,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
        ShellContextMenuAction::MoveToTrash | ShellContextMenuAction::EmptyTrash => (
            ContextMenuGlyph::Trash,
            [0.725, 0.110, 0.110, 1.0],
            [1.000, 0.910, 0.910, 1.0],
        ),
        ShellContextMenuAction::MoveToTrashAsAdministrator => (
            ContextMenuGlyph::Trash,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
        ShellContextMenuAction::RestoreFromTrash => (
            ContextMenuGlyph::Restore,
            [0.016, 0.471, 0.341, 1.0],
            [0.906, 0.973, 0.937, 1.0],
        ),
        ShellContextMenuAction::DeletePermanently => (
            ContextMenuGlyph::Delete,
            [0.725, 0.110, 0.110, 1.0],
            [1.000, 0.910, 0.910, 1.0],
        ),
        ShellContextMenuAction::AddToPlaces => (
            ContextMenuGlyph::Place,
            [0.059, 0.463, 0.431, 1.0],
            [0.902, 1.000, 0.984, 1.0],
        ),
        ShellContextMenuAction::AddNetworkFolder => (
            ContextMenuGlyph::Place,
            [0.059, 0.463, 0.431, 1.0],
            [0.902, 1.000, 0.984, 1.0],
        ),
        ShellContextMenuAction::CreateNew => (
            ContextMenuGlyph::Create,
            [0.059, 0.298, 0.506, 1.0],
            [0.906, 0.945, 0.984, 1.0],
        ),
        ShellContextMenuAction::Paste => (
            ContextMenuGlyph::Paste,
            [0.016, 0.471, 0.341, 1.0],
            [0.906, 0.973, 0.937, 1.0],
        ),
        ShellContextMenuAction::PasteAsAdministrator => (
            ContextMenuGlyph::Paste,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
        ShellContextMenuAction::SelectAll => (
            ContextMenuGlyph::Select,
            [0.122, 0.310, 0.749, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::ViewMode => (
            ContextMenuGlyph::Pane,
            [0.114, 0.306, 0.847, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::ToggleHiddenFiles => (
            ContextMenuGlyph::Hidden,
            NEUTRAL_ICON_COLOR,
            [0.933, 0.945, 0.961, 1.0],
        ),
        ShellContextMenuAction::Refresh => (
            ContextMenuGlyph::Refresh,
            [0.059, 0.463, 0.431, 1.0],
            [0.902, 1.000, 0.984, 1.0],
        ),
        ShellContextMenuAction::Properties => (
            ContextMenuGlyph::Properties,
            PROPERTIES_ICON_COLOR,
            [0.933, 0.945, 0.961, 1.0],
        ),
        ShellContextMenuAction::RemovePlace => (
            ContextMenuGlyph::Remove,
            [0.725, 0.110, 0.110, 1.0],
            [1.000, 0.910, 0.910, 1.0],
        ),
        ShellContextMenuAction::MountDevice => (
            ContextMenuGlyph::Restore,
            [0.016, 0.471, 0.341, 1.0],
            [0.906, 0.973, 0.937, 1.0],
        ),
        ShellContextMenuAction::UnmountDevice
        | ShellContextMenuAction::EjectDevice
        | ShellContextMenuAction::SafelyRemoveDevice => (
            ContextMenuGlyph::Open,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
    }
}

fn context_menu_item_label(item: &ShellContextMenuItem, show_hidden: bool) -> String {
    match item.command {
        ShellContextMenuCommand::Builtin(action) => {
            action.label_for_hidden_state(show_hidden).to_string()
        }
        _ => item.label.clone(),
    }
}

fn context_menu_item_icon_style(
    item: &ShellContextMenuItem,
) -> (ContextMenuGlyph, [f32; 4], [f32; 4]) {
    match &item.icon {
        ShellContextMenuIcon::Builtin(action) => context_menu_icon_style(*action),
        ShellContextMenuIcon::Service(_) => (
            ContextMenuGlyph::Properties,
            PROPERTIES_ICON_COLOR,
            [0.933, 0.945, 0.961, 1.0],
        ),
        ShellContextMenuIcon::Application(_) => (
            ContextMenuGlyph::OpenWith,
            [0.263, 0.220, 0.792, 1.0],
            [0.933, 0.929, 1.000, 1.0],
        ),
    }
}

fn drop_menu_item_icon_style(icon: ShellDropMenuIcon) -> (ContextMenuGlyph, [f32; 4], [f32; 4]) {
    match icon {
        ShellDropMenuIcon::Copy => context_menu_icon_style(ShellContextMenuAction::Copy),
        ShellDropMenuIcon::Move => context_menu_icon_style(ShellContextMenuAction::Cut),
        ShellDropMenuIcon::Link => context_menu_icon_style(ShellContextMenuAction::CopyLocation),
        ShellDropMenuIcon::Cancel => context_menu_icon_style(ShellContextMenuAction::RemovePlace),
    }
}

pub(crate) fn push_context_menu_shadow(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let scale = scale_factor.max(1.0);
    let radius = (6.0 * scale).round().max(1.0);
    for (dy, spread, alpha) in [(1.0, 1.0, 0.10), (3.0, 3.0, 0.08), (7.0, 8.0, 0.05)] {
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: rect.x - (spread * scale).round(),
                y: rect.y + (dy * scale).round() - (spread * scale).round(),
                width: rect.width + (spread * 2.0 * scale).round(),
                height: rect.height + (spread * 2.0 * scale).round(),
            },
            clip,
            radius + (spread * scale).round(),
            [0.000, 0.000, 0.000, alpha],
            size,
        );
    }
}

struct ContextMenuGlyphPainter<'a> {
    vertices: &'a mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    size: PhysicalSize<u32>,
    unit: f32,
}

impl<'a> ContextMenuGlyphPainter<'a> {
    fn new(
        vertices: &'a mut Vec<QuadVertex>,
        bounds: ViewRect,
        clip: ViewRect,
        size: PhysicalSize<u32>,
    ) -> Self {
        Self {
            vertices,
            bounds,
            clip,
            size,
            unit: bounds.width.min(bounds.height) / CONTEXT_MENU_ICON_SIZE,
        }
    }

    fn push_piece(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        color: [f32; 4],
    ) {
        let piece = ViewRect {
            x: self.bounds.x + (x * self.unit).round(),
            y: self.bounds.y + (y * self.unit).round(),
            width: (width * self.unit).round().max(1.0),
            height: (height * self.unit).round().max(1.0),
        };
        push_clipped_rounded_rect(
            self.vertices,
            piece,
            self.clip,
            (radius * self.unit).round(),
            color,
            self.size,
        );
    }
}

fn push_context_menu_icon(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    glyph: ContextMenuGlyph,
    colors: ContextMenuIconColors,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let ContextMenuIconColors {
        foreground: fg,
        background: bg,
    } = colors;
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        (5.0 * scale_factor).round().max(1.0),
        bg,
        size,
    );
    let mut painter = ContextMenuGlyphPainter::new(vertices, rect, clip, size);
    match glyph {
        ContextMenuGlyph::Open => {
            painter.push_piece(5.0, 5.0, 6.0, 3.0, 1.0, fg);
            painter.push_piece(4.0, 7.0, 10.0, 7.0, 2.0, fg);
        }
        ContextMenuGlyph::OpenWith => {
            for (x, y) in [(5.0, 5.0), (10.0, 5.0), (5.0, 10.0), (10.0, 10.0)] {
                painter.push_piece(x, y, 3.0, 3.0, 1.0, fg);
            }
        }
        ContextMenuGlyph::Pane => {
            painter.push_piece(4.0, 4.0, 10.0, 10.0, 2.0, fg);
            painter.push_piece(8.0, 5.0, 1.0, 8.0, 0.0, bg);
            painter.push_piece(5.0, 8.0, 8.0, 1.0, 0.0, bg);
        }
        ContextMenuGlyph::Hidden => {
            painter.push_piece(4.0, 8.0, 10.0, 3.0, 2.0, fg);
            painter.push_piece(7.0, 6.0, 4.0, 7.0, 2.0, fg);
            painter.push_piece(8.0, 8.0, 2.0, 3.0, 1.0, bg);
        }
        ContextMenuGlyph::Copy => {
            painter.push_piece(6.0, 4.0, 7.0, 9.0, 1.0, fg);
            painter.push_piece(4.0, 6.0, 7.0, 9.0, 1.0, fg);
            painter.push_piece(5.0, 7.0, 5.0, 7.0, 0.0, bg);
        }
        ContextMenuGlyph::Cut => {
            painter.push_piece(4.0, 5.0, 3.0, 3.0, 2.0, fg);
            painter.push_piece(4.0, 11.0, 3.0, 3.0, 2.0, fg);
            painter.push_piece(8.0, 6.0, 6.0, 2.0, 1.0, fg);
            painter.push_piece(8.0, 11.0, 6.0, 2.0, 1.0, fg);
        }
        ContextMenuGlyph::Location => {
            painter.push_piece(5.0, 4.0, 8.0, 8.0, 4.0, fg);
            painter.push_piece(8.0, 7.0, 2.0, 2.0, 1.0, bg);
            painter.push_piece(8.0, 11.0, 2.0, 4.0, 1.0, fg);
        }
        ContextMenuGlyph::Rename => {
            painter.push_piece(4.0, 10.0, 8.0, 3.0, 1.0, fg);
            painter.push_piece(11.0, 8.0, 3.0, 3.0, 1.0, fg);
            painter.push_piece(4.0, 14.0, 9.0, 1.0, 0.0, fg);
        }
        ContextMenuGlyph::Trash => {
            painter.push_piece(5.0, 5.0, 8.0, 2.0, 1.0, fg);
            painter.push_piece(6.0, 8.0, 6.0, 7.0, 1.0, fg);
            painter.push_piece(7.0, 9.0, 1.0, 5.0, 0.0, bg);
            painter.push_piece(10.0, 9.0, 1.0, 5.0, 0.0, bg);
        }
        ContextMenuGlyph::Restore => {
            painter.push_piece(5.0, 5.0, 2.0, 8.0, 1.0, fg);
            painter.push_piece(6.0, 11.0, 7.0, 2.0, 1.0, fg);
            painter.push_piece(11.0, 8.0, 2.0, 4.0, 1.0, fg);
            painter.push_piece(9.0, 7.0, 5.0, 2.0, 1.0, fg);
        }
        ContextMenuGlyph::Delete => {
            painter.push_piece(5.0, 5.0, 2.0, 2.0, 1.0, fg);
            painter.push_piece(8.0, 8.0, 2.0, 2.0, 1.0, fg);
            painter.push_piece(11.0, 11.0, 2.0, 2.0, 1.0, fg);
            painter.push_piece(11.0, 5.0, 2.0, 2.0, 1.0, fg);
            painter.push_piece(5.0, 11.0, 2.0, 2.0, 1.0, fg);
        }
        ContextMenuGlyph::Place => {
            painter.push_piece(5.0, 4.0, 8.0, 11.0, 1.0, fg);
            painter.push_piece(7.0, 11.0, 4.0, 4.0, 0.0, bg);
        }
        ContextMenuGlyph::Create => {
            painter.push_piece(8.0, 4.0, 2.0, 10.0, 1.0, fg);
            painter.push_piece(4.0, 8.0, 10.0, 2.0, 1.0, fg);
        }
        ContextMenuGlyph::Paste => {
            painter.push_piece(5.0, 5.0, 8.0, 10.0, 1.0, fg);
            painter.push_piece(7.0, 4.0, 4.0, 3.0, 1.0, fg);
            painter.push_piece(7.0, 9.0, 4.0, 1.0, 0.0, bg);
            painter.push_piece(7.0, 12.0, 4.0, 1.0, 0.0, bg);
        }
        ContextMenuGlyph::Select => {
            painter.push_piece(5.0, 5.0, 8.0, 2.0, 1.0, fg);
            painter.push_piece(5.0, 11.0, 8.0, 2.0, 1.0, fg);
            painter.push_piece(5.0, 5.0, 2.0, 8.0, 1.0, fg);
            painter.push_piece(11.0, 5.0, 2.0, 8.0, 1.0, fg);
        }
        ContextMenuGlyph::Refresh => {
            painter.push_piece(5.0, 5.0, 8.0, 2.0, 1.0, fg);
            painter.push_piece(5.0, 5.0, 2.0, 8.0, 1.0, fg);
            painter.push_piece(5.0, 11.0, 8.0, 2.0, 1.0, fg);
            painter.push_piece(11.0, 9.0, 2.0, 4.0, 1.0, fg);
            painter.push_piece(10.0, 4.0, 4.0, 4.0, 1.0, fg);
        }
        ContextMenuGlyph::Properties => {
            painter.push_piece(8.0, 4.0, 2.0, 2.0, 1.0, fg);
            painter.push_piece(8.0, 8.0, 2.0, 6.0, 1.0, fg);
            painter.push_piece(7.0, 14.0, 4.0, 1.0, 0.0, fg);
        }
        ContextMenuGlyph::Remove => {
            painter.push_piece(4.0, 8.0, 10.0, 2.0, 1.0, fg);
        }
    }
}

fn screen_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}

fn menu_text_color(hovered: bool, theme: ContextMenuPaintTheme) -> TextColor {
    if hovered {
        theme.hover_text
    } else {
        theme.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_theme_follows_shell_theme_mode() {
        let light = ContextMenuPaintTheme::from_shell_theme(ShellTheme::for_dark_mode(false));
        let dark_shell = ShellTheme::for_dark_mode(true);
        let dark = ContextMenuPaintTheme::from_shell_theme(dark_shell);

        assert_eq!(light.surface, [1.000, 1.000, 1.000, 1.0]);
        assert_eq!(dark.surface, dark_shell.field());
        assert_eq!(dark.border, dark_shell.divider());
        assert_eq!(menu_text_color(true, dark), dark_shell.accent_text());
    }
}
