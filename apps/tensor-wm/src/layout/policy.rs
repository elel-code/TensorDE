use tensor_util::{Rect, Size};

pub use tensor_ipc::land::{LayoutKind, ParseLayoutError};

use super::{
    model::{LayoutItem, LayoutOptions, LayoutPlacement, LayoutSnapshot, LayoutState},
    scrolling,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutEngine {
    kind: LayoutKind,
    options: LayoutOptions,
}

impl LayoutEngine {
    pub fn new(kind: LayoutKind) -> Self {
        Self {
            kind,
            options: LayoutOptions::default(),
        }
    }

    pub const fn with_options(kind: LayoutKind, options: LayoutOptions) -> Self {
        Self { kind, options }
    }

    pub const fn kind(self) -> LayoutKind {
        self.kind
    }

    pub const fn options(self) -> LayoutOptions {
        self.options
    }

    pub fn arrange(
        self,
        state: &mut LayoutState,
        area: Rect,
        items: &[LayoutItem],
        focused: Option<usize>,
    ) -> LayoutSnapshot {
        match self.kind {
            LayoutKind::Scrolling1D => {
                scrolling::arrange(self.options, state, area, items, focused)
            }
            LayoutKind::Spatial2D => {
                state.reset_horizontal();
                arrange_grid(self.options, *state, area, items)
            }
            LayoutKind::MasterStack => {
                state.reset_horizontal();
                arrange_master_stack(self.options, *state, area, items)
            }
        }
    }
}

fn arrange_grid(
    options: LayoutOptions,
    state: LayoutState,
    area: Rect,
    items: &[LayoutItem],
) -> LayoutSnapshot {
    if items.is_empty() {
        return LayoutSnapshot::empty(area, state);
    }

    let columns = integer_ceil_sqrt(items.len());
    let rows = items.len().div_ceil(columns);
    let widths = split_tracks(area.width, columns, options.gap);
    let heights = split_tracks(area.height, rows, options.gap);
    let mut placements = Vec::with_capacity(items.len());
    let mut y = add_offset(area.y, options.gap);
    for height in heights {
        let mut x = add_offset(area.x, options.gap);
        for &width in &widths {
            if placements.len() == items.len() {
                return fixed_snapshot(area, state, placements);
            }
            let cell = Rect::new(x, y, width, height);
            placements.push(place_in_cell(
                cell,
                area,
                items[placements.len()].constraints.constrain(cell.size()),
            ));
            x = add_offset(add_offset(x, width), options.gap);
        }
        y = add_offset(add_offset(y, height), options.gap);
    }
    fixed_snapshot(area, state, placements)
}

fn arrange_master_stack(
    options: LayoutOptions,
    state: LayoutState,
    area: Rect,
    items: &[LayoutItem],
) -> LayoutSnapshot {
    if items.is_empty() {
        return LayoutSnapshot::empty(area, state);
    }

    let inner = inset(area, options.gap);
    if items.len() == 1 {
        let size = items[0].constraints.constrain(inner.size());
        return fixed_snapshot(area, state, vec![place_in_cell(inner, area, size)]);
    }

    let tracks_width = inner.width.saturating_sub(options.gap);
    let master_width = options.master_width.resolve(tracks_width).min(tracks_width);
    let stack_width = tracks_width.saturating_sub(master_width);
    let mut placements = Vec::with_capacity(items.len());
    let master_cell = Rect::new(inner.x, inner.y, master_width, inner.height);
    placements.push(place_in_cell(
        master_cell,
        area,
        items[0].constraints.constrain(master_cell.size()),
    ));

    let stack_x = add_offset(add_offset(inner.x, master_width), options.gap);
    let stack_heights = split_tracks(inner.height, items.len() - 1, options.gap);
    let mut y = inner.y;
    for (item, height) in items[1..].iter().zip(stack_heights) {
        let cell = Rect::new(stack_x, y, stack_width, height);
        placements.push(place_in_cell(
            cell,
            area,
            item.constraints.constrain(cell.size()),
        ));
        y = add_offset(add_offset(y, height), options.gap);
    }
    fixed_snapshot(area, state, placements)
}

fn fixed_snapshot(
    area: Rect,
    state: LayoutState,
    placements: Vec<LayoutPlacement>,
) -> LayoutSnapshot {
    LayoutSnapshot {
        viewport: area,
        placements,
        content_bounds: area,
        horizontal_offset: state.horizontal_offset,
    }
}

fn place_in_cell(cell: Rect, viewport: Rect, size: Size) -> LayoutPlacement {
    let x = add_offset(cell.x, cell.width.saturating_sub(size.width) / 2);
    let y = add_offset(cell.y, cell.height.saturating_sub(size.height) / 2);
    LayoutPlacement::new(Rect::new(x, y, size.width, size.height), viewport)
}

fn inset(area: Rect, gap: u32) -> Rect {
    Rect::new(
        add_offset(area.x, gap),
        add_offset(area.y, gap),
        area.width.saturating_sub(gap.saturating_mul(2)),
        area.height.saturating_sub(gap.saturating_mul(2)),
    )
}

fn split_tracks(length: u32, count: usize, gap: u32) -> Vec<u32> {
    let gaps = u32::try_from(count.saturating_add(1))
        .unwrap_or(u32::MAX)
        .saturating_mul(gap);
    tensor_util::split_evenly(length.saturating_sub(gaps), count)
}

fn integer_ceil_sqrt(value: usize) -> usize {
    let mut root: usize = 1;
    while root.saturating_mul(root) < value {
        root += 1;
    }
    root
}

fn add_offset(origin: i32, amount: u32) -> i32 {
    origin.saturating_add(i32::try_from(amount).unwrap_or(i32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutLength, SizeConstraints};

    const OUTPUT: Rect = Rect::new(0, 0, 1920, 1080);

    fn items(count: usize) -> Vec<LayoutItem> {
        vec![LayoutItem::default(); count]
    }

    #[test]
    fn empty_layout_has_no_placements() {
        for kind in [
            LayoutKind::Scrolling1D,
            LayoutKind::Spatial2D,
            LayoutKind::MasterStack,
        ] {
            let mut state = LayoutState::default();
            assert!(
                LayoutEngine::new(kind)
                    .arrange(&mut state, OUTPUT, &[], None)
                    .placements
                    .is_empty()
            );
        }
    }

    #[test]
    fn serialized_names_match_configuration_names() {
        assert_eq!(
            serde_json::to_string(&LayoutKind::Scrolling1D).unwrap(),
            "\"scrolling-1d\""
        );
        assert_eq!(
            serde_json::to_string(&LayoutKind::Spatial2D).unwrap(),
            "\"spatial-2d\""
        );
    }

    #[test]
    fn scrolling_layout_uses_persistent_focused_view_offset() {
        let mut state = LayoutState::default();
        let engine = LayoutEngine::new(LayoutKind::Scrolling1D);
        let first = engine.arrange(&mut state, Rect::new(0, 0, 100, 80), &items(3), Some(0));
        let last = engine.arrange(&mut state, Rect::new(0, 0, 100, 80), &items(3), Some(2));

        assert_eq!(first.placements[0].geometry, Rect::new(8, 8, 38, 64));
        assert_eq!(last.horizontal_offset, -46);
        assert!(last.placements[2].visible.is_some());
    }

    #[test]
    fn spatial_layout_builds_a_gapped_compact_grid() {
        let mut state = LayoutState::default();
        let snapshot =
            LayoutEngine::new(LayoutKind::Spatial2D).arrange(&mut state, OUTPUT, &items(4), None);

        assert_eq!(snapshot.placements[0].geometry, Rect::new(8, 8, 948, 528));
        assert_eq!(
            snapshot.placements[3].geometry,
            Rect::new(964, 544, 948, 528)
        );
    }

    #[test]
    fn master_stack_layout_uses_configured_ratio() {
        let mut state = LayoutState::default();
        let snapshot =
            LayoutEngine::new(LayoutKind::MasterStack).arrange(&mut state, OUTPUT, &items(3), None);

        assert_eq!(snapshot.placements[0].geometry, Rect::new(8, 8, 1042, 1064));
        assert_eq!(
            snapshot.placements[1].geometry,
            Rect::new(1058, 8, 854, 520)
        );
        assert_eq!(
            snapshot.placements[2].geometry,
            Rect::new(1058, 536, 854, 520)
        );
    }

    #[test]
    fn item_constraints_are_applied_inside_spatial_cells() {
        let mut state = LayoutState::default();
        let item = LayoutItem::new(
            SizeConstraints::new(Size::new(1, 1), Some(200), Some(100)),
            Some(LayoutLength::fixed(900)),
        );
        let snapshot = LayoutEngine::new(LayoutKind::Spatial2D).arrange(
            &mut state,
            Rect::new(0, 0, 500, 300),
            &[item],
            None,
        );

        assert_eq!(
            snapshot.placements[0].geometry,
            Rect::new(150, 100, 200, 100)
        );
    }

    #[test]
    fn non_scrolling_layout_resets_horizontal_state() {
        let mut state = LayoutState {
            horizontal_offset: -300,
        };

        let snapshot =
            LayoutEngine::new(LayoutKind::Spatial2D).arrange(&mut state, OUTPUT, &items(1), None);

        assert_eq!(snapshot.horizontal_offset, 0);
        assert_eq!(state.horizontal_offset, 0);
    }
}
