//! WE auxiliary clear/prep command contract for native Vulkan scene recording.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/reconstructed/cpp/wallpaper64/layer/resource_update_0x1402065e0.cpp`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneGraphTarget, SceneLayerAuxCompositeTargetsResidency, SceneLayerCompositorOperation,
    SceneLayerCompositorPlan, SceneObjectId, SceneResidentResource, SceneResourceResidencyPlan,
    WE_AUX_CLEAR_PREP_VMA, WE_LAYER_AUX_CLEAR_MATERIAL_OFFSET, WE_LAYER_AUX_CLEAR_TARGET_OFFSET,
    WE_LAYER_AUX_EFFECT_TARGET_OFFSET, WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET,
    WE_LAYER_AUX_MATERIAL_TARGET_OFFSET,
};

use super::layer_compositor_scheduler::{
    NativeVulkanSceneLayerCompositorRecordingBlockKind,
    NativeVulkanSceneLayerCompositorSchedulePlan,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxClearPrepFramePlan {
    pub active_block_count: usize,
    pub command_count: usize,
    pub target_push_count: usize,
    pub clear_target_count: usize,
    pub material_scope_count: usize,
    pub material_target_draw_count: usize,
    pub target_pop_count: usize,
    pub commands: Vec<NativeVulkanSceneLayerAuxClearPrepCommandPlan>,
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxClearPrepCommandPlan {
    pub command_index: usize,
    pub block_index: usize,
    pub step_index_start: usize,
    pub step_index_end: usize,
    pub object: SceneObjectId,
    pub clear_target_offset: u32,
    pub clear_target: SceneGraphTarget,
    pub clear_source_width: u32,
    pub clear_source_height: u32,
    pub clear_target_width: u32,
    pub clear_target_height: u32,
    pub clear_uv_y_flipped: bool,
    pub clear_target_color_format: u32,
    pub clear_target_aux_format: u32,
    pub clear_target_r9_selector: u32,
    pub clear_target_resource_selector: u32,
    pub clear_target_cache_selector: u32,
    pub material_target_offset: u32,
    pub effect_target_offset: u32,
    pub generated_material_offset: u32,
    pub clear_material_offset: u32,
    pub clear_color: [u32; 4],
    pub clear_prep_vma: u64,
    pub target_stack_operation: &'static str,
    pub clear_call_site: &'static str,
    pub first_material_scope: &'static str,
    pub first_draw_receiver: &'static str,
    pub second_material_scope: &'static str,
    pub second_draw_receiver: &'static str,
    pub restore_operation: &'static str,
    pub reference_points: [&'static str; 5],
    pub command_order: [&'static str; 9],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_aux_clear_prep(
    schedule: &NativeVulkanSceneLayerCompositorSchedulePlan,
    residency: &SceneResourceResidencyPlan,
) -> Result<NativeVulkanSceneLayerAuxClearPrepFramePlan, String> {
    if schedule.clear_prep_recorder_required_count == 0 {
        return Ok(NativeVulkanSceneLayerAuxClearPrepFramePlan::empty());
    }

    let mut commands = Vec::with_capacity(schedule.clear_prep_recorder_required_count);
    for block in &schedule.recording_blocks {
        if block.kind
            != NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired
        {
            continue;
        }
        let step = schedule.steps.get(block.step_index_start).ok_or_else(|| {
            format!(
                "scene layer aux clear-prep block {} starts outside schedule at step {}",
                block.block_index, block.step_index_start
            )
        })?;
        if block.step_index_end != block.step_index_start.saturating_add(1) {
            return Err(format!(
                "scene layer aux clear-prep block {} must contain one WE [50] step, got {}..{}",
                block.block_index, block.step_index_start, block.step_index_end
            ));
        }
        let targets = aux_targets_for_object(residency, step.object).ok_or_else(|| {
            format!(
                "scene layer aux clear-prep block {} object {:?} has no complete SceneLayerAuxCompositeTargets residency for aux+0x3e8",
                block.block_index, step.object
            )
        })?;
        commands.push(NativeVulkanSceneLayerAuxClearPrepCommandPlan::from_block(
            commands.len(),
            block.block_index,
            block.step_index_start,
            block.step_index_end,
            step.object,
            targets,
        )?);
    }

    if commands.len() != schedule.clear_prep_recorder_required_count {
        return Err(format!(
            "scene layer aux clear-prep expected {} active [50] block(s), planned {}",
            schedule.clear_prep_recorder_required_count,
            commands.len()
        ));
    }

    Ok(NativeVulkanSceneLayerAuxClearPrepFramePlan::from_commands(
        commands,
    ))
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_aux_clear_prep_from_compositor(
    layer_compositor: &SceneLayerCompositorPlan,
    residency: &SceneResourceResidencyPlan,
) -> Result<NativeVulkanSceneLayerAuxClearPrepFramePlan, String> {
    if layer_compositor
        .layers
        .iter()
        .all(|layer| !layer.has_active_aux_clear_target)
    {
        return Ok(NativeVulkanSceneLayerAuxClearPrepFramePlan::empty());
    }

    let mut commands = Vec::new();
    let mut global_command_index = 0usize;
    for layer in &layer_compositor.layers {
        for command in &layer.commands {
            if command.operation != SceneLayerCompositorOperation::ClearPrep {
                global_command_index = global_command_index.saturating_add(1);
                continue;
            }
            if !layer.has_active_aux_clear_target {
                global_command_index = global_command_index.saturating_add(1);
                continue;
            }
            let targets = aux_targets_for_object(residency, layer.object).ok_or_else(|| {
                format!(
                    "scene layer aux clear-prep object {:?} has active [50] layer command but no complete SceneLayerAuxCompositeTargets residency",
                    layer.object
                )
            })?;
            commands.push(NativeVulkanSceneLayerAuxClearPrepCommandPlan::from_block(
                commands.len(),
                global_command_index,
                global_command_index,
                global_command_index.saturating_add(1),
                layer.object,
                targets,
            )?);
            global_command_index = global_command_index.saturating_add(1);
        }
    }

    Ok(NativeVulkanSceneLayerAuxClearPrepFramePlan::from_commands(
        commands,
    ))
}

impl NativeVulkanSceneLayerAuxClearPrepFramePlan {
    pub(in crate::renderer::native_vulkan) fn empty() -> Self {
        Self {
            active_block_count: 0,
            command_count: 0,
            target_push_count: 0,
            clear_target_count: 0,
            material_scope_count: 0,
            material_target_draw_count: 0,
            target_pop_count: 0,
            commands: Vec::new(),
            command_order: aux_clear_prep_frame_order(),
        }
    }

    fn from_commands(commands: Vec<NativeVulkanSceneLayerAuxClearPrepCommandPlan>) -> Self {
        Self {
            active_block_count: commands.len(),
            command_count: commands.len(),
            target_push_count: commands.len(),
            clear_target_count: commands.len(),
            material_scope_count: commands.len().saturating_mul(2),
            material_target_draw_count: commands.len().saturating_mul(2),
            target_pop_count: commands.len(),
            commands,
            command_order: aux_clear_prep_frame_order(),
        }
    }
}

impl NativeVulkanSceneLayerAuxClearPrepCommandPlan {
    fn from_block(
        command_index: usize,
        block_index: usize,
        step_index_start: usize,
        step_index_end: usize,
        object: SceneObjectId,
        targets: SceneLayerAuxCompositeTargetsResidency,
    ) -> Result<Self, String> {
        if !targets.clear_prep_ready {
            return Err(format!(
                "scene layer aux clear-prep object {object:?} has incomplete aux target/material residency"
            ));
        }
        Ok(Self {
            command_index,
            block_index,
            step_index_start,
            step_index_end,
            object,
            clear_target_offset: WE_LAYER_AUX_CLEAR_TARGET_OFFSET,
            clear_target: SceneGraphTarget::LayerAuxClear(object),
            clear_source_width: targets.clear_source_width,
            clear_source_height: targets.clear_source_height,
            clear_target_width: targets.clear_target_width,
            clear_target_height: targets.clear_target_height,
            clear_uv_y_flipped: targets.clear_uv_y_flipped,
            clear_target_color_format: targets.clear_target_color_format,
            clear_target_aux_format: targets.clear_target_aux_format,
            clear_target_r9_selector: targets.clear_target_r9_selector,
            clear_target_resource_selector: targets.clear_target_resource_selector,
            clear_target_cache_selector: targets.clear_target_cache_selector,
            material_target_offset: WE_LAYER_AUX_MATERIAL_TARGET_OFFSET,
            effect_target_offset: WE_LAYER_AUX_EFFECT_TARGET_OFFSET,
            generated_material_offset: WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET,
            clear_material_offset: WE_LAYER_AUX_CLEAR_MATERIAL_OFFSET,
            clear_color: [0, 0, 0, 0],
            clear_prep_vma: WE_AUX_CLEAR_PREP_VMA,
            target_stack_operation: "push [aux+0x3e8] target and call target vtable +0x48",
            clear_call_site: "wrapper +0x118/+0x120 transparent black clear",
            first_material_scope: "0x140155fc0([aux+0x410]) -> draw [aux+0x3f0] -> 0x140157430",
            first_draw_receiver: "[aux+0x3f0].vtable+0x8",
            second_material_scope: "0x140155fc0([aux+0x408]) -> draw [aux+0x3f8] -> 0x140157430",
            second_draw_receiver: "[aux+0x3f8].vtable+0x8",
            restore_operation: "pop [aux+0x3e8] target stack and restore previous target",
            reference_points: [
                "reverse-engineered/docs/exe/blend-and-render.md: 0x140207740 aux clear/prep cluster",
                "reverse-engineered/docs/exe/d3d11-context-calls.md: [layer+0x4b8]+0x3e8 early-out and restore",
                "reverse-engineered/reconstructed/cpp/wallpaper64/layer/resource_update_0x1402065e0.cpp: 0x140209540 resource-update route",
                "0x14020a07b creates [aux+0x3e8] with desc +0x2c/+0x30, color format 0/0xe, aux format 0x1b",
                "0x14020a2bd stores [aux+0x410]; 0x14020a573 releases/zeros [aux+0x3e8]",
            ],
            command_order: [
                "require_complete_layer_aux_composite_targets",
                "push_aux_0x3e8_target_scope",
                "clear_aux_0x3e8_to_transparent_black",
                "bind_aux_0x410_material_scope",
                "draw_aux_0x3f0_target_vtable_8",
                "bind_aux_0x408_material_scope",
                "draw_aux_0x3f8_target_vtable_8",
                "release_aux_material_scopes",
                "pop_aux_0x3e8_target_scope",
            ],
        })
    }
}

fn aux_targets_for_object(
    residency: &SceneResourceResidencyPlan,
    object: SceneObjectId,
) -> Option<SceneLayerAuxCompositeTargetsResidency> {
    residency
        .resources
        .iter()
        .find_map(|resource| match resource {
            SceneResidentResource::LayerAuxCompositeTargets(targets)
                if targets.object == object && targets.clear_prep_ready =>
            {
                Some(*targets)
            }
            _ => None,
        })
}

fn aux_clear_prep_frame_order() -> [&'static str; 8] {
    [
        "read_layer_compositor_active_clear_prep_blocks",
        "join_blocks_to_scene_layer_aux_composite_targets",
        "require_aux_0x3e8_0x3f0_0x3f8_0x408_0x410",
        "preserve_0x14020a07b_target_create_arguments",
        "emit_target_push_clear_material_draw_pop_sequence",
        "preserve_we_0x140207740_order",
        "keep_descriptor_heap_model",
        "emit_layer_aux_clear_prep_frame_plan",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneLayerAuxCompositeTargetsResidency, SceneLayerCompositorBlendKey,
        SceneLayerCompositorCommand, SceneLayerCompositorCondition, SceneLayerCompositorEntry,
        SceneLayerCompositorLayer, SceneLayerCompositorOperation, SceneLayerCompositorPlan,
        SceneLayerCompositorRoute, SceneLayerCompositorTarget,
        WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT, WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR,
        WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR, WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_compositor_scheduler::{
        NativeVulkanSceneLayerCompositorRecordingBlock,
        NativeVulkanSceneLayerCompositorScheduleStep,
        NativeVulkanSceneLayerCompositorScheduledKind,
    };

    #[test]
    fn aux_clear_prep_plan_emits_fixed_we_command_sequence() {
        let object = SceneObjectId(7);
        let schedule = active_clear_schedule(object);
        let plan = native_vulkan_plan_scene_layer_aux_clear_prep(
            &schedule,
            &residency_with_aux_targets(object, true),
        )
        .expect("aux clear-prep plan");

        assert_eq!(plan.active_block_count, 1);
        assert_eq!(plan.target_push_count, 1);
        assert_eq!(plan.clear_target_count, 1);
        assert_eq!(plan.material_scope_count, 2);
        assert_eq!(plan.material_target_draw_count, 2);
        assert_eq!(plan.target_pop_count, 1);
        assert_eq!(plan.commands[0].clear_prep_vma, 0x140207740);
        assert_eq!(plan.commands[0].clear_target_offset, 0x3e8);
        assert_eq!(
            plan.commands[0].clear_target,
            SceneGraphTarget::LayerAuxClear(object)
        );
        assert_eq!(plan.commands[0].clear_target_width, 3840);
        assert_eq!(plan.commands[0].clear_target_height, 2160);
        assert_eq!(plan.commands[0].clear_target_color_format, 0);
        assert_eq!(
            plan.commands[0].clear_target_aux_format,
            WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT
        );
        assert_eq!(
            plan.commands[0].clear_target_r9_selector,
            WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR
        );
        assert_eq!(
            plan.commands[0].clear_target_resource_selector,
            WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR
        );
        assert_eq!(
            plan.commands[0].clear_target_cache_selector,
            WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR
        );
        assert_eq!(plan.commands[0].material_target_offset, 0x3f0);
        assert_eq!(plan.commands[0].effect_target_offset, 0x3f8);
        assert_eq!(plan.commands[0].generated_material_offset, 0x408);
        assert_eq!(plan.commands[0].clear_material_offset, 0x410);
        assert_eq!(plan.commands[0].clear_color, [0, 0, 0, 0]);
        assert!(
            plan.commands[0]
                .command_order
                .contains(&"draw_aux_0x3f8_target_vtable_8")
        );
    }

    #[test]
    fn aux_clear_prep_plan_rejects_missing_or_incomplete_aux_targets() {
        let object = SceneObjectId(7);
        let schedule = active_clear_schedule(object);

        let missing = native_vulkan_plan_scene_layer_aux_clear_prep(
            &schedule,
            &SceneResourceResidencyPlan::default(),
        )
        .expect_err("missing aux targets must fail");
        assert!(missing.contains("aux+0x3e8"));

        let incomplete = native_vulkan_plan_scene_layer_aux_clear_prep(
            &schedule,
            &residency_with_aux_targets(object, false),
        )
        .expect_err("incomplete aux targets must fail");
        assert!(incomplete.contains("aux+0x3e8"));
    }

    #[test]
    fn aux_clear_prep_plan_is_empty_without_active_clear_blocks() {
        let schedule = NativeVulkanSceneLayerCompositorSchedulePlan {
            clear_prep_recorder_required_count: 0,
            recording_block_count: 0,
            recording_blocks: Vec::new(),
            ..empty_schedule()
        };

        let plan = native_vulkan_plan_scene_layer_aux_clear_prep(
            &schedule,
            &SceneResourceResidencyPlan::default(),
        )
        .expect("empty aux clear-prep plan");

        assert_eq!(plan.command_count, 0);
        assert!(plan.commands.is_empty());
    }

    #[test]
    fn aux_clear_prep_from_compositor_uses_active_clear_command_order() {
        let object = SceneObjectId(7);
        let plan = native_vulkan_plan_scene_layer_aux_clear_prep_from_compositor(
            &active_clear_compositor(object),
            &residency_with_aux_targets(object, true),
        )
        .expect("aux clear-prep from compositor");

        assert_eq!(plan.command_count, 1);
        assert_eq!(plan.commands[0].block_index, 1);
        assert_eq!(plan.commands[0].step_index_start, 1);
        assert_eq!(plan.commands[0].step_index_end, 2);
        assert_eq!(plan.commands[0].object, object);
        assert_eq!(
            plan.commands[0].clear_target,
            SceneGraphTarget::LayerAuxClear(object)
        );
    }

    fn residency_with_aux_targets(
        object: SceneObjectId,
        complete: bool,
    ) -> SceneResourceResidencyPlan {
        SceneResourceResidencyPlan {
            resources: vec![SceneResidentResource::LayerAuxCompositeTargets(
                SceneLayerAuxCompositeTargetsResidency {
                    object,
                    clear_target_3e8: true,
                    material_target_3f0: true,
                    effect_target_3f8: complete,
                    generated_material_408: true,
                    clear_material_410: true,
                    clear_source_width: 3840,
                    clear_source_height: 2160,
                    clear_target_width: 3840,
                    clear_target_height: 2160,
                    clear_uv_y_flipped: false,
                    clear_target_color_format: 0,
                    clear_target_aux_format: WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT,
                    clear_target_r9_selector: WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR,
                    clear_target_resource_selector: WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR,
                    clear_target_cache_selector: WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR,
                    clear_prep_ready: complete,
                },
            )],
        }
    }

    fn active_clear_schedule(
        object: SceneObjectId,
    ) -> NativeVulkanSceneLayerCompositorSchedulePlan {
        NativeVulkanSceneLayerCompositorSchedulePlan {
            command_count: 1,
            clear_prep_recorder_required_count: 1,
            recording_block_count: 1,
            recording_blocks: vec![NativeVulkanSceneLayerCompositorRecordingBlock {
                block_index: 0,
                step_index_start: 0,
                step_index_end: 1,
                command_count: 1,
                kind: NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired,
                graph_pass_index: None,
                graph_draw_index_start: None,
                graph_draw_index_end: None,
                token_recording_step_index: None,
                command_order: vec!["require_layer_target_clear_prep_recorder"],
            }],
            steps: vec![NativeVulkanSceneLayerCompositorScheduleStep {
                global_command_index: 0,
                layer_index: 0,
                layer_command_index: 0,
                object,
                route: crate::engine::scene_engine::SceneLayerCompositorRoute::ObjectFinalMeshComposite,
                entry: SceneLayerCompositorEntry::ClearPrepEntry50,
                operation: SceneLayerCompositorOperation::ClearPrep,
                scheduled_kind: NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepRecorderRequired,
                graph_pass_index: None,
                graph_draw_index: None,
                token_recording_step_index: None,
                command_order: vec!["require_layer_target_clear_recorder"],
            }],
            ..empty_schedule()
        }
    }

    fn active_clear_compositor(object: SceneObjectId) -> SceneLayerCompositorPlan {
        SceneLayerCompositorPlan {
            layer_count: 1,
            command_count: 2,
            object_final_layer_count: 1,
            tokenized_layer_count: 0,
            layers: vec![SceneLayerCompositorLayer {
                object,
                route: SceneLayerCompositorRoute::ObjectFinalMeshComposite,
                uses_tokenized_subdraw: false,
                has_active_aux_clear_target: true,
                commands: vec![
                    SceneLayerCompositorCommand {
                        entry: SceneLayerCompositorEntry::NormalRenderEntry32,
                        operation: SceneLayerCompositorOperation::NormalRender,
                        condition: SceneLayerCompositorCondition::Always,
                        source: None,
                        target: SceneLayerCompositorTarget::ObjectFinal(object),
                        blend_key: SceneLayerCompositorBlendKey::Inherit,
                    },
                    SceneLayerCompositorCommand {
                        entry: SceneLayerCompositorEntry::ClearPrepEntry50,
                        operation: SceneLayerCompositorOperation::ClearPrep,
                        condition: SceneLayerCompositorCondition::Always,
                        source: None,
                        target: SceneLayerCompositorTarget::EffectTarget3f8,
                        blend_key: SceneLayerCompositorBlendKey::Inherit,
                    },
                ],
            }],
            command_order: [
                "read_scene_objects_in_author_order",
                "classify_object_final_composite_route",
                "append_normal_render_entry_32",
                "append_active_aux_clear_entry_50",
                "append_full_layer_composite_entry_51",
                "append_tokenized_entries_52_53",
                "append_alpha_mask_helper_commands",
                "preserve_we_layer_command_order",
                "emit_scene_layer_compositor_plan",
            ],
        }
    }

    fn empty_schedule() -> NativeVulkanSceneLayerCompositorSchedulePlan {
        NativeVulkanSceneLayerCompositorSchedulePlan {
            layer_count: 1,
            command_count: 0,
            direct_mesh_graph_command_count: 0,
            object_final_producer_command_count: 0,
            object_final_composite_command_count: 0,
            alpha_mask_token_draw_list_command_count: 0,
            token_program_no_draw_count: 0,
            clear_prep_early_out_no_draw_count: 0,
            clear_prep_recorder_required_count: 0,
            recording_block_count: 0,
            mesh_graph_draw_span_block_count: 0,
            alpha_mask_token_recording_block_count: 0,
            no_draw_marker_block_count: 0,
            all_alpha_mask_commands_recordable: true,
            steps: Vec::new(),
            recording_blocks: Vec::new(),
            command_order: [
                "read_scene_layer_compositor_order",
                "join_direct_layers_to_mesh_graph_draws",
                "join_object_final_producers_to_effect_runtime",
                "join_object_final_composites_to_graph_passes",
                "join_tokenized_commands_to_alpha_mask_token_recording",
                "coalesce_consecutive_mesh_graph_draws_into_recording_blocks",
                "reject_missing_alpha_mask_token_draw_list_steps",
                "emit_schedule_for_present_frame_recorder",
            ],
        }
    }
}
