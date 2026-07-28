//! Dynamic-rendering local-read command metadata and synchronization.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - Vulkan 1.4 `VK_KHR_dynamic_rendering_local_read` / roadmap-2026 revision 11
//! - Vulkan 1.4 valid usage VUIDs 09512..09525
//!
//! This module is deliberately independent of scene shader names and graph
//! indices. It only turns an already-proven typed attachment contract into
//! Vulkan mapping structs and a by-region synchronization dependency.

use std::collections::HashSet;

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::scene::BuiltinSceneLocalReadShader;

use super::descriptor_layout::ScenePipelineShaderDescriptorAccess;

mod scope_plan;

pub(super) use scope_plan::{
    SceneLocalReadScopePassRole, SceneLocalReadScopePlan, scene_local_read_scope_plans,
};

const LOCAL_READ_PRODUCER_STAGE: vk::PipelineStageFlags2 =
    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
const LOCAL_READ_CONSUMER_STAGE: vk::PipelineStageFlags2 =
    vk::PipelineStageFlags2::FRAGMENT_SHADER;
const LOCAL_READ_WRITE_ACCESS: vk::AccessFlags2 = vk::AccessFlags2::COLOR_ATTACHMENT_WRITE;
const LOCAL_READ_READ_ACCESS: vk::AccessFlags2 = vk::AccessFlags2::INPUT_ATTACHMENT_READ;

/// Physical-device limits consumed by the local-read pipeline contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SceneLocalReadDeviceLimits {
    pub(super) max_color_attachments: u32,
    pub(super) max_per_stage_descriptor_input_attachments: u32,
}

impl SceneLocalReadDeviceLimits {
    pub(super) const fn new(
        max_color_attachments: u32,
        max_per_stage_descriptor_input_attachments: u32,
    ) -> Self {
        Self {
            max_color_attachments,
            max_per_stage_descriptor_input_attachments,
        }
    }

    pub(super) fn from_physical_device_limits(limits: &vk::PhysicalDeviceLimits) -> Self {
        Self::new(
            limits.max_color_attachments,
            limits.max_per_stage_descriptor_input_attachments,
        )
    }
}

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
        Self::new_for_scope(
            color_attachment_locations,
            color_attachment_input_indices,
            max_color_attachments,
            max_per_stage_descriptor_input_attachments,
            true,
        )
    }

    fn new_for_scope(
        color_attachment_locations: &[u32],
        color_attachment_input_indices: &[u32],
        max_color_attachments: u32,
        max_per_stage_descriptor_input_attachments: u32,
        input_attachment_required: bool,
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
        if input_attachment_required
            && !color_attachment_input_indices
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

/// Fully typed graphics-pipeline metadata for one proven local-read scope.
///
/// The shader interface, descriptor access, color formats, attachment
/// locations, and input indices are validated together so no Vulkan pipeline
/// can infer one layer from another.  Scene command recording still owns the
/// decision to create and execute such a scope.
#[derive(Debug, Clone)]
pub(super) struct SceneLocalReadPipelineMetadata<'a> {
    shader: Option<&'a BuiltinSceneLocalReadShader>,
    color_attachment_formats: Vec<vk::Format>,
    attachment_mapping: SceneLocalReadAttachmentMapping,
}

impl<'a> SceneLocalReadPipelineMetadata<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        descriptor_access: &ScenePipelineShaderDescriptorAccess,
        shader: Option<&'a BuiltinSceneLocalReadShader>,
        color_attachment_formats: &[vk::Format],
        color_attachment_locations: &[u32],
        color_attachment_input_indices: &[u32],
        limits: SceneLocalReadDeviceLimits,
    ) -> Result<Self, String> {
        let shader = validate_scene_local_read_shader_variant(descriptor_access, shader)?;
        let shader_indices = shader
            .input_attachments
            .iter()
            .map(|input| input.input_attachment_index)
            .collect::<Vec<_>>();
        validate_unique_bounded_values(
            &shader_indices,
            limits.max_per_stage_descriptor_input_attachments,
            "shader input attachment index",
        )?;
        validate_unique_bounded_values(
            shader.color_output_locations,
            limits.max_color_attachments,
            "shader color output location",
        )?;

        if color_attachment_formats.len() != color_attachment_locations.len() {
            return Err(format!(
                "local-read pipeline color format/location arrays differ in length ({} vs {})",
                color_attachment_formats.len(),
                color_attachment_locations.len()
            ));
        }
        if color_attachment_formats.is_empty() {
            return Err("local-read pipeline has no color attachments".to_owned());
        }
        if color_attachment_formats
            .iter()
            .any(|format| *format == vk::Format::UNDEFINED)
        {
            return Err("local-read pipeline color attachment format is undefined".to_owned());
        }

        let attachment_mapping = SceneLocalReadAttachmentMapping::new(
            color_attachment_locations,
            color_attachment_input_indices,
            limits.max_color_attachments,
            limits.max_per_stage_descriptor_input_attachments,
        )?;
        validate_exact_non_unused_values(
            attachment_mapping.color_attachment_locations(),
            shader.color_output_locations,
            "color attachment location",
        )?;
        validate_exact_non_unused_values(
            attachment_mapping.color_attachment_input_indices(),
            &shader_indices,
            "input attachment index",
        )?;

        Ok(Self {
            shader: Some(shader),
            color_attachment_formats: color_attachment_formats.to_vec(),
            attachment_mapping,
        })
    }

    pub(super) fn output_only(
        descriptor_access: &ScenePipelineShaderDescriptorAccess,
        color_attachment_formats: &[vk::Format],
        color_attachment_locations: &[u32],
        limits: SceneLocalReadDeviceLimits,
    ) -> Result<Self, String> {
        let input_indices = vec![vk::ATTACHMENT_UNUSED; color_attachment_locations.len()];
        Self::output_only_with_input_mapping(
            descriptor_access,
            color_attachment_formats,
            color_attachment_locations,
            &input_indices,
            limits,
        )
    }

    pub(super) fn output_only_with_input_mapping(
        descriptor_access: &ScenePipelineShaderDescriptorAccess,
        color_attachment_formats: &[vk::Format],
        color_attachment_locations: &[u32],
        color_attachment_input_indices: &[u32],
        limits: SceneLocalReadDeviceLimits,
    ) -> Result<Self, String> {
        validate_unique_values(&descriptor_access.sampled_slots, "sampled slot")?;
        if !descriptor_access.input_attachment_slots.is_empty() {
            return Err(
                "local-read producer pipeline cannot declare input-attachment slots".to_owned(),
            );
        }
        if color_attachment_formats.len() != color_attachment_locations.len() {
            return Err(format!(
                "local-read producer color format/location arrays differ in length ({} vs {})",
                color_attachment_formats.len(),
                color_attachment_locations.len()
            ));
        }
        if color_attachment_formats.is_empty()
            || color_attachment_formats
                .iter()
                .any(|format| *format == vk::Format::UNDEFINED)
        {
            return Err(
                "local-read producer pipeline requires defined color attachment formats"
                    .to_owned(),
            );
        }
        validate_exact_non_unused_values(
            color_attachment_locations,
            &[0],
            "color attachment location",
        )?;
        let attachment_mapping = SceneLocalReadAttachmentMapping::new_for_scope(
            color_attachment_locations,
            color_attachment_input_indices,
            limits.max_color_attachments,
            limits.max_per_stage_descriptor_input_attachments,
            false,
        )?;
        Ok(Self {
            shader: None,
            color_attachment_formats: color_attachment_formats.to_vec(),
            attachment_mapping,
        })
    }

    pub(super) fn local_read_fragment_spirv(&self) -> Option<&'a [u32]> {
        self.shader.map(|shader| shader.fragment_spirv)
    }

    pub(super) fn color_attachment_formats(&self) -> &[vk::Format] {
        &self.color_attachment_formats
    }

    pub(super) fn input_attachment_binding(&self, slot: u32) -> Option<u32> {
        self.shader?
            .input_attachments
            .iter()
            .find(|input| input.slot == slot)
            .map(|input| input.binding)
    }

    pub(super) fn attachment_location_info(&self) -> vk::RenderingAttachmentLocationInfo {
        self.attachment_mapping.attachment_location_info()
    }

    pub(super) fn input_attachment_index_info(
        &self,
    ) -> vk::RenderingInputAttachmentIndexInfo {
        self.attachment_mapping.input_attachment_index_info()
    }

    pub(super) fn color_blend_attachments(
        &self,
        active: vk::PipelineColorBlendAttachmentState,
    ) -> Vec<vk::PipelineColorBlendAttachmentState> {
        let inactive = vk::PipelineColorBlendAttachmentState::builder()
            .blend_enable(false)
            .color_write_mask(vk::ColorComponentFlags::empty())
            .build();
        self.attachment_mapping
            .color_attachment_locations()
            .iter()
            .map(|location| {
                if *location == vk::ATTACHMENT_UNUSED {
                    inactive
                } else {
                    active
                }
            })
            .collect()
    }
}

pub(super) fn validate_scene_local_read_shader_variant<'a>(
    descriptor_access: &ScenePipelineShaderDescriptorAccess,
    shader: Option<&'a BuiltinSceneLocalReadShader>,
) -> Result<&'a BuiltinSceneLocalReadShader, String> {
    validate_unique_values(&descriptor_access.sampled_slots, "sampled slot")?;
    validate_unique_values(
        &descriptor_access.input_attachment_slots,
        "input-attachment slot",
    )?;
    if descriptor_access.input_attachment_slots.is_empty() {
        return Err(
            "local-read pipeline metadata requires a typed input-attachment slot".to_owned(),
        );
    }
    if descriptor_access
        .sampled_slots
        .iter()
        .any(|slot| descriptor_access.input_attachment_slots.contains(slot))
    {
        return Err(
            "local-read pipeline descriptor access overlaps sampled and input-attachment slots"
                .to_owned(),
        );
    }
    let shader = shader.ok_or_else(|| {
        "local-read pipeline shader has no explicit subpassInput variant".to_owned()
    })?;
    if shader.fragment_spirv.is_empty() {
        return Err("local-read pipeline subpassInput shader variant is empty".to_owned());
    }
    if shader.input_attachments.is_empty() {
        return Err("local-read pipeline shader interface has no input attachments".to_owned());
    }
    if shader.color_output_locations.is_empty() {
        return Err("local-read pipeline shader interface has no color outputs".to_owned());
    }

    let shader_slots = shader
        .input_attachments
        .iter()
        .map(|input| input.slot)
        .collect::<Vec<_>>();
    let shader_indices = shader
        .input_attachments
        .iter()
        .map(|input| input.input_attachment_index)
        .collect::<Vec<_>>();
    let shader_bindings = shader
        .input_attachments
        .iter()
        .map(|input| input.binding)
        .collect::<Vec<_>>();
    validate_unique_values(&shader_slots, "shader input-attachment slot")?;
    if shader_indices.contains(&vk::ATTACHMENT_UNUSED) {
        return Err(
            "local-read shader input attachment index cannot be VK_ATTACHMENT_UNUSED".to_owned(),
        );
    }
    validate_unique_values(&shader_indices, "shader input attachment index")?;
    validate_unique_values(&shader_bindings, "shader input-attachment binding")?;
    validate_unique_values(shader.color_output_locations, "shader color output location")?;
    if shader
        .color_output_locations
        .contains(&vk::ATTACHMENT_UNUSED)
    {
        return Err("local-read pipeline shader output location is VK_ATTACHMENT_UNUSED".to_owned());
    }
    if !same_value_set(
        &descriptor_access.input_attachment_slots,
        &shader_slots,
    ) {
        return Err(format!(
            "local-read descriptor input slots {:?} do not match shader interface {:?}",
            descriptor_access.input_attachment_slots, shader_slots
        ));
    }
    Ok(shader)
}

fn validate_exact_non_unused_values(
    mapped: &[u32],
    required: &[u32],
    label: &str,
) -> Result<(), String> {
    let mapped = mapped
        .iter()
        .copied()
        .filter(|value| *value != vk::ATTACHMENT_UNUSED)
        .collect::<Vec<_>>();
    if !same_value_set(&mapped, required) {
        return Err(format!(
            "local-read mapped {label}s {mapped:?} do not match shader interface {required:?}"
        ));
    }
    Ok(())
}

fn same_value_set(left: &[u32], right: &[u32]) -> bool {
    left.len() == right.len()
        && left.iter().all(|value| right.contains(value))
        && right.iter().all(|value| left.contains(value))
}

fn validate_unique_values(values: &[u32], label: &str) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        if !seen.insert(*value) {
            return Err(format!("local-read {label} {value} is duplicated"));
        }
    }
    Ok(())
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

/// Transitions a retained effect target from the ordinary sampled resting
/// layout into a local-read rendering scope.  The broad destination masks
/// cover both attachments because either one may be loaded/written, while the
/// source attachment is later read through the explicit by-region dependency.
pub(super) fn scene_local_read_scope_entry_barrier(
    image: vk::Image,
    subresource_range: vk::ImageSubresourceRange,
) -> vk::ImageMemoryBarrier2 {
    vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .src_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .dst_stage_mask(
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags2::FRAGMENT_SHADER,
        )
        .dst_access_mask(
            vk::AccessFlags2::COLOR_ATTACHMENT_READ
                | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags2::INPUT_ATTACHMENT_READ,
        )
        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .new_layout(vk::ImageLayout::RENDERING_LOCAL_READ)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(subresource_range)
        .build()
}

/// Returns a local-read attachment to the retained sampled resting layout
/// after every authored write in the scope has been stored.
pub(super) fn scene_local_read_scope_exit_barrier(
    image: vk::Image,
    subresource_range: vk::ImageSubresourceRange,
) -> vk::ImageMemoryBarrier2 {
    vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags2::FRAGMENT_SHADER,
        )
        .src_access_mask(
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags2::INPUT_ATTACHMENT_READ,
        )
        .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
        .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .old_layout(vk::ImageLayout::RENDERING_LOCAL_READ)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
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
    use crate::renderer::native_vulkan::scene::BuiltinSceneInputAttachment;
    use crate::renderer::native_vulkan::scene::native_vulkan_scene_shader_for_key;

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
    fn device_limits_snapshot_reads_the_vulkan_14_physical_limits() {
        let mut physical_limits = vk::PhysicalDeviceLimits::default();
        physical_limits.max_color_attachments = 6;
        physical_limits.max_per_stage_descriptor_input_attachments = 5;

        assert_eq!(
            SceneLocalReadDeviceLimits::from_physical_device_limits(&physical_limits),
            SceneLocalReadDeviceLimits::new(6, 5)
        );
    }

    fn catalog_local_read_shader() -> BuiltinSceneLocalReadShader {
        native_vulkan_scene_shader_for_key("we/passthrough")
            .expect("passthrough shader")
            .local_read_shader
            .expect("passthrough local-read shader")
    }

    fn descriptor_access(
        sampled_slots: &[u32],
        input_attachment_slots: &[u32],
    ) -> ScenePipelineShaderDescriptorAccess {
        ScenePipelineShaderDescriptorAccess {
            sampled_slots: sampled_slots.to_vec(),
            input_attachment_slots: input_attachment_slots.to_vec(),
        }
    }

    #[test]
    fn pipeline_metadata_carries_exact_shader_attachment_and_blend_mapping() {
        let shader = catalog_local_read_shader();
        let metadata = SceneLocalReadPipelineMetadata::new(
            &descriptor_access(&[1], &[0]),
            Some(&shader),
            &[vk::Format::R8G8B8A8_UNORM, vk::Format::R8G8B8A8_UNORM],
            &[vk::ATTACHMENT_UNUSED, 0],
            &[0, vk::ATTACHMENT_UNUSED],
            SceneLocalReadDeviceLimits::new(8, 8),
        )
        .expect("valid local-read pipeline metadata");

        assert_eq!(
            metadata.local_read_fragment_spirv(),
            Some(shader.fragment_spirv)
        );
        assert_eq!(
            metadata.color_attachment_formats(),
            &[vk::Format::R8G8B8A8_UNORM, vk::Format::R8G8B8A8_UNORM]
        );
        assert_eq!(metadata.input_attachment_binding(0), Some(64));
        assert_eq!(metadata.input_attachment_binding(1), None);

        let locations = metadata.attachment_location_info();
        let indices = metadata.input_attachment_index_info();
        assert_eq!(locations.color_attachment_count, 2);
        assert_eq!(indices.color_attachment_count, 2);
        unsafe {
            assert_eq!(
                std::slice::from_raw_parts(
                    locations.color_attachment_locations,
                    locations.color_attachment_count as usize,
                ),
                &[vk::ATTACHMENT_UNUSED, 0]
            );
            assert_eq!(
                std::slice::from_raw_parts(
                    indices.color_attachment_input_indices,
                    indices.color_attachment_count as usize,
                ),
                &[0, vk::ATTACHMENT_UNUSED]
            );
        }

        let active = vk::PipelineColorBlendAttachmentState::builder()
            .blend_enable(true)
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            )
            .build();
        let blend_attachments = metadata.color_blend_attachments(active);
        assert_eq!(blend_attachments.len(), 2);
        assert_eq!(
            blend_attachments[0].color_write_mask,
            vk::ColorComponentFlags::empty()
        );
        assert_eq!(blend_attachments[0].blend_enable, vk::FALSE);
        assert_eq!(blend_attachments[1], active);
    }

    #[test]
    fn output_only_pipeline_metadata_uses_the_same_two_attachment_scope_without_input() {
        let metadata = SceneLocalReadPipelineMetadata::output_only(
            &descriptor_access(&[1], &[]),
            &[vk::Format::R8G8B8A8_UNORM, vk::Format::R8G8B8A8_UNORM],
            &[0, vk::ATTACHMENT_UNUSED],
            SceneLocalReadDeviceLimits::new(8, 8),
        )
        .expect("valid local-read producer metadata");

        assert_eq!(metadata.local_read_fragment_spirv(), None);
        assert_eq!(metadata.input_attachment_binding(0), None);
        let indices = metadata.input_attachment_index_info();
        unsafe {
            assert_eq!(
                std::slice::from_raw_parts(
                    indices.color_attachment_input_indices,
                    indices.color_attachment_count as usize,
                ),
                &[vk::ATTACHMENT_UNUSED, vk::ATTACHMENT_UNUSED]
            );
        }
    }

    #[test]
    fn pipeline_metadata_rejects_missing_or_mismatched_shader_interface() {
        let shader = catalog_local_read_shader();
        let formats = [vk::Format::R8G8B8A8_UNORM];

        let missing = SceneLocalReadPipelineMetadata::new(
            &descriptor_access(&[], &[0]),
            None,
            &formats,
            &[0],
            &[0],
            SceneLocalReadDeviceLimits::new(8, 8),
        )
        .expect_err("input contract requires an explicit shader variant");
        assert!(missing.contains("no explicit subpassInput variant"));

        let overlap = SceneLocalReadPipelineMetadata::new(
            &descriptor_access(&[0], &[0]),
            Some(&shader),
            &formats,
            &[0],
            &[0],
            SceneLocalReadDeviceLimits::new(8, 8),
        )
        .expect_err("sampled and input slots must remain disjoint");
        assert!(overlap.contains("overlaps sampled and input-attachment slots"));

        let mismatch = SceneLocalReadPipelineMetadata::new(
            &descriptor_access(&[], &[1]),
            Some(&shader),
            &formats,
            &[0],
            &[0],
            SceneLocalReadDeviceLimits::new(8, 8),
        )
        .expect_err("shader and descriptor slots must match");
        assert!(mismatch.contains("do not match shader interface"));

        let empty_shader = BuiltinSceneLocalReadShader {
            fragment_spirv: &[],
            ..shader
        };
        let empty = SceneLocalReadPipelineMetadata::new(
            &descriptor_access(&[], &[0]),
            Some(&empty_shader),
            &formats,
            &[0],
            &[0],
            SceneLocalReadDeviceLimits::new(8, 8),
        )
        .expect_err("empty shader must fail");
        assert!(empty.contains("shader variant is empty"));

        const INVALID_INPUT: [BuiltinSceneInputAttachment; 1] = [BuiltinSceneInputAttachment {
            slot: 0,
            input_attachment_index: vk::ATTACHMENT_UNUSED,
            binding: 64,
        }];
        let invalid_index_shader = BuiltinSceneLocalReadShader {
            input_attachments: &INVALID_INPUT,
            ..shader
        };
        let invalid_index = SceneLocalReadPipelineMetadata::new(
            &descriptor_access(&[], &[0]),
            Some(&invalid_index_shader),
            &formats,
            &[0],
            &[0],
            SceneLocalReadDeviceLimits::new(8, 8),
        )
        .expect_err("shader input attachment index must be concrete");
        assert!(invalid_index.contains("cannot be VK_ATTACHMENT_UNUSED"));
    }

    #[test]
    fn pipeline_metadata_rejects_incomplete_scope_and_device_limit_metadata() {
        let shader = catalog_local_read_shader();
        let access = descriptor_access(&[], &[0]);

        for (formats, locations, indices, max_color, max_input, message) in [
            (
                &[vk::Format::R8G8B8A8_UNORM][..],
                &[vk::ATTACHMENT_UNUSED, 0][..],
                &[0, vk::ATTACHMENT_UNUSED][..],
                8,
                8,
                "format count",
            ),
            (
                &[vk::Format::UNDEFINED][..],
                &[0][..],
                &[0][..],
                8,
                8,
                "undefined format",
            ),
            (
                &[vk::Format::R8G8B8A8_UNORM][..],
                &[0][..],
                &[1][..],
                8,
                8,
                "input index mismatch",
            ),
            (
                &[vk::Format::R8G8B8A8_UNORM][..],
                &[1][..],
                &[0][..],
                8,
                8,
                "color output mismatch",
            ),
            (
                &[vk::Format::R8G8B8A8_UNORM][..],
                &[0][..],
                &[0][..],
                1,
                0,
                "input device limit",
            ),
        ] {
            let error = SceneLocalReadPipelineMetadata::new(
                &access,
                Some(&shader),
                formats,
                locations,
                indices,
                SceneLocalReadDeviceLimits::new(max_color, max_input),
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

        let entry = scene_local_read_scope_entry_barrier(image, color_range());
        assert_eq!(entry.old_layout, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        assert_eq!(entry.new_layout, vk::ImageLayout::RENDERING_LOCAL_READ);
        assert!(
            entry
                .dst_access_mask
                .contains(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        );
        assert!(
            entry
                .dst_access_mask
                .contains(vk::AccessFlags2::INPUT_ATTACHMENT_READ)
        );

        let exit = scene_local_read_scope_exit_barrier(image, color_range());
        assert_eq!(exit.old_layout, vk::ImageLayout::RENDERING_LOCAL_READ);
        assert_eq!(exit.new_layout, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        assert_eq!(exit.dst_access_mask, vk::AccessFlags2::SHADER_SAMPLED_READ);
    }
}
