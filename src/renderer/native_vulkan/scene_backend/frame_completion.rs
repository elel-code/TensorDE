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
}
