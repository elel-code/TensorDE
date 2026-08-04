//! Typed timestamp-query ownership and command recording.

use std::fmt;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use crate::backend::DeviceOwner;
use crate::{Error, Result, SubmissionLease, SubmissionResource};

/// Describes one retained timestamp query set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimestampQuerySetDescriptor {
    pub label: Option<String>,
    pub count: u32,
}

/// A checked index into a [`TimestampQuerySet`].
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TimestampQuery(u32);

impl TimestampQuery {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Pipeline boundary at which a timestamp is written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimestampWriteStage {
    TopOfPipe,
    BottomOfPipe,
}

impl TimestampWriteStage {
    pub(crate) const fn to_vk(self) -> vk::PipelineStageFlags2 {
        match self {
            Self::TopOfPipe => vk::PipelineStageFlags2::TOP_OF_PIPE,
            Self::BottomOfPipe => vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
        }
    }
}

struct TimestampQuerySetInner {
    owner: Arc<DeviceOwner>,
    handle: vk::QueryPool,
    label: Option<String>,
    count: u32,
    timestamp_period_nanoseconds: f32,
    timestamp_valid_bits: u32,
}

impl fmt::Debug for TimestampQuerySetInner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TimestampQuerySetInner")
            .field("label", &self.label)
            .field("count", &self.count)
            .field(
                "timestamp_period_nanoseconds",
                &self.timestamp_period_nanoseconds,
            )
            .field("timestamp_valid_bits", &self.timestamp_valid_bits)
            .finish_non_exhaustive()
    }
}

impl Drop for TimestampQuerySetInner {
    fn drop(&mut self) {
        unsafe { self.owner.device.destroy_query_pool(self.handle, None) };
    }
}

/// Retained timestamp query storage owned by the shared renderer.
#[derive(Clone)]
pub struct TimestampQuerySet {
    inner: Arc<TimestampQuerySetInner>,
}

impl fmt::Debug for TimestampQuerySet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(formatter)
    }
}

impl TimestampQuerySet {
    pub(crate) fn new(
        owner: Arc<DeviceOwner>,
        descriptor: &TimestampQuerySetDescriptor,
    ) -> Result<Self> {
        if descriptor.count == 0 {
            return Err(Error::Validation(
                "timestamp query set count must be nonzero".into(),
            ));
        }
        let queue_families = unsafe {
            owner
                .instance_owner()
                .instance
                .get_physical_device_queue_family_properties(owner.physical_device())
        };
        let graphics_queue_family = owner.graphics_queue_family();
        let timestamp_valid_bits = queue_families
            .get(graphics_queue_family as usize)
            .ok_or_else(|| {
                Error::Validation(format!(
                    "graphics queue family {} is missing for timestamp queries",
                    graphics_queue_family
                ))
            })?
            .timestamp_valid_bits;
        if timestamp_valid_bits == 0 {
            return Err(Error::Validation(format!(
                "graphics queue family {} does not support timestamp queries",
                graphics_queue_family
            )));
        }
        let properties = unsafe {
            owner
                .instance_owner()
                .instance
                .get_physical_device_properties(owner.physical_device())
        };
        let timestamp_period_nanoseconds = properties.limits.timestamp_period;
        if !timestamp_period_nanoseconds.is_finite() || timestamp_period_nanoseconds <= 0.0 {
            return Err(Error::Validation(format!(
                "device reported invalid timestamp period {timestamp_period_nanoseconds}"
            )));
        }
        let info = vk::QueryPoolCreateInfo::builder()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(descriptor.count);
        let handle = unsafe { owner.device.create_query_pool(&info, None) }
            .map_err(|source| Error::vulkan("vkCreateQueryPool(timestamp)", source))?;
        Ok(Self {
            inner: Arc::new(TimestampQuerySetInner {
                owner,
                handle,
                label: descriptor.label.clone(),
                count: descriptor.count,
                timestamp_period_nanoseconds,
                timestamp_valid_bits,
            }),
        })
    }

    pub const fn query(index: u32) -> TimestampQuery {
        TimestampQuery::new(index)
    }

    pub fn count(&self) -> u32 {
        self.inner.count
    }

    pub fn label(&self) -> Option<&str> {
        self.inner.label.as_deref()
    }

    pub fn timestamp_period_nanoseconds(&self) -> f32 {
        self.inner.timestamp_period_nanoseconds
    }

    pub fn timestamp_valid_bits(&self) -> u32 {
        self.inner.timestamp_valid_bits
    }

    /// Reads a completed query set without waiting on the device or queue.
    /// The caller must first establish completion through the submission timeline.
    pub fn read_completed(&self) -> Result<TimestampQueryResults> {
        let mut values = vec![0u64; self.inner.count as usize];
        let bytes = unsafe {
            std::slice::from_raw_parts_mut(
                values.as_mut_ptr().cast::<u8>(),
                values.len() * size_of::<u64>(),
            )
        };
        unsafe {
            self.inner
                .owner
                .device
                .get_query_pool_results(
                    self.inner.handle,
                    0,
                    self.inner.count,
                    bytes,
                    size_of::<u64>() as u64,
                    vk::QueryResultFlags::_64,
                )
                .map_err(|source| Error::vulkan("vkGetQueryPoolResults(timestamp)", source))?;
        }
        Ok(TimestampQueryResults {
            values,
            timestamp_period_nanoseconds: self.inner.timestamp_period_nanoseconds,
            timestamp_valid_bits: self.inner.timestamp_valid_bits,
        })
    }

    pub(crate) fn raw(&self) -> vk::QueryPool {
        self.inner.handle
    }

    pub(crate) fn validate_range(
        &self,
        owner: &Arc<DeviceOwner>,
        first: TimestampQuery,
        count: u32,
    ) -> Result<()> {
        if !Arc::ptr_eq(owner, &self.inner.owner) {
            return Err(Error::Validation(
                "timestamp query set was created by a different Device".into(),
            ));
        }
        if count == 0
            || first
                .index()
                .checked_add(count)
                .is_none_or(|end| end > self.inner.count)
        {
            return Err(Error::Validation(format!(
                "timestamp query range {}..+{} exceeds set count {}",
                first.index(),
                count,
                self.inner.count
            )));
        }
        Ok(())
    }
}

impl SubmissionResource for TimestampQuerySet {
    fn submission_lease(&self) -> SubmissionLease {
        SubmissionLease::new(Arc::clone(&self.inner))
    }
}

/// Completed timestamp values plus the selected device's counter semantics.
#[derive(Clone, Debug, PartialEq)]
pub struct TimestampQueryResults {
    values: Vec<u64>,
    timestamp_period_nanoseconds: f32,
    timestamp_valid_bits: u32,
}

impl TimestampQueryResults {
    pub fn timestamp_period_nanoseconds(&self) -> f32 {
        self.timestamp_period_nanoseconds
    }

    pub fn timestamp_valid_bits(&self) -> u32 {
        self.timestamp_valid_bits
    }

    pub fn value(&self, query: TimestampQuery) -> Option<u64> {
        self.values.get(query.index() as usize).copied()
    }

    pub fn duration_micros(&self, start: TimestampQuery, end: TimestampQuery) -> Result<f64> {
        let start = self.value(start).ok_or_else(|| {
            Error::Validation("timestamp duration start query is out of range".into())
        })?;
        let end = self.value(end).ok_or_else(|| {
            Error::Validation("timestamp duration end query is out of range".into())
        })?;
        let ticks = timestamp_delta(start, end, self.timestamp_valid_bits);
        Ok(ticks as f64 * f64::from(self.timestamp_period_nanoseconds) / 1_000.0)
    }
}

fn timestamp_delta(start: u64, end: u64, valid_bits: u32) -> u64 {
    let mask = if valid_bits >= u64::BITS {
        u64::MAX
    } else {
        (1u64 << valid_bits) - 1
    };
    end.wrapping_sub(start) & mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_delta_handles_counter_wrap() {
        assert_eq!(timestamp_delta(250, 5, 8), 11);
        assert_eq!(timestamp_delta(u64::MAX - 2, 4, 64), 7);
    }

    #[test]
    fn completed_results_convert_ticks_with_the_device_period() {
        let results = TimestampQueryResults {
            values: vec![10, 50],
            timestamp_period_nanoseconds: 2.5,
            timestamp_valid_bits: 64,
        };
        assert_eq!(
            results
                .duration_micros(TimestampQuery::new(0), TimestampQuery::new(1))
                .unwrap(),
            0.1
        );
    }
}
