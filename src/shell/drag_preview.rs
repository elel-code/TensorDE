//! Drag-preview *layout* helpers used when building the Wayland DnD icon.
//!
//! The compositor owns the on-screen drag surface via `wl_data_device` /
//! `DragIcon`. This module only measures geometry (hotspot, icon/label rects)
//! from the live item layout — it does not paint an in-window overlay.

use crate::windowing::PhysicalSize;
use fika_core::ViewPoint;

use crate::shell::drag_preview_layout::{
    SingleDragPreviewLayout, pane_single_drag_preview_layout, place_single_drag_preview_layout,
};
use crate::shell::metrics::{PLACES_ICON_SIZE, PLACES_ROW_HEIGHT, PLACES_SIDEBAR_WIDTH};
use crate::{ShellInternalDragSource, ShellScene, TEXT_FONT_SIZE, TEXT_LINE_HEIGHT};

impl ShellScene {
    fn drag_preview_text_width(&self, label: &str) -> f32 {
        let line_height = self.text_line_height();
        let font_size = (TEXT_FONT_SIZE * line_height / TEXT_LINE_HEIGHT).max(1.0);
        self.text_hit_tests
            .borrow_mut()
            .no_wrap_width(label, font_size, line_height)
    }
}

fn single_layout_for_source(
    scene: &ShellScene,
    source: &ShellInternalDragSource,
    label: &str,
    press_point: ViewPoint,
    size: PhysicalSize<u32>,
) -> Option<SingleDragPreviewLayout> {
    let natural_text_width = scene.drag_preview_text_width(label);
    match source {
        ShellInternalDragSource::PaneItem { pane, index, .. } => {
            let view = scene.pane_view(*pane)?;
            let item_layout = scene.pane_projection(*pane, size).and_then(|projection| {
                let layout_index = projection
                    .view
                    .filtered_indexes
                    .iter()
                    .position(|entry_index| entry_index == index)?;
                projection
                    .visible_items
                    .iter()
                    .find(|item| item.layout.model_index == layout_index)
                    .map(|item| item.layout)
            });
            let icon_size = scene.drag_preview_icon_size_for_pane_item(view, item_layout);
            Some(pane_single_drag_preview_layout(
                view.view_mode,
                item_layout,
                icon_size,
                natural_text_width,
                scene.text_line_height(),
                scene.ui_scale(),
                Some(press_point),
            ))
        }
        ShellInternalDragSource::Place { index } => {
            scene.places.get(*index)?;
            let row = scene
                .place_row_rects(size)
                .into_iter()
                .find(|(row_index, _)| row_index == index)
                .map(|(_, rect)| rect);
            let row_width = row
                .map(|rect| rect.width)
                .unwrap_or_else(|| scene.scale_metric(PLACES_SIDEBAR_WIDTH - 16.0));
            let row_height = scene.scale_metric(PLACES_ROW_HEIGHT);
            // Qt's QListView drag path keeps the pointer's press point in the
            // rendered row as the hotspot. This avoids a visible jump when a
            // place is grabbed away from the row's top-left corner.
            let hotspot = row
                .map(|rect| ViewPoint {
                    x: press_point.x - rect.x,
                    y: press_point.y - rect.y,
                })
                .unwrap_or(ViewPoint {
                    x: row_width / 2.0,
                    y: row_height / 2.0,
                });
            Some(place_single_drag_preview_layout(
                row_width,
                row_height,
                scene.scale_metric(PLACES_ICON_SIZE),
                scene.text_line_height(),
                hotspot,
                scene.ui_scale(),
            ))
        }
    }
}

pub(crate) fn drag_preview_layout_for_source(
    scene: &ShellScene,
    source: &ShellInternalDragSource,
    label: &str,
    press_point: ViewPoint,
    size: PhysicalSize<u32>,
) -> Option<SingleDragPreviewLayout> {
    single_layout_for_source(scene, source, label, press_point, size)
}
