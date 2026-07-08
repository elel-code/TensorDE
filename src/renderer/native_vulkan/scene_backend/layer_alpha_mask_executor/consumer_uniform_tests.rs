use super::super::consumer_command::{
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
};
use super::*;
use crate::engine::scene_engine::{
    SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorTarget, SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::pipeline::NativeVulkanScenePipelineVertexLayout;
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorVkFormat;

#[test]
fn generated_consumer_uniform_plan_pins_screen_uv_and_active_clipping_upload() {
    let commands = runtime_commands(vec![command()]);
    let plan = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_uniforms(&commands)
        .expect("generated consumer uniform contract");

    assert_eq!(plan.consumer_draw_count, 1);
    assert_eq!(plan.uniform_binding_count, 1);
    assert_eq!(plan.screen_uv_contract_count, 1);
    assert_eq!(plan.active_clipping_upload_contract_count, 1);
    assert_eq!(plan.slot8_alpha_sample_count, 1);

    let binding = &plan.bindings[0];
    assert_eq!(binding.command_index, 7);
    assert_eq!(binding.consumer_draw_index, 3);
    assert_eq!(binding.shader, "we/genericimage4");
    assert_eq!(
        binding.shader_combo_values,
        vec!["CLIPPINGTARGET=1".to_owned(), "CLIPPINGUVS=1".to_owned()]
    );
    assert_eq!(binding.source_mask, SceneGraphTarget::FullAlphaMask);
    assert_eq!(
        binding.color_target,
        SceneGraphTarget::ObjectFinal(SceneObjectId(77))
    );
    assert_eq!(
        binding.screen_uv_formula,
        "(v_ScreenPos.xy / v_ScreenPos.z) * 0.5 + 0.5"
    );
    assert_eq!(
        binding.alpha_apply_formula,
        "gl_FragColor.a *= texSample2D(g_Texture8, screenUV).r"
    );
    assert_eq!(binding.active_clipping_max_count, 0x0b);
    assert_eq!(binding.active_clipping_count_state_offset, 0x12ea);
    assert_eq!(binding.active_clipping_raw_dword_state_offset, 0x1330);
    assert_eq!(binding.active_clipping_index_state_offset, 0x1334);
    assert_eq!(binding.active_clipping_weight_state_offset, 0x1360);
    assert_eq!(binding.active_clipping_transform_state_offset, 0x0cb0);
    assert_eq!(binding.active_clipping_optional_flag_state_offset, 0x138c);
    assert_eq!(binding.active_clipping_optional_float_state_offset, 0x1390);
    assert_eq!(binding.active_clipping_bitset_layer_aux_offset, 0x0398);
    assert_eq!(binding.active_clipping_weight_layer_aux_offset, 0x03a0);
    assert_eq!(binding.material_uniform_buffer_handle, 0x4200);
    assert_eq!(binding.material_uniform_device_address, 0x4280);
    assert_eq!(binding.material_uniform_bytes, 48);
    assert_eq!(binding.material_uniform_payload_hash, 0x1234);
    assert_eq!(
        binding.gpu_uniform_upload_status,
        "retained generated-material uniform buffer resolved from +0x428 state"
    );
    assert!(
        binding
            .reference_points
            .iter()
            .any(|reference| reference.contains("active clipping uniform upload"))
    );
}

#[test]
fn generated_consumer_uniform_plan_rejects_missing_clippinguvs_combo() {
    let mut command = command();
    command.shader_combo_values = vec!["CLIPPINGTARGET=1".to_owned()];
    let commands = runtime_commands(vec![command]);

    let err = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_uniforms(&commands)
        .expect_err("CLIPPINGUVS must be present");

    assert!(err.contains("CLIPPINGTARGET=1 and CLIPPINGUVS=1"));
}

#[test]
fn generated_consumer_uniform_plan_rejects_non_full_mask_source() {
    let mut command = command();
    command.source_mask = SceneGraphTarget::FullAlphaMaskIntermediate;
    let commands = runtime_commands(vec![command]);

    let err = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_uniforms(&commands)
        .expect_err("generated consumer must sample full mask");

    assert!(err.contains("must sample FullAlphaMask"));
}

fn runtime_commands(
    commands: Vec<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan>,
) -> NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan {
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan {
        command_count: commands.len(),
        warmed_pipeline_count: 1,
        descriptor_heap_bind_count: commands.len(),
        target_scope_count: commands.len(),
        pipeline_bind_count: commands.len(),
        resource_heap_bind_count: commands.len(),
        rt_method_8_bridge_count: commands.len(),
        rt_method_8_indexed_draw_count: commands.len(),
        commands,
        command_order: [
            "require_warmed_genericimage4_clippingtarget_pipelines",
            "resolve_generated_clippingtarget_heap_binds",
            "join_generated_draw_target_pipeline_contracts",
            "join_rt_method_8_bridge_plan",
            "preserve_token1_effective_alpha_formula",
            "build_generated_consumer_command_plan",
        ],
    }
}

fn command() -> NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan {
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan {
        consumer_draw_index: 3,
        command_index: 7,
        object: SceneObjectId(77),
        shader: "we/genericimage4",
        shader_combo_values: vec!["CLIPPINGTARGET=1".to_owned(), "CLIPPINGUVS=1".to_owned()],
        source_mask: SceneGraphTarget::FullAlphaMask,
        draw_receiver: SceneLayerCompositorTarget::LayerTarget490,
        color_target: SceneGraphTarget::ObjectFinal(SceneObjectId(77)),
        target_format: NativeVulkanSceneTextureDescriptorVkFormat::B8G8R8A8Unorm,
        target_format_label: "B8G8R8A8_UNORM",
        width: 3840,
        height: 2160,
        pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
        vertex_layout: NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
        heap_bind_index: 4,
        heap_slice_index: 4,
        base_resource_descriptor_index: 8,
        base_sampler_descriptor_index: 24,
        resource_descriptor_count: 3,
        texture_count: 2,
        material_uniform_buffer_handle: 0x4200,
        material_uniform_device_address: 0x4280,
        material_uniform_bytes: 48,
        material_uniform_payload_hash: 0x1234,
        shader_mappings: vec![
            "WE PSSetConstantBuffers(slot=3) -> alpha-mask-heap-slice-offset0".to_owned(),
            "we.texture_slot0.g_Texture0 -> alpha-mask-heap-slice-offset1".to_owned(),
            "we.texture_slot8.g_Texture8 -> alpha-mask-heap-slice-offset2".to_owned(),
        ],
        material_source: "local generated material variant +0x428",
        blend_byte_source: "subdraw+0x40 -> generated material +0x1f0",
        geometry_source: "0x14020b15e first/current MDLV entry-owner geometry for [layer+0x490] RT method [8]",
        rt_method8_bridge_index: 0,
        rt_method8_call_site: "0x14020908c",
        rt_method8_method_vma: "0x1400eacd0",
        effective_alpha_formula: "src.a * FullAlphaMask.r with translucent src-alpha/inv-src-alpha blend",
        pipeline_bind_count: 1,
        resource_heap_bind_count: 1,
        target_bind_count: 1,
        rt_method_8_indexed_draw_count: 1,
        draw_call: "[layer+0x490].vtable+0x40",
        command_order: [
            "require_warmed_genericimage4_clippingtarget_pipeline_variant",
            "resolve_generated_clippingtarget_resource_heap_bind",
            "resolve_layer_0x490_current_color_target_scope",
            "preserve_generated_material_0x428_and_blend_0x1f0",
            "validate_rt_method_8_bridge_call_site",
            "bind_generated_clippingtarget_pipeline_variant",
            "bind_generated_clippingtarget_resource_heap_ext",
            "record_rt_method_8_indexed_vector_draw",
        ],
    }
}
