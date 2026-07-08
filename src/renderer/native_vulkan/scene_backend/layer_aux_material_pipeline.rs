//! Pipeline facts for WE auxiliary material scoped draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/materials/util/fullscreenlayer.json`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.vert`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/passthrough.frag`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneBlendContract, SceneGraphPipelineClass, SceneGraphTarget, SceneMaterialRenderState,
    SceneObjectId,
};

use super::layer_aux_clear_scope::{
    NativeVulkanSceneLayerAuxClearScopeCommandPlan, NativeVulkanSceneLayerAuxClearScopeFramePlan,
};
use super::layer_aux_material_commands::{
    NativeVulkanSceneLayerAuxMaterialCommandFramePlan,
    NativeVulkanSceneLayerAuxMaterialCommandPlan, NativeVulkanSceneLayerAuxScopedMaterialDrawKind,
};
use super::layer_aux_material_draws::NativeVulkanSceneLayerAuxMaterialDrawReceiverKind;
use super::pipeline::{
    NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineResourceHeapClass,
    NativeVulkanScenePipelineVertexLayout,
};

pub(in crate::renderer::native_vulkan) const WE_AUX_FULLSCREEN_LAYER_MATERIAL: &str =
    "materials/util/fullscreenlayer.json";
pub(in crate::renderer::native_vulkan) const WE_AUX_FULLSCREEN_LAYER_SHADER: &str =
    "util/passthrough";
pub(in crate::renderer::native_vulkan) const WE_AUX_FULLSCREEN_LAYER_TEXTURE_SOURCE: &str =
    "_rt_FullFrameBuffer";
pub(in crate::renderer::native_vulkan) const WE_AUX_FULLSCREEN_LAYER_TEXTURE_SLOT: u32 = 0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialPipelineFramePlan {
    pub active_command_count: usize,
    pub clear_pipeline_count: usize,
    pub generated_material_entry_pipeline_required_count: usize,
    pub cache_key_count: usize,
    pub clear_keys: Vec<NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan>,
    pub generated_requirements: Vec<NativeVulkanSceneLayerAuxGeneratedMaterialPipelineRequirement>,
    pub command_order: [&'static str; 6],
    #[serde(skip)]
    cache_keys: Vec<NativeVulkanScenePipelineCacheKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan {
    pub command_index: usize,
    pub block_index: usize,
    pub object: SceneObjectId,
    pub material: &'static str,
    pub shader: &'static str,
    pub source: &'static str,
    pub target: SceneGraphTarget,
    pub target_format: &'static str,
    pub texture_slot: u32,
    pub texture_slot_mask: u32,
    pub pipeline_class: SceneGraphPipelineClass,
    pub vertex_layout: NativeVulkanScenePipelineVertexLayout,
    pub resource_heap: NativeVulkanScenePipelineResourceHeapClass,
    pub draw_receiver: NativeVulkanSceneLayerAuxMaterialDrawReceiverKind,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxGeneratedMaterialPipelineRequirement
{
    pub command_index: usize,
    pub block_index: usize,
    pub object: SceneObjectId,
    pub draw_receiver: NativeVulkanSceneLayerAuxMaterialDrawReceiverKind,
    pub material_offset: u32,
    pub target_offset: u32,
    pub layout_bitmask: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub material_entry_source: &'static str,
    pub shader_source_required: &'static str,
    pub command_order: [&'static str; 5],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_aux_material_pipelines(
    clear_scopes: &NativeVulkanSceneLayerAuxClearScopeFramePlan,
    material_commands: &NativeVulkanSceneLayerAuxMaterialCommandFramePlan,
) -> Result<NativeVulkanSceneLayerAuxMaterialPipelineFramePlan, String> {
    if material_commands.command_count == 0 {
        if clear_scopes.command_count != 0 {
            return Err(format!(
                "scene layer aux material pipeline needs material commands for {} clear scope(s), got 0",
                clear_scopes.command_count
            ));
        }
        return Ok(NativeVulkanSceneLayerAuxMaterialPipelineFramePlan::empty());
    }
    if !material_commands.covers_clear_scopes(clear_scopes) {
        return Err(format!(
            "scene layer aux material pipeline needs scoped material commands for {} clear scope(s), got {}",
            clear_scopes.command_count, material_commands.command_count
        ));
    }

    let mut clear_keys = Vec::with_capacity(material_commands.command_count);
    let mut generated_requirements = Vec::with_capacity(material_commands.command_count);
    let mut cache_keys = Vec::new();
    for command in &material_commands.commands {
        let scope = clear_scopes
            .commands
            .iter()
            .find(|scope| scope.command_index == command.clear_scope_command_index)
            .ok_or_else(|| {
                format!(
                    "scene layer aux material pipeline command {} has no clear scope {}",
                    command.command_index, command.clear_scope_command_index
                )
            })?;
        validate_scope_for_command(scope, command)?;
        let clear_key = NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan::from_command_and_scope(
            command, scope,
        )?;
        let cache_key = clear_key.cache_key()?;
        if !cache_keys.iter().any(|existing| existing == &cache_key) {
            cache_keys.push(cache_key);
        }
        clear_keys.push(clear_key);
        generated_requirements.push(
            NativeVulkanSceneLayerAuxGeneratedMaterialPipelineRequirement::from_command(command)?,
        );
    }

    Ok(NativeVulkanSceneLayerAuxMaterialPipelineFramePlan {
        active_command_count: material_commands.command_count,
        clear_pipeline_count: clear_keys.len(),
        generated_material_entry_pipeline_required_count: generated_requirements.len(),
        cache_key_count: cache_keys.len(),
        clear_keys,
        generated_requirements,
        command_order: aux_material_pipeline_frame_order(),
        cache_keys,
    })
}

impl NativeVulkanSceneLayerAuxMaterialPipelineFramePlan {
    pub(in crate::renderer::native_vulkan) fn empty() -> Self {
        Self {
            active_command_count: 0,
            clear_pipeline_count: 0,
            generated_material_entry_pipeline_required_count: 0,
            cache_key_count: 0,
            clear_keys: Vec::new(),
            generated_requirements: Vec::new(),
            command_order: aux_material_pipeline_frame_order(),
            cache_keys: Vec::new(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn cache_keys(
        &self,
    ) -> &[NativeVulkanScenePipelineCacheKey] {
        &self.cache_keys
    }

    pub(in crate::renderer::native_vulkan) fn covers_material_commands(
        &self,
        material_commands: &NativeVulkanSceneLayerAuxMaterialCommandFramePlan,
    ) -> bool {
        self.active_command_count == material_commands.command_count
            && self.clear_pipeline_count == material_commands.command_count
            && self.generated_material_entry_pipeline_required_count
                == material_commands.command_count
            && material_commands.commands.iter().all(|command| {
                self.clear_keys.iter().any(|key| {
                    key.command_index == command.command_index
                        && key.block_index == command.block_index
                        && key.object == command.object
                }) && self.generated_requirements.iter().any(|requirement| {
                    requirement.command_index == command.command_index
                        && requirement.block_index == command.block_index
                        && requirement.object == command.object
                })
            })
    }
}

impl NativeVulkanSceneLayerAuxMaterialPipelineKeyPlan {
    fn from_command_and_scope(
        command: &NativeVulkanSceneLayerAuxMaterialCommandPlan,
        scope: &NativeVulkanSceneLayerAuxClearScopeCommandPlan,
    ) -> Result<Self, String> {
        let draw = &command.clear_material_draw;
        if draw.draw_kind
            != NativeVulkanSceneLayerAuxScopedMaterialDrawKind::ClearMaterialAux410ToAux3f0
            || draw.receiver_kind
                != NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f0ClearMaterialNonIndexed
        {
            return Err(format!(
                "scene layer aux material pipeline command {} expected aux+0x410 fullscreenlayer clear draw",
                command.command_index
            ));
        }
        let target_format = aux_target_format(scope.target_format)?;
        Ok(Self {
            command_index: command.command_index,
            block_index: command.block_index,
            object: command.object,
            material: WE_AUX_FULLSCREEN_LAYER_MATERIAL,
            shader: WE_AUX_FULLSCREEN_LAYER_SHADER,
            source: WE_AUX_FULLSCREEN_LAYER_TEXTURE_SOURCE,
            target: scope.target,
            target_format: aux_target_format_label(target_format),
            texture_slot: WE_AUX_FULLSCREEN_LAYER_TEXTURE_SLOT,
            texture_slot_mask: 1u32 << WE_AUX_FULLSCREEN_LAYER_TEXTURE_SLOT,
            pipeline_class: SceneGraphPipelineClass::LayerUtilityIndexed,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv,
            resource_heap: NativeVulkanScenePipelineResourceHeapClass::LayerAuxMaterial,
            draw_receiver: draw.receiver_kind,
            command_order: [
                "read_materials_util_fullscreenlayer_json",
                "select_util_passthrough_shader",
                "bind_rt_full_frame_buffer_as_g_texture0",
                "select_aux_0x3e8_color_target_format",
                "select_position_uv_triangle_receiver_aux_0x3f0",
                "derive_resource_heap_scoped_pipeline_key",
            ],
        })
    }

    fn cache_key(&self) -> Result<NativeVulkanScenePipelineCacheKey, String> {
        Ok(NativeVulkanScenePipelineCacheKey {
            shader: self.shader.to_owned(),
            shader_combo_values: Vec::new(),
            blend: SceneBlendContract::TranslucentAlpha,
            render_state: SceneMaterialRenderState::translucent_2d(),
            pipeline_class: self.pipeline_class,
            vertex_layout: self.vertex_layout,
            resource_heap: self.resource_heap,
            target_format: aux_target_format(self.target_format)?,
            texture_slot_mask: self.texture_slot_mask,
        })
    }
}

impl NativeVulkanSceneLayerAuxGeneratedMaterialPipelineRequirement {
    fn from_command(
        command: &NativeVulkanSceneLayerAuxMaterialCommandPlan,
    ) -> Result<Self, String> {
        let draw = &command.generated_material_draw;
        if draw.draw_kind
            != NativeVulkanSceneLayerAuxScopedMaterialDrawKind::GeneratedMaterialAux408ToAux3f8
            || draw.receiver_kind
                != NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed
        {
            return Err(format!(
                "scene layer aux generated material pipeline command {} expected aux+0x408 indexed draw",
                command.command_index
            ));
        }
        Ok(Self {
            command_index: command.command_index,
            block_index: command.block_index,
            object: command.object,
            draw_receiver: draw.receiver_kind,
            material_offset: draw.material_offset,
            target_offset: draw.target_offset,
            layout_bitmask: draw.layout_bitmask,
            vertex_stride_bytes: draw.vertex_stride_bytes,
            vertex_count: draw.vertex_count,
            index_count: draw.index_count,
            material_entry_source: "active material entry [aux+0x18] + [aux+0x390] * 0xc8",
            shader_source_required: "generated aux+0x408 material shader and resource heap slice must come from retained material entry, not mesh fallback",
            command_order: [
                "preserve_generated_material_state_prep",
                "read_active_material_entry_shader_contract",
                "derive_generated_material_pipeline_key_from_entry",
                "bind_generated_material_resource_heap_slice",
                "record_indexed_aux_0x3f8_receiver_draw",
            ],
        })
    }
}

fn validate_scope_for_command(
    scope: &NativeVulkanSceneLayerAuxClearScopeCommandPlan,
    command: &NativeVulkanSceneLayerAuxMaterialCommandPlan,
) -> Result<(), String> {
    if scope.block_index != command.block_index || scope.object != command.object {
        return Err(format!(
            "scene layer aux material pipeline scope/command mismatch: scope block {} object {:?}, command block {} object {:?}",
            scope.block_index, scope.object, command.block_index, command.object
        ));
    }
    if scope.material_draw_command_index != command.command_index {
        return Err(format!(
            "scene layer aux material pipeline scope references material command {}, got {}",
            scope.material_draw_command_index, command.command_index
        ));
    }
    Ok(())
}

fn aux_target_format(label: &str) -> Result<vk::Format, String> {
    match label {
        "R8G8B8A8_UNORM" => Ok(vk::Format::R8G8B8A8_UNORM),
        "R16G16B16A16_SFLOAT" => Ok(vk::Format::R16G16B16A16_SFLOAT),
        _ => Err(format!(
            "scene layer aux material pipeline has unsupported aux target format {label}"
        )),
    }
}

fn aux_target_format_label(format: vk::Format) -> &'static str {
    match format {
        vk::Format::R8G8B8A8_UNORM => "R8G8B8A8_UNORM",
        vk::Format::R16G16B16A16_SFLOAT => "R16G16B16A16_SFLOAT",
        _ => "other",
    }
}

fn aux_material_pipeline_frame_order() -> [&'static str; 6] {
    [
        "read_aux_material_scoped_commands",
        "derive_fullscreenlayer_passthrough_clear_pipeline",
        "keep_aux_material_pipeline_on_layer_aux_resource_heap",
        "retain_aux_0x3f0_position_uv_receiver_layout",
        "require_active_entry_shader_for_aux_0x408",
        "feed_layer_compositor_aux_recorder",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency, SceneLayerAuxCompositeTargetsResidency,
        SceneLayerCompositorEntry, SceneLayerCompositorOperation, SceneLayerCompositorRoute,
        SceneResidentResource, SceneResourceResidencyPlan, WE_LAYER_AUX_CLEAR_TARGET_AUX_FORMAT,
        WE_LAYER_AUX_CLEAR_TARGET_CACHE_SELECTOR, WE_LAYER_AUX_CLEAR_TARGET_R9_SELECTOR,
        WE_LAYER_AUX_CLEAR_TARGET_RESOURCE_SELECTOR,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_aux_clear_prep::native_vulkan_plan_scene_layer_aux_clear_prep;
    use crate::renderer::native_vulkan::scene_backend::layer_aux_material_commands::native_vulkan_plan_scene_layer_aux_material_commands;
    use crate::renderer::native_vulkan::scene_backend::layer_aux_material_draws::native_vulkan_plan_scene_layer_aux_material_draws;
    use crate::renderer::native_vulkan::scene_backend::layer_compositor_scheduler::{
        NativeVulkanSceneLayerCompositorRecordingBlock,
        NativeVulkanSceneLayerCompositorRecordingBlockKind,
        NativeVulkanSceneLayerCompositorSchedulePlan, NativeVulkanSceneLayerCompositorScheduleStep,
        NativeVulkanSceneLayerCompositorScheduledKind,
    };
    use crate::renderer::native_vulkan::scene_backend::render_target::{
        NativeVulkanSceneRenderTargetLoadOp, NativeVulkanSceneRenderTargetScopePlan,
    };

    #[test]
    fn aux_material_pipeline_plan_derives_fullscreenlayer_passthrough_key() {
        let object = SceneObjectId(1530);
        let clear_prep =
            native_vulkan_plan_scene_layer_aux_clear_prep(&schedule(object), &residency(object))
                .expect("clear prep");
        let material_draws =
            native_vulkan_plan_scene_layer_aux_material_draws(&clear_prep, &residency(object))
                .expect("material draws");
        let scopes = clear_scope_for_material_draws(&material_draws);
        let material_commands =
            native_vulkan_plan_scene_layer_aux_material_commands(&scopes, &material_draws)
                .expect("material commands");

        let plan =
            native_vulkan_plan_scene_layer_aux_material_pipelines(&scopes, &material_commands)
                .expect("aux material pipeline plan");

        assert_eq!(plan.active_command_count, 1);
        assert_eq!(plan.clear_pipeline_count, 1);
        assert_eq!(plan.generated_material_entry_pipeline_required_count, 1);
        assert!(plan.covers_material_commands(&material_commands));
        let key = &plan.clear_keys[0];
        assert_eq!(key.material, WE_AUX_FULLSCREEN_LAYER_MATERIAL);
        assert_eq!(key.shader, WE_AUX_FULLSCREEN_LAYER_SHADER);
        assert_eq!(key.source, WE_AUX_FULLSCREEN_LAYER_TEXTURE_SOURCE);
        assert_eq!(key.target, SceneGraphTarget::LayerAuxClear(object));
        assert_eq!(key.texture_slot_mask, 1);
        assert_eq!(
            key.resource_heap,
            NativeVulkanScenePipelineResourceHeapClass::LayerAuxMaterial
        );
        assert_eq!(
            plan.cache_keys()[0].vertex_layout,
            NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv
        );
        assert_eq!(
            plan.cache_keys()[0].resource_heap,
            NativeVulkanScenePipelineResourceHeapClass::LayerAuxMaterial
        );
        assert_eq!(
            plan.cache_keys()[0].target_format,
            vk::Format::R8G8B8A8_UNORM
        );
        let generated = &plan.generated_requirements[0];
        assert_eq!(
            generated.draw_receiver,
            NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed
        );
        assert_eq!(generated.layout_bitmask, 0x180000f);
        assert_eq!(generated.vertex_stride_bytes, 80);
        assert_eq!(generated.vertex_count, 4106);
        assert_eq!(generated.index_count, 23_988);
    }

    fn clear_scope_for_material_draws(
        material_draws: &super::super::layer_aux_material_draws::NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
    ) -> NativeVulkanSceneLayerAuxClearScopeFramePlan {
        let command = &material_draws.commands[0];
        NativeVulkanSceneLayerAuxClearScopeFramePlan {
            active_block_count: 1,
            command_count: 1,
            target_scope_count: 1,
            transparent_clear_count: 1,
            material_draw_receiver_count: 2,
            commands: vec![NativeVulkanSceneLayerAuxClearScopeCommandPlan {
                command_index: command.command_index,
                block_index: command.block_index,
                object: command.object,
                target: SceneGraphTarget::LayerAuxClear(command.object),
                target_format: "R8G8B8A8_UNORM",
                width: 3840,
                height: 2160,
                initial_layout: "undefined",
                final_layout: "color-attachment-optimal",
                clear_color_bits: [0, 0, 0, 0],
                target_scope: NativeVulkanSceneRenderTargetScopePlan {
                    width: 3840,
                    height: 2160,
                    load_op: NativeVulkanSceneRenderTargetLoadOp::Clear,
                    begin_command_order: [
                        "cmd_pipeline_barrier2_color_attachment",
                        "cmd_begin_rendering",
                    ],
                    end_command_order: ["cmd_end_rendering", "retain_color_attachment_layout"],
                },
                material_draw_command_index: command.command_index,
                material_draw_receiver_count: 2,
                reference_points: ["test", "test", "test", "test", "test"],
                command_order: [
                    "resolve_retained_aux_0x3e8_target",
                    "validate_0x14020a07b_extent_and_format",
                    "begin_aux_0x3e8_target_scope_with_transparent_clear",
                    "record_aux_0x410_to_aux_0x3f0_draw",
                    "record_aux_0x408_to_aux_0x3f8_draw",
                    "end_aux_0x3e8_target_scope",
                    "retain_color_attachment_layout_for_following_reads",
                ],
            }],
            command_order: [
                "read_aux_clear_prep_commands",
                "require_aux_material_draw_receiver_plan",
                "resolve_retained_layer_aux_clear_target",
                "validate_aux_target_extent_and_format",
                "plan_transparent_black_clear_scope",
                "attach_material_draw_receivers_to_scope",
                "retain_explicit_target_layout",
                "feed_layer_compositor_command_blocks",
            ],
        }
    }

    fn schedule(object: SceneObjectId) -> NativeVulkanSceneLayerCompositorSchedulePlan {
        NativeVulkanSceneLayerCompositorSchedulePlan {
            layer_count: 1,
            command_count: 1,
            direct_mesh_graph_command_count: 0,
            object_final_producer_command_count: 0,
            object_final_composite_command_count: 0,
            alpha_mask_token_draw_list_command_count: 0,
            token_program_no_draw_count: 0,
            clear_prep_early_out_no_draw_count: 0,
            clear_prep_recorder_required_count: 1,
            recording_block_count: 1,
            mesh_graph_draw_span_block_count: 0,
            alpha_mask_token_recording_block_count: 0,
            no_draw_marker_block_count: 0,
            all_alpha_mask_commands_recordable: true,
            steps: vec![NativeVulkanSceneLayerCompositorScheduleStep {
                global_command_index: 0,
                layer_index: 0,
                layer_command_index: 0,
                object,
                route: SceneLayerCompositorRoute::ObjectFinalMeshComposite,
                entry: SceneLayerCompositorEntry::ClearPrepEntry50,
                operation: SceneLayerCompositorOperation::ClearPrep,
                scheduled_kind:
                    NativeVulkanSceneLayerCompositorScheduledKind::LayerTargetClearPrepRecorderRequired,
                graph_pass_index: None,
                graph_draw_index: None,
                token_recording_step_index: None,
                command_order: vec!["classify_clear_prep_for_test"],
            }],
            recording_blocks: vec![NativeVulkanSceneLayerCompositorRecordingBlock {
                block_index: 0,
                step_index_start: 0,
                step_index_end: 1,
                command_count: 1,
                kind:
                    NativeVulkanSceneLayerCompositorRecordingBlockKind::LayerTargetClearPrepRecorderRequired,
                graph_pass_index: None,
                graph_draw_index_start: None,
                graph_draw_index_end: None,
                token_recording_step_index: None,
                command_order: vec!["record_clear_prep_for_test"],
            }],
            command_order: [
                "walk_layer_compositor_layers",
                "classify_layer_commands",
                "attach_mesh_graph_draw_indices",
                "attach_alpha_mask_token_recording_steps",
                "coalesce_no_draw_markers",
                "coalesce_contiguous_recording_blocks",
                "count_command_block_kinds",
                "emit_layer_compositor_schedule_plan",
            ],
        }
    }

    fn residency(object: SceneObjectId) -> SceneResourceResidencyPlan {
        SceneResourceResidencyPlan {
            resources: vec![
                SceneResidentResource::LayerAuxCompositeTargets(
                    SceneLayerAuxCompositeTargetsResidency {
                        object,
                        clear_target_3e8: true,
                        material_target_3f0: true,
                        effect_target_3f8: true,
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
                        clear_prep_ready: true,
                    },
                ),
                SceneResidentResource::LayerAlphaMaskRtMethod8MdlvGeometry(
                    SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency {
                        object,
                        entry_owner_index: 0,
                        layout_key: 0x180000f,
                        vertex_stride_bytes: 80,
                        vertex_count: 4106,
                        index_count: 23_988,
                        vertex_bytes: 328_480,
                        index_bytes: 47_976,
                        source_record_count: 44,
                        subdraw_count: 4,
                    },
                ),
            ],
        }
    }
}
