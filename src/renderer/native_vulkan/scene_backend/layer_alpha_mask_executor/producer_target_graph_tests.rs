use super::super::producer_draws::{
    CLIPPINGMASKIMAGE4_MATERIAL, CLIPPINGMASKIMAGE4_SHADER,
    NativeVulkanSceneLayerAlphaMaskProducerDrawPlan,
    NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
};
use super::super::{
    CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT, CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
    NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan, NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskTargetPlan,
};
use super::*;
use crate::engine::scene_engine::{
    SceneGraphPipelineClass, SceneLayerCompositorCondition, SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;

#[test]
fn producer_target_graph_maps_clear_and_load_scopes() {
    let plan = native_vulkan_plan_scene_layer_alpha_mask_producer_target_graph(
        &runtime(),
        &producer_draws(vec![
            draw(
                0,
                SceneGraphTarget::FullAlphaMask,
                SceneLayerCompositorCondition::Token1OrToken2FirstPair,
                0,
                true,
                NativeVulkanSceneRenderTargetLoadOp::Clear,
            ),
            draw(
                1,
                SceneGraphTarget::FullAlphaMaskIntermediate,
                SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask,
                1,
                false,
                NativeVulkanSceneRenderTargetLoadOp::Load,
            ),
        ]),
    )
    .expect("producer target graph");

    assert_eq!(plan.producer_draw_count, 2);
    assert_eq!(plan.target_scope_count, 2);
    assert_eq!(plan.clear_target_scope_count, 1);
    assert_eq!(plan.load_target_scope_count, 1);
    assert_eq!(plan.clear_allows_undefined_target_count, 1);
    assert_eq!(plan.load_requires_initialized_target_count, 1);

    let full = &plan.scopes[0];
    assert_eq!(full.target_scope_index, 0);
    assert_eq!(full.target, SceneGraphTarget::FullAlphaMask);
    assert_eq!(full.target_byte, 0);
    assert_eq!(full.required_layout, "color-attachment-optimal");
    assert_eq!(full.load_op, NativeVulkanSceneRenderTargetLoadOp::Clear);
    assert!(full.allows_undefined_initial_layout);
    assert!(!full.requires_initialized_initial_layout);
    assert_eq!(full.target_color_attachment_write_count, 1);

    let intermediate = &plan.scopes[1];
    assert_eq!(intermediate.target_scope_index, 1);
    assert_eq!(
        intermediate.target,
        SceneGraphTarget::FullAlphaMaskIntermediate
    );
    assert_eq!(intermediate.target_byte, 1);
    assert_eq!(
        intermediate.load_op,
        NativeVulkanSceneRenderTargetLoadOp::Load
    );
    assert!(!intermediate.allows_undefined_initial_layout);
    assert!(intermediate.requires_initialized_initial_layout);
}

#[test]
fn producer_target_graph_rejects_target_byte_mismatch() {
    let err = native_vulkan_plan_scene_layer_alpha_mask_producer_target_graph(
        &runtime(),
        &producer_draws(vec![draw(
            0,
            SceneGraphTarget::FullAlphaMaskIntermediate,
            SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask,
            0,
            false,
            NativeVulkanSceneRenderTargetLoadOp::Load,
        )]),
    )
    .expect_err("target byte 0 cannot write intermediate");

    assert!(err.contains("target byte 0 cannot write"));
}

fn runtime() -> NativeVulkanSceneLayerAlphaMaskRuntimePlan {
    NativeVulkanSceneLayerAlphaMaskRuntimePlan {
        tokenized_layer_count: 1,
        command_count: 2,
        required_target_count: 2,
        pipeline_warmup: NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan {
            cache_key_count: 0,
            keys: Vec::new(),
            command_order: [
                "select_clippingmaskimage4_shader",
                "select_puppet_skinning_mesh_vertex_layout",
                "select_r8_unorm_alpha_mask_target_format",
                "include_required_we_slots_0_1_for_mask_generator",
            ],
            cache_keys: Vec::new(),
        },
        target_scope_count: 0,
        alpha_mask_attachment_write_count: 0,
        alpha_mask_shader_sample_count: 0,
        token_program_dispatch_count: 0,
        draw_clipping_mask_count: 0,
        draw_style_copy_back_count: 0,
        generated_clipping_target_draw_count: 0,
        transfer_copy_count: 0,
        targets: vec![
            NativeVulkanSceneLayerAlphaMaskTargetPlan {
                target: SceneGraphTarget::FullAlphaMask,
                format: "R8_UNORM",
                width: 1920,
                height: 1080,
                scale: 2,
            },
            NativeVulkanSceneLayerAlphaMaskTargetPlan {
                target: SceneGraphTarget::FullAlphaMaskIntermediate,
                format: "R8_UNORM",
                width: 1920,
                height: 1080,
                scale: 2,
            },
        ],
        commands: Vec::new(),
        command_order: [
            "read_we_vtable_52_53_token_program",
            "validate_full_alpha_mask_targets_r8_half_extent",
            "derive_clippingmaskimage4_pipeline_warmup_key",
            "lower_clippingmaskimage4_to_alpha_mask_attachment_writes",
            "lower_flattexture_copy_back_to_draw_blend_key_0x100",
            "preserve_generated_clippingtarget_full_mask_sample",
            "track_alpha_mask_usage_like_godot_rendering_device_graph",
        ],
    }
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
        clear_target_scope_count: draws
            .iter()
            .filter(|draw| draw.target_scope_load_op == NativeVulkanSceneRenderTargetLoadOp::Clear)
            .count(),
        load_target_scope_count: draws
            .iter()
            .filter(|draw| draw.target_scope_load_op == NativeVulkanSceneRenderTargetLoadOp::Load)
            .count(),
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
    target: SceneGraphTarget,
    condition: SceneLayerCompositorCondition,
    target_byte: u8,
    clear_first: bool,
    load_op: NativeVulkanSceneRenderTargetLoadOp,
) -> NativeVulkanSceneLayerAlphaMaskProducerDrawPlan {
    NativeVulkanSceneLayerAlphaMaskProducerDrawPlan {
        producer_draw_index,
        command_index: producer_draw_index + 1,
        object: SceneObjectId(7),
        condition,
        target,
        target_byte,
        clear_first,
        target_scope_load_op: load_op,
        material: CLIPPINGMASKIMAGE4_MATERIAL,
        shader: CLIPPINGMASKIMAGE4_SHADER,
        pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
        target_format: "R8_UNORM",
        texture_slot_mask: CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
        optional_morph_texture_slot: CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT,
        heap_bind_count: 1,
        heap_bind_indices: vec![producer_draw_index + 1],
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
