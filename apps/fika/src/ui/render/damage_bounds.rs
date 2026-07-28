use crate::windowing::PhysicalSize;
use fika_core::ViewRect;

use crate::ShellPaneItemTarget;
use crate::ui::drop_menu::ShellDropTarget;
use crate::ui::render::damage_snapshot::{
    ContextMenuDamageState, DropMenuDamageState, ShellRenderDamageSnapshot,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellRenderDamageKind {
    Clean,
    Bounded,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellRenderDamage {
    pub(crate) kind: ShellRenderDamageKind,
    pub(crate) bounds: Option<ViewRect>,
    pub(crate) rect_count: usize,
    pub(crate) area_px: f32,
}

impl ShellRenderDamage {
    fn clean() -> Self {
        Self {
            kind: ShellRenderDamageKind::Clean,
            bounds: None,
            rect_count: 0,
            area_px: 0.0,
        }
    }

    pub(crate) fn full(size: PhysicalSize<u32>) -> Self {
        let bounds = full_surface_rect(size);
        Self {
            kind: ShellRenderDamageKind::Full,
            bounds: Some(bounds),
            rect_count: 1,
            area_px: rect_area(bounds),
        }
    }

    fn bounded(rects: Vec<ViewRect>) -> Option<Self> {
        let mut iter = rects.into_iter();
        let first = iter.next()?;
        let mut bounds = first;
        let mut rect_count = 1;
        let mut area_px = rect_area(first);
        for rect in iter {
            bounds = union_rect(bounds, rect);
            rect_count += 1;
            area_px += rect_area(rect);
        }
        Some(Self {
            kind: ShellRenderDamageKind::Bounded,
            bounds: Some(bounds),
            rect_count,
            area_px,
        })
    }

    #[cfg(test)]
    pub(crate) fn between(
        previous: Option<&ShellRenderDamageSnapshot>,
        current: &ShellRenderDamageSnapshot,
        async_results_changed: bool,
    ) -> Self {
        Self::between_with_async_damage(previous, current, async_results_changed, Vec::new())
    }

    pub(crate) fn between_with_async_damage(
        previous: Option<&ShellRenderDamageSnapshot>,
        current: &ShellRenderDamageSnapshot,
        force_full_async_results_changed: bool,
        async_damage_rects: Vec<ViewRect>,
    ) -> Self {
        let Some(previous) = previous else {
            return Self::full(current.size);
        };
        if force_full_async_results_changed || previous.size != current.size {
            return Self::full(current.size);
        }
        if previous.folder_previewless_dirty_key == current.folder_previewless_dirty_key {
            return Self::bounded(async_damage_rects).unwrap_or_else(Self::clean);
        }
        if previous.dirty_key == current.dirty_key {
            return Self::clean();
        }
        if previous.hoverless_dirty_key == current.hoverless_dirty_key {
            let rects = hover_damage_rects(previous, current);
            if let Some(damage) = Self::bounded(rects) {
                return damage;
            }
        }
        if previous.hoverless_folder_previewless_dirty_key
            == current.hoverless_folder_previewless_dirty_key
        {
            let mut rects = hover_damage_rects(previous, current);
            rects.extend(async_damage_rects);
            if let Some(damage) = Self::bounded(rects) {
                return damage;
            }
        }
        Self::full(current.size)
    }

    pub(crate) fn kind_label(self) -> &'static str {
        match self.kind {
            ShellRenderDamageKind::Clean => "clean",
            ShellRenderDamageKind::Bounded => "bounded",
            ShellRenderDamageKind::Full => "full",
        }
    }

    pub(crate) fn scissor_rect(self, size: PhysicalSize<u32>) -> Option<DamageScissorRect> {
        damage_scissor_rect(self.bounds?, size)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DamageScissorRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

fn hover_damage_rects(
    previous: &ShellRenderDamageSnapshot,
    current: &ShellRenderDamageSnapshot,
) -> Vec<ViewRect> {
    let mut rects = Vec::new();
    if previous.hovered_item != current.hovered_item {
        push_item_damage_rect(&mut rects, previous, previous.hovered_item);
        push_item_damage_rect(&mut rects, current, current.hovered_item);
    }
    if previous.hovered_place != current.hovered_place {
        push_place_damage_rect(&mut rects, previous, previous.hovered_place);
        push_place_damage_rect(&mut rects, current, current.hovered_place);
    }
    if previous.dnd_hover_target != current.dnd_hover_target {
        push_dnd_hover_damage_rect(&mut rects, previous, previous.dnd_hover_target.as_ref());
        push_dnd_hover_damage_rect(&mut rects, current, current.dnd_hover_target.as_ref());
    }
    push_context_menu_damage_rects(
        &mut rects,
        previous.context_menu.as_ref(),
        current.context_menu.as_ref(),
    );
    push_drop_menu_damage_rects(
        &mut rects,
        previous.drop_menu.as_ref(),
        current.drop_menu.as_ref(),
    );
    if (previous.places_scroll_y - current.places_scroll_y).abs() > f32::EPSILON {
        push_stable_or_changed_damage_rect(
            &mut rects,
            previous.places_sidebar_rect,
            current.places_sidebar_rect,
        );
    }
    push_changed_damage_rect(
        &mut rects,
        previous.rubber_band_rect,
        current.rubber_band_rect,
    );
    push_stable_or_changed_damage_rect(
        &mut rects,
        previous.location_draft_rect,
        current.location_draft_rect,
    );
    push_stable_or_changed_damage_rect(&mut rects, previous.task_area_rect, current.task_area_rect);
    rects
}

fn push_item_damage_rect(
    rects: &mut Vec<ViewRect>,
    snapshot: &ShellRenderDamageSnapshot,
    target: Option<ShellPaneItemTarget>,
) {
    if let Some(rect) = target.and_then(|target| snapshot.item_rect(target)) {
        rects.push(rect);
    }
}

fn push_place_damage_rect(
    rects: &mut Vec<ViewRect>,
    snapshot: &ShellRenderDamageSnapshot,
    index: Option<usize>,
) {
    if let Some(rect) = index.and_then(|index| snapshot.place_rect(index)) {
        rects.push(rect);
    }
}

fn push_dnd_hover_damage_rect(
    rects: &mut Vec<ViewRect>,
    snapshot: &ShellRenderDamageSnapshot,
    target: Option<&ShellDropTarget>,
) {
    let rect = match target {
        Some(ShellDropTarget::PaneItem {
            pane,
            index,
            is_dir: true,
            ..
        }) => snapshot.item_rect(ShellPaneItemTarget {
            pane: *pane,
            index: *index,
        }),
        Some(ShellDropTarget::Place { index, .. }) => snapshot.place_rect(*index),
        Some(ShellDropTarget::PlacesGap { index }) => snapshot.place_gap_rect(*index),
        Some(
            ShellDropTarget::PaneItem { is_dir: false, .. }
            | ShellDropTarget::PaneBlank { .. }
            | ShellDropTarget::PlacesBlank,
        )
        | None => None,
    };
    if let Some(rect) = rect {
        rects.push(rect);
    }
}

fn push_context_menu_damage_rects(
    rects: &mut Vec<ViewRect>,
    previous: Option<&ContextMenuDamageState>,
    current: Option<&ContextMenuDamageState>,
) {
    let (Some(previous), Some(current)) = (previous, current) else {
        push_changed_damage_rect(
            rects,
            previous.map(|state| state.overlay_rect),
            current.map(|state| state.overlay_rect),
        );
        return;
    };
    if previous.root_rect != current.root_rect
        || previous.root_row_rects.len() != current.root_row_rects.len()
    {
        push_changed_damage_rect(
            rects,
            Some(previous.overlay_rect),
            Some(current.overlay_rect),
        );
        return;
    }
    if previous.hovered_row != current.hovered_row {
        push_context_menu_root_row_damage(rects, previous, previous.hovered_row);
        push_context_menu_root_row_damage(rects, current, current.hovered_row);
    }
    if previous.active_submenu != current.active_submenu
        || previous.active_submenu_row != current.active_submenu_row
    {
        if let Some(rect) = previous.submenu_rect {
            rects.push(rect);
        }
        if let Some(rect) = current.submenu_rect {
            rects.push(rect);
        }
    } else if previous.hovered_submenu_row != current.hovered_submenu_row {
        push_context_menu_submenu_row_damage(rects, previous, previous.hovered_submenu_row);
        push_context_menu_submenu_row_damage(rects, current, current.hovered_submenu_row);
    }
}

fn push_context_menu_root_row_damage(
    rects: &mut Vec<ViewRect>,
    state: &ContextMenuDamageState,
    row: Option<usize>,
) {
    if let Some(rect) = row.and_then(|row| state.root_row_rect(row)) {
        rects.push(rect);
    }
}

fn push_context_menu_submenu_row_damage(
    rects: &mut Vec<ViewRect>,
    state: &ContextMenuDamageState,
    row: Option<usize>,
) {
    if let Some(rect) = row.and_then(|row| state.submenu_row_rect(row)) {
        rects.push(rect);
    }
}

fn push_drop_menu_damage_rects(
    rects: &mut Vec<ViewRect>,
    previous: Option<&DropMenuDamageState>,
    current: Option<&DropMenuDamageState>,
) {
    let (Some(previous), Some(current)) = (previous, current) else {
        push_changed_damage_rect(
            rects,
            previous.map(|state| state.overlay_rect),
            current.map(|state| state.overlay_rect),
        );
        return;
    };
    if previous.overlay_rect != current.overlay_rect
        || previous.row_rects.len() != current.row_rects.len()
    {
        push_changed_damage_rect(
            rects,
            Some(previous.overlay_rect),
            Some(current.overlay_rect),
        );
        return;
    }
    if previous.hovered_row != current.hovered_row {
        push_drop_menu_row_damage(rects, previous, previous.hovered_row);
        push_drop_menu_row_damage(rects, current, current.hovered_row);
    }
}

fn push_drop_menu_row_damage(
    rects: &mut Vec<ViewRect>,
    state: &DropMenuDamageState,
    row: Option<usize>,
) {
    if let Some(rect) = row.and_then(|row| state.row_rect(row)) {
        rects.push(rect);
    }
}

fn push_changed_damage_rect(
    rects: &mut Vec<ViewRect>,
    previous: Option<ViewRect>,
    current: Option<ViewRect>,
) {
    if previous == current {
        return;
    }
    if let Some(rect) = previous {
        rects.push(rect);
    }
    if let Some(rect) = current {
        rects.push(rect);
    }
}

fn push_stable_or_changed_damage_rect(
    rects: &mut Vec<ViewRect>,
    previous: Option<ViewRect>,
    current: Option<ViewRect>,
) {
    match (previous, current) {
        (Some(previous), Some(current)) if previous == current => rects.push(current),
        (previous, current) => push_changed_damage_rect(rects, previous, current),
    }
}

pub(crate) fn full_surface_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width as f32,
        height: size.height as f32,
    }
}

pub(crate) fn rect_area(rect: ViewRect) -> f32 {
    rect.width.max(0.0) * rect.height.max(0.0)
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

pub(crate) fn damage_scissor_rect(
    rect: ViewRect,
    size: PhysicalSize<u32>,
) -> Option<DamageScissorRect> {
    let max_width = size.width.max(1);
    let max_height = size.height.max(1);
    let x = rect.x.floor().max(0.0).min(max_width as f32) as u32;
    let y = rect.y.floor().max(0.0).min(max_height as f32) as u32;
    let right = rect.right().ceil().max(0.0).min(max_width as f32) as u32;
    let bottom = rect.bottom().ceil().max(0.0).min(max_height as f32) as u32;
    (right > x && bottom > y).then_some(DamageScissorRect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}
