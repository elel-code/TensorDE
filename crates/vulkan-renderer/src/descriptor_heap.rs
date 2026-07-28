use std::fmt;
use std::ops::Range;

use crate::FrameToken;

/// Byte range inside a descriptor heap buffer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptorAllocation {
    range: Range<u64>,
}

impl DescriptorAllocation {
    pub const fn offset(&self) -> u64 {
        self.range.start
    }

    pub const fn size(&self) -> u64 {
        self.range.end - self.range.start
    }

    pub const fn range(&self) -> &Range<u64> {
        &self.range
    }
}

/// Bounded first-fit allocator for one sampler or resource descriptor heap.
///
/// The implementation-reserved prefix is never returned. Freed ranges enter a
/// retirement list and become reusable only after the renderer timeline has
/// completed the associated frame.
#[derive(Debug)]
pub struct DescriptorHeapAllocator {
    heap_size: u64,
    alignment: u64,
    usable_start: u64,
    free: Vec<Range<u64>>,
    retired: Vec<(u64, Range<u64>)>,
}

impl DescriptorHeapAllocator {
    pub fn new(
        heap_size: u64,
        reserved_prefix: u64,
        alignment: u64,
    ) -> Result<Self, DescriptorHeapError> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(DescriptorHeapError::InvalidAlignment(alignment));
        }
        let usable_start =
            align_up(reserved_prefix, alignment).ok_or(DescriptorHeapError::RangeOverflow)?;
        if usable_start >= heap_size {
            return Err(DescriptorHeapError::ReservedRangeConsumesHeap {
                heap_size,
                reserved_prefix,
                alignment,
            });
        }
        Ok(Self {
            heap_size,
            alignment,
            usable_start,
            free: vec![usable_start..heap_size],
            retired: Vec::new(),
        })
    }

    pub const fn heap_size(&self) -> u64 {
        self.heap_size
    }

    pub const fn usable_start(&self) -> u64 {
        self.usable_start
    }

    pub const fn alignment(&self) -> u64 {
        self.alignment
    }

    pub fn allocate(
        &mut self,
        size: u64,
        descriptor_alignment: u64,
    ) -> Result<DescriptorAllocation, DescriptorHeapError> {
        if size == 0 {
            return Err(DescriptorHeapError::ZeroSize);
        }
        if descriptor_alignment == 0 || !descriptor_alignment.is_power_of_two() {
            return Err(DescriptorHeapError::InvalidAlignment(descriptor_alignment));
        }
        let alignment = self.alignment.max(descriptor_alignment);
        for index in 0..self.free.len() {
            let free = self.free[index].clone();
            let Some(start) = align_up(free.start, alignment) else {
                continue;
            };
            let Some(end) = start.checked_add(size) else {
                continue;
            };
            if end > free.end {
                continue;
            }
            self.free.remove(index);
            if free.start < start {
                self.free.push(free.start..start);
            }
            if end < free.end {
                self.free.push(end..free.end);
            }
            self.normalize_free_ranges();
            return Ok(DescriptorAllocation { range: start..end });
        }
        Err(DescriptorHeapError::OutOfMemory {
            requested: size,
            available: self.available_bytes(),
        })
    }

    pub fn retire(&mut self, allocation: DescriptorAllocation, after: FrameToken) {
        debug_assert!(allocation.range.start >= self.usable_start);
        debug_assert!(allocation.range.end <= self.heap_size);
        self.retired.push((after.value(), allocation.range));
    }

    pub fn reclaim(&mut self, completed_timeline: u64) -> usize {
        let mut pending = Vec::with_capacity(self.retired.len());
        let mut reclaimed = 0;
        for (timeline, range) in self.retired.drain(..) {
            if timeline <= completed_timeline {
                self.free.push(range);
                reclaimed += 1;
            } else {
                pending.push((timeline, range));
            }
        }
        self.retired = pending;
        if reclaimed > 0 {
            self.normalize_free_ranges();
        }
        reclaimed
    }

    pub fn available_bytes(&self) -> u64 {
        self.free.iter().map(|range| range.end - range.start).sum()
    }

    pub fn pending_retirements(&self) -> usize {
        self.retired.len()
    }

    fn normalize_free_ranges(&mut self) {
        self.free.sort_by_key(|range| range.start);
        let mut merged = Vec::<Range<u64>>::with_capacity(self.free.len());
        for range in self.free.drain(..) {
            if let Some(previous) = merged.last_mut()
                && range.start <= previous.end
            {
                previous.end = previous.end.max(range.end);
                continue;
            }
            merged.push(range);
        }
        self.free = merged;
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let mask = alignment.checked_sub(1)?;
    value.checked_add(mask).map(|value| value & !mask)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorHeapError {
    InvalidAlignment(u64),
    ZeroSize,
    RangeOverflow,
    ReservedRangeConsumesHeap {
        heap_size: u64,
        reserved_prefix: u64,
        alignment: u64,
    },
    OutOfMemory {
        requested: u64,
        available: u64,
    },
}

impl fmt::Display for DescriptorHeapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for DescriptorHeapError {}

#[cfg(test)]
mod tests {
    use crate::FrameClock;

    use super::*;

    #[test]
    fn reserved_prefix_and_descriptor_alignment_are_never_violated() {
        let mut heap = DescriptorHeapAllocator::new(1024, 65, 64).unwrap();
        let allocation = heap.allocate(32, 128).unwrap();
        assert_eq!(heap.usable_start(), 128);
        assert_eq!(allocation.offset(), 128);
        assert_eq!(allocation.offset() % 128, 0);
    }

    #[test]
    fn range_is_reused_only_after_timeline_completion() {
        let mut clock = FrameClock::default();
        let frame = clock.allocate().unwrap();
        let mut heap = DescriptorHeapAllocator::new(256, 64, 64).unwrap();
        let allocation = heap.allocate(192, 64).unwrap();
        heap.retire(allocation, frame);
        assert!(matches!(
            heap.allocate(64, 64),
            Err(DescriptorHeapError::OutOfMemory { .. })
        ));
        assert_eq!(heap.reclaim(frame.value() - 1), 0);
        assert_eq!(heap.reclaim(frame.value()), 1);
        assert_eq!(heap.allocate(192, 64).unwrap().range(), &(64..256));
    }

    #[test]
    fn fragmented_ranges_merge_after_retirement() {
        let mut clock = FrameClock::default();
        let frame = clock.allocate().unwrap();
        let mut heap = DescriptorHeapAllocator::new(512, 64, 64).unwrap();
        let first = heap.allocate(64, 64).unwrap();
        let second = heap.allocate(64, 64).unwrap();
        heap.retire(first, frame);
        heap.retire(second, frame);
        heap.reclaim(frame.value());
        assert_eq!(heap.allocate(128, 64).unwrap().range(), &(64..192));
    }
}
