use crate::windowing::PhysicalSize;
use tensor_files_core::ViewRect;

use crate::ui::create_rename::{CreateEntryKind, ShellCreateDialog, ShellRenameDialog};
use crate::ui::metrics::{
    CREATE_DIALOG_BUTTON_GAP, CREATE_DIALOG_BUTTON_HEIGHT, CREATE_DIALOG_BUTTON_WIDTH,
    CREATE_DIALOG_HEIGHT, CREATE_DIALOG_TITLE_HEIGHT, CREATE_DIALOG_WIDTH, RENAME_DIALOG_HEIGHT,
    RENAME_DIALOG_TITLE_HEIGHT, RENAME_DIALOG_WIDTH, scaled_dialog_metric,
};

#[cfg(test)]
pub(crate) fn create_dialog_rect(dialog: &ShellCreateDialog, size: PhysicalSize<u32>) -> ViewRect {
    create_dialog_rect_scaled(dialog, size, 1.0)
}

pub(crate) fn create_dialog_rect_scaled(
    _dialog: &ShellCreateDialog,
    size: PhysicalSize<u32>,
    _scale_factor: f32,
) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}

pub(crate) fn create_dialog_window_size_scaled(scale_factor: f32) -> PhysicalSize<u32> {
    PhysicalSize::new(
        scaled_dialog_metric(CREATE_DIALOG_WIDTH, scale_factor)
            .ceil()
            .max(1.0) as u32,
        scaled_dialog_metric(CREATE_DIALOG_HEIGHT, scale_factor)
            .ceil()
            .max(1.0) as u32,
    )
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn create_kind_button_rect(dialog_rect: ViewRect, kind: CreateEntryKind) -> ViewRect {
    create_kind_button_rect_scaled(dialog_rect, kind, 1.0)
}

pub(crate) fn create_kind_button_rect_scaled(
    dialog_rect: ViewRect,
    kind: CreateEntryKind,
    scale_factor: f32,
) -> ViewRect {
    let x = match kind {
        CreateEntryKind::Folder => dialog_rect.x + scaled_dialog_metric(16.0, scale_factor),
        CreateEntryKind::File => {
            dialog_rect.x
                + scaled_dialog_metric(16.0, scale_factor)
                + scaled_dialog_metric(96.0, scale_factor)
        }
    };
    ViewRect {
        x,
        y: dialog_rect.y
            + scaled_dialog_metric(CREATE_DIALOG_TITLE_HEIGHT, scale_factor)
            + scaled_dialog_metric(14.0, scale_factor),
        width: scaled_dialog_metric(88.0, scale_factor),
        height: scaled_dialog_metric(CREATE_DIALOG_BUTTON_HEIGHT, scale_factor),
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn create_dialog_input_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_input_rect_scaled(dialog_rect, 1.0)
}

pub(crate) fn create_dialog_input_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let margin = scaled_dialog_metric(16.0, scale_factor);
    ViewRect {
        x: dialog_rect.x + margin,
        y: dialog_rect.y
            + scaled_dialog_metric(CREATE_DIALOG_TITLE_HEIGHT, scale_factor)
            + scaled_dialog_metric(60.0, scale_factor),
        width: (dialog_rect.width - margin * 2.0).max(1.0),
        height: scaled_dialog_metric(30.0, scale_factor),
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn create_dialog_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_cancel_button_rect_scaled(dialog_rect, 1.0)
}

pub(crate) fn create_dialog_cancel_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let right = dialog_rect.right() - scaled_dialog_metric(16.0, scale_factor);
    let button_width = scaled_dialog_metric(CREATE_DIALOG_BUTTON_WIDTH, scale_factor);
    let button_height = scaled_dialog_metric(CREATE_DIALOG_BUTTON_HEIGHT, scale_factor);
    ViewRect {
        x: right
            - button_width * 2.0
            - scaled_dialog_metric(CREATE_DIALOG_BUTTON_GAP, scale_factor),
        y: dialog_rect.bottom() - scaled_dialog_metric(16.0, scale_factor) - button_height,
        width: button_width,
        height: button_height,
    }
}

#[cfg(test)]
pub(crate) fn create_dialog_commit_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_commit_button_rect_scaled(dialog_rect, 1.0)
}

pub(crate) fn create_dialog_commit_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let right = dialog_rect.right() - scaled_dialog_metric(16.0, scale_factor);
    let button_width = scaled_dialog_metric(CREATE_DIALOG_BUTTON_WIDTH, scale_factor);
    let button_height = scaled_dialog_metric(CREATE_DIALOG_BUTTON_HEIGHT, scale_factor);
    ViewRect {
        x: right - button_width,
        y: dialog_rect.bottom() - scaled_dialog_metric(16.0, scale_factor) - button_height,
        width: button_width,
        height: button_height,
    }
}

#[cfg(test)]
pub(crate) fn rename_dialog_rect(dialog: &ShellRenameDialog, size: PhysicalSize<u32>) -> ViewRect {
    rename_dialog_rect_scaled(dialog, size, 1.0)
}

pub(crate) fn rename_dialog_rect_scaled(
    _dialog: &ShellRenameDialog,
    size: PhysicalSize<u32>,
    _scale_factor: f32,
) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}

pub(crate) fn rename_dialog_window_size_scaled(scale_factor: f32) -> PhysicalSize<u32> {
    PhysicalSize::new(
        scaled_dialog_metric(RENAME_DIALOG_WIDTH, scale_factor)
            .ceil()
            .max(1.0) as u32,
        scaled_dialog_metric(RENAME_DIALOG_HEIGHT, scale_factor)
            .ceil()
            .max(1.0) as u32,
    )
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn rename_dialog_input_rect(dialog_rect: ViewRect) -> ViewRect {
    rename_dialog_input_rect_scaled(dialog_rect, 1.0)
}

pub(crate) fn rename_dialog_input_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    let margin = scaled_dialog_metric(16.0, scale_factor);
    ViewRect {
        x: dialog_rect.x + margin,
        y: dialog_rect.y
            + scaled_dialog_metric(RENAME_DIALOG_TITLE_HEIGHT, scale_factor)
            + scaled_dialog_metric(18.0, scale_factor),
        width: (dialog_rect.width - margin * 2.0).max(1.0),
        height: scaled_dialog_metric(30.0, scale_factor),
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn rename_dialog_cancel_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_cancel_button_rect(dialog_rect)
}

pub(crate) fn rename_dialog_cancel_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    create_dialog_cancel_button_rect_scaled(dialog_rect, scale_factor)
}

#[cfg(test)]
pub(crate) fn rename_dialog_commit_button_rect(dialog_rect: ViewRect) -> ViewRect {
    create_dialog_commit_button_rect(dialog_rect)
}

pub(crate) fn rename_dialog_commit_button_rect_scaled(
    dialog_rect: ViewRect,
    scale_factor: f32,
) -> ViewRect {
    create_dialog_commit_button_rect_scaled(dialog_rect, scale_factor)
}
