use super::super::consumer_draws::native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws;
use super::super::copy_back_pipeline::NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan;
use super::super::producer_draws::native_vulkan_plan_scene_layer_alpha_mask_producer_draws;
use super::super::producer_pipeline::native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines;
use super::super::producer_target_graph::native_vulkan_plan_scene_layer_alpha_mask_producer_target_graph;
use super::super::producer_uniform::native_vulkan_plan_scene_layer_alpha_mask_producer_uniforms;
use super::super::resource_binds::{
    NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan,
    NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan,
};
use super::super::token_schedule::native_vulkan_plan_scene_layer_alpha_mask_token_schedule;
use super::super::{
    NativeVulkanSceneLayerAlphaMaskAccess, NativeVulkanSceneLayerAlphaMaskCommandPlan,
    NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan,
    NativeVulkanSceneLayerAlphaMaskLayerTargetBinding,
    NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan, NativeVulkanSceneLayerAlphaMaskTargetPlan,
    NativeVulkanSceneLayerAlphaMaskTextureBindRole,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_targets,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_uniforms,
};
use super::*;
use crate::engine::scene_engine::{SceneLayerCompositorBlendKey, ScenePuppetId, SceneResourceId};
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::{
    NativeVulkanSceneLayerAlphaMaskHeapSliceBinding, NativeVulkanSceneLayerAlphaMaskHeapSliceKey,
    NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding,
    NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
    NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan,
};
use crate::renderer::native_vulkan::scene_backend::material_uniforms::NativeVulkanSceneMaterialUniformKey;
use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;
use vulkanalia::vk;
use vulkanalia::vk::HasBuilder;

#[test]
fn recorder_requirements_classify_pending_and_ready_steps() {
    let runtime = runtime(vec![
        token_program(),
        draw_mask(SceneLayerCompositorTarget::FullAlphaMask),
        draw_mask(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
        copy_back(),
        generated_target(),
    ]);
    let resource_binds = resource_binds_for_runtime(&runtime, true);
    let schedule =
        native_vulkan_plan_scene_layer_alpha_mask_token_schedule(&runtime, &resource_binds)
            .expect("token schedule");
    let producer_draws = native_vulkan_plan_scene_layer_alpha_mask_producer_draws(
        &runtime,
        &resource_binds,
        &schedule,
    )
    .expect("producer draws");
    let producer_target_graph =
        native_vulkan_plan_scene_layer_alpha_mask_producer_target_graph(&runtime, &producer_draws)
            .expect("producer target graph");
    let producer_pipelines = native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines(
        &producer_draws,
        &resource_binds,
    )
    .expect("producer pipelines");
    let producer_uniforms = native_vulkan_plan_scene_layer_alpha_mask_producer_uniforms(
        &producer_draws,
        &producer_pipelines,
    )
    .expect("producer uniforms");
    let generated_consumer_draws =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws(
            &runtime,
            &resource_binds,
            &schedule,
        )
        .expect("generated consumer draws");
    let (
        generated_consumer_targets,
        generated_consumer_pipelines,
        generated_consumer_commands,
        generated_consumer_uniforms,
    ) = generated_consumer_target_pipeline_command_uniform_plans(&generated_consumer_draws);

    let plan = native_vulkan_plan_scene_layer_alpha_mask_recorder_requirements(
        &runtime,
        &resource_binds,
        &schedule,
        &producer_draws,
        &producer_target_graph,
        &producer_uniforms,
        &generated_consumer_draws,
        &generated_consumer_targets,
        &generated_consumer_pipelines,
        &generated_consumer_commands,
        &generated_consumer_uniforms,
    )
    .expect("recorder requirements");

    assert_eq!(plan.step_count, 5);
    assert_eq!(plan.requirement_count, 5);
    assert_eq!(plan.token_program_requirement_count, 1);
    assert_eq!(plan.clippingmaskimage4_producer_requirement_count, 2);
    assert_eq!(plan.flattexture_copy_back_ready_requirement_count, 1);
    assert_eq!(plan.generated_clippingtarget_consumer_requirement_count, 1);
    assert_eq!(plan.pending_recorder_requirement_count, 3);
    assert_eq!(plan.ready_graph_node_requirement_count, 1);
    assert_eq!(plan.no_draw_requirement_count, 1);
    assert_eq!(plan.requirements[1].shader, Some("we/clippingmaskimage4"));
    assert_eq!(
        plan.requirements[1].pipeline_class,
        Some(SceneGraphPipelineClass::PuppetSkinning)
    );
    assert_eq!(
        plan.requirements[1].target_mask,
        Some(SceneGraphTarget::FullAlphaMask)
    );
    assert_eq!(plan.requirements[1].producer_draw_index, Some(0));
    assert_eq!(plan.requirements[1].producer_target_scope_index, Some(0));
    assert_eq!(plan.requirements[1].producer_uniform_index, Some(0));
    assert_eq!(
        plan.requirements[1].target_scope_load_op,
        Some(NativeVulkanSceneRenderTargetLoadOp::Clear)
    );
    assert_eq!(
        plan.requirements[1].requires_initialized_initial_layout,
        Some(false)
    );
    assert_eq!(plan.requirements[2].producer_draw_index, Some(1));
    assert_eq!(plan.requirements[2].producer_target_scope_index, Some(1));
    assert_eq!(plan.requirements[2].producer_uniform_index, Some(1));
    assert_eq!(
        plan.requirements[2].target_scope_load_op,
        Some(NativeVulkanSceneRenderTargetLoadOp::Load)
    );
    assert_eq!(
        plan.requirements[2].requires_initialized_initial_layout,
        Some(true)
    );
    assert!(
        plan.requirements[1]
            .missing_we_facts
            .iter()
            .all(|fact| !fact.contains("clear_first"))
    );
    assert!(
        plan.requirements[1]
            .missing_we_facts
            .iter()
            .all(|fact| !fact.contains("g_RenderVar0"))
    );
    assert_eq!(plan.requirements[3].shader, Some("util/minimalalpha"));
    assert_eq!(
        plan.requirements[3].recording_status,
        NativeVulkanSceneLayerAlphaMaskTokenRecordingStatus::ReadyFlatTextureCopyBackGraphNode
    );
    assert!(plan.requirements[3].missing_we_facts.is_empty());
    assert_eq!(plan.requirements[4].generated_consumer_draw_index, Some(0));
    assert_eq!(
        plan.requirements[4].generated_consumer_uniform_index,
        Some(0)
    );
    assert_eq!(
        plan.requirements[4].source_mask,
        Some(SceneGraphTarget::FullAlphaMask)
    );
    assert_eq!(plan.requirements[4].shader, Some("we/genericimage4"));
    assert_eq!(
        plan.requirements[4].pipeline_class,
        Some(SceneGraphPipelineClass::PuppetSkinning)
    );
    assert_eq!(plan.requirements[4].target_format, Some("B8G8R8A8_UNORM"));
    assert_eq!(
        plan.requirements[4].target_mask,
        Some(SceneGraphTarget::ObjectFinal(SceneObjectId(7)))
    );
    assert!(
        plan.requirements[4]
            .missing_we_facts
            .iter()
            .all(|fact| !fact.contains("CLIPPINGUVS"))
    );
    assert!(
        plan.requirements[4]
            .reference_points
            .iter()
            .any(|reference| reference.contains("CLIPPINGUVS projected screen UV"))
    );
    assert!(
        plan.requirements[4]
            .missing_we_facts
            .iter()
            .all(|fact| !fact.contains("+0x1f0 lowering"))
    );
    assert!(
        plan.requirements[4]
            .missing_we_facts
            .iter()
            .all(|fact| !fact.contains("+0x428"))
    );
    assert_eq!(plan.requirements[4].missing_we_facts.len(), 1);
}

#[test]
fn recorder_requirements_reject_copy_back_without_retained_draw_bind() {
    let runtime = runtime(vec![
        token_program(),
        draw_mask(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
        copy_back(),
    ]);
    let resource_binds = resource_binds_for_runtime(&runtime, false);
    let schedule =
        native_vulkan_plan_scene_layer_alpha_mask_token_schedule(&runtime, &resource_binds)
            .expect("token schedule");
    let producer_draws = native_vulkan_plan_scene_layer_alpha_mask_producer_draws(
        &runtime,
        &resource_binds,
        &schedule,
    )
    .expect("producer draws");
    let producer_target_graph =
        native_vulkan_plan_scene_layer_alpha_mask_producer_target_graph(&runtime, &producer_draws)
            .expect("producer target graph");
    let producer_pipelines = native_vulkan_plan_scene_layer_alpha_mask_producer_pipelines(
        &producer_draws,
        &resource_binds,
    )
    .expect("producer pipelines");
    let producer_uniforms = native_vulkan_plan_scene_layer_alpha_mask_producer_uniforms(
        &producer_draws,
        &producer_pipelines,
    )
    .expect("producer uniforms");
    let generated_consumer_draws =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws(
            &runtime,
            &resource_binds,
            &schedule,
        )
        .expect("generated consumer draws");
    let (
        generated_consumer_targets,
        generated_consumer_pipelines,
        generated_consumer_commands,
        generated_consumer_uniforms,
    ) = generated_consumer_target_pipeline_command_uniform_plans(&generated_consumer_draws);

    let err = native_vulkan_plan_scene_layer_alpha_mask_recorder_requirements(
        &runtime,
        &resource_binds,
        &schedule,
        &producer_draws,
        &producer_target_graph,
        &producer_uniforms,
        &generated_consumer_draws,
        &generated_consumer_targets,
        &generated_consumer_pipelines,
        &generated_consumer_commands,
        &generated_consumer_uniforms,
    )
    .expect_err("copy-back must require retained draw bind");

    assert!(err.contains("requires exactly one retained copy-back draw heap bind"));
}

fn generated_consumer_target_pipeline_command_uniform_plans(
    generated_consumer_draws: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
) -> (
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerUniformPlan,
) {
    let targets = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_targets(
        generated_consumer_draws,
        |object, target| {
            Ok(NativeVulkanSceneLayerAlphaMaskLayerTargetBinding {
                object,
                layer_target: target,
                color_target: SceneGraphTarget::ObjectFinal(object),
                format: vk::Format::B8G8R8A8_UNORM,
                width: 3840,
                height: 2160,
                pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
            })
        },
    )
    .expect("generated consumer targets");
    let pipelines =
        native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets(
            generated_consumer_draws,
            &targets,
        )
        .expect("generated consumer pipelines");
    let commands =
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan::from_draws_targets_pipelines_and_heap(
            generated_consumer_draws,
            &targets,
            &pipelines,
            |heap_bind_index| Ok(generated_consumer_bind_info(heap_bind_index)),
            pipelines.cache_keys().len(),
        )
        .expect("generated consumer commands");
    let uniforms = native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_uniforms(&commands)
        .expect("generated consumer uniforms");
    (targets, pipelines, commands, uniforms)
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
    include_copy_back_draw_bind: bool,
) -> NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
    let token_commands = runtime
        .commands
        .iter()
        .enumerate()
        .map(|(command_index, command)| {
            let requirement = requirement_for_operation(command.operation);
            let matched_heap_bind_indices = if requirement
                == NativeVulkanSceneLayerAlphaMaskBindRequirement::TokenProgramNoResourceBind
            {
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
                requirement,
                matched_bind_count: matched_heap_bind_indices.len(),
                matched_heap_bind_indices,
                command_order: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    let binds = heap_binds_for_runtime(runtime, include_copy_back_draw_bind);
    let token_command_resource_bind_count = token_commands
        .iter()
        .map(|command| command.matched_bind_count)
        .sum();
    let draw_clipping_mask_command_bind_count = token_commands
        .iter()
        .filter(|command| {
            command.requirement
                == NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4
        })
        .map(|command| command.matched_bind_count)
        .sum();
    let generated_clippingtarget_command_bind_count = token_commands
        .iter()
        .filter(|command| {
            command.requirement
                == NativeVulkanSceneLayerAlphaMaskBindRequirement::GeneratedClippingTarget
        })
        .map(|command| command.matched_bind_count)
        .sum();
    let clippingmaskimage4_bind_count = binds
        .iter()
        .filter(|bind| {
            matches!(
                bind.role,
                NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 { .. }
            )
        })
        .count();
    let generated_clippingtarget_bind_count = binds
        .iter()
        .filter(|bind| {
            bind.role == NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget
        })
        .count();
    let flattexture_copy_back_bind_count = binds
        .iter()
        .filter(|bind| {
            bind.role == NativeVulkanSceneLayerAlphaMaskTextureBindRole::FlatTextureCopyBack
        })
        .count();
    let copy_back_draw_binds = if include_copy_back_draw_bind {
        runtime
            .commands
            .iter()
            .enumerate()
            .filter(|(_, command)| {
                command.operation == SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
            })
            .map(|(command_index, command)| {
                NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan {
                    copy_back_draw_index: 0,
                    command_index,
                    object: command.object,
                    shader: "util/minimalalpha",
                    texture_slot: 0,
                    texture_source: NativeVulkanSceneTextureDescriptorSource::GraphTarget(
                        SceneGraphTarget::FullAlphaMaskIntermediate,
                    ),
                    bind_index: command_index,
                    heap_bind_index: command_index,
                    heap_slice_index: command_index,
                    base_resource_descriptor_index: command_index,
                    base_sampler_descriptor_index: command_index,
                    command_order: [
                        "read_flattexture_copy_back_draw_resource",
                        "select_flattexture_copy_back_heap_bind",
                        "bind_flattexture_copy_back_resource_heap",
                        "draw_minimalalpha_copy_back",
                    ],
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
        heap_bind_count: binds.len(),
        resource_heap_bind_count: binds.len(),
        clippingmaskimage4_bind_count,
        generated_clippingtarget_bind_count,
        flattexture_copy_back_bind_count,
        token_command_count: token_commands.len(),
        token_command_resource_bind_count,
        draw_clipping_mask_command_bind_count,
        generated_clippingtarget_command_bind_count,
        copy_back_command_count: copy_back_draw_binds.len(),
        copy_back_draw_resource_count: copy_back_draw_binds.len(),
        copy_back_draw_bind_count: copy_back_draw_binds.len(),
        binds,
        token_commands,
        copy_back_draws: Vec::new(),
        copy_back_draw_binds,
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

fn heap_binds_for_runtime(
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    include_copy_back_draw_bind: bool,
) -> Vec<NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan> {
    runtime
        .commands
        .iter()
        .enumerate()
        .filter_map(|(command_index, command)| {
            heap_bind_for_command(command_index, command, include_copy_back_draw_bind)
        })
        .collect()
}

fn heap_bind_for_command(
    command_index: usize,
    command: &NativeVulkanSceneLayerAlphaMaskCommandPlan,
    include_copy_back_draw_bind: bool,
) -> Option<NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan> {
    match command.operation {
        SceneLayerCompositorOperation::TokenProgramDispatch => None,
        SceneLayerCompositorOperation::DrawClippingMask => Some(alpha_mask_heap_bind(
            command_index,
            command.object,
            "we/clippingmaskimage4",
            NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 {
                clipping_record_index: command_index as u32,
            },
            command.operation,
            vec![
                (
                    0,
                    NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                        SceneResourceId(100 + command_index as u32),
                    ),
                ),
                (
                    1,
                    NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                        SceneResourceId(200 + command_index as u32),
                    ),
                ),
            ],
        )),
        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
            if include_copy_back_draw_bind =>
        {
            Some(alpha_mask_heap_bind(
                command_index,
                command.object,
                "util/minimalalpha",
                NativeVulkanSceneLayerAlphaMaskTextureBindRole::FlatTextureCopyBack,
                command.operation,
                vec![(
                    0,
                    NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                        SceneGraphTarget::FullAlphaMaskIntermediate,
                    ),
                )],
            ))
        }
        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask => None,
        SceneLayerCompositorOperation::DrawGeneratedClippingTarget => Some(alpha_mask_heap_bind(
            command_index,
            command.object,
            "we/genericimage4",
            NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget,
            command.operation,
            vec![
                (
                    0,
                    NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                        SceneResourceId(300 + command_index as u32),
                    ),
                ),
                (
                    8,
                    NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                        SceneGraphTarget::FullAlphaMask,
                    ),
                ),
            ],
        )),
        _ => None,
    }
}

fn alpha_mask_heap_bind(
    heap_bind_index: usize,
    object: SceneObjectId,
    shader: &str,
    role: NativeVulkanSceneLayerAlphaMaskTextureBindRole,
    operation: SceneLayerCompositorOperation,
    slots: Vec<(u32, NativeVulkanSceneLayerAlphaMaskDescriptorSource)>,
) -> NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan {
    let heap_slice_bindings = slots
        .iter()
        .map(
            |(slot, source)| NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
                slot: *slot,
                source: *source,
            },
        )
        .collect::<Vec<_>>();
    let material = (role
        == NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget)
        .then(|| NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding {
            key: NativeVulkanSceneMaterialUniformKey {
                object,
                shader: shader.to_owned(),
            },
            buffer_handle: 0x4200 + heap_bind_index as u64,
            device_address: 0x4280 + heap_bind_index as u64,
            record_index: heap_bind_index,
            bytes: 48,
            payload_hash: 0x1000 + heap_bind_index as u64,
        });
    let has_material = material.is_some();
    let mut shader_mappings = Vec::new();
    if has_material {
        shader_mappings
            .push("WE PSSetConstantBuffers(slot=3) -> alpha-mask-heap-slice-offset0".to_owned());
    }
    let texture_offset = usize::from(has_material);
    shader_mappings.extend(slots.iter().enumerate().map(|(ordinal, (slot, _))| {
        format!(
            "we.texture_slot{slot}.g_Texture{slot} -> alpha-mask-heap-slice-offset{}",
            ordinal + texture_offset
        )
    }));

    NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan {
        heap_bind_index,
        object,
        puppet: ScenePuppetId(5),
        shader: shader.to_owned(),
        role,
        operation,
        bind: NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan {
            heap_bind_index,
            object,
            puppet: ScenePuppetId(5),
            shader: shader.to_owned(),
            role,
            heap_slice_index: heap_bind_index,
            heap_slice: NativeVulkanSceneLayerAlphaMaskHeapSliceKey {
                shader: shader.to_owned(),
                bindings: heap_slice_bindings,
            },
            material,
            base_resource_descriptor_index: heap_bind_index.saturating_mul(2),
            base_sampler_descriptor_index: heap_bind_index.saturating_mul(2) + 16,
            resource_descriptor_count: slots.len() + usize::from(has_material),
            texture_count: slots.len(),
            shader_mappings,
            command_order: ["cmd_bind_resource_heap_ext", "cmd_bind_sampler_heap_ext"],
        },
    }
}

fn generated_consumer_bind_info(
    heap_bind_index: usize,
) -> NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
    let object = SceneObjectId(7);
    let shader = "we/genericimage4".to_owned();
    let bindings = vec![
        NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
            slot: 0,
            source: NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                SceneResourceId(300 + heap_bind_index as u32),
            ),
        },
        NativeVulkanSceneLayerAlphaMaskHeapSliceBinding {
            slot: 8,
            source: NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                SceneGraphTarget::FullAlphaMask,
            ),
        },
    ];
    NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
        heap_bind_index,
        object,
        puppet: ScenePuppetId(5),
        shader: shader.clone(),
        role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget,
        heap_slice_index: heap_bind_index,
        heap_slice: NativeVulkanSceneLayerAlphaMaskHeapSliceKey { shader, bindings },
        material: Some(NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding {
            key: NativeVulkanSceneMaterialUniformKey {
                object,
                shader: "we/genericimage4".to_owned(),
            },
            buffer_handle: 0x4200 + heap_bind_index as u64,
            device_address: 0x4280 + heap_bind_index as u64,
            record_index: heap_bind_index,
            bytes: 48,
            payload_hash: 0x1000 + heap_bind_index as u64,
        }),
        base_resource_descriptor_index: heap_bind_index.saturating_mul(2),
        base_sampler_descriptor_index: heap_bind_index.saturating_mul(2) + 16,
        resource_descriptor_count: 3,
        texture_count: 2,
        shader_mappings: vec![
            "WE PSSetConstantBuffers(slot=3) -> alpha-mask-heap-slice-offset0".to_owned(),
            "we.texture_slot0.g_Texture0 -> alpha-mask-heap-slice-offset1".to_owned(),
            "we.texture_slot8.g_Texture8 -> alpha-mask-heap-slice-offset2".to_owned(),
        ],
        resource_bind: vk::BindHeapInfoEXT::builder().build(),
        sampler_bind: vk::BindHeapInfoEXT::builder().build(),
    }
}

fn requirement_for_operation(
    operation: SceneLayerCompositorOperation,
) -> NativeVulkanSceneLayerAlphaMaskBindRequirement {
    match operation {
        SceneLayerCompositorOperation::TokenProgramDispatch => {
            NativeVulkanSceneLayerAlphaMaskBindRequirement::TokenProgramNoResourceBind
        }
        SceneLayerCompositorOperation::DrawClippingMask => {
            NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4
        }
        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask => {
            NativeVulkanSceneLayerAlphaMaskBindRequirement::FlatTextureCopyBackSeparateDrawResourceBind
        }
        SceneLayerCompositorOperation::DrawGeneratedClippingTarget => {
            NativeVulkanSceneLayerAlphaMaskBindRequirement::GeneratedClippingTarget
        }
        _ => unreachable!("test only uses alpha-mask operations"),
    }
}

fn token_program() -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
    command(
        SceneLayerCompositorOperation::TokenProgramDispatch,
        SceneLayerCompositorCondition::Always,
        None,
        SceneLayerCompositorTarget::LayerTarget490,
        super::super::NativeVulkanSceneLayerAlphaMaskAccess::TokenProgram,
        NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
        SceneLayerCompositorBlendKey::Inherit,
    )
}

fn draw_mask(target: SceneLayerCompositorTarget) -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
    let condition = match target {
        SceneLayerCompositorTarget::FullAlphaMask => {
            SceneLayerCompositorCondition::Token1OrToken2FirstPair
        }
        SceneLayerCompositorTarget::FullAlphaMaskIntermediate => {
            SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask
        }
        _ => unreachable!("test only uses alpha-mask targets"),
    };
    command(
        SceneLayerCompositorOperation::DrawClippingMask,
        condition,
        None,
        target,
        NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskAttachmentWrite,
        NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
        SceneLayerCompositorBlendKey::Inherit,
    )
}

fn copy_back() -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
    command(
        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
        SceneLayerCompositorCondition::Token2AfterIntermediateMask,
        Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
        SceneLayerCompositorTarget::FullAlphaMask,
        NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskSampleAndAttachmentWrite,
        NativeVulkanSceneLayerAlphaMaskCopyMethod::FlatTextureDrawDestColorBlendKey0x100,
        SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
    )
}

fn generated_target() -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
    command(
        SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
        SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
        Some(SceneLayerCompositorTarget::FullAlphaMask),
        SceneLayerCompositorTarget::LayerTarget490,
        NativeVulkanSceneLayerAlphaMaskAccess::FullMaskSampleForGeneratedTarget,
        NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
        SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0,
    )
}

fn command(
    operation: SceneLayerCompositorOperation,
    condition: SceneLayerCompositorCondition,
    source: Option<SceneLayerCompositorTarget>,
    target: SceneLayerCompositorTarget,
    access: NativeVulkanSceneLayerAlphaMaskAccess,
    copy_method: NativeVulkanSceneLayerAlphaMaskCopyMethod,
    blend_key: SceneLayerCompositorBlendKey,
) -> NativeVulkanSceneLayerAlphaMaskCommandPlan {
    NativeVulkanSceneLayerAlphaMaskCommandPlan {
        object: SceneObjectId(7),
        entry: match operation {
            SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask => {
                SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed
            }
            SceneLayerCompositorOperation::TokenProgramDispatch => {
                SceneLayerCompositorEntry::TokenizedCompositeEntry52
            }
            _ => SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
        },
        operation,
        condition,
        source,
        target,
        source_graph_target: source.and_then(graph_target),
        target_graph_target: graph_target(target),
        access,
        copy_method,
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
