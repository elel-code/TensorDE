use super::{FrameError, HeapAllocation};

/// Bounded timeline-aware allocator for the descriptor heap's usable range.
#[derive(Debug)]
pub(super) struct DescriptorHeap {
    pub(super) capacity: u64,
    pub(super) alignment: u64,
    pub(super) first_usable_offset: u64,
    cursor: u64,
    active: Vec<ActiveAllocation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveAllocation {
    allocation: HeapAllocation,
    retire_timeline: u64,
}

impl DescriptorHeap {
    pub(super) fn new(
        capacity: u64,
        alignment: u64,
        reserved_range: u64,
    ) -> Result<Self, FrameError> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(FrameError::InvalidDescriptorAlignment { alignment });
        }
        let first_usable_offset =
            super::align_up(reserved_range, alignment).ok_or(FrameError::DescriptorSizeOverflow)?;
        if capacity <= first_usable_offset {
            return Err(FrameError::DescriptorHeapTooSmall {
                capacity,
                reserved: first_usable_offset,
            });
        }
        Ok(Self {
            capacity,
            alignment,
            first_usable_offset,
            cursor: first_usable_offset,
            active: Vec::new(),
        })
    }

    pub(super) fn allocate(
        &mut self,
        size: u64,
        retire_timeline: u64,
    ) -> Result<HeapAllocation, FrameError> {
        let size =
            super::align_up(size, self.alignment).ok_or(FrameError::DescriptorSizeOverflow)?;
        if size > self.capacity.saturating_sub(self.first_usable_offset) {
            return Err(FrameError::DescriptorRequestTooLarge {
                requested: size,
                capacity: self.capacity,
            });
        }

        let start = super::align_up(self.cursor, self.alignment)
            .ok_or(FrameError::DescriptorSizeOverflow)?;
        let offset = if self.fits(start, size) {
            start
        } else if self.fits(self.first_usable_offset, size) {
            self.first_usable_offset
        } else {
            return Err(FrameError::DescriptorHeapExhausted {
                requested: size,
                capacity: self.capacity,
            });
        };
        let allocation = HeapAllocation { offset, size };
        self.cursor = offset.saturating_add(size);
        self.active.push(ActiveAllocation {
            allocation,
            retire_timeline,
        });
        Ok(allocation)
    }

    fn fits(&self, offset: u64, size: u64) -> bool {
        let Some(end) = offset.checked_add(size) else {
            return false;
        };
        end <= self.capacity
            && self.active.iter().all(|active| {
                let active_end = active
                    .allocation
                    .offset
                    .saturating_add(active.allocation.size);
                end <= active.allocation.offset || offset >= active_end
            })
    }

    pub(super) fn reclaim(&mut self, completed_timeline: u64) {
        self.active
            .retain(|active| active.retire_timeline > completed_timeline);
        if self.active.is_empty() && self.cursor >= self.capacity {
            self.cursor = self.first_usable_offset;
        }
    }

    pub(super) fn cancel(&mut self, allocation: HeapAllocation) {
        let Some(index) = self
            .active
            .iter()
            .position(|active| active.allocation == allocation)
        else {
            return;
        };
        self.active.remove(index);
        if self.cursor == allocation.offset.saturating_add(allocation.size) {
            self.cursor = allocation.offset;
        }
    }
}
