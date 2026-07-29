//! Temporary set/binding-to-heap mappings for scene shaders not yet native.

use crate::renderer::native_vulkan::scene::BuiltinSceneDescriptorHeapMode;
use crate::renderer::native_vulkan::{
    NativeVulkanDescriptorHeapShaderBindingMapping,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_mixed_input_attachment_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping,
};

use super::super::descriptor_layout::{
    ScenePipelineDescriptorLayout, ScenePipelineShaderDescriptorAccess,
};
use super::super::local_read::SceneLocalReadPipelineMetadata;

pub(super) fn scene_fragment_descriptor_mappings(
    mode: BuiltinSceneDescriptorHeapMode,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_layout: &ScenePipelineDescriptorLayout,
    descriptor_access: &ScenePipelineShaderDescriptorAccess,
    local_read_metadata: Option<&SceneLocalReadPipelineMetadata<'_>>,
) -> Result<Vec<NativeVulkanDescriptorHeapShaderBindingMapping>, String> {
    if mode == BuiltinSceneDescriptorHeapMode::Native {
        return Ok(Vec::new());
    }

    let mut mappings = Vec::new();
    if descriptor_layout.material_uniform_enabled {
        mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                descriptor_heap_plan,
                3,
                0,
                1,
            )?,
        );
    }
    for slot in &descriptor_access.sampled_slots {
        let sampled_index = descriptor_layout
            .sampled_slots
            .iter()
            .position(|candidate| candidate == slot)
            .ok_or_else(|| {
                format!(
                    "scene shader sampled slot {slot} is absent from the global descriptor layout"
                )
            })?;
        mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping(
                descriptor_heap_plan,
                scene_sampled_shader_binding(*slot),
                0,
                descriptor_layout.sampled_resource_offset() + sampled_index,
                0,
                sampled_index,
            )?,
        );
    }
    for slot in &descriptor_access.input_attachment_slots {
        let shader_binding = local_read_metadata
            .and_then(|metadata| metadata.input_attachment_binding(*slot))
            .ok_or_else(|| {
                format!(
                    "scene shader input-attachment slot {slot} has no typed local-read shader binding"
                )
            })?;
        let input_index = descriptor_layout
            .input_attachment_slots
            .iter()
            .position(|candidate| candidate == slot)
            .ok_or_else(|| {
                format!(
                    "scene shader input-attachment slot {slot} is absent from the global descriptor layout"
                )
            })?;
        mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_mixed_input_attachment_binding_mapping(
                descriptor_heap_plan,
                shader_binding,
                0,
                descriptor_layout.input_attachment_resource_offset() + input_index,
            )?,
        );
    }
    Ok(mappings)
}

fn scene_sampled_shader_binding(slot: u32) -> u32 {
    if slot == 3 { 35 } else { slot }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_vulkan::{
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
        NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
        native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    };

    #[test]
    fn native_fragment_stage_has_no_set_binding_mapping_chain() {
        let layout = ScenePipelineDescriptorLayout {
            sampled_slots: vec![0],
            input_attachment_slots: Vec::new(),
            material_uniform_enabled: true,
            skinning_storage_enabled: false,
            scene_owned_uniform_count: 0,
        };
        let access = ScenePipelineShaderDescriptorAccess {
            sampled_slots: vec![0],
            input_attachment_slots: Vec::new(),
        };
        let plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    max_sampler_heap_size: 4096,
                    image_descriptor_size: 32,
                    image_descriptor_alignment: 32,
                    buffer_descriptor_size: 32,
                    buffer_descriptor_alignment: 32,
                    sampler_descriptor_size: 16,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );
        assert!(plan.backend_ready);
        assert!(
            scene_fragment_descriptor_mappings(
                BuiltinSceneDescriptorHeapMode::Native,
                &plan,
                &layout,
                &access,
                None,
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            scene_fragment_descriptor_mappings(
                BuiltinSceneDescriptorHeapMode::Mapped,
                &plan,
                &layout,
                &access,
                None,
            )
            .unwrap()
            .len(),
            2
        );
    }
}
