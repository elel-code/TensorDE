//! Target graph contract for WE `clippingmaskimage4` producer draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{SceneGraphTarget, SceneObjectId};
use crate::renderer::native_vulkan::scene_backend::render_target::NativeVulkanSceneRenderTargetLoadOp;

use super::producer_draws::NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan;
use super::{
    NativeVulkanSceneLayerAlphaMaskRuntimePlan, NativeVulkanSceneLayerAlphaMaskTargetPlan,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan
{
    pub producer_draw_count: usize,
    pub target_scope_count: usize,
    pub clear_target_scope_count: usize,
    pub load_target_scope_count: usize,
    pub load_requires_initialized_target_count: usize,
    pub clear_allows_undefined_target_count: usize,
    pub scopes: Vec<NativeVulkanSceneLayerAlphaMaskProducerTargetScopePlan>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskProducerTargetScopePlan
{
    pub target_scope_index: usize,
    pub producer_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub target: SceneGraphTarget,
    pub target_byte: u8,
    pub width: u32,
    pub height: u32,
    pub format: &'static str,
    pub required_layout: &'static str,
    pub load_op: NativeVulkanSceneRenderTargetLoadOp,
    pub clear_first: bool,
    pub allows_undefined_initial_layout: bool,
    pub requires_initialized_initial_layout: bool,
    pub target_color_attachment_write_count: usize,
    pub current_layout_source: &'static str,
    pub command_order: [&'static str; 5],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_producer_target_graph(
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    producer_draws: &NativeVulkanSceneLayerAlphaMaskProducerDrawRuntimePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan, String> {
    if runtime.tokenized_layer_count == 0 {
        return Ok(NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan::empty());
    }

    let mut scopes = Vec::with_capacity(producer_draws.draws.len());
    for draw in &producer_draws.draws {
        let target = runtime
            .targets
            .iter()
            .find(|target| target.target == draw.target)
            .ok_or_else(|| {
                format!(
                    "scene layer alpha-mask producer draw {} references missing target {:?}",
                    draw.producer_draw_index, draw.target
                )
            })?;
        validate_producer_target_scope(draw.target_byte, draw.target_scope_load_op, target)?;
        let (allows_undefined_initial_layout, requires_initialized_initial_layout) =
            producer_initial_layout_policy(draw.target_scope_load_op);
        scopes.push(NativeVulkanSceneLayerAlphaMaskProducerTargetScopePlan {
            target_scope_index: scopes.len(),
            producer_draw_index: draw.producer_draw_index,
            command_index: draw.command_index,
            object: draw.object,
            target: draw.target,
            target_byte: draw.target_byte,
            width: target.width,
            height: target.height,
            format: target.format,
            required_layout: "color-attachment-optimal",
            load_op: draw.target_scope_load_op,
            clear_first: draw.clear_first,
            allows_undefined_initial_layout,
            requires_initialized_initial_layout,
            target_color_attachment_write_count: 1,
            current_layout_source: "retained_offscreen_target_store_at_record_time",
            command_order: [
                "resolve_producer_alpha_mask_target",
                "validate_r8_unorm_half_extent_target",
                "map_target_byte_to_full_or_intermediate",
                "map_clear_first_to_clear_or_initialized_load",
                "require_color_attachment_write_scope",
            ],
        });
    }

    Ok(NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan::from_scopes(scopes))
}

impl NativeVulkanSceneLayerAlphaMaskProducerTargetGraphPlan {
    fn empty() -> Self {
        Self {
            producer_draw_count: 0,
            target_scope_count: 0,
            clear_target_scope_count: 0,
            load_target_scope_count: 0,
            load_requires_initialized_target_count: 0,
            clear_allows_undefined_target_count: 0,
            scopes: Vec::new(),
            command_order: producer_target_graph_command_order(),
        }
    }

    fn from_scopes(scopes: Vec<NativeVulkanSceneLayerAlphaMaskProducerTargetScopePlan>) -> Self {
        Self {
            producer_draw_count: scopes.len(),
            target_scope_count: scopes.len(),
            clear_target_scope_count: scopes
                .iter()
                .filter(|scope| scope.load_op == NativeVulkanSceneRenderTargetLoadOp::Clear)
                .count(),
            load_target_scope_count: scopes
                .iter()
                .filter(|scope| scope.load_op == NativeVulkanSceneRenderTargetLoadOp::Load)
                .count(),
            load_requires_initialized_target_count: scopes
                .iter()
                .filter(|scope| scope.requires_initialized_initial_layout)
                .count(),
            clear_allows_undefined_target_count: scopes
                .iter()
                .filter(|scope| scope.allows_undefined_initial_layout)
                .count(),
            scopes,
            command_order: producer_target_graph_command_order(),
        }
    }
}

fn validate_producer_target_scope(
    target_byte: u8,
    load_op: NativeVulkanSceneRenderTargetLoadOp,
    target: &NativeVulkanSceneLayerAlphaMaskTargetPlan,
) -> Result<(), String> {
    match (target_byte, target.target) {
        (0, SceneGraphTarget::FullAlphaMask) | (1, SceneGraphTarget::FullAlphaMaskIntermediate) => {
        }
        _ => {
            return Err(format!(
                "scene layer alpha-mask producer target byte {target_byte} cannot write {:?}",
                target.target
            ));
        }
    }
    if target.format != "R8_UNORM" {
        return Err(format!(
            "scene layer alpha-mask producer target {:?} must be R8_UNORM, got {}",
            target.target, target.format
        ));
    }
    if target.width == 0 || target.height == 0 || target.scale != 2 {
        return Err(format!(
            "scene layer alpha-mask producer target {:?} requires non-zero half-resolution target, got {}x{} scale {}",
            target.target, target.width, target.height, target.scale
        ));
    }
    if target_byte == 0 && load_op != NativeVulkanSceneRenderTargetLoadOp::Clear {
        return Err(
            "scene layer alpha-mask full producer target byte 0 must clear first".to_owned(),
        );
    }
    if target_byte == 1 && load_op != NativeVulkanSceneRenderTargetLoadOp::Load {
        return Err(
            "scene layer alpha-mask intermediate producer target byte 1 must load existing target"
                .to_owned(),
        );
    }
    Ok(())
}

fn producer_initial_layout_policy(load_op: NativeVulkanSceneRenderTargetLoadOp) -> (bool, bool) {
    match load_op {
        NativeVulkanSceneRenderTargetLoadOp::Clear => (true, false),
        NativeVulkanSceneRenderTargetLoadOp::Load => (false, true),
    }
}

fn producer_target_graph_command_order() -> [&'static str; 6] {
    [
        "read_clippingmaskimage4_producer_draws",
        "resolve_runtime_alpha_mask_targets",
        "validate_target_byte_matches_graph_target",
        "validate_r8_unorm_half_resolution_scope",
        "map_clear_first_to_initial_layout_policy",
        "plan_color_attachment_write_scopes",
    ]
}

#[cfg(test)]
#[path = "producer_target_graph_tests.rs"]
mod tests;
