//! Dynamic-rendering local-read command metadata and synchronization.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - Vulkan 1.4 `VK_KHR_dynamic_rendering_local_read` / roadmap-2026 revision 11
//! - Vulkan 1.4 valid usage VUIDs 09512..09525
//!
//! This module is deliberately independent of scene shader names and graph
//! indices. It only turns an already-proven typed attachment contract into
//! Vulkan mapping structs and a by-region synchronization dependency.

use std::collections::HashSet;

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

const LOCAL_READ_PRODUCER_STAGE: vk::PipelineStageFlags2 =
    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
const LOCAL_READ_CONSUMER_STAGE: vk::PipelineStageFlags2 =
    vk::PipelineStageFlags2::FRAGMENT_SHADER;
const LOCAL_READ_WRITE_ACCESS: vk::AccessFlags2 = vk::AccessFlags2::COLOR_ATTACHMENT_WRITE;
const LOCAL_READ_READ_ACCESS: vk::AccessFlags2 = vk::AccessFlags2::INPUT_ATTACHMENT_READ;

/// The dynamic-rendering color-output and input-attachment index mapping for
/// one rendering scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SceneLocalReadAttachmentMapping {
    color_attachment_locations: Vec<u32>,
    color_attachment_input_indices: Vec<u32>,
}

impl SceneLocalReadAttachmentMapping {
    /// Validates and retains one pair of Vulkan attachment arrays.
    ///
    /// `color_attachment_locations` and `color_attachment_input_indices` are
    /// both indexed by the color attachment array passed to
    /// `VkRenderingInfo`. `VK_ATTACHMENT_UNUSED` is permitted in either array;
    /// all other values must be unique and within the device limits supplied by
    /// the caller. A mapping with no actual input attachment is rejected so it
    /// cannot accidentally open a local-read scope for an ordinary sampled
    /// edge.
    pub(super) fn new(
        color_attachment_locations: &[u32],
        color_attachment_input_indices: &[u32],
        max_color_attachments: u32,
        max_per_stage_descriptor_input_attachments: u32,
    ) -> Result<Self, String> {
        if color_attachment_locations.len() != color_attachment_input_indices.len() {
            return Err(format!(
                "local-read color location/input-index arrays differ in length ({} vs {})",
                color_attachment_locations.len(),
                color_attachment_input_indices.len()
            ));
        }
        let color_attachment_count = u32::try_from(color_attachment_locations.len())
            .map_err(|_| "local-read color attachment count exceeds u32".to_owned())?;
        if color_attachment_count > max_color_attachments {
            return Err(format!(
                "local-read color attachment count {color_attachment_count} exceeds device limit {max_color_attachments}"
            ));
        }
        validate_unique_bounded_values(
            color_attachment_locations,
            max_color_attachments,
            "color attachment location",
        )?;
        validate_unique_bounded_values(
            color_attachment_input_indices,
            max_per_stage_descriptor_input_attachments,
            "input attachment index",
        )?;
        if !color_attachment_input_indices
            .iter()
            .any(|index| *index != vk::ATTACHMENT_UNUSED)
        {
            return Err("local-read mapping has no input attachment index".to_owned());
        }

        Ok(Self {
            color_attachment_locations: color_attachment_locations.to_vec(),
            color_attachment_input_indices: color_attachment_input_indices.to_vec(),
        })
    }

    pub(super) fn color_attachment_count(&self) -> u32 {
        self.color_attachment_locations.len() as u32
    }

    pub(super) fn color_attachment_locations(&self) -> &[u32] {
        &self.color_attachment_locations
    }

    pub(super) fn color_attachment_input_indices(&self) -> &[u32] {
        &self.color_attachment_input_indices
    }

    /// Builds the mapping used by dynamic rendering and graphics-pipeline
    /// metadata. The returned raw pointers remain valid while `self` lives.
    pub(super) fn attachment_location_info(&self) -> vk::RenderingAttachmentLocationInfo {
        vk::RenderingAttachmentLocationInfo::builder()
            .color_attachment_locations(&self.color_attachment_locations)
            .build()
    }

    /// Builds the input-attachment mapping used by dynamic rendering and
    /// graphics-pipeline metadata. The returned raw pointers remain valid while
    /// `self` lives.
    pub(super) fn input_attachment_index_info(&self) -> vk::RenderingInputAttachmentIndexInfo {
        vk::RenderingInputAttachmentIndexInfo::builder()
            .color_attachment_input_indices(&self.color_attachment_input_indices)
            .build()
    }
}

fn validate_unique_bounded_values(
    values: &[u32],
    upper_bound: u32,
    label: &str,
) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        if *value == vk::ATTACHMENT_UNUSED {
            continue;
        }
        if *value >= upper_bound {
            return Err(format!(
                "local-read {label} {value} exceeds device limit {upper_bound}"
            ));
        }
        if !seen.insert(*value) {
            return Err(format!("local-read {label} {value} is duplicated"));
        }
    }
    Ok(())
}

/// Records both mapping commands inside an active dynamic-rendering scope.
///
/// The mapping is retained by the caller for the full duration of command
/// recording; Vulkan copies the arrays when each command is recorded.
pub(super) unsafe fn record_scene_local_read_attachment_mapping(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    mapping: &SceneLocalReadAttachmentMapping,
) {
    let location_info = mapping.attachment_location_info();
    let input_attachment_index_info = mapping.input_attachment_index_info();
    unsafe {
        device.cmd_set_rendering_attachment_locations(command_buffer, &location_info);
        device.cmd_set_rendering_input_attachment_indices(
            command_buffer,
            &input_attachment_index_info,
        );
    }
}

/// Produces the layout transition used before entering a local-read rendering
/// scope when the producer last left the image in color-attachment layout.
pub(super) fn scene_local_read_attachment_transition_barrier(
    image: vk::Image,
    subresource_range: vk::ImageSubresourceRange,
) -> vk::ImageMemoryBarrier2 {
    vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(LOCAL_READ_PRODUCER_STAGE)
        .src_access_mask(LOCAL_READ_WRITE_ACCESS)
        .dst_stage_mask(LOCAL_READ_CONSUMER_STAGE)
        .dst_access_mask(LOCAL_READ_READ_ACCESS)
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::RENDERING_LOCAL_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(subresource_range)
        .build()
}

/// Produces the by-region dependency between an authored color write and a
/// later exact-pixel input-attachment read in the same rendering scope.
pub(super) fn scene_local_read_producer_to_consumer_barrier(
    image: vk::Image,
    subresource_range: vk::ImageSubresourceRange,
) -> vk::ImageMemoryBarrier2 {
    vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(LOCAL_READ_PRODUCER_STAGE)
        .src_access_mask(LOCAL_READ_WRITE_ACCESS)
        .dst_stage_mask(LOCAL_READ_CONSUMER_STAGE)
        .dst_access_mask(LOCAL_READ_READ_ACCESS)
        .old_layout(vk::ImageLayout::RENDERING_LOCAL_READ)
        .new_layout(vk::ImageLayout::RENDERING_LOCAL_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(subresource_range)
        .build()
}

/// Builds the synchronization2 dependency carrying the required by-region
/// flag. The caller must keep `barrier` alive while recording the dependency.
pub(super) fn scene_local_read_by_region_dependency(
    barrier: &vk::ImageMemoryBarrier2,
) -> vk::DependencyInfo {
    vk::DependencyInfo::builder()
        .dependency_flags(vk::DependencyFlags::BY_REGION)
        .image_memory_barriers(std::slice::from_ref(barrier))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color_range() -> vk::ImageSubresourceRange {
        vk::ImageSubresourceRange::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1)
            .build()
    }

    #[test]
    fn mapping_builds_matching_vulkan_location_and_input_index_arrays() {
        let mapping = SceneLocalReadAttachmentMapping::new(&[0, 2], &[0, 1], 4, 4)
            .expect("valid local-read mapping");
        let locations = mapping.attachment_location_info();
        let indices = mapping.input_attachment_index_info();

        assert_eq!(mapping.color_attachment_count(), 2);
        assert_eq!(locations.color_attachment_count, 2);
        assert_eq!(indices.color_attachment_count, 2);
        unsafe {
            assert_eq!(
                std::slice::from_raw_parts(
                    locations.color_attachment_locations,
                    locations.color_attachment_count as usize,
                ),
                &[0, 2]
            );
            assert_eq!(
                std::slice::from_raw_parts(
                    indices.color_attachment_input_indices,
                    indices.color_attachment_count as usize,
                ),
                &[0, 1]
            );
        }
    }

    #[test]
    fn mapping_allows_unused_entries_but_rejects_ambiguous_or_out_of_range_values() {
        let mapping = SceneLocalReadAttachmentMapping::new(
            &[0, vk::ATTACHMENT_UNUSED, 2],
            &[vk::ATTACHMENT_UNUSED, 1, 2],
            4,
            4,
        )
        .expect("unused entries are valid");
        assert_eq!(mapping.color_attachment_locations(), &[0, vk::ATTACHMENT_UNUSED, 2]);
        assert_eq!(
            mapping.color_attachment_input_indices(),
            &[vk::ATTACHMENT_UNUSED, 1, 2]
        );

        for (locations, indices, max_color, max_input, message) in [
            (&[0][..], &[0, 1][..], 4, 4, "length"),
            (&[0, 0][..], &[0, 1][..], 4, 4, "duplicate location"),
            (&[4][..], &[0][..], 4, 4, "location limit"),
            (&[0, 1][..], &[0, 0][..], 4, 4, "duplicate input"),
            (&[0][..], &[4][..], 4, 4, "input limit"),
            (&[0][..], &[vk::ATTACHMENT_UNUSED][..], 4, 4, "missing input"),
        ] {
            let error = SceneLocalReadAttachmentMapping::new(
                locations, indices, max_color, max_input,
            )
            .expect_err(message);
            assert!(!error.is_empty(), "{message}");
        }
    }

    #[test]
    fn producer_consumer_barrier_uses_local_read_layout_and_by_region() {
        let image = vk::Image::from_raw(17);
        let barrier = scene_local_read_producer_to_consumer_barrier(image, color_range());
        assert_eq!(barrier.image, image);
        assert_eq!(barrier.old_layout, vk::ImageLayout::RENDERING_LOCAL_READ);
        assert_eq!(barrier.new_layout, vk::ImageLayout::RENDERING_LOCAL_READ);
        assert_eq!(barrier.src_stage_mask, LOCAL_READ_PRODUCER_STAGE);
        assert_eq!(barrier.dst_stage_mask, LOCAL_READ_CONSUMER_STAGE);
        assert_eq!(barrier.src_access_mask, LOCAL_READ_WRITE_ACCESS);
        assert_eq!(barrier.dst_access_mask, LOCAL_READ_READ_ACCESS);
        assert_eq!(barrier.src_queue_family_index, vk::QUEUE_FAMILY_IGNORED);
        assert_eq!(barrier.dst_queue_family_index, vk::QUEUE_FAMILY_IGNORED);

        let dependency = scene_local_read_by_region_dependency(&barrier);
        assert_eq!(dependency.dependency_flags, vk::DependencyFlags::BY_REGION);
        assert_eq!(dependency.image_memory_barrier_count, 1);
        unsafe {
            assert_eq!(*dependency.image_memory_barriers, barrier);
        }

        let transition =
            scene_local_read_attachment_transition_barrier(image, color_range());
        assert_eq!(transition.old_layout, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        assert_eq!(transition.new_layout, vk::ImageLayout::RENDERING_LOCAL_READ);
    }
}
