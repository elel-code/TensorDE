use crate::windowing::PhysicalSize;
use fika_core::ViewRect;

use crate::shell::context_menu::{
    ShellContextMenu, ShellContextSubmenu, context_menu_items, context_submenu_actions,
};
use crate::shell::drop_menu::{ShellDropMenu, ShellDropTarget, drop_menu_items};
use crate::shell::menu_geometry::{
    context_menu_rect_scaled, context_menu_submenu_rect, drop_menu_rect_scaled,
    scaled_context_menu_metric,
};
use crate::shell::metrics::*;
use crate::shell::pane::ShellPaneProjection;
use crate::shell::render::dirty_key::{ShellRenderDirtyKey, ShellRenderDirtyKeyContext};
use crate::{
    RubberBand, ShellPaneItemTarget, ShellScene, intersect_rect, pane_content_rect_to_screen,
};

#[derive(Clone, Debug)]
pub(crate) struct ShellRenderDamageSnapshot {
    pub(crate) dirty_key: ShellRenderDirtyKey,
    pub(super) hoverless_dirty_key: ShellRenderDirtyKey,
    pub(super) folder_previewless_dirty_key: ShellRenderDirtyKey,
    pub(super) hoverless_folder_previewless_dirty_key: ShellRenderDirtyKey,
    pub(super) hovered_item: Option<ShellPaneItemTarget>,
    pub(super) hovered_place: Option<usize>,
    pub(super) dnd_hover_target: Option<ShellDropTarget>,
    item_rects: Vec<(ShellPaneItemTarget, ViewRect)>,
    place_rects: Vec<(usize, ViewRect)>,
    place_gap_rects: Vec<(usize, ViewRect)>,
    pub(crate) places_sidebar_rect: Option<ViewRect>,
    pub(super) places_scroll_y: f32,
    pub(crate) context_menu: Option<ContextMenuDamageState>,
    pub(crate) drop_menu: Option<DropMenuDamageState>,
    pub(crate) rubber_band_rect: Option<ViewRect>,
    pub(crate) location_draft_rect: Option<ViewRect>,
    pub(crate) task_area_rect: Option<ViewRect>,
    pub(super) size: PhysicalSize<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContextMenuDamageState {
    pub(super) hovered_row: Option<usize>,
    pub(super) hovered_submenu_row: Option<usize>,
    pub(super) active_submenu: Option<ShellContextSubmenu>,
    pub(super) active_submenu_row: Option<usize>,
    pub(super) root_rect: ViewRect,
    pub(crate) overlay_rect: ViewRect,
    pub(super) root_row_rects: Vec<(usize, ViewRect)>,
    pub(super) submenu_rect: Option<ViewRect>,
    pub(super) submenu_row_rects: Vec<(usize, ViewRect)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DropMenuDamageState {
    pub(super) hovered_row: Option<usize>,
    pub(crate) overlay_rect: ViewRect,
    pub(super) row_rects: Vec<(usize, ViewRect)>,
}

impl ShellRenderDamageSnapshot {
    #[cfg(test)]
    pub(crate) fn from_scene(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
        projections: &[ShellPaneProjection<'_>],
        dirty_key: ShellRenderDirtyKey,
    ) -> Self {
        let context = ShellRenderDirtyKeyContext::from_scene(scene, projections);
        Self::from_scene_inner(scene, size, projections, dirty_key, &context)
    }

    fn from_scene_inner(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
        projections: &[ShellPaneProjection<'_>],
        dirty_key: ShellRenderDirtyKey,
        context: &ShellRenderDirtyKeyContext,
    ) -> Self {
        let mut item_rects = Vec::new();
        for projection in projections {
            for item in &projection.visible_items {
                let Some(entry_index) = projection
                    .view
                    .filtered_indexes
                    .get(item.layout.model_index)
                    .copied()
                else {
                    continue;
                };
                let target = ShellPaneItemTarget {
                    pane: projection.geometry.kind,
                    index: entry_index,
                };
                let rect = pane_content_rect_to_screen(item.layout.visual_rect, projection);
                if let Some(rect) = intersect_rect(rect, projection.geometry.content) {
                    item_rects.push((target, rect));
                }
            }
        }
        let place_rects = if scene.places_visible {
            scene.place_row_rects(size)
        } else {
            Vec::new()
        };
        let place_gap_rects = if scene.places_visible {
            scene.place_gap_rects(size)
        } else {
            Vec::new()
        };
        let places_sidebar_rect = if scene.places_visible {
            let rect = scene.places_sidebar_rect(size);
            (rect.width > 0.0 && rect.height > 0.0).then_some(rect)
        } else {
            None
        };
        let context_menu = scene
            .context_menu
            .as_ref()
            .map(|menu| context_menu_damage_state(menu, size, scene.ui_scale()));
        let drop_menu = scene
            .drop_menu
            .as_ref()
            .map(|menu| drop_menu_damage_state(menu, size, scene.ui_scale()));
        let rubber_band_rect = rubber_band_damage_rect(scene, size, projections);
        let location_draft_rect = scene
            .location_draft_pane()
            .and_then(|pane| scene.pane_path_bar_rect(pane, size));
        let task_area_rect = scene.places_task_area_rect(size);
        let hoverless_dirty_key =
            ShellRenderDirtyKey::from_scene_ignoring_hover_with_context(scene, size, context);
        let folder_previewless_dirty_key =
            ShellRenderDirtyKey::from_scene_ignoring_folder_preview_roles_with_context(
                scene, size, context,
            );
        let hoverless_folder_previewless_dirty_key =
            ShellRenderDirtyKey::from_scene_ignoring_hover_and_folder_preview_roles_with_context(
                scene, size, context,
            );
        Self {
            dirty_key,
            hoverless_dirty_key,
            folder_previewless_dirty_key,
            hoverless_folder_previewless_dirty_key,
            hovered_item: scene.hovered_item,
            hovered_place: scene.hovered_place,
            dnd_hover_target: scene.dnd_hover_target.clone(),
            item_rects,
            place_rects,
            place_gap_rects,
            places_sidebar_rect,
            places_scroll_y: scene.places_scroll_y,
            context_menu,
            drop_menu,
            rubber_band_rect,
            location_draft_rect,
            task_area_rect,
            size,
        }
    }

    pub(crate) fn from_scene_with_dirty_key_context(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
        projections: &[ShellPaneProjection<'_>],
        dirty_key: ShellRenderDirtyKey,
        context: &ShellRenderDirtyKeyContext,
    ) -> Self {
        Self::from_scene_inner(scene, size, projections, dirty_key, context)
    }

    pub(super) fn item_rect(&self, target: ShellPaneItemTarget) -> Option<ViewRect> {
        self.item_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == target).then_some(*rect))
    }

    pub(super) fn place_rect(&self, index: usize) -> Option<ViewRect> {
        self.place_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == index).then_some(*rect))
    }

    pub(super) fn place_gap_rect(&self, index: usize) -> Option<ViewRect> {
        self.place_gap_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == index).then_some(*rect))
    }
}

impl ContextMenuDamageState {
    pub(super) fn root_row_rect(&self, row: usize) -> Option<ViewRect> {
        self.root_row_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == row).then_some(*rect))
    }

    pub(super) fn submenu_row_rect(&self, row: usize) -> Option<ViewRect> {
        self.submenu_row_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == row).then_some(*rect))
    }
}

impl DropMenuDamageState {
    pub(super) fn row_rect(&self, row: usize) -> Option<ViewRect> {
        self.row_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == row).then_some(*rect))
    }
}

fn context_menu_damage_state(
    menu: &ShellContextMenu,
    size: PhysicalSize<u32>,
    scale: f32,
) -> ContextMenuDamageState {
    let root_rect =
        context_menu_shadow_damage_rect(context_menu_rect_scaled(menu, size, scale), size, scale);
    let root_row_rects = (0..context_menu_items(menu).len())
        .filter_map(|row| {
            context_menu_root_row_rect(menu, size, scale, row).map(|rect| (row, rect))
        })
        .collect::<Vec<_>>();
    let submenu_rect = context_menu_submenu_rect(menu, size, scale)
        .map(|rect| context_menu_shadow_damage_rect(rect, size, scale));
    let overlay_rect = submenu_rect
        .map(|submenu_rect| union_rect(root_rect, submenu_rect))
        .unwrap_or(root_rect);
    let submenu_row_rects = menu
        .active_submenu
        .map(|submenu| {
            (0..context_submenu_actions(submenu, menu).len())
                .filter_map(|row| {
                    context_menu_submenu_row_rect(menu, size, scale, row).map(|rect| (row, rect))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ContextMenuDamageState {
        hovered_row: menu.hovered_row,
        hovered_submenu_row: menu.hovered_submenu_row,
        active_submenu: menu.active_submenu,
        active_submenu_row: menu.active_submenu_row,
        root_rect,
        overlay_rect,
        root_row_rects,
        submenu_rect,
        submenu_row_rects,
    }
}

fn context_menu_root_row_rect(
    menu: &ShellContextMenu,
    size: PhysicalSize<u32>,
    scale: f32,
    row: usize,
) -> Option<ViewRect> {
    (row < context_menu_items(menu).len()).then(|| {
        let rect = context_menu_rect_scaled(menu, size, scale);
        context_menu_row_rect(rect, scale, row)
    })
}

fn context_menu_submenu_row_rect(
    menu: &ShellContextMenu,
    size: PhysicalSize<u32>,
    scale: f32,
    row: usize,
) -> Option<ViewRect> {
    let submenu = menu.active_submenu?;
    (row < context_submenu_actions(submenu, menu).len()).then(|| {
        let rect = context_menu_submenu_rect(menu, size, scale)?;
        Some(context_menu_row_rect(rect, scale, row))
    })?
}

fn context_menu_row_rect(rect: ViewRect, scale: f32, row: usize) -> ViewRect {
    let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    ViewRect {
        x: rect.x,
        y: rect.y + padding_y + row as f32 * row_height,
        width: rect.width,
        height: row_height,
    }
}

fn context_menu_shadow_damage_rect(
    rect: ViewRect,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let scale = scale_factor.max(1.0);
    let shadow = ViewRect {
        x: rect.x - (8.0 * scale).round(),
        y: rect.y + (7.0 * scale).round() - (8.0 * scale).round(),
        width: rect.width + (16.0 * scale).round(),
        height: rect.height + (16.0 * scale).round(),
    };
    intersect_rect(shadow, surface_rect(size)).unwrap_or(shadow)
}

fn drop_menu_damage_state(
    menu: &ShellDropMenu,
    size: PhysicalSize<u32>,
    scale: f32,
) -> DropMenuDamageState {
    let rect = drop_menu_rect_scaled(menu, size, scale);
    let overlay_rect = context_menu_shadow_damage_rect(rect, size, scale);
    let row_rects = (0..drop_menu_items().len())
        .map(|row| (row, context_menu_row_rect(rect, scale, row)))
        .collect();
    DropMenuDamageState {
        hovered_row: menu.hovered_row,
        overlay_rect,
        row_rects,
    }
}

fn rubber_band_damage_rect(
    scene: &ShellScene,
    size: PhysicalSize<u32>,
    projections: &[ShellPaneProjection<'_>],
) -> Option<ViewRect> {
    let rect = scene
        .rubber_band
        .as_ref()
        .and_then(RubberBand::active_rect)?;
    let projection = projections
        .iter()
        .find(|projection| projection.geometry.kind == scene.active_pane())?;
    let rect = pane_content_rect_to_screen(rect, projection);
    let rect = intersect_rect(rect, projection.geometry.content)?;
    intersect_rect(outset_rect(rect, 1.0), surface_rect(size))
}

fn outset_rect(rect: ViewRect, outset: f32) -> ViewRect {
    let outset = outset.max(0.0);
    ViewRect {
        x: rect.x - outset,
        y: rect.y - outset,
        width: rect.width + outset * 2.0,
        height: rect.height + outset * 2.0,
    }
}

fn surface_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width as f32,
        height: size.height as f32,
    }
}

fn union_rect(left: ViewRect, right: ViewRect) -> ViewRect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let right_edge = left.right().max(right.right());
    let bottom = left.bottom().max(right.bottom());
    ViewRect {
        x,
        y,
        width: right_edge - x,
        height: bottom - y,
    }
}
