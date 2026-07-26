//! Value-only DRM vblank metadata and fixed-capacity completion batches.

use std::time::Duration;

/// drm-rs reads at most 1024 bytes per `receive_events` call. A page-flip
/// record is 32 bytes, so one kernel read cannot contain more than 32 flips.
pub const MAX_VBLANK_EVENTS_PER_READ: usize = 32;

/// Clock domain used by a kernel page-flip timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum VblankClock {
    Monotonic,
    Realtime,
}

/// Timing metadata attached to one completed page flip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VblankMetadata {
    pub timestamp: Duration,
    pub sequence: u32,
    pub clock: VblankClock,
}

/// Value-only page-flip completion. DRM handles and file descriptors remain in
/// the platform adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VblankEvent {
    pub device_id: u64,
    pub crtc_id: u32,
    pub metadata: VblankMetadata,
}

/// Stack-backed result of one bounded DRM event read.
#[derive(Debug)]
pub struct VblankBatch {
    events: [Option<VblankEvent>; MAX_VBLANK_EVENTS_PER_READ],
    len: usize,
}

impl VblankBatch {
    #[inline]
    pub fn new() -> Self {
        Self {
            events: std::array::from_fn(|_| None),
            len: 0,
        }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Append without allocating. Returns the event when the kernel batch
    /// exceeds the size guaranteed by the drm-rs read buffer.
    #[inline]
    pub fn push(&mut self, event: VblankEvent) -> Result<(), VblankEvent> {
        let Some(slot) = self.events.get_mut(self.len) else {
            return Err(event);
        };
        *slot = Some(event);
        self.len += 1;
        Ok(())
    }

    /// Consume populated slots in place. This avoids relocating the entire
    /// fixed array into an array iterator on every completed DRM read.
    pub fn drain(&mut self) -> impl Iterator<Item = VblankEvent> + '_ {
        let len = std::mem::take(&mut self.len);
        self.events[..len]
            .iter_mut()
            .map(|event| event.take().expect("populated vblank slot"))
    }
}

impl Default for VblankBatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(crtc_id: u32) -> VblankEvent {
        VblankEvent {
            device_id: 7,
            crtc_id,
            metadata: VblankMetadata {
                timestamp: Duration::from_millis(1),
                sequence: 11,
                clock: VblankClock::Monotonic,
            },
        }
    }

    #[test]
    fn batch_stays_fixed_and_preserves_order() {
        let mut batch = VblankBatch::new();
        batch.push(event(9)).unwrap();
        batch.push(event(10)).unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch.drain().collect::<Vec<_>>(), vec![event(9), event(10)]);
        assert!(batch.is_empty());
    }

    #[test]
    fn batch_reports_capacity_without_growth() {
        let mut batch = VblankBatch::new();
        for _ in 0..MAX_VBLANK_EVENTS_PER_READ {
            batch.push(event(9)).unwrap();
        }
        assert_eq!(batch.push(event(9)), Err(event(9)));
    }
}
