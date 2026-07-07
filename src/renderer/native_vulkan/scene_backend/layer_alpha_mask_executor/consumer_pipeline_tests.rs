use super::*;
use crate::engine::scene_engine::{
    SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorOperation,
    SceneLayerCompositorTarget, SceneObjectId,
};

#[test]
fn generated_consumer_pipeline_derives_clippingtarget_shader_variant_key() {
    let consumer_draws = consumer_draws(vec![consumer_binding(0, 4)]);

    let plan = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines(
        &consumer_draws,
        NativeVulkanSceneTextureDescriptorVkFormat::B8G8R8A8Unorm,
        SceneGraphPipelineClass::PuppetSkinning,
    )
    .expect("generated consumer pipeline plan");

    assert_eq!(plan.consumer_draw_count, 1);
    assert_eq!(plan.pipeline_binding_count, 1);
    assert_eq!(plan.cache_key_count, 1);
    assert_eq!(plan.texture_slot_mask, (1u32 << 0) | (1u32 << 8));
    assert_eq!(plan.shader_combo_override_count, 2);
    let binding = &plan.bindings[0];
    assert_eq!(binding.shader, "we/genericimage4");
    assert_eq!(
        binding.shader_combo_values,
        native_vulkan_scene_pipeline_shader_combo_values(&[
            ("CLIPPINGTARGET", 1),
            ("CLIPPINGUVS", 1),
        ])
    );
    assert_eq!(
        binding.vertex_layout,
        NativeVulkanScenePipelineVertexLayout::SceneMeshV0
    );
    assert_eq!(binding.heap_bind_index, 4);
    assert_eq!(binding.resource_descriptor_count, 3);
    assert_eq!(binding.texture_count, 2);
    assert_eq!(binding.material_uniform_buffer_handle, 0x4204);
    assert_eq!(binding.material_uniform_device_address, 0x4284);
    assert_eq!(
        binding.material_source,
        "local generated material variant +0x428"
    );
    assert_eq!(
        binding.blend_byte_source,
        "subdraw+0x40 -> generated material +0x1f0"
    );

    let cache_key = &plan.cache_keys()[0];
    assert_eq!(cache_key.shader, "we/genericimage4");
    assert_eq!(cache_key.shader_combo_values, binding.shader_combo_values);
    assert_eq!(
        cache_key.pipeline_class,
        SceneGraphPipelineClass::PuppetSkinning
    );
    assert_eq!(
        cache_key.target_format,
        vulkanalia::vk::Format::B8G8R8A8_UNORM
    );
    assert_eq!(cache_key.texture_slot_mask, (1u32 << 0) | (1u32 << 8));
}

#[test]
fn generated_consumer_pipeline_rejects_alpha_mask_target_format() {
    let consumer_draws = consumer_draws(vec![consumer_binding(0, 4)]);

    let err = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines(
        &consumer_draws,
        NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm,
        SceneGraphPipelineClass::PuppetSkinning,
    )
    .expect_err("generated consumer needs color target");

    assert!(err.contains("color layer target format"));
}

#[test]
fn generated_consumer_pipeline_rejects_non_mesh_geometry_class() {
    let consumer_draws = consumer_draws(vec![consumer_binding(0, 4)]);

    let err = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines(
        &consumer_draws,
        NativeVulkanSceneTextureDescriptorVkFormat::B8G8R8A8Unorm,
        SceneGraphPipelineClass::Quad,
    )
    .expect_err("generated consumer needs retained subdraw geometry");

    assert!(err.contains("mesh/subdraw geometry"));
}

fn consumer_draws(
    bindings: Vec<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan>,
) -> NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan {
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan {
        command_count: bindings.len(),
        consumer_draw_count: bindings.len(),
        heap_binding_count: bindings.len(),
        texture_slot_mask: bindings
            .iter()
            .fold(0u32, |mask, binding| mask | binding.texture_slot_mask),
        bindings,
        command_order: [
            "read_generated_clippingtarget_schedule_steps",
            "resolve_single_generated_clippingtarget_heap_bind",
            "validate_genericimage4_clippingtarget_slots_0_8",
            "preserve_generated_material_0x428",
            "preserve_subdraw_blend_byte_to_material_0x1f0",
            "preserve_layer_0x490_generated_draw_receiver",
        ],
    }
}

fn consumer_binding(
    consumer_draw_index: usize,
    heap_bind_index: usize,
) -> NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan {
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan {
        consumer_draw_index,
        command_index: consumer_draw_index + 7,
        object: SceneObjectId(77),
        operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
        source_mask: SceneGraphTarget::FullAlphaMask,
        target: SceneLayerCompositorTarget::LayerTarget490,
        target_receiver: "[layer+0x490]",
        draw_receiver_vtable_offset: "0x40",
        shader: "we/genericimage4",
        texture_slot_mask: (1u32 << 0) | (1u32 << 8),
        required_texture_slots: [0, 8],
        heap_bind_index,
        heap_slice_index: heap_bind_index,
        base_resource_descriptor_index: heap_bind_index * 2,
        base_sampler_descriptor_index: heap_bind_index * 2 + 16,
        resource_descriptor_count: 3,
        texture_count: 2,
        material_uniform_buffer_handle: 0x4200 + heap_bind_index as u64,
        material_uniform_device_address: 0x4280 + heap_bind_index as u64,
        material_uniform_bytes: 48,
        material_uniform_payload_hash: 0x1234 + heap_bind_index as u64,
        blend_byte_source: "subdraw+0x40 -> generated material +0x1f0",
        generated_material_source: "local generated material variant +0x428",
        shader_mappings: vec![
            "WE PSSetConstantBuffers(slot=3) -> alpha-mask-heap-slice-offset0".to_owned(),
            "we.texture_slot0.g_Texture0 -> alpha-mask-heap-slice-offset1".to_owned(),
            "we.texture_slot8.g_Texture8 -> alpha-mask-heap-slice-offset2".to_owned(),
        ],
        command_order: [
            "read_generated_clippingtarget_token_step",
            "match_single_generated_clippingtarget_heap_bind",
            "validate_slot0_source_and_slot8_full_alpha_mask",
            "preserve_subdraw_blend_byte_to_generated_material_0x1f0",
            "preserve_layer_0x490_rt_method_8_draw_receiver",
            "defer_uniform_and_geometry_lowering_to_generated_consumer_recorder",
        ],
    }
}
