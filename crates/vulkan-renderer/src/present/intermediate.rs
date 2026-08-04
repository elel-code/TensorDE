//! Bounded timeline-safe storage for transient color dependencies.
//!
//! The pool is intentionally semantic-free: products supply only extent,
//! format, usage and timeline facts. A compositor backdrop, scene-engine
//! effect, capture tap or color transform can therefore share the same
//! retained Vulkan allocation policy without sharing product graph types.

use crate::{
    Error, Extent2D, MemoryAllocator, OffscreenColorTarget, OffscreenColorTargets,
    OffscreenColorTargetsDescriptor, Result, TextureFormat, TextureUsages,
};

/// Fixed resource and extent limits for one retained color-target pool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedColorTargetPoolDescriptor {
    pub label: Option<String>,
    pub max_targets: usize,
    pub max_retained_bytes: u64,
    pub max_extent: Extent2D,
}

impl RetainedColorTargetPoolDescriptor {
    fn validate(&self) -> Result<()> {
        if self.max_targets == 0 {
            return Err(Error::Validation(
                "retained color-target pool requires at least one target".into(),
            ));
        }
        if self.max_retained_bytes == 0 {
            return Err(Error::Validation(
                "retained color-target pool byte limit must be non-zero".into(),
            ));
        }
        if self.max_extent.is_empty() {
            return Err(Error::Validation(
                "retained color-target pool maximum extent must be non-zero".into(),
            ));
        }
        Ok(())
    }
}

/// Generic physical requirements for one retained target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetainedColorTargetRequest {
    pub extent: Extent2D,
    pub format: TextureFormat,
    /// Extra roles beyond the mandatory color-attachment and sampled roles.
    pub additional_usage: TextureUsages,
}

impl RetainedColorTargetRequest {
    fn validate(self, descriptor: &RetainedColorTargetPoolDescriptor) -> Result<()> {
        if self.extent.is_empty() {
            return Err(Error::Validation(
                "retained color-target request extent must be non-zero".into(),
            ));
        }
        if self.extent.width > descriptor.max_extent.width
            || self.extent.height > descriptor.max_extent.height
        {
            return Err(Error::Validation(format!(
                "retained color-target request {}x{} exceeds pool maximum {}x{}",
                self.extent.width,
                self.extent.height,
                descriptor.max_extent.width,
                descriptor.max_extent.height,
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TargetKey {
    extent: Extent2D,
    format: TextureFormat,
    additional_usage: TextureUsages,
}

impl From<RetainedColorTargetRequest> for TargetKey {
    fn from(request: RetainedColorTargetRequest) -> Self {
        Self {
            extent: request.extent,
            format: request.format,
            additional_usage: request.additional_usage,
        }
    }
}

#[derive(Debug)]
struct PoolEntry<T> {
    key: TargetKey,
    target: T,
    allocation_size: u64,
    retire_timeline: u64,
    reservation: Option<u64>,
    last_used: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetainedColorTargetReservation {
    id: u64,
    acquired_completed_timeline: u64,
}

#[derive(Debug)]
struct PoolAcquisition<'a, T> {
    target: &'a T,
    reservation: RetainedColorTargetReservation,
}

#[derive(Debug)]
struct PoolBatchAcquisition<'a, T, const N: usize> {
    targets: [&'a T; N],
    reservations: [RetainedColorTargetReservation; N],
}

#[derive(Debug)]
struct RetainedPool<T> {
    descriptor: RetainedColorTargetPoolDescriptor,
    entries: Vec<PoolEntry<T>>,
    retained_bytes: u64,
    use_serial: u64,
    next_reservation: u64,
}

impl<T> RetainedPool<T> {
    fn new(descriptor: RetainedColorTargetPoolDescriptor) -> Result<Self> {
        descriptor.validate()?;
        Ok(Self {
            entries: Vec::with_capacity(descriptor.max_targets),
            descriptor,
            retained_bytes: 0,
            use_serial: 0,
            next_reservation: 1,
        })
    }

    fn acquire_with(
        &mut self,
        request: RetainedColorTargetRequest,
        completed_timeline: u64,
        create: impl FnOnce() -> Result<(T, u64)>,
    ) -> Result<PoolAcquisition<'_, T>> {
        request.validate(&self.descriptor)?;
        let key = TargetKey::from(request);
        self.use_serial = self.use_serial.saturating_add(1);
        let reservation = RetainedColorTargetReservation {
            id: self.next_reservation,
            acquired_completed_timeline: completed_timeline,
        };
        self.next_reservation = self.next_reservation.checked_add(1).ok_or_else(|| {
            Error::Validation("retained target reservation space exhausted".into())
        })?;

        if let Some(index) = self.entries.iter().position(|entry| {
            entry.key == key
                && entry.reservation.is_none()
                && entry.retire_timeline <= completed_timeline
        }) {
            let entry = &mut self.entries[index];
            entry.reservation = Some(reservation.id);
            entry.last_used = self.use_serial;
            return Ok(PoolAcquisition {
                target: &entry.target,
                reservation,
            });
        }

        if self.entries.len() >= self.descriptor.max_targets
            && !self.entries.iter().any(|entry| {
                entry.reservation.is_none() && entry.retire_timeline <= completed_timeline
            })
        {
            return Err(pool_exhausted(&self.descriptor, self.retained_bytes));
        }

        let (target, allocation_size) = create()?;
        if allocation_size == 0 || allocation_size > self.descriptor.max_retained_bytes {
            return Err(Error::Validation(format!(
                "retained color target allocation of {allocation_size} bytes exceeds pool limit {}",
                self.descriptor.max_retained_bytes
            )));
        }

        while self.entries.len() >= self.descriptor.max_targets
            || self
                .retained_bytes
                .checked_add(allocation_size)
                .is_none_or(|bytes| bytes > self.descriptor.max_retained_bytes)
        {
            let Some(index) = self.oldest_retired(completed_timeline) else {
                return Err(pool_exhausted(&self.descriptor, self.retained_bytes));
            };
            let removed = self.entries.swap_remove(index);
            self.retained_bytes = self
                .retained_bytes
                .checked_sub(removed.allocation_size)
                .expect("retained byte accounting includes every pool entry");
        }

        self.retained_bytes = self
            .retained_bytes
            .checked_add(allocation_size)
            .ok_or_else(|| Error::Validation("retained byte accounting overflowed".into()))?;
        self.entries.push(PoolEntry {
            key,
            target,
            allocation_size,
            retire_timeline: completed_timeline,
            reservation: Some(reservation.id),
            last_used: self.use_serial,
        });
        Ok(PoolAcquisition {
            target: &self
                .entries
                .last()
                .expect("new retained target was appended")
                .target,
            reservation,
        })
    }

    fn acquire_batch_with<const N: usize>(
        &mut self,
        requests: [RetainedColorTargetRequest; N],
        completed_timeline: u64,
        mut create: impl FnMut(RetainedColorTargetRequest) -> Result<(T, u64)>,
    ) -> Result<PoolBatchAcquisition<'_, T, N>> {
        if N == 0 {
            return Err(Error::Validation(
                "retained color-target acquisition batch must not be empty".into(),
            ));
        }
        let mut reservations = [None; N];
        for (index, request) in requests.into_iter().enumerate() {
            match self.acquire_with(request, completed_timeline, || create(request)) {
                Ok(acquired) => {
                    // End the target borrow before the next mutable pool
                    // acquisition; final stable borrows are rebuilt only
                    // after the complete batch has succeeded.
                    let _target = acquired.target;
                    reservations[index] = Some(acquired.reservation);
                }
                Err(error) => {
                    for reservation in reservations.into_iter().flatten() {
                        self.release(reservation)
                            .expect("same-batch reservations remain live until rollback");
                    }
                    return Err(error);
                }
            }
        }
        let reservations = reservations.map(|reservation| {
            reservation.expect("every batch position was acquired before target lookup")
        });
        let targets = std::array::from_fn(|index| {
            let reservation = reservations[index];
            &self
                .entries
                .iter()
                .find(|entry| entry.reservation == Some(reservation.id))
                .expect("new batch reservation maps to one retained target")
                .target
        });
        Ok(PoolBatchAcquisition {
            targets,
            reservations,
        })
    }

    fn retire(
        &mut self,
        reservation: RetainedColorTargetReservation,
        retire_timeline: u64,
    ) -> Result<()> {
        self.retire_batch([reservation], retire_timeline)
    }

    fn release(&mut self, reservation: RetainedColorTargetReservation) -> Result<()> {
        self.release_batch([reservation])
    }

    fn retire_batch<const N: usize>(
        &mut self,
        reservations: [RetainedColorTargetReservation; N],
        retire_timeline: u64,
    ) -> Result<()> {
        self.validate_reservations(&reservations)?;
        for reservation in reservations {
            if retire_timeline <= reservation.acquired_completed_timeline {
                return Err(Error::Validation(format!(
                    "retained color-target retirement timeline {retire_timeline} must be newer than the acquisition completed timeline {}",
                    reservation.acquired_completed_timeline
                )));
            }
        }
        for reservation in reservations {
            let entry = self.reserved_entry(reservation)?;
            entry.retire_timeline = retire_timeline;
            entry.reservation = None;
        }
        Ok(())
    }

    fn release_batch<const N: usize>(
        &mut self,
        reservations: [RetainedColorTargetReservation; N],
    ) -> Result<()> {
        self.validate_reservations(&reservations)?;
        for reservation in reservations {
            self.reserved_entry(reservation)?.reservation = None;
        }
        Ok(())
    }

    fn validate_reservations<const N: usize>(
        &self,
        reservations: &[RetainedColorTargetReservation; N],
    ) -> Result<()> {
        if N == 0 {
            return Err(Error::Validation(
                "retained color-target reservation batch must not be empty".into(),
            ));
        }
        for (index, reservation) in reservations.iter().enumerate() {
            if reservations[..index]
                .iter()
                .any(|previous| previous.id == reservation.id)
            {
                return Err(Error::Validation(format!(
                    "retained color-target reservation {} is duplicated",
                    reservation.id
                )));
            }
            if !self
                .entries
                .iter()
                .any(|entry| entry.reservation == Some(reservation.id))
            {
                return Err(Error::Validation(format!(
                    "retained color-target reservation {} is stale",
                    reservation.id
                )));
            }
        }
        Ok(())
    }

    fn reserved_entry(
        &mut self,
        reservation: RetainedColorTargetReservation,
    ) -> Result<&mut PoolEntry<T>> {
        self.entries
            .iter_mut()
            .find(|entry| entry.reservation == Some(reservation.id))
            .ok_or_else(|| {
                Error::Validation(format!(
                    "retained color-target reservation {} is stale",
                    reservation.id
                ))
            })
    }

    fn oldest_retired(&self, completed_timeline: u64) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.reservation.is_none() && entry.retire_timeline <= completed_timeline
            })
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(index, _)| index)
    }

    fn trim(&mut self, completed_timeline: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|entry| {
            if entry.reservation.is_none() && entry.retire_timeline <= completed_timeline {
                self.retained_bytes = self
                    .retained_bytes
                    .checked_sub(entry.allocation_size)
                    .expect("retained byte accounting includes every pool entry");
                false
            } else {
                true
            }
        });
        before - self.entries.len()
    }
}

fn pool_exhausted(descriptor: &RetainedColorTargetPoolDescriptor, retained_bytes: u64) -> Error {
    Error::Validation(format!(
        "retained color-target pool is exhausted (targets={}, retained_bytes={retained_bytes}, byte_limit={})",
        descriptor.max_targets, descriptor.max_retained_bytes
    ))
}

/// Allocator-backed retained color targets with bounded memory and timeline reuse.
#[derive(Debug)]
pub struct RetainedColorTargetPool {
    allocator: MemoryAllocator,
    pool: RetainedPool<OffscreenColorTargets>,
}

/// One reserved target. Recording may borrow the image/view, then the caller
/// must retire or release the copied reservation token after this borrow ends.
#[derive(Clone, Copy, Debug)]
pub struct AcquiredRetainedColorTarget<'a> {
    pub target: OffscreenColorTarget<'a>,
    reservation: RetainedColorTargetReservation,
}

/// A fixed-size reservation batch acquired as one rollback-safe transaction.
#[derive(Clone, Copy, Debug)]
pub struct AcquiredRetainedColorTargets<'a, const N: usize> {
    pub targets: [OffscreenColorTarget<'a>; N],
    reservations: [RetainedColorTargetReservation; N],
}

impl<const N: usize> AcquiredRetainedColorTargets<'_, N> {
    pub const fn reservations(&self) -> [RetainedColorTargetReservation; N] {
        self.reservations
    }
}

impl AcquiredRetainedColorTarget<'_> {
    pub const fn reservation(&self) -> RetainedColorTargetReservation {
        self.reservation
    }
}

impl RetainedColorTargetPool {
    pub fn new(
        allocator: MemoryAllocator,
        descriptor: RetainedColorTargetPoolDescriptor,
    ) -> Result<Self> {
        Ok(Self {
            allocator,
            pool: RetainedPool::new(descriptor)?,
        })
    }

    /// Reserve one target for recording without claiming GPU submission.
    ///
    /// After the returned borrow ends, call [`Self::retire`] if work was
    /// submitted or [`Self::release`] if recording/submission was abandoned.
    pub fn acquire(
        &mut self,
        request: RetainedColorTargetRequest,
        completed_timeline: u64,
    ) -> Result<AcquiredRetainedColorTarget<'_>> {
        let acquired = self.acquire_batch([request], completed_timeline)?;
        Ok(AcquiredRetainedColorTarget {
            target: acquired.targets[0],
            reservation: acquired.reservations[0],
        })
    }

    /// Reserve a fixed target batch atomically from the caller's perspective.
    /// If any lane cannot be acquired, every earlier reservation is released.
    pub fn acquire_batch<const N: usize>(
        &mut self,
        requests: [RetainedColorTargetRequest; N],
        completed_timeline: u64,
    ) -> Result<AcquiredRetainedColorTargets<'_, N>> {
        let allocator = self.allocator.clone();
        let label = self.pool.descriptor.label.clone();
        let acquired =
            self.pool
                .acquire_batch_with(requests, completed_timeline, move |request| {
                    let targets = allocator.create_offscreen_color_targets(
                        &OffscreenColorTargetsDescriptor {
                            label: label.clone(),
                            extent: request.extent,
                            format: request.format,
                            frame_slots: 1,
                            additional_usage: request.additional_usage,
                        },
                    )?;
                    let allocation_size = targets.allocation_size();
                    Ok((targets, allocation_size))
                })?;
        Ok(AcquiredRetainedColorTargets {
            targets: acquired.targets.map(|target| {
                target
                    .target(0)
                    .expect("retained targets always contain their fixed frame slot")
            }),
            reservations: acquired.reservations,
        })
    }

    /// Mark a reservation immutable until the submitted timeline completes.
    pub fn retire(
        &mut self,
        reservation: RetainedColorTargetReservation,
        retire_timeline: u64,
    ) -> Result<()> {
        self.pool.retire(reservation, retire_timeline)
    }

    /// Roll back an unsubmitted reservation for immediate safe reuse.
    pub fn release(&mut self, reservation: RetainedColorTargetReservation) -> Result<()> {
        self.pool.release(reservation)
    }

    /// Retire every reservation in a successfully submitted batch together.
    pub fn retire_batch<const N: usize>(
        &mut self,
        reservations: [RetainedColorTargetReservation; N],
        retire_timeline: u64,
    ) -> Result<()> {
        self.pool.retire_batch(reservations, retire_timeline)
    }

    /// Roll back every reservation in an abandoned batch together.
    pub fn release_batch<const N: usize>(
        &mut self,
        reservations: [RetainedColorTargetReservation; N],
    ) -> Result<()> {
        self.pool.release_batch(reservations)
    }

    pub const fn descriptor(&self) -> &RetainedColorTargetPoolDescriptor {
        &self.pool.descriptor
    }

    pub fn target_count(&self) -> usize {
        self.pool.entries.len()
    }

    pub const fn retained_bytes(&self) -> u64 {
        self.pool.retained_bytes
    }

    /// Drop every target whose last use is known complete.
    pub fn trim(&mut self, completed_timeline: u64) -> usize {
        let removed = self.pool.trim(completed_timeline);
        self.allocator.trim();
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(
        max_targets: usize,
        max_retained_bytes: u64,
    ) -> RetainedColorTargetPoolDescriptor {
        RetainedColorTargetPoolDescriptor {
            label: Some("test".into()),
            max_targets,
            max_retained_bytes,
            max_extent: Extent2D::new(512, 512),
        }
    }

    fn request(width: u32) -> RetainedColorTargetRequest {
        RetainedColorTargetRequest {
            extent: Extent2D::new(width, 64),
            format: TextureFormat::Rgba8Unorm,
            additional_usage: TextureUsages::STORAGE,
        }
    }

    #[test]
    fn matching_target_reuses_only_after_timeline_completion() {
        let mut pool = RetainedPool::<u32>::new(descriptor(2, 1_024)).unwrap();
        let acquired = pool
            .acquire_with(request(64), 0, || Ok((10_u32, 256)))
            .unwrap();
        let first = *acquired.target;
        let reservation = acquired.reservation;
        pool.retire(reservation, 4).unwrap();
        let acquired = pool
            .acquire_with(request(64), 0, || Ok((20_u32, 256)))
            .unwrap();
        let second = *acquired.target;
        let reservation = acquired.reservation;
        pool.retire(reservation, 5).unwrap();
        let acquired = pool
            .acquire_with(request(64), 4, || panic!("retired target should be reused"))
            .unwrap();
        let reused = *acquired.target;
        let reservation = acquired.reservation;
        pool.release(reservation).unwrap();

        assert_eq!((first, second, reused), (10, 20, 10));
        assert_eq!(pool.entries.len(), 2);
    }

    #[test]
    fn exhausted_pool_fails_without_calling_the_allocator() {
        let mut pool = RetainedPool::new(descriptor(1, 1_024)).unwrap();
        let acquired = pool
            .acquire_with(request(64), 0, || Ok((10_u32, 256)))
            .unwrap();
        let reservation = acquired.reservation;
        pool.retire(reservation, 4).unwrap();

        let error = pool
            .acquire_with(request(128), 0, || panic!("busy pool must not allocate"))
            .unwrap_err();

        assert!(error.to_string().contains("pool is exhausted"));
        assert_eq!(pool.entries.len(), 1);
    }

    #[test]
    fn retired_lru_entries_are_evicted_for_extent_and_byte_pressure() {
        let mut pool = RetainedPool::new(descriptor(3, 600)).unwrap();
        for (width, value, bytes, timeline) in [(64, 10_u32, 200, 2), (96, 20, 200, 3)] {
            let acquired = pool
                .acquire_with(request(width), 0, || Ok((value, bytes)))
                .unwrap();
            let reservation = acquired.reservation;
            pool.retire(reservation, timeline).unwrap();
        }
        let acquired = pool
            .acquire_with(request(128), 3, || Ok((30_u32, 400)))
            .unwrap();
        let reservation = acquired.reservation;
        pool.retire(reservation, 4).unwrap();

        assert_eq!(pool.entries.len(), 2);
        assert_eq!(pool.retained_bytes, 600);
        assert!(pool.entries.iter().any(|entry| entry.target == 30));
        assert!(pool.entries.iter().any(|entry| entry.target == 20));
    }

    #[test]
    fn trim_never_drops_busy_targets() {
        let mut pool = RetainedPool::new(descriptor(2, 1_024)).unwrap();
        for (width, value, timeline) in [(64, 10_u32, 2), (96, 20, 5)] {
            let acquired = pool
                .acquire_with(request(width), 0, || Ok((value, 256)))
                .unwrap();
            let reservation = acquired.reservation;
            pool.retire(reservation, timeline).unwrap();
        }

        assert_eq!(pool.trim(2), 1);
        assert_eq!(pool.entries.len(), 1);
        assert_eq!(pool.entries[0].target, 20);
        assert_eq!(pool.retained_bytes, 256);
    }

    #[test]
    fn invalid_limits_extent_and_retirement_fail_before_mutation() {
        assert!(RetainedPool::<u32>::new(descriptor(0, 1_024)).is_err());
        let mut pool = RetainedPool::<u32>::new(descriptor(2, 1_024)).unwrap();
        assert!(
            pool.acquire_with(request(1_024), 0, || panic!("invalid extent"))
                .is_err()
        );
        let acquired = pool
            .acquire_with(request(64), 0, || Ok((10_u32, 256)))
            .unwrap();
        let reservation = acquired.reservation;
        assert!(pool.retire(reservation, 0).is_err());
        pool.release(reservation).unwrap();
    }

    #[test]
    fn retirement_must_advance_past_the_acquisition_completion() {
        let mut pool = RetainedPool::new(descriptor(1, 1_024)).unwrap();
        let acquired = pool
            .acquire_with(request(64), 7, || Ok((10_u32, 256)))
            .unwrap();
        let reservation = acquired.reservation;

        let error = pool.retire(reservation, 7).unwrap_err();
        assert!(error.to_string().contains("must be newer"));
        pool.release(reservation).unwrap();

        let acquired = pool
            .acquire_with(request(64), 7, || {
                panic!("failed retirement must preserve a releasable reservation")
            })
            .unwrap();
        let reservation = acquired.reservation;
        pool.retire(reservation, 8).unwrap();
    }

    #[test]
    fn unsubmitted_reservation_can_be_released_for_immediate_reuse() {
        let mut pool = RetainedPool::new(descriptor(1, 1_024)).unwrap();
        let acquired = pool
            .acquire_with(request(64), 0, || Ok((10_u32, 256)))
            .unwrap();
        let reservation = acquired.reservation;
        pool.release(reservation).unwrap();

        let acquired = pool
            .acquire_with(request(64), 0, || {
                panic!("released target should be reused")
            })
            .unwrap();
        assert_eq!(*acquired.target, 10);
    }

    #[test]
    fn batch_acquisition_rolls_back_earlier_reservations_on_failure() {
        let mut pool = RetainedPool::new(descriptor(2, 300)).unwrap();
        let error = pool
            .acquire_batch_with([request(64), request(96)], 0, |request| {
                Ok((request.extent.width, 200))
            })
            .unwrap_err();
        assert!(error.to_string().contains("pool is exhausted"));
        assert!(pool.entries.iter().all(|entry| entry.reservation.is_none()));

        let acquired = pool
            .acquire_with(request(64), 0, || {
                panic!("the first batch lane remains retained and reusable")
            })
            .unwrap();
        assert_eq!(*acquired.target, 64);
    }

    #[test]
    fn batch_retirement_validates_every_token_before_mutation() {
        let mut pool = RetainedPool::new(descriptor(2, 1_024)).unwrap();
        let acquired = pool
            .acquire_batch_with([request(64), request(96)], 3, |request| {
                Ok((request.extent.width, 256))
            })
            .unwrap();
        let reservations = acquired.reservations;
        let duplicate = [reservations[0], reservations[0]];

        assert!(pool.retire_batch(duplicate, 4).is_err());
        pool.release_batch(reservations).unwrap();
        assert!(pool.entries.iter().all(|entry| entry.reservation.is_none()));
    }
}
