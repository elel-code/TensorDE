use crate::windowing::PhysicalSize;
use tensor_files_core::ViewRect;

use crate::ui::create_rename::geometry::{
    create_dialog_cancel_button_rect_scaled, create_dialog_commit_button_rect_scaled,
    create_dialog_input_rect_scaled, create_dialog_rect_scaled, create_kind_button_rect_scaled,
    rename_dialog_cancel_button_rect_scaled, rename_dialog_commit_button_rect_scaled,
    rename_dialog_input_rect_scaled, rename_dialog_rect_scaled,
};
use crate::ui::create_rename::{CreateEntryKind, ShellCreateDialog, ShellRenameDialog};
use crate::ui::metrics::{
    CREATE_DIALOG_TITLE_HEIGHT, RENAME_DIALOG_TITLE_HEIGHT, scaled_dialog_metric,
};
use crate::ui::popup::style::PopupTheme;
use crate::ui::text_input::{cursor_with_preedit, text_with_preedit};
use crate::{
    LabelAlignment, LabelWrap, QuadVertex, TextFrameBuilder, push_clipped_rect_outline,
    push_clipped_rounded_rect, push_rect,
};

#[derive(Clone, Copy)]
struct DialogPaintContext {
    theme: PopupTheme,
    clip: ViewRect,
    scale: f32,
    size: PhysicalSize<u32>,
}

struct DialogButtonState {
    active: bool,
    privileged: bool,
}

struct DialogInputText<'a> {
    value: &'a str,
    cursor: usize,
    rect: ViewRect,
}

pub(crate) fn push_create_dialog(
    dialog: &ShellCreateDialog,
    theme: PopupTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    push_create_dialog_surface(dialog, theme, scale, vertices, text, size);
}

fn push_create_dialog_surface(
    dialog: &ShellCreateDialog,
    theme: PopupTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let screen = screen_rect(size);
    let rect = create_dialog_rect_scaled(dialog, size, scale);
    let title_height = scaled_dialog_metric(CREATE_DIALOG_TITLE_HEIGHT, scale);
    let margin = scaled_dialog_metric(16.0, scale);
    push_clipped_rounded_rect(
        vertices,
        rect,
        screen,
        scaled_dialog_metric(8.0, scale),
        theme.surface,
        size,
    );
    push_clipped_rect_outline(vertices, rect, screen, 1.0, theme.border, size);
    push_rect(
        vertices,
        ViewRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: title_height,
        },
        theme.header,
        size,
    );
    push_rect(
        vertices,
        ViewRect {
            x: rect.x,
            y: rect.y + title_height - scaled_dialog_metric(1.0, scale).max(1.0),
            width: rect.width,
            height: scaled_dialog_metric(1.0, scale).max(1.0),
        },
        theme.divider,
        size,
    );
    let title = if dialog.privileged {
        "Create New as Administrator"
    } else {
        "Create New"
    };
    text.push_label(
        title,
        ViewRect {
            x: rect.x + margin,
            y: rect.y + scaled_dialog_metric(12.0, scale),
            width: (rect.width - margin * 2.0).max(1.0),
            height: scaled_dialog_metric(18.0, scale),
        },
        rect,
        theme.title_text,
    );

    for kind in [CreateEntryKind::Folder, CreateEntryKind::File] {
        let button = create_kind_button_rect_scaled(rect, kind, scale);
        let active = dialog.kind == kind;
        push_clipped_rounded_rect(
            vertices,
            button,
            rect,
            scaled_dialog_metric(5.0, scale),
            if active {
                theme.button_primary_soft
            } else {
                theme.button_secondary
            },
            size,
        );
        push_clipped_rect_outline(vertices, button, rect, 1.0, theme.border, size);
        text.push_label_aligned(
            kind.label(),
            ViewRect {
                x: button.x + scaled_dialog_metric(10.0, scale),
                y: button.y + scaled_dialog_metric(4.0, scale),
                width: (button.width - scaled_dialog_metric(20.0, scale)).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            if active {
                theme.inverse_text
            } else {
                theme.body_text
            },
            LabelAlignment::Center,
        );
    }

    let input = create_dialog_input_rect_scaled(rect, scale);
    push_clipped_rounded_rect(
        vertices,
        input,
        rect,
        scaled_dialog_metric(5.0, scale),
        theme.input,
        size,
    );
    push_clipped_rect_outline(vertices, input, rect, 1.0, theme.field_focus, size);
    let anchor = if dialog.replace_on_insert {
        0
    } else {
        dialog.name.len()
    };
    let display_name = text_with_preedit(
        &dialog.name,
        dialog.name.len(),
        anchor,
        dialog.preedit.as_ref(),
    );
    let display_cursor = cursor_with_preedit(
        &dialog.name,
        dialog.name.len(),
        anchor,
        dialog.preedit.as_ref(),
    );
    let paint = DialogPaintContext {
        theme,
        clip: rect,
        scale,
        size,
    };
    push_dialog_input_text(
        vertices,
        text,
        DialogInputText {
            value: &display_name,
            cursor: display_cursor,
            rect: input,
        },
        &paint,
    );

    if let Some(error) = dialog.error.as_ref() {
        text.push_label(
            error,
            ViewRect {
                x: rect.x + margin,
                y: input.bottom() + scaled_dialog_metric(8.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            theme.error_text,
        );
    }

    let cancel = create_dialog_cancel_button_rect_scaled(rect, scale);
    let commit = create_dialog_commit_button_rect_scaled(rect, scale);
    for (label, button, active) in [("Cancel", cancel, false), ("Create", commit, true)] {
        push_dialog_button(
            vertices,
            text,
            label,
            button,
            DialogButtonState {
                active,
                privileged: dialog.privileged,
            },
            &paint,
        );
    }
}

pub(crate) fn push_rename_dialog(
    dialog: &ShellRenameDialog,
    theme: PopupTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    push_rename_dialog_surface(dialog, theme, scale, vertices, text, size);
}

fn push_rename_dialog_surface(
    dialog: &ShellRenameDialog,
    theme: PopupTheme,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let screen = screen_rect(size);
    let rect = rename_dialog_rect_scaled(dialog, size, scale);
    let title_height = scaled_dialog_metric(RENAME_DIALOG_TITLE_HEIGHT, scale);
    let margin = scaled_dialog_metric(16.0, scale);
    push_clipped_rounded_rect(
        vertices,
        rect,
        screen,
        scaled_dialog_metric(8.0, scale),
        theme.surface,
        size,
    );
    push_clipped_rect_outline(vertices, rect, screen, 1.0, theme.border, size);
    push_rect(
        vertices,
        ViewRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: title_height,
        },
        theme.header,
        size,
    );
    push_rect(
        vertices,
        ViewRect {
            x: rect.x,
            y: rect.y + title_height - scaled_dialog_metric(1.0, scale).max(1.0),
            width: rect.width,
            height: scaled_dialog_metric(1.0, scale).max(1.0),
        },
        theme.divider,
        size,
    );
    let title = match (dialog.is_dir, dialog.privileged) {
        (true, true) => "Rename Folder as Administrator",
        (false, true) => "Rename File as Administrator",
        (true, false) => "Rename Folder",
        (false, false) => "Rename File",
    };
    text.push_label(
        title,
        ViewRect {
            x: rect.x + margin,
            y: rect.y + scaled_dialog_metric(12.0, scale),
            width: (rect.width - margin * 2.0).max(1.0),
            height: scaled_dialog_metric(18.0, scale),
        },
        rect,
        theme.title_text,
    );

    let input = rename_dialog_input_rect_scaled(rect, scale);
    push_clipped_rounded_rect(
        vertices,
        input,
        rect,
        scaled_dialog_metric(5.0, scale),
        theme.input,
        size,
    );
    push_clipped_rect_outline(vertices, input, rect, 1.0, theme.field_focus, size);
    let anchor = if dialog.replace_on_insert {
        0
    } else {
        dialog.name.len()
    };
    let display_name = text_with_preedit(
        &dialog.name,
        dialog.name.len(),
        anchor,
        dialog.preedit.as_ref(),
    );
    let display_cursor = cursor_with_preedit(
        &dialog.name,
        dialog.name.len(),
        anchor,
        dialog.preedit.as_ref(),
    );
    let paint = DialogPaintContext {
        theme,
        clip: rect,
        scale,
        size,
    };
    push_dialog_input_text(
        vertices,
        text,
        DialogInputText {
            value: &display_name,
            cursor: display_cursor,
            rect: input,
        },
        &paint,
    );

    if let Some(error) = dialog.error.as_ref() {
        text.push_label(
            error,
            ViewRect {
                x: rect.x + margin,
                y: input.bottom() + scaled_dialog_metric(8.0, scale),
                width: (rect.width - margin * 2.0).max(1.0),
                height: scaled_dialog_metric(18.0, scale),
            },
            rect,
            theme.error_text,
        );
    }

    let cancel = rename_dialog_cancel_button_rect_scaled(rect, scale);
    let commit = rename_dialog_commit_button_rect_scaled(rect, scale);
    for (label, button, active) in [("Cancel", cancel, false), ("Rename", commit, true)] {
        push_dialog_button(
            vertices,
            text,
            label,
            button,
            DialogButtonState {
                active,
                privileged: dialog.privileged,
            },
            &paint,
        );
    }
}

fn push_dialog_input_text(
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    input: DialogInputText<'_>,
    paint: &DialogPaintContext,
) {
    let DialogInputText {
        value,
        cursor,
        rect: input,
    } = input;
    let DialogPaintContext {
        theme, scale, size, ..
    } = *paint;
    let text_rect = ViewRect {
        x: input.x + scaled_dialog_metric(10.0, scale),
        y: input.y + (input.height - scaled_dialog_metric(18.0, scale)) / 2.0,
        width: (input.width - scaled_dialog_metric(20.0, scale)).max(1.0),
        height: scaled_dialog_metric(18.0, scale),
    };
    text.push_label_aligned_no_wrap(
        value,
        text_rect,
        input,
        theme.body_text,
        LabelAlignment::Start,
    );
    let cursor_x = text.measure_label_cursor_x(
        value,
        text_rect,
        cursor,
        LabelAlignment::Start,
        LabelWrap::None,
    );
    let caret_width = scaled_dialog_metric(1.0, scale).max(1.0);
    let caret_height = scaled_dialog_metric(17.0, scale)
        .min(input.height - scaled_dialog_metric(10.0, scale))
        .max(1.0);
    let caret_x = (text_rect.x + cursor_x).clamp(
        text_rect.x,
        (text_rect.right() - caret_width).max(text_rect.x),
    );
    push_clipped_rounded_rect(
        vertices,
        ViewRect {
            x: caret_x,
            y: input.y + (input.height - caret_height) / 2.0,
            width: caret_width,
            height: caret_height,
        },
        input,
        caret_width / 2.0,
        theme.field_focus,
        size,
    );
}

fn push_dialog_button(
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    label: &str,
    button: ViewRect,
    state: DialogButtonState,
    paint: &DialogPaintContext,
) {
    push_clipped_rounded_rect(
        vertices,
        button,
        paint.clip,
        scaled_dialog_metric(5.0, paint.scale),
        if state.active && state.privileged {
            paint.theme.button_warning
        } else if state.active {
            paint.theme.button_primary
        } else {
            paint.theme.button_secondary
        },
        paint.size,
    );
    push_clipped_rect_outline(
        vertices,
        button,
        paint.clip,
        1.0,
        paint.theme.border,
        paint.size,
    );
    text.push_label_aligned(
        label,
        ViewRect {
            x: button.x + scaled_dialog_metric(10.0, paint.scale),
            y: button.y + scaled_dialog_metric(4.0, paint.scale),
            width: (button.width - scaled_dialog_metric(20.0, paint.scale)).max(1.0),
            height: scaled_dialog_metric(18.0, paint.scale),
        },
        paint.clip,
        if state.active {
            paint.theme.inverse_text
        } else {
            paint.theme.body_text
        },
        LabelAlignment::Center,
    );
}

fn screen_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}
