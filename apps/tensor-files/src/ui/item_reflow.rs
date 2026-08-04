use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::windowing::PhysicalSize;
use tensor_files_core::ViewRect;

use crate::ui::animation::item_reflow_rect_moved;
use crate::ui::metrics::ITEM_REFLOW_ANIMATION_DELAY;
use crate::ui::pane::ShellPaneId;
use crate::{ShellScene, pane_content_rect_to_screen};

type ReflowRectsByPane = Vec<(ShellPaneId, HashMap<PathBuf, ViewRect>)>;

#[derive(Default)]
pub(crate) struct ShellItemReflowRuntime {
    pending: Option<ShellPendingItemReflow>,
}

struct ShellPendingItemReflow {
    previous_rects_by_pane: ReflowRectsByPane,
    next_rects_by_pane: ReflowRectsByPane,
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
        if !rects_by_pane_moved(&previous_rects_by_pane, &next_rects_by_pane) {
            if self.pending.is_some() {
                self.pending = None;
            }
            return false;
        }
        self.pending = Some(ShellPendingItemReflow {
            previous_rects_by_pane,
            next_rects_by_pane,
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

    fn pending_offset_for_path(&self, pane: ShellPaneId, path: &Path) -> Option<(f32, f32)> {
        let pending = self.pending.as_ref()?;
        let from = rect_for_path(&pending.previous_rects_by_pane, pane, path)?;
        let to = rect_for_path(&pending.next_rects_by_pane, pane, path)?;
        item_reflow_rect_moved(from, to).then_some((from.x - to.x, from.y - to.y))
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
        pending
            .next_rects_by_pane
            .retain(|(pending_pane, _)| *pending_pane != pane);
        if pending.previous_rects_by_pane.is_empty() {
            self.pending = None;
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|pending| pending.deadline)
    }
}

pub(crate) fn visible_item_rects_by_path_for_pane(
    scene: &ShellScene,
    pane: ShellPaneId,
    size: PhysicalSize<u32>,
) -> HashMap<PathBuf, ViewRect> {
    let Some(projection) = scene.pane_projection(pane, size) else {
        return HashMap::new();
    };
    let mut rects = HashMap::with_capacity(projection.visible_items.len());
    for item in &projection.visible_items {
        let Some(entry_index) = projection
            .view
            .filtered_indexes
            .get(item.layout.model_index)
            .copied()
        else {
            continue;
        };
        let Some(path) = scene.entry_path_for_pane_view(projection.view, entry_index) else {
            continue;
        };
        rects.insert(
            path,
            pane_content_rect_to_screen(item.layout.visual_rect, &projection),
        );
    }
    rects
}

pub(crate) fn visible_item_rects_by_path_for_open_panes(
    scene: &ShellScene,
    size: PhysicalSize<u32>,
) -> Vec<(ShellPaneId, HashMap<PathBuf, ViewRect>)> {
    ShellPaneId::ALL
        .into_iter()
        .filter_map(|pane| {
            let rects = visible_item_rects_by_path_for_pane(scene, pane, size);
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
        .unwrap_or_else(|| visible_item_rects_by_path_for_open_panes(scene, previous_size));
    scene.clamp_scroll(next_size);
    let next_rects = next_rects_by_pane(scene, &previous_rects, next_size);
    scene
        .item_reflow
        .schedule(previous_rects, next_rects, next_size)
}

pub(crate) fn start_item_reflow_transitions(
    scene: &mut ShellScene,
    pane: ShellPaneId,
    previous_rects: HashMap<PathBuf, ViewRect>,
    size: PhysicalSize<u32>,
) -> bool {
    scene.item_reflow.clear_pane(pane);
    let next_rects = visible_item_rects_by_path_for_pane(scene, pane, size);
    scene
        .animations
        .start_item_reflow(pane, previous_rects, next_rects)
}

pub(crate) fn start_item_reflow_transitions_for_panes(
    scene: &mut ShellScene,
    previous_rects_by_pane: Vec<(ShellPaneId, HashMap<PathBuf, ViewRect>)>,
    size: PhysicalSize<u32>,
) -> bool {
    previous_rects_by_pane
        .into_iter()
        .fold(false, |started, (pane, previous_rects)| {
            start_item_reflow_transitions(scene, pane, previous_rects, size) || started
        })
}

pub(crate) fn item_reflow_offset_for_path(
    scene: &ShellScene,
    pane: ShellPaneId,
    path: &Path,
) -> Option<(f32, f32)> {
    if let Some(offset) = scene.item_reflow.pending_offset_for_path(pane, path) {
        return Some(offset);
    }
    scene.animations.item_reflow_offset_for_path(pane, path)
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
            start_item_reflow_transitions(scene, pane, previous_rects, size) || started
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
    previous_rects_by_pane: &[(ShellPaneId, HashMap<PathBuf, ViewRect>)],
    size: PhysicalSize<u32>,
) -> ReflowRectsByPane {
    previous_rects_by_pane
        .iter()
        .map(|(pane, _)| {
            (
                *pane,
                visible_item_rects_by_path_for_pane(scene, *pane, size),
            )
        })
        .collect()
}

fn rects_by_pane_moved(
    previous_rects_by_pane: &[(ShellPaneId, HashMap<PathBuf, ViewRect>)],
    next_rects_by_pane: &[(ShellPaneId, HashMap<PathBuf, ViewRect>)],
) -> bool {
    previous_rects_by_pane.iter().any(|(pane, previous_rects)| {
        let Some((_, next_rects)) = next_rects_by_pane
            .iter()
            .find(|(next_pane, _)| next_pane == pane)
        else {
            return false;
        };
        next_rects.iter().any(|(path, to)| {
            previous_rects
                .get(path)
                .is_some_and(|from| item_reflow_rect_moved(*from, *to))
        })
    })
}

fn rect_for_path(
    rects_by_pane: &[(ShellPaneId, HashMap<PathBuf, ViewRect>)],
    pane: ShellPaneId,
    path: &Path,
) -> Option<ViewRect> {
    rects_by_pane
        .iter()
        .find(|(rects_pane, _)| *rects_pane == pane)?
        .1
        .get(path)
        .copied()
}

fn clone_rects_by_pane(
    rects_by_pane: &[(ShellPaneId, HashMap<PathBuf, ViewRect>)],
) -> ReflowRectsByPane {
    rects_by_pane
        .iter()
        .map(|(pane, rects)| (*pane, rects.clone()))
        .collect()
}
