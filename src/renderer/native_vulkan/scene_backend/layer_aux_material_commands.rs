//! WE auxiliary material scoped draw command planning for active layer [50].
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/tools/audit_opacity_final_alpha_path.py`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneObjectId, WE_LAYER_AUX_CLEAR_MATERIAL_OFFSET, WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET,
};

use super::layer_aux_clear_scope::{
    NativeVulkanSceneLayerAuxClearScopeCommandPlan, NativeVulkanSceneLayerAuxClearScopeFramePlan,
};
use super::layer_aux_material_draws::{
    NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
    NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
    NativeVulkanSceneLayerAuxMaterialDrawReceiverKind,
};

pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_BIND_HELPER_VMA: u64 = 0x140155fc0;
pub(in crate::renderer::native_vulkan) const WE_AUX_MATERIAL_RELEASE_HELPER_VMA: u64 = 0x140157430;
pub(in crate::renderer::native_vulkan) const WE_AUX_CLEAR_MATERIAL_BIND_CALL_VMA: u64 = 0x140207832;
pub(in crate::renderer::native_vulkan) const WE_AUX_CLEAR_MATERIAL_TARGET_DRAW_CALL_VMA: u64 =
    0x140207848;
pub(in crate::renderer::native_vulkan) const WE_AUX_CLEAR_MATERIAL_RELEASE_CALL_VMA: u64 =
    0x140207859;
pub(in crate::renderer::native_vulkan) const WE_AUX_GENERATED_STATE_PREP_REGION: &str =
    "0x14020785e..0x140207a8d";
pub(in crate::renderer::native_vulkan) const WE_AUX_GENERATED_MATERIAL_BIND_CALL_VMA: u64 =
    0x140207a9b;
pub(in crate::renderer::native_vulkan) const WE_AUX_GENERATED_MATERIAL_TARGET_DRAW_CALL_VMA: u64 =
    0x140207ab1;
pub(in crate::renderer::native_vulkan) const WE_AUX_GENERATED_MATERIAL_RELEASE_CALL_VMA: u64 =
    0x140207ac2;
pub(in crate::renderer::native_vulkan) const WE_AUX_GENERATED_STATE_CLEANUP_REGION: &str =
    "0x140207ac7..0x140207af4";
pub(in crate::renderer::native_vulkan) const WE_AUX_TARGET_RESTORE_REGION: &str =
    "0x140207b02..0x140207b39";
pub(in crate::renderer::native_vulkan) const WE_AUX_GENERATED_ACTIVE_ENTRY_BLEND_BYTE_SOURCE: &str =
    "[aux+0x18] + [aux+0x390] * 0xc8 + 0x1c -> [state+0x12e9]";
pub(in crate::renderer::native_vulkan) const WE_AUX_GENERATED_VEC4_SOURCE: &str =
    "[aux+0x350] + [state+0x12e9] * 0x10 -> [state+0x12ec]";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialCommandFramePlan {
    pub active_block_count: usize,
    pub command_count: usize,
    pub scoped_draw_count: usize,
    pub material_bind_count: usize,
    pub material_release_count: usize,
    pub target_draw_count: usize,
    pub generated_state_prep_count: usize,
    pub generated_state_cleanup_count: usize,
    pub commands: Vec<NativeVulkanSceneLayerAuxMaterialCommandPlan>,
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialCommandPlan {
    pub command_index: usize,
    pub block_index: usize,
    pub object: SceneObjectId,
    pub clear_scope_command_index: usize,
    pub clear_material_draw: NativeVulkanSceneLayerAuxScopedMaterialDrawPlan,
    pub generated_material_draw: NativeVulkanSceneLayerAuxScopedMaterialDrawPlan,
    pub target_restore_region: &'static str,
    pub reference_points: [&'static str; 5],
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxScopedMaterialDrawPlan {
    pub draw_kind: NativeVulkanSceneLayerAuxScopedMaterialDrawKind,
    pub material_offset: u32,
    pub target_offset: u32,
    pub receiver_kind: NativeVulkanSceneLayerAuxMaterialDrawReceiverKind,
    pub bind_helper_vma: u64,
    pub bind_call_vma: u64,
    pub target_draw_call_vma: u64,
    pub release_helper_vma: u64,
    pub release_call_vma: u64,
    pub target_draw_method_vma: u64,
    pub layout_bitmask: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub state_prep_region: Option<&'static str>,
    pub state_cleanup_region: Option<&'static str>,
    pub generated_active_entry_blend_byte_source: Option<&'static str>,
    pub generated_vec4_source: Option<&'static str>,
    pub matrix_stack_operation: Option<&'static str>,
    pub color_factor_operation: Option<&'static str>,
    pub pipeline_status: &'static str,
    pub resource_heap_status: &'static str,
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAuxScopedMaterialDrawKind {
    ClearMaterialAux410ToAux3f0,
    GeneratedMaterialAux408ToAux3f8,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_aux_material_commands(
    clear_scopes: &NativeVulkanSceneLayerAuxClearScopeFramePlan,
    material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
) -> Result<NativeVulkanSceneLayerAuxMaterialCommandFramePlan, String> {
    if clear_scopes.command_count == 0 {
        if material_draws.command_count != 0 {
            return Err(format!(
                "scene layer aux material commands need clear scopes for {} material draw command(s), got 0",
                material_draws.command_count
            ));
        }
        return Ok(NativeVulkanSceneLayerAuxMaterialCommandFramePlan::empty());
    }
    if !clear_scopes.covers_material_draws(material_draws) {
        return Err(format!(
            "scene layer aux material commands need clear scopes for {} material draw command(s), got {}",
            material_draws.command_count, clear_scopes.command_count
        ));
    }

    let mut commands = Vec::with_capacity(clear_scopes.command_count);
    for scope in &clear_scopes.commands {
        let material = material_draws
            .commands
            .iter()
            .find(|command| {
                command.block_index == scope.block_index && command.object == scope.object
            })
            .ok_or_else(|| {
                format!(
                    "scene layer aux material command block {} object {:?} has no material receiver plan",
                    scope.block_index, scope.object
                )
            })?;
        commands.push(
            NativeVulkanSceneLayerAuxMaterialCommandPlan::from_scope_and_material(scope, material)?,
        );
    }

    Ok(NativeVulkanSceneLayerAuxMaterialCommandFramePlan::from_commands(commands))
}

impl NativeVulkanSceneLayerAuxMaterialCommandFramePlan {
    pub(in crate::renderer::native_vulkan) fn empty() -> Self {
        Self {
            active_block_count: 0,
            command_count: 0,
            scoped_draw_count: 0,
            material_bind_count: 0,
            material_release_count: 0,
            target_draw_count: 0,
            generated_state_prep_count: 0,
            generated_state_cleanup_count: 0,
            commands: Vec::new(),
            command_order: aux_material_command_frame_order(),
        }
    }

    fn from_commands(commands: Vec<NativeVulkanSceneLayerAuxMaterialCommandPlan>) -> Self {
        let scoped_draw_count = commands.len().saturating_mul(2);
        Self {
            active_block_count: commands.len(),
            command_count: commands.len(),
            scoped_draw_count,
            material_bind_count: scoped_draw_count,
            material_release_count: scoped_draw_count,
            target_draw_count: scoped_draw_count,
            generated_state_prep_count: commands.len(),
            generated_state_cleanup_count: commands.len(),
            commands,
            command_order: aux_material_command_frame_order(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn covers_clear_scopes(
        &self,
        clear_scopes: &NativeVulkanSceneLayerAuxClearScopeFramePlan,
    ) -> bool {
        self.command_count == clear_scopes.command_count
            && self.scoped_draw_count == clear_scopes.material_draw_receiver_count
            && clear_scopes.commands.iter().all(|scope| {
                self.commands.iter().any(|command| {
                    command.block_index == scope.block_index
                        && command.object == scope.object
                        && command.clear_scope_command_index == scope.command_index
                })
            })
    }
}

impl NativeVulkanSceneLayerAuxMaterialCommandPlan {
    fn from_scope_and_material(
        scope: &NativeVulkanSceneLayerAuxClearScopeCommandPlan,
        material: &NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
    ) -> Result<Self, String> {
        if scope.material_draw_command_index != material.command_index {
            return Err(format!(
                "scene layer aux material command block {} has clear-scope material index {}, got {}",
                scope.block_index, scope.material_draw_command_index, material.command_index
            ));
        }
        Ok(Self {
            command_index: scope.command_index,
            block_index: scope.block_index,
            object: scope.object,
            clear_scope_command_index: scope.command_index,
            clear_material_draw: NativeVulkanSceneLayerAuxScopedMaterialDrawPlan::clear_material(
                material,
            )?,
            generated_material_draw:
                NativeVulkanSceneLayerAuxScopedMaterialDrawPlan::generated_material(material)?,
            target_restore_region: WE_AUX_TARGET_RESTORE_REGION,
            reference_points: [
                "reverse-engineered/docs/exe/blend-and-render.md: 0x140207824..0x140207859 draws [aux+0x410] through [aux+0x3f0]",
                "reverse-engineered/docs/exe/blend-and-render.md: 0x14020785e..0x140207a8d prepares generated material state",
                "reverse-engineered/docs/exe/blend-and-render.md: 0x140207a94..0x140207ac2 draws [aux+0x408] through [aux+0x3f8]",
                "reverse-engineered/docs/exe/d3d11-context-calls.md: 0x140155fc0 bind and 0x140157430 release material scope",
                "references/godot/servers/rendering/rendering_device_graph.cpp: material and draw commands are ordered inside the target access scope",
            ],
            command_order: [
                "enter_aux_clear_scope",
                "bind_aux_0x410_material",
                "draw_aux_0x3f0_target_receiver",
                "release_aux_0x410_material",
                "prepare_aux_generated_material_state",
                "bind_aux_0x408_material_and_draw_aux_0x3f8",
                "cleanup_aux_generated_material_state",
                "restore_parent_target_scope",
            ],
        })
    }
}

impl NativeVulkanSceneLayerAuxScopedMaterialDrawPlan {
    fn clear_material(
        material: &NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
    ) -> Result<Self, String> {
        let receiver = &material.clear_material;
        if receiver.receiver_kind
            != NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f0ClearMaterialNonIndexed
            || receiver.material_offset != WE_LAYER_AUX_CLEAR_MATERIAL_OFFSET
        {
            return Err(format!(
                "scene layer aux material command block {} expected aux+0x410 -> aux+0x3f0 receiver",
                material.block_index
            ));
        }
        Ok(Self {
            draw_kind: NativeVulkanSceneLayerAuxScopedMaterialDrawKind::ClearMaterialAux410ToAux3f0,
            material_offset: receiver.material_offset,
            target_offset: receiver.target_offset,
            receiver_kind: receiver.receiver_kind,
            bind_helper_vma: WE_AUX_MATERIAL_BIND_HELPER_VMA,
            bind_call_vma: WE_AUX_CLEAR_MATERIAL_BIND_CALL_VMA,
            target_draw_call_vma: WE_AUX_CLEAR_MATERIAL_TARGET_DRAW_CALL_VMA,
            release_helper_vma: WE_AUX_MATERIAL_RELEASE_HELPER_VMA,
            release_call_vma: WE_AUX_CLEAR_MATERIAL_RELEASE_CALL_VMA,
            target_draw_method_vma: receiver.draw_method_vma,
            layout_bitmask: receiver.layout_bitmask,
            vertex_stride_bytes: receiver.vertex_stride_bytes,
            vertex_count: receiver.vertex_count,
            index_count: receiver.index_count,
            state_prep_region: None,
            state_cleanup_region: None,
            generated_active_entry_blend_byte_source: None,
            generated_vec4_source: None,
            matrix_stack_operation: None,
            color_factor_operation: None,
            pipeline_status: "requires aux clear material pipeline binding before vkCmdDraw",
            resource_heap_status: "requires aux clear material resource heap slice before draw",
            command_order: [
                "bind_material_scope_0x140155fc0",
                "commit_material_state",
                "bind_aux_0x3f0_target_vertex_stream",
                "record_target_vtable_1_draw",
                "release_material_scope_0x140157430",
                "preserve_no_target_switch_inside_draw",
                "feed_aux_command_block_recorder",
            ],
        })
    }

    fn generated_material(
        material: &NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
    ) -> Result<Self, String> {
        let receiver = &material.generated_material;
        if receiver.receiver_kind
            != NativeVulkanSceneLayerAuxMaterialDrawReceiverKind::Aux3f8GeneratedMaterialIndexed
            || receiver.material_offset != WE_LAYER_AUX_GENERATED_MATERIAL_OFFSET
        {
            return Err(format!(
                "scene layer aux material command block {} expected aux+0x408 -> aux+0x3f8 receiver",
                material.block_index
            ));
        }
        Ok(Self {
            draw_kind:
                NativeVulkanSceneLayerAuxScopedMaterialDrawKind::GeneratedMaterialAux408ToAux3f8,
            material_offset: receiver.material_offset,
            target_offset: receiver.target_offset,
            receiver_kind: receiver.receiver_kind,
            bind_helper_vma: WE_AUX_MATERIAL_BIND_HELPER_VMA,
            bind_call_vma: WE_AUX_GENERATED_MATERIAL_BIND_CALL_VMA,
            target_draw_call_vma: WE_AUX_GENERATED_MATERIAL_TARGET_DRAW_CALL_VMA,
            release_helper_vma: WE_AUX_MATERIAL_RELEASE_HELPER_VMA,
            release_call_vma: WE_AUX_GENERATED_MATERIAL_RELEASE_CALL_VMA,
            target_draw_method_vma: receiver.draw_method_vma,
            layout_bitmask: receiver.layout_bitmask,
            vertex_stride_bytes: receiver.vertex_stride_bytes,
            vertex_count: receiver.vertex_count,
            index_count: receiver.index_count,
            state_prep_region: Some(WE_AUX_GENERATED_STATE_PREP_REGION),
            state_cleanup_region: Some(WE_AUX_GENERATED_STATE_CLEANUP_REGION),
            generated_active_entry_blend_byte_source: Some(
                WE_AUX_GENERATED_ACTIVE_ENTRY_BLEND_BYTE_SOURCE,
            ),
            generated_vec4_source: Some(WE_AUX_GENERATED_VEC4_SOURCE),
            matrix_stack_operation: Some(
                "push state+0x30/state+0x40 matrices, write identity to current matrix, copy to state+0x38, then pop both stacks",
            ),
            color_factor_operation: Some(
                "when object+0x4b0 == 0, copy object color/scalar fields into state+0x120..0x12c before binding aux+0x408",
            ),
            pipeline_status: "requires aux generated material pipeline binding before vkCmdDrawIndexed",
            resource_heap_status: "requires aux generated material resource heap slice and generated uniform state before draw",
            command_order: [
                "prepare_generated_material_state",
                "bind_material_scope_0x140155fc0",
                "commit_material_state",
                "bind_aux_0x3f8_target_indexed_stream",
                "record_target_vtable_1_draw",
                "release_material_scope_0x140157430",
                "cleanup_generated_material_state",
            ],
        })
    }
}

fn aux_material_command_frame_order() -> [&'static str; 8] {
    [
        "read_aux_clear_scope_commands",
        "require_aux_material_draw_receivers",
        "emit_aux_0x410_material_scope",
        "emit_aux_0x3f0_target_draw_command",
        "emit_generated_material_state_prep",
        "emit_aux_0x408_material_scope",
        "emit_aux_0x3f8_target_draw_command",
        "feed_compositor_command_block_recorder",
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
    use crate::renderer::native_vulkan::scene_backend::layer_aux_material_draws::native_vulkan_plan_scene_layer_aux_material_draws;
    use crate::renderer::native_vulkan::scene_backend::layer_compositor_scheduler::{
        NativeVulkanSceneLayerCompositorRecordingBlock,
        NativeVulkanSceneLayerCompositorRecordingBlockKind,
        NativeVulkanSceneLayerCompositorSchedulePlan, NativeVulkanSceneLayerCompositorScheduleStep,
        NativeVulkanSceneLayerCompositorScheduledKind,
    };

    #[test]
    fn aux_material_command_plan_preserves_we_scope_draw_order() {
        let object = SceneObjectId(1530);
        let clear_prep =
            native_vulkan_plan_scene_layer_aux_clear_prep(&schedule(object), &residency(object))
                .expect("clear prep");
        let material_draws =
            native_vulkan_plan_scene_layer_aux_material_draws(&clear_prep, &residency(object))
                .expect("material draws");
        let clear_scopes =
            NativeVulkanSceneLayerAuxClearScopeFramePlan::from_test_command_for_material_draws(
                &material_draws,
            );

        let plan =
            native_vulkan_plan_scene_layer_aux_material_commands(&clear_scopes, &material_draws)
                .expect("material command plan");

        assert_eq!(plan.active_block_count, 1);
        assert_eq!(plan.scoped_draw_count, 2);
        assert_eq!(plan.material_bind_count, 2);
        assert_eq!(plan.material_release_count, 2);
        assert!(plan.covers_clear_scopes(&clear_scopes));
        let command = &plan.commands[0];
        assert_eq!(
            command.clear_material_draw.draw_kind,
            NativeVulkanSceneLayerAuxScopedMaterialDrawKind::ClearMaterialAux410ToAux3f0
        );
        assert_eq!(
            command.clear_material_draw.bind_call_vma,
            WE_AUX_CLEAR_MATERIAL_BIND_CALL_VMA
        );
        assert_eq!(
            command.clear_material_draw.target_draw_call_vma,
            WE_AUX_CLEAR_MATERIAL_TARGET_DRAW_CALL_VMA
        );
        assert_eq!(
            command.generated_material_draw.draw_kind,
            NativeVulkanSceneLayerAuxScopedMaterialDrawKind::GeneratedMaterialAux408ToAux3f8
        );
        assert_eq!(
            command.generated_material_draw.state_prep_region,
            Some(WE_AUX_GENERATED_STATE_PREP_REGION)
        );
        assert_eq!(
            command
                .generated_material_draw
                .generated_active_entry_blend_byte_source,
            Some(WE_AUX_GENERATED_ACTIVE_ENTRY_BLEND_BYTE_SOURCE)
        );
        assert_eq!(
            command.generated_material_draw.state_cleanup_region,
            Some(WE_AUX_GENERATED_STATE_CLEANUP_REGION)
        );
        assert_eq!(command.target_restore_region, WE_AUX_TARGET_RESTORE_REGION);
    }

    #[test]
    fn aux_material_command_plan_requires_scope_coverage() {
        let object = SceneObjectId(1530);
        let clear_prep =
            native_vulkan_plan_scene_layer_aux_clear_prep(&schedule(object), &residency(object))
                .expect("clear prep");
        let material_draws =
            native_vulkan_plan_scene_layer_aux_material_draws(&clear_prep, &residency(object))
                .expect("material draws");

        let err = native_vulkan_plan_scene_layer_aux_material_commands(
            &NativeVulkanSceneLayerAuxClearScopeFramePlan::empty(),
            &material_draws,
        )
        .expect_err("scope coverage is required");

        assert!(err.contains("clear scopes"));
    }

    trait AuxClearScopeTestPlan {
        fn from_test_command_for_material_draws(
            material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
        ) -> NativeVulkanSceneLayerAuxClearScopeFramePlan;
    }

    impl AuxClearScopeTestPlan for NativeVulkanSceneLayerAuxClearScopeFramePlan {
        fn from_test_command_for_material_draws(
            material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
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
                    target: crate::engine::scene_engine::SceneGraphTarget::LayerAuxClear(
                        command.object,
                    ),
                    target_format: "R8G8B8A8_UNORM",
                    width: 3840,
                    height: 2160,
                    initial_layout: "undefined",
                    final_layout: "color-attachment-optimal",
                    clear_color_bits: [0, 0, 0, 0],
                    target_scope:
                        crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetScopePlan {
                            width: 3840,
                            height: 2160,
                            load_op:
                                crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp::Clear,
                            begin_command_order: [
                                "cmd_pipeline_barrier2_color_attachment",
                                "cmd_begin_rendering",
                            ],
                            end_command_order: [
                                "cmd_end_rendering",
                                "retain_color_attachment_layout",
                            ],
                        },
                    material_draw_command_index: command.command_index,
                    material_draw_receiver_count: 2,
                    reference_points: [
                        "test",
                        "test",
                        "test",
                        "test",
                        "test",
                    ],
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
                route: SceneLayerCompositorRoute::DirectSwapchain,
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
