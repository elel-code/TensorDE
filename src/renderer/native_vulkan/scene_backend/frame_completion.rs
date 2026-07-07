//! Scene frame completion keys for retained Vulkan resource retirement.
//!
//! References:
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameSubmission {
    pub frame_slot: u32,
    pub submission_index: u64,
}

impl NativeVulkanSceneFrameSubmission {
    pub(in crate::renderer::native_vulkan) fn new(frame_slot: u32, submission_index: u64) -> Self {
        Self {
            frame_slot,
            submission_index,
        }
    }

    pub(in crate::renderer::native_vulkan) fn covers(self, retired: Self) -> bool {
        self.frame_slot == retired.frame_slot && self.submission_index >= retired.submission_index
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameSlotState {
    pub frame_slot: u32,
    pub last_submitted: Option<NativeVulkanSceneFrameSubmission>,
    pub last_completed: Option<NativeVulkanSceneFrameSubmission>,
    pub in_flight: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameCompletion {
    pub submission: NativeVulkanSceneFrameSubmission,
    pub newly_completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameCompletionTracker {
    slots: Vec<NativeVulkanSceneFrameSlotState>,
    last_submission_index: u64,
}

impl NativeVulkanSceneFrameCompletionTracker {
    pub(in crate::renderer::native_vulkan) fn new(frame_slot_count: u32) -> Result<Self, String> {
        if frame_slot_count == 0 {
            return Err(
                "scene frame completion tracker requires at least one frame slot".to_owned(),
            );
        }
        let mut slots = Vec::with_capacity(frame_slot_count as usize);
        for frame_slot in 0..frame_slot_count {
            slots.push(NativeVulkanSceneFrameSlotState {
                frame_slot,
                last_submitted: None,
                last_completed: None,
                in_flight: false,
            });
        }
        Ok(Self {
            slots,
            last_submission_index: 0,
        })
    }

    pub(in crate::renderer::native_vulkan) fn begin_frame(
        &mut self,
        frame_slot: u32,
    ) -> Result<NativeVulkanSceneFrameSubmission, String> {
        if self.slot(frame_slot)?.in_flight {
            return Err(format!(
                "scene frame slot {frame_slot} is still in flight; wait for its fence/timeline before recording a replacement frame"
            ));
        }

        let submission_index = self
            .last_submission_index
            .checked_add(1)
            .ok_or_else(|| "scene frame submission index overflow".to_owned())?;
        self.last_submission_index = submission_index;
        let submission = NativeVulkanSceneFrameSubmission::new(frame_slot, submission_index);
        let slot = self.slot_mut(frame_slot)?;
        slot.last_submitted = Some(submission);
        slot.in_flight = true;
        Ok(submission)
    }

    pub(in crate::renderer::native_vulkan) fn complete_frame(
        &mut self,
        completed: NativeVulkanSceneFrameSubmission,
    ) -> Result<NativeVulkanSceneFrameCompletion, String> {
        let slot = self.slot_mut(completed.frame_slot)?;
        if slot.last_completed == Some(completed) && !slot.in_flight {
            return Ok(NativeVulkanSceneFrameCompletion {
                submission: completed,
                newly_completed: false,
            });
        }
        match slot.last_submitted {
            Some(submitted) if submitted == completed => {
                slot.last_completed = Some(completed);
                slot.in_flight = false;
                Ok(NativeVulkanSceneFrameCompletion {
                    submission: completed,
                    newly_completed: true,
                })
            }
            Some(submitted) => Err(format!(
                "scene frame completion for slot {} reported submission {}, but last submitted is {}",
                completed.frame_slot, completed.submission_index, submitted.submission_index
            )),
            None => Err(format!(
                "scene frame completion for slot {} has no submitted frame",
                completed.frame_slot
            )),
        }
    }

    pub(in crate::renderer::native_vulkan) fn slot_state(
        &self,
        frame_slot: u32,
    ) -> Result<NativeVulkanSceneFrameSlotState, String> {
        Ok(*self.slot(frame_slot)?)
    }

    pub(in crate::renderer::native_vulkan) fn in_flight_count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.in_flight).count()
    }

    pub(in crate::renderer::native_vulkan) fn frame_slot_count(&self) -> usize {
        self.slots.len()
    }

    fn slot(&self, frame_slot: u32) -> Result<&NativeVulkanSceneFrameSlotState, String> {
        self.slots
            .get(frame_slot as usize)
            .ok_or_else(|| format!("scene frame slot {frame_slot} is outside completion tracker"))
    }

    fn slot_mut(
        &mut self,
        frame_slot: u32,
    ) -> Result<&mut NativeVulkanSceneFrameSlotState, String> {
        self.slots
            .get_mut(frame_slot as usize)
            .ok_or_else(|| format!("scene frame slot {frame_slot} is outside completion tracker"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneFrameResourceRelease {
    pub completed_submission: NativeVulkanSceneFrameSubmission,
    pub gpu_buffers: usize,
    pub material_uniform_buffers: usize,
    pub texture_images: usize,
    pub texture_staging_buffers: usize,
}

impl NativeVulkanSceneFrameResourceRelease {
    pub(in crate::renderer::native_vulkan) fn total_retirements(self) -> usize {
        self.gpu_buffers
            .saturating_add(self.material_uniform_buffers)
            .saturating_add(self.texture_images)
            .saturating_add(self.texture_staging_buffers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_submission_only_covers_same_completed_slot() {
        let completed = NativeVulkanSceneFrameSubmission::new(1, 9);

        assert!(completed.covers(NativeVulkanSceneFrameSubmission::new(1, 8)));
        assert!(completed.covers(NativeVulkanSceneFrameSubmission::new(1, 9)));
        assert!(!completed.covers(NativeVulkanSceneFrameSubmission::new(1, 10)));
        assert!(!completed.covers(NativeVulkanSceneFrameSubmission::new(0, 8)));
    }

    #[test]
    fn frame_resource_release_sums_all_resource_classes() {
        let release = NativeVulkanSceneFrameResourceRelease {
            completed_submission: NativeVulkanSceneFrameSubmission::new(2, 12),
            gpu_buffers: 3,
            material_uniform_buffers: 11,
            texture_images: 5,
            texture_staging_buffers: 7,
        };

        assert_eq!(release.total_retirements(), 26);
    }

    #[test]
    fn frame_completion_tracker_issues_monotonic_submissions_per_swapchain_slot() {
        let mut tracker = NativeVulkanSceneFrameCompletionTracker::new(3).expect("tracker");

        let first = tracker.begin_frame(2).expect("begin slot 2");
        let second = tracker.begin_frame(0).expect("begin slot 0");

        assert_eq!(tracker.frame_slot_count(), 3);
        assert_eq!(tracker.in_flight_count(), 2);
        assert_eq!(first, NativeVulkanSceneFrameSubmission::new(2, 1));
        assert_eq!(second, NativeVulkanSceneFrameSubmission::new(0, 2));
        assert!(second.covers(NativeVulkanSceneFrameSubmission::new(0, 1)));
        assert!(!second.covers(first));
    }

    #[test]
    fn frame_completion_tracker_requires_completion_before_reusing_slot() {
        let mut tracker = NativeVulkanSceneFrameCompletionTracker::new(2).expect("tracker");
        let first = tracker.begin_frame(1).expect("begin slot 1");

        let err = tracker
            .begin_frame(1)
            .expect_err("in-flight slot cannot be reused");
        assert!(err.contains("still in flight"));

        let completion = tracker.complete_frame(first).expect("complete slot 1");
        assert_eq!(
            completion,
            NativeVulkanSceneFrameCompletion {
                submission: first,
                newly_completed: true
            }
        );
        assert_eq!(tracker.in_flight_count(), 0);
        assert_eq!(tracker.slot_state(1).unwrap().last_completed, Some(first));

        let second = tracker.begin_frame(1).expect("reuse completed slot");
        assert_eq!(second, NativeVulkanSceneFrameSubmission::new(1, 2));
    }

    #[test]
    fn frame_completion_tracker_rejects_wrong_or_unknown_completion() {
        let mut tracker = NativeVulkanSceneFrameCompletionTracker::new(1).expect("tracker");
        let submitted = tracker.begin_frame(0).expect("begin slot 0");

        let wrong = tracker
            .complete_frame(NativeVulkanSceneFrameSubmission::new(
                0,
                submitted.submission_index + 1,
            ))
            .expect_err("wrong submission must fail");
        assert!(wrong.contains("last submitted"));
        let unknown = tracker
            .complete_frame(NativeVulkanSceneFrameSubmission::new(3, 1))
            .expect_err("unknown slot must fail");
        assert!(unknown.contains("outside completion tracker"));

        let first_completion = tracker.complete_frame(submitted).expect("complete");
        let duplicate_completion = tracker
            .complete_frame(submitted)
            .expect("duplicate completion is idempotent");
        assert!(first_completion.newly_completed);
        assert!(!duplicate_completion.newly_completed);
    }
}
