use std::path::Path;
use std::time::Instant;

use crate::windowing::PhysicalSize;
use bevy_ecs::entity::Entity;
use tensor_files_core::ViewRect;

use crate::ShellScene;
use crate::ui::animation::{ShellItemReflowOffsets, ShellItemReflowRects};
use crate::ui::metrics::ITEM_REFLOW_ANIMATION_DELAY;
use crate::ui::pane::ShellPaneId;

type ReflowRectsByPane = Vec<(ShellPaneId, ShellItemReflowRects)>;

#[derive(Default)]
pub(crate) struct ShellItemReflowRuntime {
    pending: Option<ShellPendingItemReflow>,
}

struct ShellPendingItemReflow {
    previous_rects_by_pane: ReflowRectsByPane,
    offsets_by_pane: [ShellItemReflowOffsets; 2],
    size: PhysicalSize<u32>,
    deadline: Instant,
}

impl ShellItemReflowRuntime {
    fn schedule(
        &mut self,
        previous_rects_by_pane: ReflowRectsByPane,
        next_rects_by_pane: ReflowRectsByPane,
        size: PhysicalSize<u32>,
    ) -> bool {
        let offsets_by_pane = pending_offsets_by_pane(&previous_rects_by_pane, &next_rects_by_pane);
        if offsets_by_pane.iter().all(ShellItemReflowOffsets::is_empty) {
            if self.pending.is_some() {
                self.pending = None;
            }
            return false;
        }
        self.pending = Some(ShellPendingItemReflow {
            previous_rects_by_pane,
            offsets_by_pane,
            size,
            deadline: Instant::now() + ITEM_REFLOW_ANIMATION_DELAY,
        });
        true
    }

    fn pending_previous_rects(&self) -> Option<ReflowRectsByPane> {
        self.pending
            .as_ref()
            .map(|pending| clone_rects_by_pane(&pending.previous_rects_by_pane))
    }

    fn pending_offset_for_entity_or_path(
        &self,
        pane: ShellPaneId,
        entity: Option<Entity>,
        path: &Path,
    ) -> Option<(f32, f32)> {
        let pending = self.pending.as_ref()?;
        pending.offsets_by_pane[pane.index()].offset_for(entity, path)
    }

    fn active_for_pane(&self, pane: ShellPaneId) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| !pending.offsets_by_pane[pane.index()].is_empty())
    }

    fn take_due(&mut self, now: Instant) -> Option<ShellPendingItemReflow> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| now >= pending.deadline)
        {
            return self.pending.take();
        }
        None
    }

    fn clear_pane(&mut self, pane: ShellPaneId) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        pending
            .previous_rects_by_pane
            .retain(|(pending_pane, _)| *pending_pane != pane);
        pending.offsets_by_pane[pane.index()].clear();
        if pending.previous_rects_by_pane.is_empty() {
            self.pending = None;
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|pending| pending.deadline)
    }
}

pub(crate) fn visible_item_reflow_rects_for_pane(
    scene: &ShellScene,
    pane: ShellPaneId,
    size: PhysicalSize<u32>,
) -> ShellItemReflowRects {
    let Some((view, geometry, layout)) = scene.pane_layout_context(pane, size) else {
        return ShellItemReflowRects::default();
    };
    let mut rects = ShellItemReflowRects::default();
    layout.for_each_visible_item(|item| {
        let Some(entry_index) = view.filtered_indexes.get(item.model_index).copied() else {
            return;
        };
        let rect = ViewRect {
            x: item.visual_rect.x - view.scroll_x + geometry.content.x,
            y: item.visual_rect.y - view.scroll_y + geometry.content.y,
            width: item.visual_rect.width,
            height: item.visual_rect.height,
        };
        if let Some(entity) = scene
            .visible_slots
            .get(pane)
            .entity_for_entry(&view.entries[entry_index])
        {
            rects.insert_entity(entity, rect);
        } else if let Some(path) = scene.entry_path_for_pane_view(view, entry_index) {
            rects.insert_path(path, rect);
        }
    });
    rects
}

pub(crate) fn visible_item_reflow_rects_for_open_panes(
    scene: &ShellScene,
    size: PhysicalSize<u32>,
) -> ReflowRectsByPane {
    ShellPaneId::ALL
        .into_iter()
        .filter_map(|pane| {
            let rects = visible_item_reflow_rects_for_pane(scene, pane, size);
            (!rects.is_empty()).then_some((pane, rects))
        })
        .collect()
}

pub(crate) fn reflow_pane_items_after_window_resize(
    scene: &mut ShellScene,
    previous_size: PhysicalSize<u32>,
    next_size: PhysicalSize<u32>,
) -> bool {
    if previous_size.width == next_size.width {
        scene.clamp_scroll(next_size);
        return false;
    }
    let previous_rects = scene
        .item_reflow
        .pending_previous_rects()
        .unwrap_or_else(|| visible_item_reflow_rects_for_open_panes(scene, previous_size));
    scene.clamp_scroll(next_size);
    let next_rects = next_rects_by_pane(scene, &previous_rects, next_size);
    scene
        .item_reflow
        .schedule(previous_rects, next_rects, next_size)
}

pub(crate) fn start_item_reflow_transitions(
    scene: &mut ShellScene,
    pane: ShellPaneId,
    previous_rects: ShellItemReflowRects,
    size: PhysicalSize<u32>,
) -> bool {
    scene.item_reflow.clear_pane(pane);
    let next_rects = visible_item_reflow_rects_for_pane(scene, pane, size);
    scene
        .animations
        .start_item_reflow_from_rects(pane, previous_rects, next_rects)
}

pub(crate) fn start_item_reflow_transitions_for_panes(
    scene: &mut ShellScene,
    previous_rects_by_pane: ReflowRectsByPane,
    size: PhysicalSize<u32>,
) -> bool {
    previous_rects_by_pane
        .into_iter()
        .fold(false, |started, (pane, previous_rects)| {
            start_item_reflow_transitions(scene, pane, previous_rects, size) || started
        })
}

pub(crate) fn item_reflow_offset_for_entity_or_path_at(
    scene: &ShellScene,
    pane: ShellPaneId,
    entity: Option<Entity>,
    path: &Path,
    now: Instant,
) -> Option<(f32, f32)> {
    if let Some(offset) = scene
        .item_reflow
        .pending_offset_for_entity_or_path(pane, entity, path)
    {
        return Some(offset);
    }
    if let Some(offset) = entity.and_then(|entity| {
        scene
            .animations
            .item_reflow_offset_for_entity_at(pane, entity, now)
    }) {
        return Some(offset);
    }
    scene
        .animations
        .item_reflow_offset_for_path_at(pane, path, now)
}

pub(crate) fn item_reflow_active_for_pane_at(
    scene: &ShellScene,
    pane: ShellPaneId,
    now: Instant,
) -> bool {
    scene.item_reflow.active_for_pane(pane)
        || scene.animations.item_reflow_active_for_pane_at(pane, now)
}

pub(crate) fn has_item_reflow_for_pane(scene: &ShellScene, pane: ShellPaneId) -> bool {
    scene.item_reflow.active_for_pane(pane) || scene.animations.has_item_reflow_for_pane(pane)
}

pub(crate) fn clear_item_reflow_for_pane(scene: &mut ShellScene, pane: ShellPaneId) {
    scene.item_reflow.clear_pane(pane);
    scene.animations.clear_item_reflow_for_pane(pane);
}

pub(crate) fn start_due_item_reflow_transitions(scene: &mut ShellScene, now: Instant) -> bool {
    let Some(pending) = scene.item_reflow.take_due(now) else {
        return false;
    };
    let size = pending.size;
    pending
        .previous_rects_by_pane
        .into_iter()
        .fold(false, |started, (pane, previous_rects)| {
            scene.item_reflow.clear_pane(pane);
            let next_rects = visible_item_reflow_rects_for_pane(scene, pane, size);
            scene
                .animations
                .start_item_reflow_from_rects(pane, previous_rects, next_rects)
                || started
        })
}

pub(crate) fn next_item_reflow_deadline(scene: &ShellScene) -> Option<Instant> {
    scene.item_reflow.next_deadline()
}

#[cfg(test)]
pub(crate) fn has_pending_item_reflow(scene: &ShellScene) -> bool {
    scene.item_reflow.pending.is_some()
}

fn next_rects_by_pane(
    scene: &ShellScene,
    previous_rects_by_pane: &[(ShellPaneId, ShellItemReflowRects)],
    size: PhysicalSize<u32>,
) -> ReflowRectsByPane {
    previous_rects_by_pane
        .iter()
        .map(|(pane, _)| {
            (
                *pane,
                visible_item_reflow_rects_for_pane(scene, *pane, size),
            )
        })
        .collect()
}

fn pending_offsets_by_pane(
    previous_rects_by_pane: &[(ShellPaneId, ShellItemReflowRects)],
    next_rects_by_pane: &[(ShellPaneId, ShellItemReflowRects)],
) -> [ShellItemReflowOffsets; 2] {
    let mut offsets_by_pane = std::array::from_fn(|_| ShellItemReflowOffsets::default());
    for (pane, previous_rects) in previous_rects_by_pane {
        let Some((_, next_rects)) = next_rects_by_pane
            .iter()
            .find(|(next_pane, _)| next_pane == pane)
        else {
            continue;
        };
        offsets_by_pane[pane.index()] =
            ShellItemReflowOffsets::from_rects(previous_rects, next_rects);
    }
    offsets_by_pane
}

fn clone_rects_by_pane(rects_by_pane: &[(ShellPaneId, ShellItemReflowRects)]) -> ReflowRectsByPane {
    rects_by_pane
        .iter()
        .map(|(pane, rects)| (*pane, rects.clone()))
        .collect()
}
