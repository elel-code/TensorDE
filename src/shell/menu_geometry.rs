use crate::windowing::PhysicalSize;
use fika_core::{ViewPoint, ViewRect};

use crate::shell::context_menu::{
    ShellContextMenu, ShellContextSubmenu, context_menu_items, context_submenu_actions,
};
use crate::shell::drop_menu::{ShellDropMenu, drop_menu_items};
use crate::shell::metrics::{
    CONTEXT_MENU_ROW_HEIGHT, CONTEXT_MENU_VERTICAL_PADDING, CONTEXT_MENU_VIEWPORT_MARGIN,
    CONTEXT_MENU_WIDTH,
};

#[cfg(test)]
pub(crate) fn context_menu_rect(menu: &ShellContextMenu, size: PhysicalSize<u32>) -> ViewRect {
    context_menu_rect_scaled(menu, size, 1.0)
}

#[cfg(test)]
pub(crate) fn drop_menu_rect(menu: &ShellDropMenu, size: PhysicalSize<u32>) -> ViewRect {
    drop_menu_rect_scaled(menu, size, 1.0)
}

pub(crate) fn drop_menu_rect_scaled(
    menu: &ShellDropMenu,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_context_menu_metric(CONTEXT_MENU_VIEWPORT_MARGIN, scale_factor);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale_factor);
    let vertical_padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale_factor);
    let menu_width = scaled_context_menu_metric(CONTEXT_MENU_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let menu_height = (vertical_padding * 2.0 + drop_menu_items().len() as f32 * row_height)
        .min((height - margin * 2.0).max(1.0))
        .max(1.0);
    ViewRect {
        x: popup_menu_axis(menu.position.x, menu_width, width, margin),
        y: popup_menu_axis(menu.position.y, menu_height, height, margin),
        width: menu_width,
        height: menu_height,
    }
}

pub(crate) fn drop_menu_row_at_screen_point(
    menu: &ShellDropMenu,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> Option<usize> {
    let rect = drop_menu_rect_scaled(menu, size, scale_factor);
    if !rect.contains(point) {
        return None;
    }
    let padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale_factor);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale_factor);
    let row_y = point.y - rect.y - padding;
    if row_y < 0.0 {
        return None;
    }
    let row = (row_y / row_height).floor() as usize;
    (row < drop_menu_items().len()).then_some(row)
}

pub(crate) fn context_menu_rect_scaled(
    menu: &ShellContextMenu,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_context_menu_metric(CONTEXT_MENU_VIEWPORT_MARGIN, scale_factor);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale_factor);
    let vertical_padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale_factor);
    let menu_width = scaled_context_menu_metric(CONTEXT_MENU_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let menu_height = (vertical_padding * 2.0 + context_menu_items(menu).len() as f32 * row_height)
        .min((height - margin * 2.0).max(1.0))
        .max(1.0);
    ViewRect {
        x: popup_menu_axis(menu.position.x, menu_width, width, margin),
        y: popup_menu_axis(menu.position.y, menu_height, height, margin),
        width: menu_width,
        height: menu_height,
    }
}

pub(crate) fn context_menu_row_at_screen_point(
    menu: &ShellContextMenu,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> Option<usize> {
    let rect = context_menu_rect_scaled(menu, size, scale_factor);
    if !rect.contains(point) {
        return None;
    }
    let padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale_factor);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale_factor);
    let row_y = point.y - rect.y - padding;
    if row_y < 0.0 {
        return None;
    }
    let row = (row_y / row_height).floor() as usize;
    (row < context_menu_items(menu).len()).then_some(row)
}

pub(crate) fn context_menu_submenu_rect(
    menu: &ShellContextMenu,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> Option<ViewRect> {
    let submenu = menu.active_submenu?;
    let parent_row = menu.active_submenu_row.or(menu.hovered_row)?;
    let submenu_len = context_submenu_actions(submenu, menu).len();
    if submenu_len == 0 {
        return None;
    }
    let root = context_menu_rect_scaled(menu, size, scale_factor);
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let margin = scaled_context_menu_metric(CONTEXT_MENU_VIEWPORT_MARGIN, scale_factor);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale_factor);
    let vertical_padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale_factor);
    let submenu_width = scaled_context_menu_metric(CONTEXT_MENU_WIDTH, scale_factor)
        .min((width - margin * 2.0).max(1.0))
        .max(1.0);
    let submenu_height = (vertical_padding * 2.0 + submenu_len as f32 * row_height)
        .min((height - margin * 2.0).max(1.0))
        .max(1.0);
    let preferred_x = root.right() - 1.0;
    let x = if preferred_x + submenu_width <= width - margin {
        preferred_x
    } else {
        (root.x - submenu_width + 1.0).max(margin.min((width - submenu_width).max(0.0)))
    };
    let anchor_y = root.y + vertical_padding + parent_row as f32 * row_height;
    Some(ViewRect {
        x,
        y: popup_menu_axis(anchor_y, submenu_height, height, margin),
        width: submenu_width,
        height: submenu_height,
    })
}

pub(crate) fn context_submenu_row_at_screen_point(
    menu: &ShellContextMenu,
    submenu: ShellContextSubmenu,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> Option<usize> {
    let rect = context_menu_submenu_rect(menu, size, scale_factor)?;
    if !rect.contains(point) {
        return None;
    }
    let padding = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale_factor);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale_factor);
    let row_y = point.y - rect.y - padding;
    if row_y < 0.0 {
        return None;
    }
    let row = (row_y / row_height).floor() as usize;
    (row < context_submenu_actions(submenu, menu).len()).then_some(row)
}

pub(crate) fn scaled_context_menu_metric(value: f32, scale_factor: f32) -> f32 {
    (value * scale_factor.max(1.0)).round().max(1.0)
}

fn popup_menu_axis(anchor: f32, size: f32, viewport_size: f32, margin: f32) -> f32 {
    let min = margin.min((viewport_size - size).max(0.0));
    let max = (viewport_size - size - margin).max(min);
    let forward = anchor.clamp(min, max);
    if anchor + size <= viewport_size - margin {
        return forward;
    }
    let flipped = anchor - size;
    if flipped >= min {
        return flipped.min(max);
    }
    forward
}
