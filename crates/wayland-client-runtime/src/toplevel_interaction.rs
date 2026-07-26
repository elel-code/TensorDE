#![allow(dead_code)] // Press-tracker helpers retained for interactive grab parity.
use std::collections::HashMap;

use wayland_client::protocol::wl_seat::WlSeat;
use wayland_protocols::xdg::shell::client::xdg_toplevel::{
    ResizeEdge as WireResizeEdge, XdgToplevel,
};

use crate::geometry::LogicalPosition;
use crate::surface::SurfaceId;

/// Edge or corner used for an interactive compositor-driven resize.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl From<ResizeEdge> for WireResizeEdge {
    fn from(value: ResizeEdge) -> Self {
        match value {
            ResizeEdge::Top => Self::Top,
            ResizeEdge::Bottom => Self::Bottom,
            ResizeEdge::Left => Self::Left,
            ResizeEdge::Right => Self::Right,
            ResizeEdge::TopLeft => Self::TopLeft,
            ResizeEdge::TopRight => Self::TopRight,
            ResizeEdge::BottomLeft => Self::BottomLeft,
            ResizeEdge::BottomRight => Self::BottomRight,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PointerPress {
    pub(crate) surface: SurfaceId,
    pub(crate) serial: u32,
    pub(crate) order: u64,
}

/// Tracks only pointer presses whose implicit grab is still active.
///
/// A seat may have several buttons down at once, so retaining only the latest
/// serial loses a still-valid older grab as soon as the newer button is
/// released. The button-keyed representation also makes release O(1).
#[derive(Default)]
pub(crate) struct PointerPressTracker {
    presses: HashMap<u32, PointerPress>,
}

impl PointerPressTracker {
    pub(crate) fn press(&mut self, button: u32, surface: SurfaceId, serial: u32, order: u64) {
        self.presses.insert(
            button,
            PointerPress {
                surface,
                serial,
                order,
            },
        );
    }

    pub(crate) fn release(&mut self, button: u32) {
        self.presses.remove(&button);
    }

    pub(crate) fn latest_for_surface(&self, surface: SurfaceId) -> Option<PointerPress> {
        self.presses
            .values()
            .filter(|press| press.surface == surface)
            .max_by_key(|press| press.order)
            .copied()
    }

    pub(crate) fn contains_serial(&self, serial: u32) -> bool {
        self.presses.values().any(|press| press.serial == serial)
    }

    pub(crate) fn remove_surface(&mut self, surface: SurfaceId) {
        self.presses.retain(|_, press| press.surface != surface);
    }

    pub(crate) fn clear(&mut self) {
        self.presses.clear();
    }
}

pub(crate) fn select_active_pointer_press<T>(
    surface: SurfaceId,
    candidates: impl IntoIterator<Item = (T, Option<SurfaceId>, bool, Option<PointerPress>)>,
) -> Option<(T, PointerPress)> {
    candidates
        .into_iter()
        .filter_map(|(context, pointer_focus, enabled, press)| {
            let press = press?;
            (enabled && pointer_focus == Some(surface) && press.surface == surface)
                .then_some((context, press))
        })
        .max_by_key(|(_, press)| press.order)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ToplevelInteraction {
    Move,
    Resize(ResizeEdge),
    WindowMenu(LogicalPosition),
}

impl ToplevelInteraction {
    pub(crate) fn send(self, toplevel: &XdgToplevel, seat: &WlSeat, serial: u32) {
        match self {
            Self::Move => toplevel._move(seat, serial),
            Self::Resize(edge) => toplevel.resize(seat, serial, edge.into()),
            Self::WindowMenu(position) => {
                toplevel.show_window_menu(seat, serial, position.x, position.y)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_restores_older_active_press() {
        let surface = SurfaceId(7);
        let mut tracker = PointerPressTracker::default();
        tracker.press(0x110, surface, 10, 1);
        tracker.press(0x111, surface, 20, 2);

        assert_eq!(tracker.latest_for_surface(surface).unwrap().serial, 20);
        tracker.release(0x111);
        assert_eq!(tracker.latest_for_surface(surface).unwrap().serial, 10);
        assert!(tracker.contains_serial(10));
        assert!(!tracker.contains_serial(20));
    }

    #[test]
    fn removing_surface_preserves_other_surface_presses() {
        let removed = SurfaceId(1);
        let retained = SurfaceId(2);
        let mut tracker = PointerPressTracker::default();
        tracker.press(0x110, removed, 10, 1);
        tracker.press(0x111, retained, 20, 2);

        tracker.remove_surface(removed);

        assert_eq!(tracker.latest_for_surface(removed), None);
        assert_eq!(tracker.latest_for_surface(retained).unwrap().serial, 20);
    }

    #[test]
    fn selection_requires_focus_enablement_and_a_matching_surface() {
        let target = SurfaceId(7);
        let other = SurfaceId(8);
        let press = |surface, serial, order| {
            Some(PointerPress {
                surface,
                serial,
                order,
            })
        };
        let candidates = [
            (1, Some(other), true, press(target, 10, 1)),
            (2, Some(target), false, press(target, 20, 2)),
            (3, Some(target), true, press(other, 30, 3)),
        ];

        assert_eq!(select_active_pointer_press(target, candidates), None);
    }

    #[test]
    fn selection_uses_newest_valid_press_across_seats() {
        let target = SurfaceId(7);
        let press = |serial, order| {
            Some(PointerPress {
                surface: target,
                serial,
                order,
            })
        };
        let candidates = [
            (11, Some(target), true, press(110, 4)),
            (12, Some(target), true, press(120, 9)),
            (13, Some(target), true, None),
        ];

        assert_eq!(
            select_active_pointer_press(target, candidates)
                .map(|(seat, press)| (seat, press.serial)),
            Some((12, 120))
        );
    }
}
