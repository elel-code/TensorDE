use tensor_util::{Rect, Size};

use super::model::{LayoutItem, LayoutOptions, LayoutPlacement, LayoutSnapshot, LayoutState};

pub(super) fn arrange(
    options: LayoutOptions,
    state: &mut LayoutState,
    area: Rect,
    items: &[LayoutItem],
    focused: Option<usize>,
) -> LayoutSnapshot {
    if items.is_empty() {
        state.reset_horizontal();
        return LayoutSnapshot::empty(area, *state);
    }

    let padding = options.gap.min(area.width / 2);
    let inner_height = area.height.saturating_sub(padding.saturating_mul(2));
    let mut x = add_offset(area.x, padding);
    let mut base = Vec::with_capacity(items.len());
    let mut max_height = 0;
    for item in items {
        let requested_width = item
            .primary_size
            .unwrap_or(options.scrolling_default_width)
            .resolve_column(area.width, options.gap);
        let size = item
            .constraints
            .constrain(Size::new(requested_width, inner_height));
        let y = center_axis(add_offset(area.y, padding), inner_height, size.height);
        base.push(Rect::new(x, y, size.width, size.height));
        x = add_offset(add_offset(x, size.width), options.gap);
        max_height = max_height.max(size.height);
    }

    let content_width = distance(area.x, add_offset(x, padding.saturating_sub(options.gap)));
    update_view_offset(state, area, padding, content_width, &base, focused);

    let placements = base
        .into_iter()
        .map(|geometry| {
            let geometry = Rect::new(
                geometry.x.saturating_add(state.horizontal_offset),
                geometry.y,
                geometry.width,
                geometry.height,
            );
            LayoutPlacement::new(geometry, area)
        })
        .collect();
    LayoutSnapshot {
        viewport: area,
        placements,
        content_bounds: Rect::new(
            area.x.saturating_add(state.horizontal_offset),
            area.y,
            content_width,
            max_height.saturating_add(padding.saturating_mul(2)),
        ),
        horizontal_offset: state.horizontal_offset,
    }
}

fn update_view_offset(
    state: &mut LayoutState,
    area: Rect,
    padding: u32,
    content_width: u32,
    base: &[Rect],
    focused: Option<usize>,
) {
    if content_width <= area.width {
        state.reset_horizontal();
        return;
    }

    let view_left = i64::from(add_offset(area.x, padding));
    let view_right = i64::from(area.right().saturating_sub(to_i32(padding)));
    let content_right = i64::from(area.x) + i64::from(content_width) - i64::from(padding);
    let minimum = view_right.saturating_sub(content_right).min(0);
    let mut offset = i64::from(state.horizontal_offset).clamp(minimum, 0);

    if let Some(focused) = focused.and_then(|index| base.get(index)) {
        let left = i64::from(focused.x) + offset;
        let right = i64::from(focused.right()) + offset;
        let view_width = view_right.saturating_sub(view_left);
        if i64::from(focused.width) >= view_width {
            offset = view_left.saturating_sub(i64::from(focused.x));
        } else if left < view_left {
            offset = offset.saturating_add(view_left - left);
        } else if right > view_right {
            offset = offset.saturating_sub(right - view_right);
        }
    }

    state.horizontal_offset = clamp_i64(offset.clamp(minimum, 0));
}

fn center_axis(origin: i32, available: u32, size: u32) -> i32 {
    add_offset(origin, available.saturating_sub(size) / 2)
}

fn add_offset(origin: i32, amount: u32) -> i32 {
    origin.saturating_add(to_i32(amount))
}

fn distance(left: i32, right: i32) -> u32 {
    u32::try_from(right.saturating_sub(left)).unwrap_or(u32::MAX)
}

fn to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn clamp_i64(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutLength, SizeConstraints};

    const VIEW: Rect = Rect::new(0, 0, 100, 80);

    #[test]
    fn focus_scrolls_only_enough_to_reveal_the_target_column() {
        let mut state = LayoutState::default();
        let items = [LayoutItem::default(); 3];

        let initial = arrange(LayoutOptions::default(), &mut state, VIEW, &items, Some(0));
        assert_eq!(initial.horizontal_offset, 0);
        assert_eq!(initial.placements[0].geometry, Rect::new(8, 8, 38, 64));
        assert_eq!(initial.placements[2].visible, None);

        let focused = arrange(LayoutOptions::default(), &mut state, VIEW, &items, Some(2));
        assert_eq!(focused.horizontal_offset, -46);
        assert_eq!(focused.placements[2].geometry, Rect::new(54, 8, 38, 64));
        assert_eq!(
            focused.placements[2].visible,
            Some(Rect::new(54, 8, 38, 64))
        );
    }

    #[test]
    fn oversized_focused_column_is_left_aligned() {
        let mut state = LayoutState::default();
        let item = LayoutItem::new(SizeConstraints::default(), Some(LayoutLength::fixed(150)));

        let snapshot = arrange(LayoutOptions::default(), &mut state, VIEW, &[item], Some(0));

        assert_eq!(snapshot.horizontal_offset, 0);
        assert_eq!(snapshot.placements[0].geometry.x, 8);
        assert_eq!(
            snapshot.placements[0].visible,
            Some(Rect::new(8, 8, 92, 64))
        );
    }

    #[test]
    fn maximum_height_centers_a_short_window_without_losing_its_clip() {
        let mut state = LayoutState::default();
        let item = LayoutItem::new(SizeConstraints::new(Size::new(1, 1), None, Some(20)), None);

        let snapshot = arrange(LayoutOptions::default(), &mut state, VIEW, &[item], None);

        assert_eq!(snapshot.placements[0].geometry, Rect::new(8, 30, 38, 20));
        assert_eq!(
            snapshot.placements[0].visible,
            Some(Rect::new(8, 30, 38, 20))
        );
    }
}
