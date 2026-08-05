//! XDG popup positioner state and constraint adjustment.
//!
//! Placement follows the stable xdg-shell rules. Arithmetic saturates at the
//! compositor coordinate boundary so hostile protocol coordinates cannot wrap.

use tensor_util::{Point, Rect, Size};
use wayland_protocols::xdg::shell::server::xdg_positioner::{
    Anchor, ConstraintAdjustment, Gravity,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol) struct PositionerState {
    pub(in crate::protocol) size: Size,
    pub(in crate::protocol) anchor_rect: Rect,
    pub(in crate::protocol) anchor: Anchor,
    pub(in crate::protocol) gravity: Gravity,
    pub(in crate::protocol) adjustment: ConstraintAdjustment,
    pub(in crate::protocol) offset: Point,
    pub(in crate::protocol) reactive: bool,
    pub(in crate::protocol) parent_size: Option<Size>,
    pub(in crate::protocol) parent_configure: Option<u32>,
}

impl Default for PositionerState {
    fn default() -> Self {
        Self {
            size: Size::default(),
            anchor_rect: Rect::default(),
            anchor: Anchor::None,
            gravity: Gravity::None,
            adjustment: ConstraintAdjustment::empty(),
            offset: Point::default(),
            reactive: false,
            parent_size: None,
            parent_configure: None,
        }
    }
}

impl PositionerState {
    pub(in crate::protocol) fn complete(self) -> bool {
        self.size.width > 0
            && self.size.height > 0
            && self.anchor_rect.width > 0
            && self.anchor_rect.height > 0
    }

    fn anchor_has(self, edge: Anchor) -> bool {
        matches!(
            (edge, self.anchor),
            (
                Anchor::Top,
                Anchor::Top | Anchor::TopLeft | Anchor::TopRight
            ) | (
                Anchor::Bottom,
                Anchor::Bottom | Anchor::BottomLeft | Anchor::BottomRight
            ) | (
                Anchor::Left,
                Anchor::Left | Anchor::TopLeft | Anchor::BottomLeft
            ) | (
                Anchor::Right,
                Anchor::Right | Anchor::TopRight | Anchor::BottomRight
            )
        )
    }

    fn gravity_has(self, edge: Gravity) -> bool {
        matches!(
            (edge, self.gravity),
            (
                Gravity::Top,
                Gravity::Top | Gravity::TopLeft | Gravity::TopRight
            ) | (
                Gravity::Bottom,
                Gravity::Bottom | Gravity::BottomLeft | Gravity::BottomRight
            ) | (
                Gravity::Left,
                Gravity::Left | Gravity::TopLeft | Gravity::BottomLeft
            ) | (
                Gravity::Right,
                Gravity::Right | Gravity::TopRight | Gravity::BottomRight
            )
        )
    }

    pub(in crate::protocol) fn geometry(self) -> Rect {
        let anchor_x = if self.anchor_has(Anchor::Left) {
            self.anchor_rect.x
        } else if self.anchor_has(Anchor::Right) {
            add(self.anchor_rect.x, axis(self.anchor_rect.width))
        } else {
            add(self.anchor_rect.x, axis(self.anchor_rect.width / 2))
        };
        let anchor_y = if self.anchor_has(Anchor::Top) {
            self.anchor_rect.y
        } else if self.anchor_has(Anchor::Bottom) {
            add(self.anchor_rect.y, axis(self.anchor_rect.height))
        } else {
            add(self.anchor_rect.y, axis(self.anchor_rect.height / 2))
        };

        let mut x = add(anchor_x, self.offset.x);
        let mut y = add(anchor_y, self.offset.y);
        if self.gravity_has(Gravity::Top) {
            xdg_sub_assign(&mut y, axis(self.size.height));
        } else if !self.gravity_has(Gravity::Bottom) {
            xdg_sub_assign(&mut y, axis(self.size.height / 2));
        }
        if self.gravity_has(Gravity::Left) {
            xdg_sub_assign(&mut x, axis(self.size.width));
        } else if !self.gravity_has(Gravity::Right) {
            xdg_sub_assign(&mut x, axis(self.size.width / 2));
        }
        Rect::new(x, y, self.size.width, self.size.height)
    }

    pub(in crate::protocol) fn constrained_geometry(mut self, target: Rect) -> Rect {
        let mut geometry = self.geometry();
        let mut offsets = Offsets::new(target, geometry);

        if offsets.horizontal() && self.adjustment.contains(ConstraintAdjustment::FlipX) {
            let mut flipped = self;
            flipped.anchor = invert_anchor_x(flipped.anchor);
            flipped.gravity = invert_gravity_x(flipped.gravity);
            let candidate = flipped.geometry();
            let candidate_offsets = Offsets::new(target, candidate);
            if !candidate_offsets.horizontal() {
                self = flipped;
                geometry = candidate;
                offsets.left = 0;
                offsets.right = 0;
            }
        }
        if offsets.vertical() && self.adjustment.contains(ConstraintAdjustment::FlipY) {
            let mut flipped = self;
            flipped.anchor = invert_anchor_y(flipped.anchor);
            flipped.gravity = invert_gravity_y(flipped.gravity);
            let candidate = flipped.geometry();
            let candidate_offsets = Offsets::new(target, candidate);
            if !candidate_offsets.vertical() {
                geometry = candidate;
                offsets.top = 0;
                offsets.bottom = 0;
            }
        }

        if offsets.horizontal() && self.adjustment.contains(ConstraintAdjustment::SlideX) {
            if offsets.left > 0 {
                geometry.x = add_i64(geometry.x, offsets.left);
            } else if offsets.right > 0 {
                geometry.x = add_i64(geometry.x, -offsets.right.min(-offsets.left));
            }
            let next = Offsets::new(target, geometry);
            offsets.left = next.left;
            offsets.right = next.right;
        }
        if offsets.vertical() && self.adjustment.contains(ConstraintAdjustment::SlideY) {
            if offsets.top > 0 {
                geometry.y = add_i64(geometry.y, offsets.top);
            } else if offsets.bottom > 0 {
                geometry.y = add_i64(geometry.y, -offsets.bottom.min(-offsets.top));
            }
            let next = Offsets::new(target, geometry);
            offsets.top = next.top;
            offsets.bottom = next.bottom;
        }

        if self.adjustment.contains(ConstraintAdjustment::ResizeX) {
            if offsets.left > 0 && offsets.left < i64::from(geometry.width) {
                let delta = offsets.left as u32;
                geometry.x = add(geometry.x, axis(delta));
                geometry.width -= delta;
            }
            if offsets.right > 0 && offsets.right < i64::from(geometry.width) {
                geometry.width -= offsets.right as u32;
            }
        }
        if self.adjustment.contains(ConstraintAdjustment::ResizeY) {
            if offsets.top > 0 && offsets.top < i64::from(geometry.height) {
                let delta = offsets.top as u32;
                geometry.y = add(geometry.y, axis(delta));
                geometry.height -= delta;
            }
            if offsets.bottom > 0 && offsets.bottom < i64::from(geometry.height) {
                geometry.height -= offsets.bottom as u32;
            }
        }
        geometry
    }
}

#[derive(Clone, Copy)]
struct Offsets {
    left: i64,
    right: i64,
    top: i64,
    bottom: i64,
}

impl Offsets {
    fn new(target: Rect, popup: Rect) -> Self {
        let target_right = i64::from(target.x) + i64::from(target.width);
        let target_bottom = i64::from(target.y) + i64::from(target.height);
        let popup_right = i64::from(popup.x) + i64::from(popup.width);
        let popup_bottom = i64::from(popup.y) + i64::from(popup.height);
        Self {
            left: i64::from(target.x) - i64::from(popup.x),
            right: popup_right - target_right,
            top: i64::from(target.y) - i64::from(popup.y),
            bottom: popup_bottom - target_bottom,
        }
    }

    fn horizontal(self) -> bool {
        self.left > 0 || self.right > 0
    }

    fn vertical(self) -> bool {
        self.top > 0 || self.bottom > 0
    }
}

fn add(left: i32, right: i32) -> i32 {
    left.saturating_add(right)
}

fn axis(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn add_i64(value: i32, delta: i64) -> i32 {
    (i64::from(value) + delta).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn xdg_sub_assign(value: &mut i32, amount: i32) {
    *value = value.saturating_sub(amount);
}

fn invert_anchor_x(value: Anchor) -> Anchor {
    match value {
        Anchor::Left => Anchor::Right,
        Anchor::Right => Anchor::Left,
        Anchor::TopLeft => Anchor::TopRight,
        Anchor::TopRight => Anchor::TopLeft,
        Anchor::BottomLeft => Anchor::BottomRight,
        Anchor::BottomRight => Anchor::BottomLeft,
        other => other,
    }
}

fn invert_anchor_y(value: Anchor) -> Anchor {
    match value {
        Anchor::Top => Anchor::Bottom,
        Anchor::Bottom => Anchor::Top,
        Anchor::TopLeft => Anchor::BottomLeft,
        Anchor::TopRight => Anchor::BottomRight,
        Anchor::BottomLeft => Anchor::TopLeft,
        Anchor::BottomRight => Anchor::TopRight,
        other => other,
    }
}

fn invert_gravity_x(value: Gravity) -> Gravity {
    match value {
        Gravity::Left => Gravity::Right,
        Gravity::Right => Gravity::Left,
        Gravity::TopLeft => Gravity::TopRight,
        Gravity::TopRight => Gravity::TopLeft,
        Gravity::BottomLeft => Gravity::BottomRight,
        Gravity::BottomRight => Gravity::BottomLeft,
        other => other,
    }
}

fn invert_gravity_y(value: Gravity) -> Gravity {
    match value {
        Gravity::Top => Gravity::Bottom,
        Gravity::Bottom => Gravity::Top,
        Gravity::TopLeft => Gravity::BottomLeft,
        Gravity::TopRight => Gravity::BottomRight,
        Gravity::BottomLeft => Gravity::TopLeft,
        Gravity::BottomRight => Gravity::TopRight,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positioner_requires_both_positive_rectangles() {
        let mut state = PositionerState::default();
        assert!(!state.complete());
        state.size = Size::new(10, 20);
        state.anchor_rect = Rect::new(5, 5, 1, 1);
        assert!(state.complete());
    }

    #[test]
    fn placement_saturates_protocol_coordinates() {
        let state = PositionerState {
            size: Size::new(10, 10),
            anchor_rect: Rect::new(i32::MAX, i32::MAX, 10, 10),
            anchor: Anchor::BottomRight,
            gravity: Gravity::BottomRight,
            offset: Point::new(10, 10),
            ..PositionerState::default()
        };
        assert_eq!(
            (state.geometry().x, state.geometry().y),
            (i32::MAX, i32::MAX)
        );
    }
}
