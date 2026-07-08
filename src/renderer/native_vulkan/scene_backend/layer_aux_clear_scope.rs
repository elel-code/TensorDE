//! WE auxiliary clear target-scope planning for active layer [50].
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/reconstructed/cpp/wallpaper64/layer/resource_update_0x1402065e0.cpp`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneGraphTarget, SceneObjectId, WE_LAYER_AUX_CLEAR_TARGET_DEFAULT_COLOR_FORMAT,
    WE_LAYER_AUX_CLEAR_TARGET_HDR_COLOR_FORMAT,
};
use crate::renderer::native_vulkan::NativeVulkanClearColor;

use super::frame_resources::NativeVulkanSceneFrameResources;
use super::layer_aux_clear_prep::{
    NativeVulkanSceneLayerAuxClearPrepCommandPlan, NativeVulkanSceneLayerAuxClearPrepFramePlan,
};
use super::layer_aux_material_draws::{
    NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
    NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
};
use super::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use super::render_target::{
    NativeVulkanSceneOffscreenRenderTarget, NativeVulkanSceneRenderTarget,
    NativeVulkanSceneRenderTargetScopePlan, native_vulkan_scene_render_target_scope_plan,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxClearScopeFramePlan {
    pub active_block_count: usize,
    pub command_count: usize,
    pub target_scope_count: usize,
    pub transparent_clear_count: usize,
    pub material_draw_receiver_count: usize,
    pub commands: Vec<NativeVulkanSceneLayerAuxClearScopeCommandPlan>,
    pub command_order: [&'static str; 8],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxClearScopeCommandPlan {
    pub command_index: usize,
    pub block_index: usize,
    pub object: SceneObjectId,
    pub target: SceneGraphTarget,
    pub target_format: &'static str,
    pub width: u32,
    pub height: u32,
    pub initial_layout: &'static str,
    pub final_layout: &'static str,
    pub clear_color_bits: [u32; 4],
    pub target_scope: NativeVulkanSceneRenderTargetScopePlan,
    pub material_draw_command_index: usize,
    pub material_draw_receiver_count: usize,
    pub reference_points: [&'static str; 5],
    pub command_order: [&'static str; 7],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_aux_clear_scopes(
    frame_resources: &NativeVulkanSceneFrameResources,
    clear_prep: &NativeVulkanSceneLayerAuxClearPrepFramePlan,
    material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
) -> Result<NativeVulkanSceneLayerAuxClearScopeFramePlan, String> {
    native_vulkan_plan_scene_layer_aux_clear_scopes_with_targets(
        clear_prep,
        material_draws,
        |target| frame_resources.offscreen_target_binding(target),
    )
}

fn native_vulkan_plan_scene_layer_aux_clear_scopes_with_targets<ResolveTarget>(
    clear_prep: &NativeVulkanSceneLayerAuxClearPrepFramePlan,
    material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
    mut resolve_target: ResolveTarget,
) -> Result<NativeVulkanSceneLayerAuxClearScopeFramePlan, String>
where
    ResolveTarget:
        FnMut(SceneGraphTarget) -> Result<NativeVulkanSceneOffscreenTargetBinding, String>,
{
    if clear_prep.command_count == 0 {
        return Ok(NativeVulkanSceneLayerAuxClearScopeFramePlan::empty());
    }
    if !material_draws.covers_clear_prep(clear_prep) {
        return Err(format!(
            "scene layer aux clear scope needs material draw receivers for {} active clear-prep command(s), got {}",
            clear_prep.command_count, material_draws.command_count
        ));
    }

    let mut commands = Vec::with_capacity(clear_prep.command_count);
    for clear_command in &clear_prep.commands {
        let material_command = material_draws
            .commands
            .iter()
            .find(|command| {
                command.block_index == clear_command.block_index
                    && command.object == clear_command.object
            })
            .ok_or_else(|| {
                format!(
                    "scene layer aux clear scope block {} object {:?} has no material draw receiver plan",
                    clear_command.block_index, clear_command.object
                )
            })?;
        let binding = resolve_target(clear_command.clear_target)?;
        commands.push(NativeVulkanSceneLayerAuxClearScopeCommandPlan::from_parts(
            clear_command,
            material_command,
            binding,
        )?);
    }

    Ok(NativeVulkanSceneLayerAuxClearScopeFramePlan::from_commands(
        commands,
    ))
}

impl NativeVulkanSceneLayerAuxClearScopeFramePlan {
    pub(in crate::renderer::native_vulkan) fn empty() -> Self {
        Self {
            active_block_count: 0,
            command_count: 0,
            target_scope_count: 0,
            transparent_clear_count: 0,
            material_draw_receiver_count: 0,
            commands: Vec::new(),
            command_order: aux_clear_scope_frame_order(),
        }
    }

    fn from_commands(commands: Vec<NativeVulkanSceneLayerAuxClearScopeCommandPlan>) -> Self {
        let material_draw_receiver_count = commands
            .iter()
            .map(|command| command.material_draw_receiver_count)
            .sum();
        Self {
            active_block_count: commands.len(),
            command_count: commands.len(),
            target_scope_count: commands.len(),
            transparent_clear_count: commands.len(),
            material_draw_receiver_count,
            commands,
            command_order: aux_clear_scope_frame_order(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn covers_material_draws(
        &self,
        material_draws: &NativeVulkanSceneLayerAuxMaterialDrawFramePlan,
    ) -> bool {
        self.command_count == material_draws.command_count
            && self.material_draw_receiver_count == material_draws.draw_receiver_count
            && material_draws.commands.iter().all(|material_command| {
                self.commands.iter().any(|scope_command| {
                    scope_command.block_index == material_command.block_index
                        && scope_command.object == material_command.object
                        && scope_command.material_draw_command_index
                            == material_command.command_index
                })
            })
    }
}

impl NativeVulkanSceneLayerAuxClearScopeCommandPlan {
    fn from_parts(
        clear_command: &NativeVulkanSceneLayerAuxClearPrepCommandPlan,
        material_command: &NativeVulkanSceneLayerAuxMaterialDrawCommandPlan,
        binding: NativeVulkanSceneOffscreenTargetBinding,
    ) -> Result<Self, String> {
        if binding.target != clear_command.clear_target {
            return Err(format!(
                "scene layer aux clear scope target mismatch: clear-prep {:?}, retained {:?}",
                clear_command.clear_target, binding.target
            ));
        }
        if binding.width != clear_command.clear_target_width
            || binding.height != clear_command.clear_target_height
        {
            return Err(format!(
                "scene layer aux clear scope {:?} extent mismatch: clear-prep {}x{}, retained {}x{}",
                clear_command.clear_target,
                clear_command.clear_target_width,
                clear_command.clear_target_height,
                binding.width,
                binding.height
            ));
        }
        let expected_format = aux_clear_color_format(clear_command.clear_target_color_format)?;
        if binding.format != expected_format {
            return Err(format!(
                "scene layer aux clear scope {:?} format mismatch: selector {:#x} expects {}, retained {}",
                clear_command.clear_target,
                clear_command.clear_target_color_format,
                aux_clear_format_label(expected_format),
                aux_clear_format_label(binding.format)
            ));
        }

        let final_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
        let render_target =
            NativeVulkanSceneRenderTarget::Offscreen(NativeVulkanSceneOffscreenRenderTarget {
                target: binding.target,
                image: binding.image,
                image_view: binding.view,
                extent: vk::Extent2D {
                    width: binding.width,
                    height: binding.height,
                },
                initial_layout: binding.current_layout,
                final_layout,
            });
        let clear_color = transparent_clear_color();
        let target_scope =
            native_vulkan_scene_render_target_scope_plan(render_target, Some(clear_color))?;

        Ok(Self {
            command_index: clear_command.command_index,
            block_index: clear_command.block_index,
            object: clear_command.object,
            target: clear_command.clear_target,
            target_format: aux_clear_format_label(binding.format),
            width: binding.width,
            height: binding.height,
            initial_layout: aux_layout_label(binding.current_layout)?,
            final_layout: aux_layout_label(final_layout)?,
            clear_color_bits: [
                clear_color.r.to_bits(),
                clear_color.g.to_bits(),
                clear_color.b.to_bits(),
                clear_color.a.to_bits(),
            ],
            target_scope,
            material_draw_command_index: material_command.command_index,
            material_draw_receiver_count: 2,
            reference_points: [
                "reverse-engineered/docs/exe/blend-and-render.md: 0x140207740 pushes aux+0x3e8 before clear and material draws",
                "reverse-engineered/docs/exe/d3d11-context-calls.md: wrapper +0x118/+0x120 clears transparent black",
                "reverse-engineered/reconstructed/cpp/wallpaper64/layer/resource_update_0x1402065e0.cpp: 0x14020a07b creates aux+0x3e8 target",
                "references/godot/servers/rendering/rendering_device_graph.cpp: target access is planned before command recording",
                "references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp: dynamic rendering target layout is explicit",
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
        })
    }
}

fn transparent_clear_color() -> NativeVulkanClearColor {
    NativeVulkanClearColor {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    }
}

fn aux_clear_color_format(selector: u32) -> Result<vk::Format, String> {
    match selector {
        WE_LAYER_AUX_CLEAR_TARGET_DEFAULT_COLOR_FORMAT => Ok(vk::Format::R8G8B8A8_UNORM),
        WE_LAYER_AUX_CLEAR_TARGET_HDR_COLOR_FORMAT => Ok(vk::Format::R16G16B16A16_SFLOAT),
        _ => Err(format!(
            "scene layer aux clear scope has unsupported 0x14020a07b color format selector {selector:#x}"
        )),
    }
}

fn aux_clear_format_label(format: vk::Format) -> &'static str {
    match format {
        vk::Format::R8G8B8A8_UNORM => "R8G8B8A8_UNORM",
        vk::Format::R16G16B16A16_SFLOAT => "R16G16B16A16_SFLOAT",
        _ => "other",
    }
}

fn aux_layout_label(layout: vk::ImageLayout) -> Result<&'static str, String> {
    match layout {
        vk::ImageLayout::UNDEFINED => Ok("undefined"),
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => Ok("color-attachment-optimal"),
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => Ok("shader-read-only-optimal"),
        vk::ImageLayout::GENERAL => Ok("general"),
        _ => Err(format!(
            "scene layer aux clear scope does not support image layout {layout:?}"
        )),
    }
}

fn aux_clear_scope_frame_order() -> [&'static str; 8] {
    [
        "read_aux_clear_prep_commands",
        "require_aux_material_draw_receiver_plan",
        "resolve_retained_layer_aux_clear_target",
        "validate_aux_target_extent_and_format",
        "plan_transparent_black_clear_scope",
        "attach_material_draw_receivers_to_scope",
        "retain_explicit_target_layout",
        "feed_layer_compositor_command_blocks",
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
    use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;
    use vulkanalia::vk::Handle;

    #[test]
    fn aux_clear_scope_plan_resolves_retained_target_scope() {
        let object = SceneObjectId(1530);
        let clear_prep =
            native_vulkan_plan_scene_layer_aux_clear_prep(&schedule(object), &residency(object))
                .expect("clear prep");
        let material_draws =
            native_vulkan_plan_scene_layer_aux_material_draws(&clear_prep, &residency(object))
                .expect("material draws");

        let plan = native_vulkan_plan_scene_layer_aux_clear_scopes_with_targets(
            &clear_prep,
            &material_draws,
            |target| Ok(binding(target, vk::ImageLayout::UNDEFINED)),
        )
        .expect("clear scope plan");

        assert_eq!(plan.active_block_count, 1);
        assert_eq!(plan.target_scope_count, 1);
        assert_eq!(plan.transparent_clear_count, 1);
        assert_eq!(plan.material_draw_receiver_count, 2);
        assert!(plan.covers_material_draws(&material_draws));
        let command = &plan.commands[0];
        assert_eq!(command.target, SceneGraphTarget::LayerAuxClear(object));
        assert_eq!(command.target_format, "R8G8B8A8_UNORM");
        assert_eq!(command.initial_layout, "undefined");
        assert_eq!(command.final_layout, "color-attachment-optimal");
        assert_eq!(command.clear_color_bits, [0, 0, 0, 0]);
        assert_eq!(
            command.target_scope.load_op,
            NativeVulkanSceneRenderTargetLoadOp::Clear
        );
        assert_eq!(
            command.target_scope.begin_command_order,
            [
                "cmd_pipeline_barrier2_color_attachment",
                "cmd_begin_rendering"
            ]
        );
        assert_eq!(
            command.target_scope.end_command_order,
            ["cmd_end_rendering", "retain_color_attachment_layout"]
        );
    }

    #[test]
    fn aux_clear_scope_plan_rejects_retained_target_mismatch() {
        let object = SceneObjectId(1530);
        let clear_prep =
            native_vulkan_plan_scene_layer_aux_clear_prep(&schedule(object), &residency(object))
                .expect("clear prep");
        let material_draws =
            native_vulkan_plan_scene_layer_aux_material_draws(&clear_prep, &residency(object))
                .expect("material draws");

        let err = native_vulkan_plan_scene_layer_aux_clear_scopes_with_targets(
            &clear_prep,
            &material_draws,
            |target| {
                let mut binding = binding(target, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
                binding.width = 1920;
                Ok(binding)
            },
        )
        .expect_err("extent mismatch must fail");

        assert!(err.contains("extent mismatch"));
    }

    fn binding(
        target: SceneGraphTarget,
        current_layout: vk::ImageLayout,
    ) -> NativeVulkanSceneOffscreenTargetBinding {
        NativeVulkanSceneOffscreenTargetBinding {
            target,
            image: vk::Image::from_raw(1),
            view: vk::ImageView::from_raw(2),
            sampler: vk::Sampler::from_raw(3),
            format: vk::Format::R8G8B8A8_UNORM,
            width: 3840,
            height: 2160,
            current_layout,
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
