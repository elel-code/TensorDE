use super::*;
use crate::engine::scene_engine::{
    SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorBlendKey,
    SceneLayerCompositorCondition, SceneLayerCompositorEntry, SceneLayerCompositorOperation,
    SceneLayerCompositorTarget, SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;

#[test]
fn rt_method8_bridge_closes_producer_and_generated_call_sites() {
    let runtime = runtime(vec![
        command(
            SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
            SceneLayerCompositorOperation::DrawClippingMask,
            SceneLayerCompositorCondition::Token1OrToken2FirstPair,
            None,
            SceneLayerCompositorTarget::FullAlphaMask,
            SceneLayerCompositorBlendKey::Inherit,
        ),
        command(
            SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53,
            SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
            SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
            Some(SceneLayerCompositorTarget::FullAlphaMask),
            SceneLayerCompositorTarget::LayerTarget490,
            SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0,
        ),
    ]);
    let producer_draws =
        super::super::producer_draws::NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan {
            command_count: 1,
            producer_draw_count: 1,
            full_mask_producer_count: 1,
            intermediate_mask_producer_count: 0,
            clear_target_scope_count: 1,
            load_target_scope_count: 0,
            texture_slot_mask: super::super::CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
            draws: vec![producer_draw(0, 0)],
            command_order: [
                "read_clippingmaskimage4_producer_steps",
                "map_token_condition_to_target_byte",
                "map_clear_first_to_target_scope_load_op",
                "bind_clippingmaskimage4_resource_heap",
                "record_layer_0x490_rt_method_8_draw",
                "retain_alpha_mask_target_layout",
            ],
        };
    let consumer_draws = super::super::consumer_draws::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan {
        command_count: 2,
        consumer_draw_count: 1,
        heap_binding_count: 1,
        texture_slot_mask: super::super::CLIPPINGTARGET_TEXTURE_SLOT_MASK,
        bindings: vec![generated_consumer_draw(0, 1)],
        command_order: [
            "read_generated_clippingtarget_schedule_steps",
            "resolve_single_generated_clippingtarget_heap_bind",
            "validate_genericimage4_clippingtarget_slots_0_8",
            "preserve_generated_material_0x428",
            "preserve_subdraw_blend_byte_to_material_0x1f0",
            "preserve_layer_0x490_generated_draw_receiver",
        ],
    };

    let plan = native_vulkan_plan_scene_layer_alpha_mask_rt_method8_bridges(
        &runtime,
        &producer_draws,
        &consumer_draws,
    )
    .expect("RT method [8] bridge plan");

    assert_eq!(plan.command_count, 2);
    assert_eq!(plan.bridge_count, 2);
    assert_eq!(plan.producer_bridge_count, 1);
    assert_eq!(plan.generated_consumer_bridge_count, 1);
    assert_eq!(plan.indexed_vector_draw_bridge_count, 2);
    assert_eq!(plan.raw_shader_resource_bind_bridge_count, 0);
    assert_eq!(plan.geometry_creation_site, "0x14020b15e");
    assert!(
        plan.geometry_source
            .contains("local/generated vertex/index arrays")
    );
    assert_eq!(
        plan.geometry_source_plan.wrapper_create_method_vma,
        "0x14009a880"
    );
    assert_eq!(plan.geometry_source_plan.wrapper_argument_count, 9);
    assert_eq!(
        plan.geometry_source_plan.wrapper_arguments[4].semantic,
        "index-data pointer"
    );
    assert_eq!(
        plan.geometry_source_plan.usage_selector.source,
        "((([layer+0x4b8]+0x18)->0x18 >> 3) & 1) * 2"
    );

    let producer = plan.bridge_for_producer_draw(0).expect("producer bridge");
    assert_eq!(producer.call_site, "0x14020d83e");
    assert_eq!(producer.receiver_field, "[layer+0x490]");
    assert_eq!(producer.method_offset, "0x40");
    assert_eq!(producer.method_vma, "0x1400eacd0");
    assert_eq!(producer.draw_call, "[layer+0x490].vtable+0x40");
    assert!(!producer.is_raw_shader_resource_bind);

    let generated = plan
        .bridge_for_generated_consumer_draw(0)
        .expect("generated bridge");
    assert_eq!(generated.call_site, "0x14020908c");
    assert_eq!(
        generated.purpose,
        NativeVulkanSceneLayerAlphaMaskRtMethod8Purpose::GeneratedClippingTargetConsumer
    );
    assert_eq!(
        generated.draw_index_argument,
        "edx is the generated subdraw/draw index selector, not a raw shader resource"
    );
}

#[test]
fn rt_method8_bridge_rejects_generated_consumer_without_layer_490_receiver() {
    let runtime = runtime(vec![command(
        SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53,
        SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
        SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
        Some(SceneLayerCompositorTarget::FullAlphaMask),
        SceneLayerCompositorTarget::LayerTarget490,
        SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0,
    )]);
    let consumer_draws = super::super::consumer_draws::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan {
        command_count: 1,
        consumer_draw_count: 1,
        heap_binding_count: 1,
        texture_slot_mask: super::super::CLIPPINGTARGET_TEXTURE_SLOT_MASK,
        bindings: vec![super::super::consumer_draws::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan {
            target_receiver: "[invalid-receiver]",
            ..generated_consumer_draw(0, 0)
        }],
        command_order: [
            "read_generated_clippingtarget_schedule_steps",
            "resolve_single_generated_clippingtarget_heap_bind",
            "validate_genericimage4_clippingtarget_slots_0_8",
            "preserve_generated_material_0x428",
            "preserve_subdraw_blend_byte_to_material_0x1f0",
            "preserve_layer_0x490_generated_draw_receiver",
        ],
    };

    let err = native_vulkan_plan_scene_layer_alpha_mask_rt_method8_bridges(
        &runtime,
        &super::super::producer_draws::NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan {
            command_count: 0,
            producer_draw_count: 0,
            full_mask_producer_count: 0,
            intermediate_mask_producer_count: 0,
            clear_target_scope_count: 0,
            load_target_scope_count: 0,
            texture_slot_mask: 0,
            draws: Vec::new(),
            command_order: [
                "read_clippingmaskimage4_producer_steps",
                "map_token_condition_to_target_byte",
                "map_clear_first_to_target_scope_load_op",
                "bind_clippingmaskimage4_resource_heap",
                "record_layer_0x490_rt_method_8_draw",
                "retain_alpha_mask_target_layout",
            ],
        },
        &consumer_draws,
    )
    .expect_err("wrong receiver must fail");

    assert!(err.contains("lost layer+0x490 receiver identity"));
}

fn runtime(
    commands: Vec<super::super::NativeVulkanSceneLayerAlphaMaskCommandPlan>,
) -> super::super::NativeVulkanSceneLayerAlphaMaskRuntimePlan {
    super::super::NativeVulkanSceneLayerAlphaMaskRuntimePlan {
        tokenized_layer_count: 1,
        command_count: commands.len(),
        required_target_count: 2,
        pipeline_warmup: super::super::NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan {
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
        targets: Vec::new(),
        commands,
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

fn command(
    entry: SceneLayerCompositorEntry,
    operation: SceneLayerCompositorOperation,
    condition: SceneLayerCompositorCondition,
    source: Option<SceneLayerCompositorTarget>,
    target: SceneLayerCompositorTarget,
    blend_key: SceneLayerCompositorBlendKey,
) -> super::super::NativeVulkanSceneLayerAlphaMaskCommandPlan {
    super::super::NativeVulkanSceneLayerAlphaMaskCommandPlan {
        object: SceneObjectId(7),
        entry,
        operation,
        condition,
        source,
        target,
        source_graph_target: source.and_then(graph_target),
        target_graph_target: graph_target(target),
        access: super::super::NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskAttachmentWrite,
        copy_method: super::super::NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
        blend_key,
    }
}

fn graph_target(target: SceneLayerCompositorTarget) -> Option<SceneGraphTarget> {
    match target {
        SceneLayerCompositorTarget::FullAlphaMask => Some(SceneGraphTarget::FullAlphaMask),
        SceneLayerCompositorTarget::FullAlphaMaskIntermediate => {
            Some(SceneGraphTarget::FullAlphaMaskIntermediate)
        }
        _ => None,
    }
}

fn producer_draw(
    producer_draw_index: usize,
    command_index: usize,
) -> super::super::producer_draws::NativeVulkanSceneLayerAlphaMaskProducerDrawPlan {
    super::super::producer_draws::NativeVulkanSceneLayerAlphaMaskProducerDrawPlan {
        producer_draw_index,
        command_index,
        object: SceneObjectId(7),
        condition: SceneLayerCompositorCondition::Token1OrToken2FirstPair,
        target: SceneGraphTarget::FullAlphaMask,
        target_byte: 0,
        clear_first: true,
        target_scope_load_op: NativeVulkanSceneRenderTargetLoadOp::Clear,
        material: super::super::producer_draws::CLIPPINGMASKIMAGE4_MATERIAL,
        shader: super::super::producer_draws::CLIPPINGMASKIMAGE4_SHADER,
        pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
        target_format: "R8_UNORM",
        texture_slot_mask: super::super::CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
        optional_morph_texture_slot: super::super::CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT,
        heap_bind_count: 1,
        heap_bind_indices: vec![0],
        subdraw_mask_texture_field_offset: "0x38",
        subdraw_invert_flag: "0x44 bit 0x2",
        draw_receiver: LAYER_490_RT_METHOD8_RECEIVER_LABEL,
        draw_receiver_vtable_offset: LAYER_490_RT_METHOD8_OFFSET,
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

fn generated_consumer_draw(
    consumer_draw_index: usize,
    command_index: usize,
) -> super::super::consumer_draws::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan {
    super::super::consumer_draws::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan {
        consumer_draw_index,
        command_index,
        object: SceneObjectId(7),
        operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
        source_mask: SceneGraphTarget::FullAlphaMask,
        target: SceneLayerCompositorTarget::LayerTarget490,
        target_receiver: LAYER_490_RT_METHOD8_RECEIVER_LABEL,
        draw_receiver_vtable_offset: LAYER_490_RT_METHOD8_OFFSET,
        shader: super::super::consumer_draws::GENERATED_CLIPPINGTARGET_SHADER,
        texture_slot_mask: super::super::CLIPPINGTARGET_TEXTURE_SLOT_MASK,
        required_texture_slots: [0, 8],
        heap_bind_index: command_index,
        heap_slice_index: command_index,
        base_resource_descriptor_index: 8,
        base_sampler_descriptor_index: 24,
        resource_descriptor_count: 3,
        texture_count: 2,
        material_uniform_buffer_handle: 0x4204,
        material_uniform_device_address: 0x4284,
        material_uniform_bytes: 48,
        material_uniform_payload_hash: 0x1238,
        blend_byte_source: "subdraw+0x40 -> generated material +0x1f0",
        generated_material_source: "local generated material variant +0x428",
        shader_mappings: Vec::new(),
        command_order: [
            "read_generated_clippingtarget_token_step",
            "match_single_generated_clippingtarget_heap_bind",
            "validate_slot0_source_and_slot8_full_alpha_mask",
            "preserve_subdraw_blend_byte_to_generated_material_0x1f0",
            "preserve_layer_0x490_rt_method_8_draw_receiver",
            "lower_receiver_to_rt_method_8_bridge_plan",
        ],
    }
}
