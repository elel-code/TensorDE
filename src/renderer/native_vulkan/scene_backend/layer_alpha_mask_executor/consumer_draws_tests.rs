use super::super::copy_back_pipeline::NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan;
use super::super::resource_binds::{
    NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
};
use super::super::token_schedule::{
    NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus,
    NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan,
    NativeVulkanSceneLayerAlphaMaskTokenScheduleStep,
    NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind,
};
use super::super::{
    NativeVulkanSceneLayerAlphaMaskAccess, NativeVulkanSceneLayerAlphaMaskCommandPlan,
    NativeVulkanSceneLayerAlphaMaskCopyMethod, NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan, NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskTextureBindRole,
};
use super::*;
use crate::engine::scene_engine::{
    SceneGraphTarget, SceneLayerCompositorBlendKey, SceneLayerCompositorCondition,
    SceneLayerCompositorEntry, SceneLayerCompositorOperation, SceneLayerCompositorTarget,
    SceneObjectId, ScenePuppetId, SceneResourceId,
};
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::{
    NativeVulkanSceneLayerAlphaMaskHeapSliceBinding, NativeVulkanSceneLayerAlphaMaskHeapSliceKey,
    NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan,
};

#[test]
fn generated_consumer_pairs_draw_with_full_mask_heap_bind() {
    let runtime = runtime(SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0);
    let schedule = schedule();
    let resource_binds = resource_binds(bind(
        3,
        vec![
            (
                0,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(SceneResourceId(
                    9,
                )),
            ),
            (
                8,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                    SceneGraphTarget::FullAlphaMask,
                ),
            ),
        ],
    ));

    let plan = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws(
        &runtime,
        &resource_binds,
        &schedule,
    )
    .expect("generated consumer plan");

    assert_eq!(plan.command_count, 1);
    assert_eq!(plan.consumer_draw_count, 1);
    assert_eq!(plan.heap_binding_count, 1);
    assert_eq!(plan.texture_slot_mask, CLIPPINGTARGET_TEXTURE_SLOT_MASK);
    let binding = &plan.bindings[0];
    assert_eq!(binding.consumer_draw_index, 0);
    assert_eq!(binding.command_index, 0);
    assert_eq!(binding.object, SceneObjectId(77));
    assert_eq!(binding.source_mask, SceneGraphTarget::FullAlphaMask);
    assert_eq!(binding.target, SceneLayerCompositorTarget::LayerTarget490);
    assert_eq!(binding.target_receiver, "[layer+0x490]");
    assert_eq!(binding.draw_receiver_vtable_offset, "0x40");
    assert_eq!(binding.shader, "we/genericimage4");
    assert_eq!(binding.required_texture_slots, [0, 8]);
    assert_eq!(binding.heap_bind_index, 3);
    assert_eq!(binding.heap_slice_index, 3);
    assert_eq!(binding.base_resource_descriptor_index, 6);
    assert_eq!(binding.base_sampler_descriptor_index, 12);
    assert_eq!(binding.texture_count, 2);
    assert_eq!(
        binding.blend_byte_source,
        "subdraw+0x40 -> generated material +0x1f0"
    );
    assert_eq!(
        binding.generated_material_source,
        "local generated material variant +0x428"
    );
    assert_eq!(
        binding.shader_mappings,
        vec![
            "set0.binding0.g_Texture0 -> alpha-mask-heap-slice-offset0".to_owned(),
            "set0.binding8.g_Texture8 -> alpha-mask-heap-slice-offset1".to_owned(),
        ]
    );
}

#[test]
fn generated_consumer_rejects_slot8_without_full_alpha_mask() {
    let runtime = runtime(SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0);
    let schedule = schedule();
    let resource_binds = resource_binds(bind(
        3,
        vec![
            (
                0,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(SceneResourceId(
                    9,
                )),
            ),
            (
                8,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                    SceneGraphTarget::FullAlphaMaskIntermediate,
                ),
            ),
        ],
    ));

    let err = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws(
        &runtime,
        &resource_binds,
        &schedule,
    )
    .expect_err("slot8 must sample full alpha mask");

    assert!(err.contains("requires g_Texture8 to sample FullAlphaMask"));
}

#[test]
fn generated_consumer_rejects_missing_slot8_heap_bind() {
    let runtime = runtime(SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0);
    let schedule = schedule();
    let resource_binds = resource_binds(bind(
        3,
        vec![
            (
                0,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(SceneResourceId(
                    9,
                )),
            ),
            (
                3,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                    SceneGraphTarget::FullAlphaMask,
                ),
            ),
        ],
    ));

    let err = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws(
        &runtime,
        &resource_binds,
        &schedule,
    )
    .expect_err("slot8 is required");

    assert!(err.contains("requires g_Texture0/g_Texture8 heap bind"));
}

#[test]
fn generated_consumer_rejects_non_subdraw_blend_lowering() {
    let runtime = runtime(SceneLayerCompositorBlendKey::Inherit);
    let schedule = schedule();
    let resource_binds = resource_binds(bind(
        3,
        vec![
            (
                0,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(SceneResourceId(
                    9,
                )),
            ),
            (
                8,
                NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                    SceneGraphTarget::FullAlphaMask,
                ),
            ),
        ],
    ));

    let err = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws(
        &runtime,
        &resource_binds,
        &schedule,
    )
    .expect_err("subdraw blend byte lowering is required");

    assert!(err.contains("must lower subdraw +0x40"));
}

fn runtime(blend_key: SceneLayerCompositorBlendKey) -> NativeVulkanSceneLayerAlphaMaskRuntimePlan {
    NativeVulkanSceneLayerAlphaMaskRuntimePlan {
        tokenized_layer_count: 1,
        command_count: 1,
        required_target_count: 2,
        pipeline_warmup: NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan::empty(),
        target_scope_count: 0,
        alpha_mask_attachment_write_count: 0,
        alpha_mask_shader_sample_count: 1,
        token_program_dispatch_count: 0,
        draw_clipping_mask_count: 0,
        draw_style_copy_back_count: 0,
        generated_clipping_target_draw_count: 1,
        transfer_copy_count: 0,
        targets: Vec::new(),
        commands: vec![NativeVulkanSceneLayerAlphaMaskCommandPlan {
            object: SceneObjectId(77),
            entry: SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53,
            operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
            condition: SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
            source: Some(SceneLayerCompositorTarget::FullAlphaMask),
            target: SceneLayerCompositorTarget::LayerTarget490,
            source_graph_target: Some(SceneGraphTarget::FullAlphaMask),
            target_graph_target: None,
            access: NativeVulkanSceneLayerAlphaMaskAccess::FullMaskSampleForGeneratedTarget,
            copy_method: NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
            blend_key,
        }],
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

fn schedule() -> NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan {
    let matched_heap_bind_indices = vec![3];
    NativeVulkanSceneLayerAlphaMaskTokenSchedulePlan {
        command_count: 1,
        scheduled_step_count: 1,
        token_program_dispatch_count: 0,
        full_mask_producer_count: 0,
        intermediate_mask_producer_count: 0,
        copy_back_after_intermediate_count: 0,
        generated_target_consumer_count: 1,
        recorder_ready_step_count: 0,
        missing_recorder_step_count: 1,
        clippingmaskimage4_pending_recorder_count: 0,
        generated_clippingtarget_pending_recorder_count: 1,
        steps: vec![NativeVulkanSceneLayerAlphaMaskTokenScheduleStep {
            command_index: 0,
            object: SceneObjectId(77),
            operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
            source: Some(SceneLayerCompositorTarget::FullAlphaMask),
            target: SceneLayerCompositorTarget::LayerTarget490,
            kind: NativeVulkanSceneLayerAlphaMaskTokenScheduleStepKind::GeneratedClippingTargetConsumer,
            matched_heap_bind_count: matched_heap_bind_indices.len(),
            matched_heap_bind_indices,
            recording_status:
                NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::PendingGeneratedClippingTargetRecorder,
            full_mask_ready_after: true,
            intermediate_mask_ready_after: false,
            command_order: vec![
                "require_full_alpha_mask_ready",
                "bind_generated_clippingtarget_heap",
                "pending_generated_clippingtarget_recorder",
            ],
        }],
        command_order: [
            "read_alpha_mask_token_stream",
            "match_token_commands_to_heap_bind_facts",
            "track_full_and_intermediate_mask_readiness",
            "place_clippingmaskimage4_producer_steps",
            "place_flattexture_copy_back_after_intermediate",
            "place_generated_clippingtarget_after_full_mask",
        ],
    }
}

fn resource_binds(
    bind: NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan,
) -> NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
        heap_bind_count: 1,
        resource_heap_bind_count: 1,
        clippingmaskimage4_bind_count: 0,
        generated_clippingtarget_bind_count: 1,
        flattexture_copy_back_bind_count: 0,
        token_command_count: 1,
        token_command_resource_bind_count: 1,
        draw_clipping_mask_command_bind_count: 0,
        generated_clippingtarget_command_bind_count: 1,
        copy_back_command_count: 0,
        copy_back_draw_resource_count: 0,
        copy_back_draw_bind_count: 0,
        binds: vec![bind],
        token_commands: Vec::new(),
        copy_back_draws: Vec::new(),
        copy_back_draw_binds: Vec::new(),
        copy_back_pipelines: NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan {
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
    slots: Vec<(u32, NativeVulkanSceneLayerAlphaMaskDescriptorSource)>,
) -> NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan {
    let bindings = slots
        .iter()
        .map(
            |(slot, source)| NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
                slot: *slot,
                source: *source,
            },
        )
        .collect::<Vec<_>>();
    let shader_mappings = slots
        .iter()
        .enumerate()
        .map(|(ordinal, (slot, _))| {
            format!("set0.binding{slot}.g_Texture{slot} -> alpha-mask-heap-slice-offset{ordinal}")
        })
        .collect::<Vec<_>>();
    NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan {
        heap_bind_index,
        object: SceneObjectId(77),
        puppet: ScenePuppetId(5),
        shader: "we/genericimage4".to_owned(),
        role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget,
        operation: SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
        bind: NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan {
            heap_bind_index,
            object: SceneObjectId(77),
            puppet: ScenePuppetId(5),
            shader: "we/genericimage4".to_owned(),
            role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget,
            heap_slice_index: heap_bind_index,
            heap_slice: NativeVulkanSceneLayerAlphaMaskHeapSliceKey {
                shader: "we/genericimage4".to_owned(),
                bindings,
            },
            base_resource_descriptor_index: 6,
            base_sampler_descriptor_index: 12,
            resource_descriptor_count: slots.len(),
            texture_count: slots.len(),
            shader_mappings,
            command_order: ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"],
        },
    }
}
