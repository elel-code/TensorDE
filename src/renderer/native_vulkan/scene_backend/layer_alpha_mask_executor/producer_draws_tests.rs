use super::super::copy_back_pipeline::NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan;
use super::super::resource_binds::NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan;
use super::super::token_schedule::native_vulkan_plan_scene_layer_alpha_mask_token_schedule;
use super::super::{
    NativeVulkanSceneLayerAlphaMaskAccess, NativeVulkanSceneLayerAlphaMaskCommandPlan,
    NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan,
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskTargetPlan,
};
use super::*;
use crate::engine::scene_engine::{
    SceneLayerCompositorBlendKey, SceneLayerCompositorCondition, SceneLayerCompositorEntry,
    SceneLayerCompositorOperation, SceneLayerCompositorTarget,
};
use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;

#[test]
fn producer_draws_map_token_conditions_to_target_bytes_and_load_ops() {
    let runtime = runtime(vec![
        token_program(),
        draw_mask(
            SceneLayerCompositorCondition::Token1OrToken2FirstPair,
            SceneLayerCompositorTarget::FullAlphaMask,
        ),
        draw_mask(
            SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask,
            SceneLayerCompositorTarget::FullAlphaMaskIntermediate,
        ),
    ]);
    let resource_binds = resource_binds_for_runtime(&runtime);
    let schedule =
        native_vulkan_plan_scene_layer_alpha_mask_token_schedule(&runtime, &resource_binds)
            .expect("token schedule");

    let plan = native_vulkan_plan_scene_layer_alpha_mask_producer_draws(
        &runtime,
        &resource_binds,
        &schedule,
    )
    .expect("producer draw plan");

    assert_eq!(plan.producer_draw_count, 2);
    assert_eq!(plan.full_mask_producer_count, 1);
    assert_eq!(plan.intermediate_mask_producer_count, 1);
    assert_eq!(plan.clear_target_scope_count, 1);
    assert_eq!(plan.load_target_scope_count, 1);
    assert_eq!(
        plan.texture_slot_mask,
        CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK
    );

    let full = &plan.draws[0];
    assert_eq!(full.target, SceneGraphTarget::FullAlphaMask);
    assert_eq!(full.target_byte, 0);
    assert!(full.clear_first);
    assert_eq!(
        full.target_scope_load_op,
        NativeVulkanSceneRenderTargetLoadOp::Clear
    );
    assert_eq!(full.material, CLIPPINGMASKIMAGE4_MATERIAL);
    assert_eq!(full.shader, CLIPPINGMASKIMAGE4_SHADER);
    assert_eq!(full.pipeline_class, SceneGraphPipelineClass::PuppetSkinning);
    assert_eq!(
        full.optional_morph_texture_slot,
        CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT
    );
    assert_eq!(full.heap_bind_indices, vec![1]);

    let intermediate = &plan.draws[1];
    assert_eq!(
        intermediate.target,
        SceneGraphTarget::FullAlphaMaskIntermediate
    );
    assert_eq!(intermediate.target_byte, 1);
    assert!(!intermediate.clear_first);
    assert_eq!(
        intermediate.target_scope_load_op,
        NativeVulkanSceneRenderTargetLoadOp::Load
    );
    assert_eq!(intermediate.draw_receiver, "[layer+0x490]");
    assert_eq!(intermediate.draw_receiver_vtable_offset, "0x40");
    assert_eq!(intermediate.heap_bind_indices, vec![2]);
}

#[test]
fn producer_draws_reject_wrong_intermediate_condition() {
    let runtime = runtime(vec![
        token_program(),
        draw_mask(
            SceneLayerCompositorCondition::Token1OrToken2FirstPair,
            SceneLayerCompositorTarget::FullAlphaMaskIntermediate,
        ),
    ]);
    let resource_binds = resource_binds_for_runtime(&runtime);
    let schedule =
        native_vulkan_plan_scene_layer_alpha_mask_token_schedule(&runtime, &resource_binds)
            .expect("token schedule");

    let err = native_vulkan_plan_scene_layer_alpha_mask_producer_draws(
        &runtime,
        &resource_binds,
        &schedule,
    )
    .expect_err("wrong condition/target pair must fail");

    assert!(err.contains("cannot map"));
}

fn runtime(
    commands: Vec<NativeVulkanSceneLayerAlphaMaskCommandPlan>,
) -> NativeVulkanSceneLayerAlphaMaskRuntimePlan {
    NativeVulkanSceneLayerAlphaMaskRuntimePlan {
        tokenized_layer_count: 1,
        command_count: commands.len(),
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

fn resource_binds_for_runtime(
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
) -> NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
    let token_commands = runtime
        .commands
        .iter()
        .enumerate()
        .map(|(command_index, command)| {
            let matched_heap_bind_indices =
                if command.operation == SceneLayerCompositorOperation::TokenProgramDispatch {
                    Vec::new()
                } else {
                    vec![command_index]
                };
            NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan {
                command_index,
                object: command.object,
                operation: command.operation,
                target: command.target,
                source: command.source,
                requirement: if command.operation
                    == SceneLayerCompositorOperation::TokenProgramDispatch
                {
                    NativeVulkanSceneLayerAlphaMaskBindRequirement::TokenProgramNoResourceBind
                } else {
                    NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4
                },
                matched_bind_count: matched_heap_bind_indices.len(),
                matched_heap_bind_indices,
                command_order: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
        heap_bind_count: 0,
        resource_heap_bind_count: 0,
        clippingmaskimage4_bind_count: 0,
        generated_clippingtarget_bind_count: 0,
        flattexture_copy_back_bind_count: 0,
        token_command_count: token_commands.len(),
        token_command_resource_bind_count: 0,
        draw_clipping_mask_command_bind_count: 0,
        generated_clippingtarget_command_bind_count: 0,
        copy_back_command_count: 0,
        copy_back_draw_resource_count: 0,
        copy_back_draw_bind_count: 0,
        binds: Vec::new(),
        token_commands,
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

fn token_program() -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
    command(
        SceneLayerCompositorOperation::TokenProgramDispatch,
        SceneLayerCompositorCondition::Always,
        SceneLayerCompositorTarget::LayerTarget490,
    )
}

fn draw_mask(
    condition: SceneLayerCompositorCondition,
    target: SceneLayerCompositorTarget,
) -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
    command(
        SceneLayerCompositorOperation::DrawClippingMask,
        condition,
        target,
    )
}

fn command(
    operation: SceneLayerCompositorOperation,
    condition: SceneLayerCompositorCondition,
    target: SceneLayerCompositorTarget,
) -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
    NativeVulkanSceneLayerAlphaMaskCommandPlan {
        object: SceneObjectId(7),
        entry: match operation {
            SceneLayerCompositorOperation::TokenProgramDispatch => {
                SceneLayerCompositorEntry::TokenizedCompositeEntry52
            }
            _ => SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
        },
        operation,
        condition,
        source: None,
        target,
        source_graph_target: None,
        target_graph_target: graph_target(target),
        access: match operation {
            SceneLayerCompositorOperation::TokenProgramDispatch => {
                NativeVulkanSceneLayerAlphaMaskAccess::TokenProgram
            }
            _ => NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskAttachmentWrite,
        },
        copy_method: super::super::NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
        blend_key: SceneLayerCompositorBlendKey::Inherit,
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
