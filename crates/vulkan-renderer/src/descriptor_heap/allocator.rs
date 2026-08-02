use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::FrameToken;

/// Byte range inside a descriptor heap buffer.
#[derive(Debug, Eq, PartialEq)]
pub struct DescriptorAllocation {
    pub(super) range: Range<u64>,
    pub(super) allocator_id: u64,
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

    /// Value-only byte range for a descriptor upload command.
    pub const fn upload_range(&self) -> DescriptorHeapUploadRange {
        DescriptorHeapUploadRange {
            offset: self.offset(),
            size: self.size(),
        }
    }
}

/// One byte range whose encoded descriptors must become visible to a
/// device-local descriptor heap before shader access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescriptorHeapUploadRange {
    pub offset: u64,
    pub size: u64,
}

/// Bounded first-fit allocator shared by one descriptor heap and its frame
/// policy owner.
///
/// Clones refer to the same allocation state. This lets a product retain
/// scene/frame policy while the shared heap remains the sole source of direct
/// descriptor ranges and write validation.
#[derive(Clone, Debug)]
pub struct DescriptorHeapAllocator {
    state: Arc<Mutex<DescriptorHeapAllocatorState>>,
}

#[derive(Debug)]
struct DescriptorHeapAllocatorState {
    id: u64,
    heap_size: u64,
    alignment: u64,
    reserved_range_offset: u64,
    reserved_range_size: u64,
    free: Vec<Range<u64>>,
    retired: Vec<(u64, Range<u64>)>,
}

impl DescriptorHeapAllocator {
    /// Creates an allocator whose implementation-reserved range is a suffix
    /// of the bound direct heap.
    pub fn new(
        heap_size: u64,
        minimum_reserved_range: u64,
        alignment: u64,
    ) -> Result<Self, DescriptorHeapError> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(DescriptorHeapError::InvalidAlignment(alignment));
        }
        let id = next_allocator_id()?;
        let reserved_range_size = align_up(minimum_reserved_range, alignment)
            .ok_or(DescriptorHeapError::RangeOverflow)?;
        if reserved_range_size >= heap_size {
            return Err(DescriptorHeapError::ReservedRangeConsumesHeap {
                heap_size,
                minimum_reserved_range,
                alignment,
            });
        }
        let reserved_range_offset = align_down(heap_size - reserved_range_size, alignment);
        if reserved_range_offset == 0 {
            return Err(DescriptorHeapError::ReservedRangeConsumesHeap {
                heap_size,
                minimum_reserved_range,
                alignment,
            });
        }
        Ok(Self {
            state: Arc::new(Mutex::new(DescriptorHeapAllocatorState {
                id,
                heap_size,
                alignment,
                reserved_range_offset,
                reserved_range_size: heap_size - reserved_range_offset,
                free: std::iter::once(0..reserved_range_offset).collect(),
                retired: Vec::new(),
            })),
        })
    }

    pub fn heap_size(&self) -> u64 {
        self.state().heap_size
    }

    pub fn reserved_range_offset(&self) -> u64 {
        self.state().reserved_range_offset
    }

    pub fn reserved_range_size(&self) -> u64 {
        self.state().reserved_range_size
    }

    pub fn alignment(&self) -> u64 {
        self.state().alignment
    }

    /// Allocates one contiguous range. `descriptor_alignment` is the exact
    /// compiler/device ABI alignment for the descriptor class; heap binding
    /// alignment does not artificially sparsify application descriptors.
    pub fn allocate(
        &self,
        size: u64,
        descriptor_alignment: u64,
    ) -> Result<DescriptorAllocation, DescriptorHeapError> {
        if size == 0 {
            return Err(DescriptorHeapError::ZeroSize);
        }
        if descriptor_alignment == 0 || !descriptor_alignment.is_power_of_two() {
            return Err(DescriptorHeapError::InvalidAlignment(descriptor_alignment));
        }
        let mut state = self.state();
        for index in 0..state.free.len() {
            let free = state.free[index].clone();
            let Some(start) = align_up(free.start, descriptor_alignment) else {
                continue;
            };
            let Some(end) = start.checked_add(size) else {
                continue;
            };
            if end > free.end {
                continue;
            }
            state.free.remove(index);
            if free.start < start {
                state.free.push(free.start..start);
            }
            if end < free.end {
                state.free.push(end..free.end);
            }
            normalize_free_ranges(&mut state);
            return Ok(DescriptorAllocation {
                range: start..end,
                allocator_id: state.id,
            });
        }
        Err(DescriptorHeapError::OutOfMemory {
            requested: size,
            available: state.free.iter().map(|range| range.end - range.start).sum(),
        })
    }

    pub fn retire(
        &self,
        allocation: DescriptorAllocation,
        after: FrameToken,
    ) -> Result<(), DescriptorHeapError> {
        self.retire_at_timeline(allocation, after.value())
    }

    /// Retires an allocation against a timeline value reserved by the shared
    /// renderer. This supports products that retain frame policy while using
    /// the renderer's device timeline namespace.
    pub fn retire_at_timeline(
        &self,
        allocation: DescriptorAllocation,
        after: u64,
    ) -> Result<(), DescriptorHeapError> {
        if after == 0 {
            return Err(DescriptorHeapError::InvalidTimelineValue);
        }
        let mut state = self.state();
        if allocation.allocator_id != state.id {
            return Err(DescriptorHeapError::WrongAllocator);
        }
        debug_assert!(allocation.range.end <= state.reserved_range_offset);
        state.retired.push((after, allocation.range));
        Ok(())
    }

    pub fn reclaim(&self, completed_timeline: u64) -> usize {
        let mut state = self.state();
        let retired = std::mem::take(&mut state.retired);
        let mut pending = Vec::with_capacity(retired.len());
        let mut reclaimed = 0;
        for (timeline, range) in retired {
            if timeline <= completed_timeline {
                state.free.push(range);
                reclaimed += 1;
            } else {
                pending.push((timeline, range));
            }
        }
        state.retired = pending;
        if reclaimed > 0 {
            normalize_free_ranges(&mut state);
        }
        reclaimed
    }

    /// Returns an allocation to the free list before it has been referenced
    /// by any submitted command buffer.
    pub fn release(&self, allocation: DescriptorAllocation) -> Result<(), DescriptorHeapError> {
        let mut state = self.state();
        if allocation.allocator_id != state.id {
            return Err(DescriptorHeapError::WrongAllocator);
        }
        debug_assert!(allocation.range.end <= state.reserved_range_offset);
        state.free.push(allocation.range);
        normalize_free_ranges(&mut state);
        Ok(())
    }

    pub fn available_bytes(&self) -> u64 {
        self.state()
            .free
            .iter()
            .map(|range| range.end - range.start)
            .sum()
    }

    pub fn pending_retirements(&self) -> usize {
        self.state().retired.len()
    }

    pub(crate) fn owns(&self, allocation: &DescriptorAllocation) -> bool {
        allocation.allocator_id == self.state().id
    }

    fn state(&self) -> MutexGuard<'_, DescriptorHeapAllocatorState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn normalize_free_ranges(state: &mut DescriptorHeapAllocatorState) {
    state.free.sort_by_key(|range| range.start);
    let mut merged = Vec::<Range<u64>>::with_capacity(state.free.len());
    for range in state.free.drain(..) {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    state.free = merged;
}

pub(super) fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let mask = alignment.checked_sub(1)?;
    value.checked_add(mask).map(|value| value & !mask)
}

pub(super) const fn align_down(value: u64, alignment: u64) -> u64 {
    value & !(alignment - 1)
}

static NEXT_ALLOCATOR_ID: AtomicU64 = AtomicU64::new(1);

fn next_allocator_id() -> Result<u64, DescriptorHeapError> {
    NEXT_ALLOCATOR_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| DescriptorHeapError::RangeOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorHeapError {
    InvalidAlignment(u64),
    InvalidDescriptorLayout,
    InvalidTimelineValue,
    ZeroSize,
    RangeOverflow,
    WrongHeapKind,
    WrongAllocator,
    ReservedRangeConsumesHeap {
        heap_size: u64,
        minimum_reserved_range: u64,
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
