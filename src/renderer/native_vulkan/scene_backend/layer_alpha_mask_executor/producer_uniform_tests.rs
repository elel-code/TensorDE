use super::super::producer_draws::{
    CLIPPINGMASKIMAGE4_MATERIAL, CLIPPINGMASKIMAGE4_SHADER,
    NativeVulkanSceneLayerAlphaMaskProducerDrawPlan,
    NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
};
use super::super::producer_pipeline::native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines;
use super::super::resource_binds::NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan;
use super::super::{
    NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskTextureBindRole,
};
use super::*;
use crate::engine::scene_engine::{
    SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorCondition,
    SceneLayerCompositorOperation, SceneObjectId, ScenePuppetId, SceneResourceId,
};
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::{
    NativeVulkanSceneLayerAlphaMaskHeapSliceBinding, NativeVulkanSceneLayerAlphaMaskHeapSliceKey,
    NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan,
};
use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;

#[test]
fn producer_uniform_plan_pins_render_var0_clear_scalar_and_morph_slot_contracts() {
    let producer_draws = producer_draws(vec![draw(0, vec![0])]);
    let resource_binds = resource_binds(vec![bind(0, 0, vec![0, 1])]);
    let pipelines = native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines(
        &producer_draws,
        &resource_binds,
    )
    .expect("producer pipeline plan");

    let plan =
        native_vulkan_plan_scene_layer_alpha_mask_producer_uniforms(&producer_draws, &pipelines)
            .expect("producer uniform plan");

    assert_eq!(plan.producer_draw_count, 1);
    assert_eq!(plan.uniform_binding_count, 1);
    assert_eq!(plan.render_var0_contract_count, 1);
    assert_eq!(plan.clear_scalar_contract_count, 1);
    assert_eq!(plan.morph_texture_contract_count, 1);
    assert_eq!(plan.slot0_slot1_sample_contract_count, 1);

    let binding = &plan.bindings[0];
    assert_eq!(binding.command_index, 1);
    assert_eq!(binding.producer_draw_index, 0);
    assert_eq!(binding.shader, "we/clippingmaskimage4");
    assert_eq!(binding.target, SceneGraphTarget::FullAlphaMask);
    assert_eq!(binding.target_byte, 0);
    assert!(binding.clear_first);
    assert_eq!(binding.heap_bind_indices, vec![0]);
    assert_eq!(binding.pipeline_binding_count, 1);
    assert_eq!(binding.texture_slot_mask, 0b11);
    assert_eq!(binding.optional_morph_texture_slot, 5);
    assert_eq!(binding.render_var0_uniform, "g_RenderVar0");
    assert_eq!(binding.render_var0_component, "x");
    assert_eq!(binding.render_var0_invert_flag_mask, 0x2);
    assert_eq!(
        binding.render_var0_formula,
        "r = mix(r, 1 - r, g_RenderVar0.x)"
    );
    assert_eq!(binding.state_render_var0_mirror_offset, "0xa8");
    assert_eq!(binding.clear_setter_vtable_offset, "0x118");
    assert_eq!(binding.clear_emit_vtable_offset, "0x120");
    assert!(binding.slot0_sample_source.contains("wrapper+0xd0"));
    assert!(binding.slot1_sample_source.contains("subdraw+0x38"));
    assert!(binding.slot5_morph_texture_source.contains("wrapper+0xf8"));
    assert_eq!(binding.slot5_morph_enable_condition, "MORPHING combo == 1");
    assert_eq!(
        binding.alpha_formula,
        "mix(pow(texture0.a, 4), texture0.a, texture1.r)"
    );
    assert!(binding.red_formula.contains("g_RenderVar0.x"));
    assert!(
        binding
            .reference_points
            .iter()
            .any(|reference| reference.contains("0x14020d7b8"))
    );
}

#[test]
fn producer_uniform_plan_preserves_multi_bind_subdraw_selection() {
    let producer_draws = producer_draws(vec![draw(0, vec![0, 1])]);
    let resource_binds = resource_binds(vec![bind(0, 0, vec![0, 1]), bind(1, 1, vec![0, 1])]);
    let pipelines = native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines(
        &producer_draws,
        &resource_binds,
    )
    .expect("producer pipeline plan");

    let plan =
        native_vulkan_plan_scene_layer_alpha_mask_producer_uniforms(&producer_draws, &pipelines)
            .expect("producer uniform plan");

    assert_eq!(plan.uniform_binding_count, 1);
    assert_eq!(plan.bindings[0].heap_bind_indices, vec![0, 1]);
    assert_eq!(plan.bindings[0].pipeline_binding_count, 2);
}

#[test]
fn producer_uniform_plan_rejects_missing_pipeline_binding() {
    let draws = producer_draws(vec![draw(0, vec![0])]);
    let empty_draws = producer_draws(Vec::new());
    let empty_binds = resource_binds(Vec::new());
    let empty_pipelines =
        native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines(&empty_draws, &empty_binds)
            .expect("empty producer pipeline plan");

    let err = native_vulkan_plan_scene_layer_alpha_mask_producer_uniforms(&draws, &empty_pipelines)
        .expect_err("producer uniform must require pipeline binding");

    assert!(err.contains("has no clippingmaskimage4 pipeline binding"));
}

fn producer_draws(
    draws: Vec<NativeVulkanSceneLayerAlphaMaskProducerDrawPlan>,
) -> NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan {
    NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan {
        command_count: draws.len(),
        producer_draw_count: draws.len(),
        full_mask_producer_count: draws
            .iter()
            .filter(|draw| draw.target == SceneGraphTarget::FullAlphaMask)
            .count(),
        intermediate_mask_producer_count: draws
            .iter()
            .filter(|draw| draw.target == SceneGraphTarget::FullAlphaMaskIntermediate)
            .count(),
        clear_target_scope_count: draws.iter().filter(|draw| draw.clear_first).count(),
        load_target_scope_count: draws.iter().filter(|draw| !draw.clear_first).count(),
        texture_slot_mask: draws
            .iter()
            .fold(0u32, |mask, draw| mask | draw.texture_slot_mask),
        draws,
        command_order: [
            "read_alpha_mask_token_schedule",
            "select_clippingmaskimage4_producer_steps",
            "validate_0x14020d6a0_command_shape",
            "map_target_byte_0_full_1_intermediate",
            "map_clear_first_to_clear_or_load_scope",
            "preserve_layer_0x490_rt_method_8_draw_receiver",
        ],
    }
}

fn draw(
    producer_draw_index: usize,
    heap_bind_indices: Vec<usize>,
) -> NativeVulkanSceneLayerAlphaMaskProducerDrawPlan {
    NativeVulkanSceneLayerAlphaMaskProducerDrawPlan {
        producer_draw_index,
        command_index: producer_draw_index + 1,
        object: SceneObjectId(77),
        condition: SceneLayerCompositorCondition::Token1OrToken2FirstPair,
        target: SceneGraphTarget::FullAlphaMask,
        target_byte: 0,
        clear_first: true,
        target_scope_load_op: NativeVulkanSceneRenderTargetLoadOp::Clear,
        material: CLIPPINGMASKIMAGE4_MATERIAL,
        shader: CLIPPINGMASKIMAGE4_SHADER,
        pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
        target_format: "R8_UNORM",
        texture_slot_mask: CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
        optional_morph_texture_slot: CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT,
        heap_bind_count: heap_bind_indices.len(),
        heap_bind_indices,
        subdraw_mask_texture_field_offset: "0x38",
        subdraw_invert_flag: "0x44 bit 0x2",
        draw_receiver: "[layer+0x490]",
        draw_receiver_vtable_offset: "0x40",
        reference_points: [
            "reverse-engineered/docs/exe/clipping-pipeline.md: 0x14020d6bc target byte",
            "reverse-engineered/docs/exe/clipping-pipeline.md: token 1/token 2 clear_first behavior",
            "reverse-engineered/docs/exe/composelayer-and-effecttarget.md: 0x14009b140/0x14009b160 clear pair",
            "reverse-engineered/docs/exe/blend-and-render.md: [layer+0x490].vtable+0x40 RT method [8]",
        ],
        command_order: [
            "read_scheduled_clippingmaskimage4_producer",
            "map_token_condition_to_target_byte",
            "map_clear_first_to_target_scope_load_op",
            "bind_clippingmaskimage4_resource_heap",
            "record_layer_0x490_rt_method_8_draw",
            "retain_alpha_mask_target_layout",
        ],
    }
}

fn resource_binds(
    binds: Vec<NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan>,
) -> NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
        heap_bind_count: binds.len(),
        resource_heap_bind_count: binds.len(),
        clippingmaskimage4_bind_count: binds.len(),
        generated_clippingtarget_bind_count: 0,
        flattexture_copy_back_bind_count: 0,
        token_command_count: 0,
        token_command_resource_bind_count: 0,
        draw_clipping_mask_command_bind_count: 0,
        generated_clippingtarget_command_bind_count: 0,
        copy_back_command_count: 0,
        copy_back_draw_resource_count: 0,
        copy_back_draw_bind_count: 0,
        binds,
        token_commands: Vec::new(),
        copy_back_draws: Vec::new(),
        copy_back_draw_binds: Vec::new(),
        copy_back_pipelines:
            super::super::copy_back_pipeline::NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan {
                pipeline_count: 0,
                cache_key_count: 0,
                texture_slot_mask: 0,
                keys: Vec::new(),
                command_order: [
                    "read_copy_back_draw_resources",
                    "read_copy_back_heap_bind_pairings",
                    "derive_minimalalpha_copy_back_pipeline_keys",
                    "map_copy_back_texture_slots_to_descriptor_heap_offsets",
                    "preserve_render_state_flattexture_copy_back_draw_shape",
                ],
                cache_keys: Vec::new(),
            },
        command_order: [
            "read_current_alpha_mask_resource_heap_plan",
            "resolve_texture_bind_bind_info",
            "classify_alpha_mask_descriptor_heap_bind",
            "match_resource_binds_to_token_commands",
            "require_heap_bind_for_tokenized_mask_draws",
            "lower_flattexture_copy_back_to_minimalalpha_draw_resource",
            "pair_flattexture_copy_back_draws_with_heap_binds",
            "derive_flattexture_copy_back_pipeline_mapping",
            "preserve_flattexture_copy_back_as_blend_key_0x100_draw",
        ],
    }
}

fn bind(
    heap_bind_index: usize,
    clipping_record_index: u32,
    slots: Vec<u32>,
) -> NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan {
    let bindings = slots
        .iter()
        .copied()
        .map(|slot| NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
            slot,
            source: NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                SceneResourceId(100 + slot),
            ),
        })
        .collect::<Vec<_>>();
    let shader_mappings = slots
        .iter()
        .enumerate()
        .map(|(ordinal, slot)| {
            format!(
                "we.texture_slot{slot}.g_Texture{slot} -> alpha-mask-heap-slice-offset{ordinal}"
            )
        })
        .collect::<Vec<_>>();
    let heap_slice = NativeVulkanSceneLayerAlphaMaskHeapSliceKey {
        shader: "we/clippingmaskimage4".to_owned(),
        bindings,
    };
    NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan {
        heap_bind_index,
        object: SceneObjectId(77),
        puppet: ScenePuppetId(5),
        shader: "we/clippingmaskimage4".to_owned(),
        role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 {
            clipping_record_index,
        },
        operation: SceneLayerCompositorOperation::DrawClippingMask,
        bind: NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan {
            heap_bind_index,
            object: SceneObjectId(77),
            puppet: ScenePuppetId(5),
            shader: "we/clippingmaskimage4".to_owned(),
            role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 {
                clipping_record_index,
            },
            heap_slice_index: heap_bind_index,
            heap_slice,
            material: None,
            base_resource_descriptor_index: 4 + heap_bind_index,
            base_sampler_descriptor_index: 8 + heap_bind_index,
            resource_descriptor_count: slots.len(),
            texture_count: slots.len(),
            shader_mappings,
            command_order: ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"],
        },
    }
}
