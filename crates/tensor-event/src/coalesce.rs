//! In-queue coalescing for high-frequency events.
//!
//! Calloop and compositor practice both teach the same lesson: pointer motion
//! and redundant vblanks must not expand the queue linearly with device rate.

use crate::event::Event;

/// Counters for observability without allocating per drop.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CoalesceStats {
    pub motion_merged: u64,
    pub vblank_merged: u64,
    pub gpu_merged: u64,
    pub redraw_merged: u64,
}

impl CoalesceStats {
    #[inline]
    pub fn record(&mut self, previous: Event, _newer: Event) {
        match previous {
            Event::Input(_) => self.motion_merged = self.motion_merged.saturating_add(1),
            Event::Output(_) => self.vblank_merged = self.vblank_merged.saturating_add(1),
            Event::Gpu(_) => self.gpu_merged = self.gpu_merged.saturating_add(1),
            Event::RedrawAll => self.redraw_merged = self.redraw_merged.saturating_add(1),
            _ => {}
        }
    }
}
