use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use super::DeviceOwner;
use crate::{FrameToken, SubmissionLease};

pub(super) struct SubmissionRetirement {
    // Declared first so leases drop before the final device owner reference.
    pending: Mutex<Vec<(u64, Vec<SubmissionLease>)>>,
    // Keeps the device alive until pending leases have been dropped.
    owner: Arc<DeviceOwner>,
}

impl SubmissionRetirement {
    pub(super) fn new(owner: Arc<DeviceOwner>) -> Self {
        Self {
            pending: Mutex::new(Vec::new()),
            owner,
        }
    }

    pub(super) fn retire_after(&self, frame: FrameToken, leases: Vec<SubmissionLease>) {
        if leases.is_empty() {
            return;
        }
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((frame.value(), leases));

        // A waiter may publish completion before this entry is installed.
        // Rechecking closes that race without putting retirement into the
        // device ownership graph.
        let completed = self.owner.completed_timeline.load(Ordering::Acquire);
        if completed >= frame.value() {
            self.retire_completed(completed);
        }
    }

    pub(super) fn retire_completed(&self, completed: u64) {
        let retired = {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut retired = Vec::new();
            let mut retained = Vec::with_capacity(pending.len());
            for (timeline, leases) in pending.drain(..) {
                if timeline <= completed {
                    retired.extend(leases);
                } else {
                    retained.push((timeline, leases));
                }
            }
            *pending = retained;
            retired
        };
        drop(retired);
    }

    pub(super) fn pending_count(&self) -> usize {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|(_, leases)| leases.len())
            .sum()
    }
}
