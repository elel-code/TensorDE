use std::time::Instant;

use crate::windowing::PhysicalSize;
use fika_core::ViewPoint;

use crate::ui::context_menu::{
    ShellContextMenu, ShellContextMenuItem, ShellContextSubmenu, context_menu_items,
};
use crate::ui::menu_geometry::{
    context_menu_rect_scaled, context_menu_row_at_screen_point, context_menu_submenu_rect,
    context_submenu_row_at_screen_point, scaled_context_menu_metric,
};
use crate::ui::metrics::{
    CONTEXT_MENU_SAFE_TRIANGLE_MARGIN, CONTEXT_MENU_SAFE_TRIANGLE_PARENT_RETENTION_WIDTH,
    CONTEXT_MENU_SUBMENU_AIM_DELAY,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShellContextMenuHoverState {
    pub(crate) hovered_row: Option<usize>,
    pub(crate) active_submenu: Option<ShellContextSubmenu>,
    pub(crate) active_submenu_row: Option<usize>,
    pub(crate) hovered_submenu_row: Option<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct ShellContextMenuSafeTriangleRuntime {
    previous_pointer: Option<ViewPoint>,
    pending: Option<PendingContextMenuHover>,
}

#[derive(Clone, Copy, Debug)]
struct PendingContextMenuHover {
    target: ShellContextMenuHoverState,
    deadline: Instant,
}

impl ShellContextMenuSafeTriangleRuntime {
    pub(crate) fn reset(&mut self) {
        self.previous_pointer = None;
        self.pending = None;
    }

    pub(crate) fn hover_state(
        &mut self,
        menu: &ShellContextMenu,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        scale: f32,
    ) -> ShellContextMenuHoverState {
        let state = self.hover_state_at(menu, point, size, scale, Instant::now());
        self.previous_pointer = Some(point);
        state
    }

    fn hover_state_at(
        &mut self,
        menu: &ShellContextMenu,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        scale: f32,
        now: Instant,
    ) -> ShellContextMenuHoverState {
        let mut evaluation =
            evaluate_context_menu_hover(menu, self.previous_pointer, point, size, scale, now);
        let Some(mut pending) = evaluation.pending else {
            self.pending = None;
            return evaluation.visible;
        };
        if let Some(current) = self.pending
            && current.target == pending.target
        {
            if now >= current.deadline {
                self.pending = None;
                return current.target;
            }
            pending.deadline = current.deadline;
        }
        evaluation.pending = Some(pending);
        self.pending = evaluation.pending;
        evaluation.visible
    }

    pub(crate) fn take_due_hover_state(
        &mut self,
        now: Instant,
    ) -> Option<ShellContextMenuHoverState> {
        let pending = self.pending?;
        if now < pending.deadline {
            return None;
        }
        self.pending = None;
        Some(pending.target)
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.pending.map(|pending| pending.deadline)
    }
}

struct ContextMenuHoverEvaluation {
    visible: ShellContextMenuHoverState,
    pending: Option<PendingContextMenuHover>,
}

#[cfg(test)]
fn context_menu_hover_state_at(
    menu: &ShellContextMenu,
    previous_pointer: Option<ViewPoint>,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
    now: Instant,
) -> ShellContextMenuHoverState {
    evaluate_context_menu_hover(menu, previous_pointer, point, size, scale, now).visible
}

fn evaluate_context_menu_hover(
    menu: &ShellContextMenu,
    previous_pointer: Option<ViewPoint>,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
    now: Instant,
) -> ContextMenuHoverEvaluation {
    let hovered_submenu_row = menu
        .active_submenu
        .and_then(|submenu| context_submenu_row_at_screen_point(menu, submenu, point, size, scale));
    if hovered_submenu_row.is_some()
        && context_menu_submenu_rect(menu, size, scale).is_some_and(|rect| rect.contains(point))
    {
        return ContextMenuHoverEvaluation {
            visible: ShellContextMenuHoverState {
                hovered_row: menu.active_submenu_row.or(menu.hovered_row),
                active_submenu: menu.active_submenu,
                active_submenu_row: menu.active_submenu_row,
                hovered_submenu_row,
            },
            pending: None,
        };
    }

    let root_items = context_menu_items(menu);
    let target = context_menu_root_hover_target(menu, point, size, scale, &root_items);

    if context_menu_should_delay_submenu_target(
        menu,
        previous_pointer,
        point,
        size,
        scale,
        target.hovered_row,
        &root_items,
    ) {
        let visible = ShellContextMenuHoverState {
            hovered_row: target.hovered_row,
            active_submenu: menu.active_submenu,
            active_submenu_row: menu.active_submenu_row,
            hovered_submenu_row,
        };
        return ContextMenuHoverEvaluation {
            visible,
            pending: Some(PendingContextMenuHover {
                target,
                deadline: now + CONTEXT_MENU_SUBMENU_AIM_DELAY,
            }),
        };
    }

    ContextMenuHoverEvaluation {
        visible: target,
        pending: None,
    }
}

fn context_menu_root_hover_target(
    _menu: &ShellContextMenu,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
    root_items: &[ShellContextMenuItem],
) -> ShellContextMenuHoverState {
    let hovered_row = context_menu_row_at_screen_point(_menu, point, size, scale)
        .filter(|row| *row < root_items.len());
    let active_submenu = hovered_row
        .and_then(|row| root_items.get(row))
        .and_then(|item| item.submenu);
    let active_submenu_row = active_submenu.and(hovered_row);
    ShellContextMenuHoverState {
        hovered_row,
        active_submenu,
        active_submenu_row,
        hovered_submenu_row: None,
    }
}

fn context_menu_should_delay_submenu_target(
    menu: &ShellContextMenu,
    previous_pointer: Option<ViewPoint>,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
    hovered_row: Option<usize>,
    root_items: &[ShellContextMenuItem],
) -> bool {
    if menu.active_submenu.is_none() {
        return false;
    }
    let target_submenu = hovered_row
        .and_then(|row| root_items.get(row))
        .and_then(|item| item.submenu);
    if target_submenu == menu.active_submenu {
        return false;
    }
    context_menu_safe_triangle_contains(menu, previous_pointer, point, size, scale)
        || context_menu_non_submenu_parent_retention_contains(
            menu,
            previous_pointer,
            point,
            size,
            scale,
            hovered_row,
            root_items,
        )
}

fn context_menu_safe_triangle_contains(
    menu: &ShellContextMenu,
    previous_pointer: Option<ViewPoint>,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
) -> bool {
    if menu.active_submenu.is_none() || menu.active_submenu_row.is_none() {
        return false;
    }
    let Some(previous) = previous_pointer else {
        return false;
    };
    let Some(submenu_rect) = context_menu_submenu_rect(menu, size, scale) else {
        return false;
    };
    let root_rect = context_menu_rect_scaled(menu, size, scale);
    let margin = scaled_context_menu_metric(CONTEXT_MENU_SAFE_TRIANGLE_MARGIN, scale);
    let submenu_opens_right = submenu_rect.x >= root_rect.right() - margin;
    let submenu_opens_left = submenu_rect.right() <= root_rect.x + margin;

    if submenu_opens_right {
        if point.x + margin < previous.x {
            return false;
        }
        if point.x >= root_rect.right() - margin
            && point.x <= submenu_rect.x + margin
            && point.y >= submenu_rect.y - margin
            && point.y <= submenu_rect.bottom() + margin
        {
            return true;
        }
        let base_x = submenu_rect.x;
        let top = ViewPoint {
            x: base_x,
            y: submenu_rect.y - margin,
        };
        let bottom = ViewPoint {
            x: base_x,
            y: submenu_rect.bottom() + margin,
        };
        return point_in_triangle(point, previous, top, bottom);
    }

    if submenu_opens_left {
        if point.x - margin > previous.x {
            return false;
        }
        if point.x <= root_rect.x + margin
            && point.x >= submenu_rect.right() - margin
            && point.y >= submenu_rect.y - margin
            && point.y <= submenu_rect.bottom() + margin
        {
            return true;
        }
        let base_x = submenu_rect.right();
        let top = ViewPoint {
            x: base_x,
            y: submenu_rect.y - margin,
        };
        let bottom = ViewPoint {
            x: base_x,
            y: submenu_rect.bottom() + margin,
        };
        return point_in_triangle(point, previous, top, bottom);
    }

    false
}

fn context_menu_non_submenu_parent_retention_contains(
    menu: &ShellContextMenu,
    previous_pointer: Option<ViewPoint>,
    point: ViewPoint,
    size: PhysicalSize<u32>,
    scale: f32,
    hovered_row: Option<usize>,
    root_items: &[ShellContextMenuItem],
) -> bool {
    if menu.active_submenu.is_none() || menu.active_submenu_row.is_none() {
        return false;
    }
    let Some(row) = hovered_row else {
        return false;
    };
    if root_items
        .get(row)
        .is_none_or(|item| item.submenu.is_some())
    {
        return false;
    }
    let Some(previous) = previous_pointer else {
        return false;
    };
    let Some(submenu_rect) = context_menu_submenu_rect(menu, size, scale) else {
        return false;
    };
    let root_rect = context_menu_rect_scaled(menu, size, scale);
    if !root_rect.contains(point) || !root_rect.contains(previous) {
        return false;
    }

    let margin = scaled_context_menu_metric(CONTEXT_MENU_SAFE_TRIANGLE_MARGIN, scale);
    let retention_width =
        scaled_context_menu_metric(CONTEXT_MENU_SAFE_TRIANGLE_PARENT_RETENTION_WIDTH, scale)
            .min(root_rect.width);
    let submenu_opens_right = submenu_rect.x >= root_rect.right() - margin;
    let submenu_opens_left = submenu_rect.right() <= root_rect.x + margin;

    if submenu_opens_right {
        let retention_left = root_rect.right() - retention_width;
        return point.x >= retention_left && previous.x >= retention_left;
    }
    if submenu_opens_left {
        let retention_right = root_rect.x + retention_width;
        return point.x <= retention_right && previous.x <= retention_right;
    }
    false
}

fn point_in_triangle(point: ViewPoint, a: ViewPoint, b: ViewPoint, c: ViewPoint) -> bool {
    let d1 = triangle_sign(point, a, b);
    let d2 = triangle_sign(point, b, c);
    let d3 = triangle_sign(point, c, a);
    let has_negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_negative && has_positive)
}

fn triangle_sign(p1: ViewPoint, p2: ViewPoint, p3: ViewPoint) -> f32 {
    (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::ui::context_menu::{
        ShellContextMenu, ShellContextSubmenu, ShellContextTarget, context_menu_items,
    };
    use crate::ui::metrics::{
        CONTEXT_MENU_ROW_HEIGHT, CONTEXT_MENU_SUBMENU_AIM_DELAY, CONTEXT_MENU_VERTICAL_PADDING,
    };
    use crate::ui::pane::ShellPaneId;

    fn blank_menu_with_create_new_submenu() -> ShellContextMenu {
        let mut menu = ShellContextMenu::new(
            ShellContextTarget::Blank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 24.0, y: 24.0 },
        );
        menu.hovered_row = Some(0);
        menu.active_submenu = Some(ShellContextSubmenu::CreateNew);
        menu.active_submenu_row = Some(0);
        menu
    }

    fn item_menu_with_open_with_submenu() -> ShellContextMenu {
        let mut menu = ShellContextMenu::new(
            ShellContextTarget::Item {
                pane: ShellPaneId::SLOT_0,
                index: 0,
                path: PathBuf::from("/tmp/plain.txt"),
                is_dir: false,
                selection_count: 1,
            },
            ViewPoint { x: 24.0, y: 24.0 },
        );
        menu.hovered_row = Some(0);
        menu.active_submenu = Some(ShellContextSubmenu::OpenWith);
        menu.active_submenu_row = Some(0);
        menu
    }

    fn row_center(menu: &ShellContextMenu, row: usize, size: PhysicalSize<u32>) -> ViewPoint {
        let rect = context_menu_rect_scaled(menu, size, 1.0);
        ViewPoint {
            x: rect.right() - 6.0,
            y: rect.y
                + CONTEXT_MENU_VERTICAL_PADDING
                + CONTEXT_MENU_ROW_HEIGHT * row as f32
                + CONTEXT_MENU_ROW_HEIGHT / 2.0,
        }
    }

    #[test]
    fn safe_triangle_highlights_crossed_root_row_while_delaying_submenu_switch() {
        let menu = blank_menu_with_create_new_submenu();
        let size = PhysicalSize::new(640, 420);
        let previous = row_center(&menu, 0, size);
        let current = row_center(&menu, 1, size);
        let now = Instant::now();

        let state = context_menu_hover_state_at(&menu, Some(previous), current, size, 1.0, now);

        assert_eq!(state.hovered_row, Some(1));
        assert_eq!(state.active_submenu, Some(ShellContextSubmenu::CreateNew));
        assert_eq!(state.active_submenu_row, Some(0));
        assert_eq!(state.hovered_submenu_row, None);
    }

    #[test]
    fn safe_triangle_highlights_non_submenu_parent_row_while_delaying_close() {
        let menu = item_menu_with_open_with_submenu();
        let size = PhysicalSize::new(640, 420);
        let root = context_menu_rect_scaled(&menu, size, 1.0);
        let previous = row_center(&menu, 0, size);
        let current = ViewPoint {
            x: root.right() - 48.0,
            y: row_center(&menu, 1, size).y,
        };
        let now = Instant::now();

        let state = context_menu_hover_state_at(&menu, Some(previous), current, size, 1.0, now);

        assert_eq!(state.hovered_row, Some(1));
        assert_eq!(state.active_submenu, Some(ShellContextSubmenu::OpenWith));
        assert_eq!(state.active_submenu_row, Some(0));
        assert_eq!(state.hovered_submenu_row, None);
    }

    #[test]
    fn safe_triangle_commits_delayed_non_submenu_close_after_deadline() {
        let menu = item_menu_with_open_with_submenu();
        let size = PhysicalSize::new(640, 420);
        let root = context_menu_rect_scaled(&menu, size, 1.0);
        let previous = row_center(&menu, 0, size);
        let current = ViewPoint {
            x: root.right() - 48.0,
            y: row_center(&menu, 1, size).y,
        };
        let now = Instant::now();
        let mut runtime = ShellContextMenuSafeTriangleRuntime {
            previous_pointer: Some(previous),
            pending: None,
        };

        let visible = runtime.hover_state_at(&menu, current, size, 1.0, now);
        let due = runtime
            .take_due_hover_state(now + CONTEXT_MENU_SUBMENU_AIM_DELAY)
            .expect("same delayed target should be committed at its deadline");

        assert_eq!(visible.hovered_row, Some(1));
        assert_eq!(visible.active_submenu, Some(ShellContextSubmenu::OpenWith));
        assert_eq!(visible.active_submenu_row, Some(0));
        assert_eq!(due.hovered_row, Some(1));
        assert_eq!(due.active_submenu, None);
        assert_eq!(due.active_submenu_row, None);
    }

    #[test]
    fn safe_triangle_closes_submenu_when_non_submenu_parent_hover_moves_away() {
        let menu = item_menu_with_open_with_submenu();
        let size = PhysicalSize::new(640, 420);
        let root = context_menu_rect_scaled(&menu, size, 1.0);
        let previous = row_center(&menu, 0, size);
        let current = ViewPoint {
            x: root.x + 12.0,
            y: row_center(&menu, 1, size).y,
        };

        let state =
            context_menu_hover_state_at(&menu, Some(previous), current, size, 1.0, Instant::now());

        assert_eq!(state.hovered_row, Some(1));
        assert_eq!(state.active_submenu, None);
        assert_eq!(state.active_submenu_row, None);
        assert_eq!(state.hovered_submenu_row, None);
    }

    #[test]
    fn safe_triangle_does_not_block_pointer_moving_away_from_submenu() {
        let menu = blank_menu_with_create_new_submenu();
        let size = PhysicalSize::new(640, 420);
        let previous = row_center(&menu, 0, size);
        let current = ViewPoint {
            x: context_menu_rect_scaled(&menu, size, 1.0).x + 12.0,
            y: row_center(&menu, 1, size).y,
        };

        let state =
            context_menu_hover_state_at(&menu, Some(previous), current, size, 1.0, Instant::now());
        let items = context_menu_items(&menu);
        let item = items.get(1).expect("blank menu row 1 should exist");

        assert_eq!(state.hovered_row, Some(1));
        assert_eq!(state.active_submenu, item.submenu);
        assert_eq!(state.active_submenu_row, item.submenu.map(|_| 1));
        assert_eq!(state.hovered_submenu_row, None);
    }

    #[test]
    fn submenu_hit_testing_still_updates_submenu_row_inside_safe_triangle() {
        let menu = blank_menu_with_create_new_submenu();
        let size = PhysicalSize::new(640, 420);
        let previous = row_center(&menu, 0, size);
        let submenu =
            context_menu_submenu_rect(&menu, size, 1.0).expect("create new submenu should layout");
        let current = ViewPoint {
            x: submenu.x + 12.0,
            y: submenu.y + CONTEXT_MENU_VERTICAL_PADDING + CONTEXT_MENU_ROW_HEIGHT * 1.5,
        };

        let state =
            context_menu_hover_state_at(&menu, Some(previous), current, size, 1.0, Instant::now());

        assert_eq!(state.hovered_row, Some(0));
        assert_eq!(state.active_submenu, Some(ShellContextSubmenu::CreateNew));
        assert_eq!(state.active_submenu_row, Some(0));
        assert_eq!(state.hovered_submenu_row, Some(1));
    }
}
