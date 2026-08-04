use crate::windowing::PhysicalSize;
use tensor_files_core::ViewRect;

use crate::ui::file_item_view::style::BREEZE_ITEM_ROUNDNESS;
use crate::ui::icon_roles::NamedIconFallback;
use crate::ui::metrics::{
    OPEN_WITH_CHOOSER_MAX_ROWS, OPEN_WITH_CHOOSER_ROW_HEIGHT, OPEN_WITH_CHOOSER_TITLE_HEIGHT,
    scaled_dialog_metric,
};
use crate::ui::open_with::geometry::{
    open_with_chooser_cancel_button_rect_scaled, open_with_chooser_default_checkbox_rect_scaled,
    open_with_chooser_list_rect_scaled, open_with_chooser_open_button_rect_scaled,
    open_with_chooser_query_rect_scaled, open_with_chooser_query_text_rect_scaled,
    open_with_chooser_rect_scaled, open_with_chooser_scrollbar_rects_scaled,
};
use crate::ui::open_with::{OpenWithTreeRow, ShellOpenWithChooser};
use crate::ui::popup::style::PopupTheme;
use crate::ui::text_input::{cursor_with_preedit, text_with_preedit};
use crate::ui::ui_chrome::push_scrollbar;
use crate::{
    IconDrawLayer, IconFrameBuilder, LabelAlignment, LabelWrap, QuadVertex, RoundedHighlightStyle,
    TextFrameBuilder, path_name_or_display, push_clipped_rect_outline,
    push_clipped_rounded_highlight, push_clipped_rounded_rect, push_rect,
};

#[derive(Clone, Copy)]
pub(crate) struct OpenWithDialogPaintConfig {
    pub(crate) theme: PopupTheme,
    pub(crate) scale: f32,
    pub(crate) caret_visible: bool,
    pub(crate) size: PhysicalSize<u32>,
}

#[derive(Clone, Copy)]
struct OpenWithRoundedBoxStyle {
    fill: [f32; 4],
    border: [f32; 4],
}

pub(crate) fn push_open_with_chooser_dialog(
    chooser: &ShellOpenWithChooser,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    icons: &mut IconFrameBuilder<'_>,
    config: OpenWithDialogPaintConfig,
) {
    push_open_with_chooser_surface(chooser, vertices, text, icons, config);
}

fn push_open_with_chooser_surface(
    chooser: &ShellOpenWithChooser,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    icons: &mut IconFrameBuilder<'_>,
    config: OpenWithDialogPaintConfig,
) {
    let OpenWithDialogPaintConfig {
        theme,
        scale,
        caret_visible,
        size,
    } = config;
    let screen = ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    };
    let rect = open_with_chooser_rect_scaled(chooser, size, scale);
    let title_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_TITLE_HEIGHT, scale);
    let margin = scaled_dialog_metric(16.0, scale);
    let row_height = scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, scale);
    let dialog_radius = scaled_dialog_metric(8.0, scale);
    push_open_with_rounded_box(
        vertices,
        rect,
        screen,
        dialog_radius,
        OpenWithRoundedBoxStyle {
            fill: theme.surface,
            border: theme.border,
        },
        scale,
        size,
    );
    let dialog_inner = open_with_inset_rect(rect, scaled_dialog_metric(1.0, scale)).unwrap_or(rect);
    let dialog_inner_radius = (dialog_radius - scaled_dialog_metric(1.0, scale)).max(1.0);
    let header_rect = ViewRect {
        x: dialog_inner.x,
        y: dialog_inner.y,
        width: dialog_inner.width,
        height: title_height.min(dialog_inner.height),
    };
    push_clipped_rounded_rect(
        vertices,
        header_rect,
        rect,
        dialog_inner_radius,
        theme.header,
        size,
    );
    if header_rect.height > dialog_inner_radius {
        push_rect(
            vertices,
            ViewRect {
                x: header_rect.x,
                y: header_rect.y + dialog_inner_radius,
                width: header_rect.width,
                height: (header_rect.height - dialog_inner_radius).max(1.0),
            },
            theme.header,
            size,
        );
    }
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: dialog_inner.x,
            y: header_rect.bottom() - scaled_dialog_metric(1.0, scale).max(1.0),
            width: dialog_inner.width,
            height: scaled_dialog_metric(1.0, scale).max(1.0),
        },
        rect,
        scaled_dialog_metric(1.0, scale),
        theme.divider,
        size,
    );
    text.push_label(
        &format!("Open With - {}", path_name_or_display(&chooser.path)),
        ViewRect {
            x: rect.x + margin,
            y: rect.y + scaled_dialog_metric(8.0, scale),
            width: (rect.width - margin * 2.0).max(1.0),
            height: scaled_dialog_metric(18.0, scale),
        },
        rect,
        theme.title_text,
    );
    text.push_label(
        chooser.mime_type.as_deref().unwrap_or("unknown MIME"),
        ViewRect {
            x: rect.x + margin,
            y: rect.y + scaled_dialog_metric(25.0, scale),
            width: (rect.width - margin * 2.0).max(1.0),
            height: scaled_dialog_metric(14.0, scale),
        },
        rect,
        theme.muted_text,
    );

    let query = open_with_chooser_query_rect_scaled(rect, scale);
    push_open_with_rounded_box(
        vertices,
        query,
        rect,
        scaled_dialog_metric(7.0, scale),
        OpenWithRoundedBoxStyle {
            fill: theme.input,
            border: theme.divider,
        },
        scale,
        size,
    );
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: query.x + scaled_dialog_metric(10.0, scale),
            y: query.bottom() - scaled_dialog_metric(2.0, scale),
            width: (query.width - scaled_dialog_metric(20.0, scale)).max(1.0),
            height: scaled_dialog_metric(1.5, scale),
        },
        query,
        scaled_dialog_metric(1.0, scale),
        theme.field_focus,
        size,
    );
    let search_icon = ViewRect {
        x: query.x + scaled_dialog_metric(10.0, scale),
        y: query.y + (query.height - scaled_dialog_metric(14.0, scale)) / 2.0,
        width: scaled_dialog_metric(14.0, scale),
        height: scaled_dialog_metric(14.0, scale),
    };
    if !icons.push_named_theme_icon(
        "edit-find",
        NamedIconFallback::Service,
        search_icon,
        query,
        IconDrawLayer::Overlay,
    ) {
        push_open_with_search_icon(vertices, search_icon, query, theme, scale, size);
    }
    let query_text_rect = open_with_chooser_query_text_rect_scaled(rect, scale);
    let display_query = text_with_preedit(
        &chooser.query,
        chooser.query_cursor,
        chooser.query_cursor,
        chooser.preedit.as_ref(),
    );
    let display_cursor = cursor_with_preedit(
        &chooser.query,
        chooser.query_cursor,
        chooser.query_cursor,
        chooser.preedit.as_ref(),
    );
    if display_query.is_empty() {
        text.push_label_aligned_no_wrap(
            "Search applications",
            query_text_rect,
            query,
            theme.muted_text,
            LabelAlignment::Start,
        );
    } else {
        text.push_label_aligned_no_wrap(
            &display_query,
            query_text_rect,
            query,
            theme.body_text,
            LabelAlignment::Start,
        );
    }
    if caret_visible {
        let cursor_x = text.measure_label_cursor_x(
            &display_query,
            query_text_rect,
            display_cursor,
            LabelAlignment::Start,
            LabelWrap::None,
        );
        let caret_width = scaled_dialog_metric(1.0, scale).max(1.0);
        let caret_height = scaled_dialog_metric(17.0, scale)
            .min(query.height - scaled_dialog_metric(10.0, scale))
            .max(1.0);
        let caret_x = (query_text_rect.x + cursor_x).clamp(
            query_text_rect.x,
            (query_text_rect.right() - caret_width).max(query_text_rect.x),
        );
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: caret_x,
                y: query.y + (query.height - caret_height) / 2.0,
                width: caret_width,
                height: caret_height,
            },
            query,
            caret_width / 2.0,
            theme.field_focus,
            size,
        );
    }

    let list = open_with_chooser_list_rect_scaled(rect, chooser, scale);
    let scrollbar = open_with_chooser_scrollbar_rects_scaled(list, chooser, scale);
    let row_content_right = scrollbar
        .as_ref()
        .map(|(track, _)| track.x - scaled_dialog_metric(8.0, scale))
        .unwrap_or_else(|| list.right() - scaled_dialog_metric(10.0, scale));
    push_open_with_rounded_box(
        vertices,
        list,
        rect,
        scaled_dialog_metric(8.0, scale),
        OpenWithRoundedBoxStyle {
            fill: theme.input,
            border: theme.divider,
        },
        scale,
        size,
    );
    let visible = chooser.visible_tree_rows();
    if visible.is_empty() {
        text.push_label(
            "No matching applications",
            ViewRect {
                x: list.x + scaled_dialog_metric(12.0, scale),
                y: list.y + scaled_dialog_metric(10.0, scale),
                width: (list.width - scaled_dialog_metric(24.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            list,
            theme.muted_text,
        );
    } else {
        for (visible_row, tree_row) in visible.iter().copied().enumerate() {
            let row = chooser.scroll_row + visible_row;
            let row_rect = ViewRect {
                x: list.x + scaled_dialog_metric(4.0, scale),
                y: list.y + visible_row as f32 * row_height + scaled_dialog_metric(3.0, scale),
                width: (row_content_right - list.x - scaled_dialog_metric(8.0, scale)).max(1.0),
                height: (row_height - scaled_dialog_metric(6.0, scale)).max(1.0),
            };
            let row_clip = ViewRect {
                x: list.x,
                y: list.y + visible_row as f32 * row_height,
                width: list.width,
                height: row_height,
            };
            let selected = row == chooser.selected_index;
            let row_radius = scaled_dialog_metric(BREEZE_ITEM_ROUNDNESS, scale);
            let row_background = theme.list_row_background(visible_row % 2 == 1);
            push_clipped_rounded_rect(vertices, row_rect, list, row_radius, row_background, size);
            if selected {
                push_clipped_rounded_highlight(
                    vertices,
                    row_rect,
                    list,
                    row_radius,
                    RoundedHighlightStyle {
                        fill: theme.selection_fill,
                        border: theme.field_focus,
                        border_width: scaled_dialog_metric(1.25, scale),
                    },
                    size,
                );
            }
            match tree_row {
                OpenWithTreeRow::Category { category, expanded } => {
                    text.push_label_aligned(
                        if expanded { "v" } else { ">" },
                        ViewRect {
                            x: row_rect.x + scaled_dialog_metric(5.0, scale),
                            y: row_rect.y + scaled_dialog_metric(10.0, scale),
                            width: scaled_dialog_metric(14.0, scale),
                            height: scaled_dialog_metric(18.0, scale),
                        },
                        row_clip,
                        if selected {
                            theme.title_text
                        } else {
                            theme.body_text
                        },
                        LabelAlignment::Center,
                    );
                    let icon_rect = ViewRect {
                        x: row_rect.x + scaled_dialog_metric(28.0, scale),
                        y: row_rect.y + (row_rect.height - scaled_dialog_metric(26.0, scale)) / 2.0,
                        width: scaled_dialog_metric(26.0, scale),
                        height: scaled_dialog_metric(26.0, scale),
                    };
                    icons.push_named_theme_icon(
                        category.icon,
                        NamedIconFallback::Application,
                        icon_rect,
                        list,
                        IconDrawLayer::Overlay,
                    );
                    text.push_label(
                        category.label,
                        ViewRect {
                            x: icon_rect.right() + scaled_dialog_metric(12.0, scale),
                            y: row_rect.y + scaled_dialog_metric(12.0, scale),
                            width: (row_content_right
                                - icon_rect.right()
                                - scaled_dialog_metric(18.0, scale))
                            .max(1.0),
                            height: scaled_dialog_metric(18.0, scale),
                        },
                        row_clip,
                        if selected {
                            theme.title_text
                        } else {
                            theme.body_text
                        },
                    );
                }
                OpenWithTreeRow::Application { app_index } => {
                    let Some(application) = chooser.applications.get(app_index) else {
                        continue;
                    };
                    let icon_rect = ViewRect {
                        x: row_rect.x + scaled_dialog_metric(58.0, scale),
                        y: row_rect.y + (row_rect.height - scaled_dialog_metric(28.0, scale)) / 2.0,
                        width: scaled_dialog_metric(28.0, scale),
                        height: scaled_dialog_metric(28.0, scale),
                    };
                    let icon_pushed = application
                        .icon
                        .as_deref()
                        .filter(|icon| !icon.is_empty())
                        .is_some_and(|icon| {
                            icons.push_named_theme_icon(
                                icon,
                                NamedIconFallback::Application,
                                icon_rect,
                                list,
                                IconDrawLayer::Overlay,
                            )
                        });
                    if !icon_pushed {
                        icons.push_named_theme_icon(
                            "application-x-executable",
                            NamedIconFallback::Application,
                            icon_rect,
                            list,
                            IconDrawLayer::Overlay,
                        );
                    }
                    let name = if application.is_default {
                        format!("{} (default)", application.name)
                    } else {
                        application.name.clone()
                    };
                    text.push_label(
                        &name,
                        ViewRect {
                            x: icon_rect.right() + scaled_dialog_metric(12.0, scale),
                            y: row_rect.y + scaled_dialog_metric(7.0, scale),
                            width: (row_content_right
                                - icon_rect.right()
                                - scaled_dialog_metric(18.0, scale))
                            .max(1.0),
                            height: scaled_dialog_metric(18.0, scale),
                        },
                        row_clip,
                        if selected {
                            theme.title_text
                        } else {
                            theme.body_text
                        },
                    );
                    text.push_label(
                        &application.id,
                        ViewRect {
                            x: icon_rect.right() + scaled_dialog_metric(12.0, scale),
                            y: row_rect.y + scaled_dialog_metric(25.0, scale),
                            width: (row_content_right
                                - icon_rect.right()
                                - scaled_dialog_metric(18.0, scale))
                            .max(1.0),
                            height: scaled_dialog_metric(14.0, scale),
                        },
                        row_clip,
                        if selected {
                            theme.soft_text
                        } else {
                            theme.muted_text
                        },
                    );
                }
            }
        }
    }

    if let Some((track, thumb)) = scrollbar {
        push_scrollbar(vertices, track, thumb, list, theme.scrollbar, size);
    }

    let default_row = open_with_chooser_default_checkbox_rect_scaled(rect, chooser, scale);
    let default_enabled = chooser.mime_type.is_some();
    let checkbox_size = scaled_dialog_metric(16.0, scale);
    let checkbox = ViewRect {
        x: default_row.x,
        y: default_row.y + (default_row.height - checkbox_size) / 2.0,
        width: checkbox_size,
        height: checkbox_size,
    };
    push_open_with_rounded_box(
        vertices,
        checkbox,
        rect,
        scaled_dialog_metric(4.0, scale),
        OpenWithRoundedBoxStyle {
            fill: if chooser.set_as_default {
                theme.button_primary
            } else {
                theme.input
            },
            border: if default_enabled {
                theme.border
            } else {
                theme.divider
            },
        },
        scale,
        size,
    );
    if chooser.set_as_default {
        push_open_with_checkbox_check(vertices, checkbox, rect, scale, size);
    }
    let default_label = chooser
        .mime_type
        .as_deref()
        .map(|mime| format!("Set as default application for {mime}"))
        .unwrap_or_else(|| "Set as default application".to_string());
    text.push_label(
        &default_label,
        ViewRect {
            x: checkbox.right() + scaled_dialog_metric(8.0, scale),
            y: default_row.y + scaled_dialog_metric(3.0, scale),
            width: (default_row.right() - checkbox.right() - scaled_dialog_metric(8.0, scale))
                .max(1.0),
            height: scaled_dialog_metric(18.0, scale),
        },
        rect,
        if default_enabled {
            theme.body_text
        } else {
            theme.muted_text
        },
    );

    if chooser.tree_row_count() > OPEN_WITH_CHOOSER_MAX_ROWS {
        let end = (chooser.scroll_row + visible.len()).min(chooser.tree_row_count());
        text.push_label(
            &format!(
                "{}-{} of {}",
                chooser.scroll_row + 1,
                end,
                chooser.tree_row_count()
            ),
            ViewRect {
                x: rect.x + margin,
                y: default_row.bottom() + scaled_dialog_metric(3.0, scale),
                width: scaled_dialog_metric(120.0, scale),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            theme.muted_text,
        );
    }

    if let Some(error) = chooser.error.as_ref() {
        text.push_label(
            error,
            ViewRect {
                x: rect.x + margin,
                y: default_row.bottom() + scaled_dialog_metric(3.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            theme.error_text,
        );
    }

    let cancel = open_with_chooser_cancel_button_rect_scaled(rect, scale);
    let open = open_with_chooser_open_button_rect_scaled(rect, scale);
    let open_enabled = chooser.selected_application().is_some();
    for (label, button, active, enabled) in [
        ("Cancel", cancel, false, true),
        ("Open", open, true, open_enabled),
    ] {
        push_open_with_rounded_box(
            vertices,
            button,
            rect,
            scaled_dialog_metric(5.0, scale),
            OpenWithRoundedBoxStyle {
                fill: if active && enabled {
                    theme.button_primary
                } else {
                    theme.button_secondary
                },
                border: theme.border,
            },
            scale,
            size,
        );
        text.push_label_aligned(
            label,
            ViewRect {
                x: button.x + scaled_dialog_metric(10.0, scale),
                y: button.y + scaled_dialog_metric(4.0, scale),
                width: (button.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            if active && enabled {
                theme.inverse_text
            } else if enabled {
                theme.body_text
            } else {
                theme.muted_text
            },
            LabelAlignment::Center,
        );
    }
}

fn push_open_with_rounded_box(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    radius: f32,
    style: OpenWithRoundedBoxStyle,
    scale: f32,
    size: PhysicalSize<u32>,
) {
    let OpenWithRoundedBoxStyle { fill, border } = style;
    let border_width = scaled_dialog_metric(1.0, scale);
    push_clipped_rounded_rect(vertices, rect, clip, radius, border, size);
    if let Some(inner) = open_with_inset_rect(rect, border_width) {
        push_clipped_rounded_rect(
            vertices,
            inner,
            clip,
            (radius - border_width).max(1.0),
            fill,
            size,
        );
    }
}

fn open_with_inset_rect(rect: ViewRect, inset: f32) -> Option<ViewRect> {
    let width = rect.width - inset * 2.0;
    let height = rect.height - inset * 2.0;
    (width > 0.0 && height > 0.0).then_some(ViewRect {
        x: rect.x + inset,
        y: rect.y + inset,
        width,
        height,
    })
}

fn push_open_with_search_icon(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    theme: PopupTheme,
    scale: f32,
    size: PhysicalSize<u32>,
) {
    let stroke = scaled_dialog_metric(1.5, scale).max(1.0);
    let lens = ViewRect {
        x: rect.x,
        y: rect.y,
        width: rect.width * 0.72,
        height: rect.height * 0.72,
    };
    push_clipped_rect_outline(vertices, lens, clip, stroke, theme.marker_neutral, size);
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: rect.x + rect.width * 0.62,
            y: rect.y + rect.height * 0.68,
            width: rect.width * 0.34,
            height: stroke,
        },
        clip,
        stroke / 2.0,
        theme.marker_neutral,
        size,
    );
}

fn push_open_with_checkbox_check(
    vertices: &mut Vec<QuadVertex>,
    checkbox: ViewRect,
    clip: ViewRect,
    scale: f32,
    size: PhysicalSize<u32>,
) {
    let stroke = scaled_dialog_metric(2.0, scale).max(1.0);
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: checkbox.x + checkbox.width * 0.25,
            y: checkbox.y + checkbox.height * 0.55,
            width: checkbox.width * 0.22,
            height: stroke,
        },
        clip,
        stroke / 2.0,
        [1.0, 1.0, 1.0, 1.0],
        size,
    );
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: checkbox.x + checkbox.width * 0.42,
            y: checkbox.y + checkbox.height * 0.30,
            width: stroke,
            height: checkbox.height * 0.44,
        },
        clip,
        stroke / 2.0,
        [1.0, 1.0, 1.0, 1.0],
        size,
    );
}
