//! Runtime resource-bind lowering for WE layer alpha-mask token commands.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneLayerCompositorOperation, SceneLayerCompositorTarget, SceneObjectId, ScenePuppetId,
};

use super::copy_back::{
    NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan,
    native_vulkan_plan_scene_layer_alpha_mask_copy_back_draws,
};
use super::copy_back_pipeline::{
    NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan,
    native_vulkan_plan_scene_layer_alpha_mask_copy_back_pipelines,
};
use super::{
    NativeVulkanSceneLayerAlphaMaskDescriptorSetRole, NativeVulkanSceneLayerAlphaMaskRuntimePlan,
};
use crate::renderer::native_vulkan::scene_backend::frame_resources::NativeVulkanSceneFrameResources;
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::{
    NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
    NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan,
};
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan
{
    pub descriptor_set_count: usize,
    pub resource_heap_bind_count: usize,
    pub clippingmaskimage4_bind_count: usize,
    pub generated_clippingtarget_bind_count: usize,
    pub flattexture_copy_back_bind_count: usize,
    pub token_command_count: usize,
    pub token_command_resource_bind_count: usize,
    pub draw_clipping_mask_command_bind_count: usize,
    pub generated_clippingtarget_command_bind_count: usize,
    pub copy_back_command_count: usize,
    pub copy_back_draw_resource_count: usize,
    pub copy_back_draw_bind_count: usize,
    pub binds: Vec<NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan>,
    pub token_commands: Vec<NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan>,
    pub copy_back_draws: Vec<NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan>,
    pub copy_back_draw_binds: Vec<NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan>,
    pub copy_back_pipelines: NativeVulkanSceneLayerAlphaMaskCopyBackPipelinePlan,
    pub command_order: [&'static str; 9],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan
{
    pub descriptor_set_index: usize,
    pub object: SceneObjectId,
    pub puppet: ScenePuppetId,
    pub shader: String,
    pub role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
    pub operation: SceneLayerCompositorOperation,
    pub bind: NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan
{
    pub command_index: usize,
    pub object: SceneObjectId,
    pub operation: SceneLayerCompositorOperation,
    pub target: SceneLayerCompositorTarget,
    pub source: Option<SceneLayerCompositorTarget>,
    pub requirement: NativeVulkanSceneLayerAlphaMaskBindRequirement,
    pub matched_bind_count: usize,
    pub matched_descriptor_set_indices: Vec<usize>,
    pub command_order: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan
{
    pub copy_back_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub shader: &'static str,
    pub texture_slot: u32,
    pub texture_source: NativeVulkanSceneTextureDescriptorSource,
    pub bind_index: usize,
    pub descriptor_set_index: usize,
    pub resource_set_index: usize,
    pub base_resource_descriptor_index: usize,
    pub base_sampler_descriptor_index: usize,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskBindRequirement {
    TokenProgramNoResourceBind,
    ClippingMaskImage4,
    FlatTextureCopyBackSeparateDrawResourceBind,
    GeneratedClippingTarget,
}

impl NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan {
    fn from_runtime_and_bind_infos(
        runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
        bind_infos: impl IntoIterator<Item = NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo>,
    ) -> Result<Self, String> {
        let mut clippingmaskimage4_bind_count = 0usize;
        let mut generated_clippingtarget_bind_count = 0usize;
        let mut flattexture_copy_back_bind_count = 0usize;
        let binds = bind_infos
            .into_iter()
            .map(|bind_info| {
                let operation = alpha_mask_operation_for_descriptor_role(bind_info.role);
                match bind_info.role {
                    NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::ClippingMaskImage4 {
                        ..
                    } => {
                        clippingmaskimage4_bind_count =
                            clippingmaskimage4_bind_count.saturating_add(1)
                    }
                    NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::GeneratedClippingTarget => {
                        generated_clippingtarget_bind_count =
                            generated_clippingtarget_bind_count.saturating_add(1)
                    }
                    NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::FlatTextureCopyBack => {
                        flattexture_copy_back_bind_count =
                            flattexture_copy_back_bind_count.saturating_add(1)
                    }
                }
                NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan {
                    descriptor_set_index: bind_info.descriptor_set_index,
                    object: bind_info.object,
                    puppet: bind_info.puppet,
                    shader: bind_info.shader.clone(),
                    role: bind_info.role,
                    operation,
                    bind: NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan::from_bind_info(
                        &bind_info,
                    ),
                }
            })
            .collect::<Vec<_>>();
        let token_commands = token_command_bind_plans(runtime, &binds)?;
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
        let copy_back_command_count = token_commands
            .iter()
            .filter(|command| {
                command.requirement
                    == NativeVulkanSceneLayerAlphaMaskBindRequirement::FlatTextureCopyBackSeparateDrawResourceBind
            })
            .count();
        let copy_back_draws = native_vulkan_plan_scene_layer_alpha_mask_copy_back_draws(runtime)?;
        let copy_back_draw_binds = copy_back_draw_bind_plans(&copy_back_draws, &binds)?;
        let copy_back_pipelines = native_vulkan_plan_scene_layer_alpha_mask_copy_back_pipelines(
            &copy_back_draws,
            &copy_back_draw_binds,
        )?;

        Ok(Self {
            descriptor_set_count: binds.len(),
            resource_heap_bind_count: binds.len(),
            clippingmaskimage4_bind_count,
            generated_clippingtarget_bind_count,
            flattexture_copy_back_bind_count,
            token_command_count: token_commands.len(),
            token_command_resource_bind_count,
            draw_clipping_mask_command_bind_count,
            generated_clippingtarget_command_bind_count,
            copy_back_command_count,
            copy_back_draw_resource_count: copy_back_draws.len(),
            copy_back_draw_bind_count: copy_back_draw_binds.len(),
            binds,
            token_commands,
            copy_back_draws,
            copy_back_draw_binds,
            copy_back_pipelines,
            command_order: [
                "read_current_alpha_mask_resource_heap_plan",
                "resolve_descriptor_set_bind_info",
                "classify_alpha_mask_descriptor_heap_bind",
                "match_resource_binds_to_token_commands",
                "require_heap_bind_for_tokenized_mask_draws",
                "lower_flattexture_copy_back_to_minimalalpha_draw_resource",
                "pair_flattexture_copy_back_draws_with_heap_binds",
                "derive_flattexture_copy_back_pipeline_mapping",
                "preserve_flattexture_copy_back_as_blend_key_0x100_draw",
            ],
        })
    }

    fn empty() -> Self {
        Self {
            descriptor_set_count: 0,
            resource_heap_bind_count: 0,
            clippingmaskimage4_bind_count: 0,
            generated_clippingtarget_bind_count: 0,
            flattexture_copy_back_bind_count: 0,
            token_command_count: 0,
            token_command_resource_bind_count: 0,
            draw_clipping_mask_command_bind_count: 0,
            generated_clippingtarget_command_bind_count: 0,
            copy_back_command_count: 0,
            copy_back_draw_resource_count: 0,
            copy_back_draw_bind_count: 0,
            binds: Vec::new(),
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
                    "preserve_target_like_flattexture_draw_shape",
                ],
                cache_keys: Vec::new(),
            },
            command_order: [
                "read_current_alpha_mask_resource_heap_plan",
                "resolve_descriptor_set_bind_info",
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
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_resource_binds(
    frame_resources: &NativeVulkanSceneFrameResources,
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan, String> {
    if runtime.tokenized_layer_count == 0 {
        return Ok(NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan::empty());
    }
    let descriptor_set_indices = frame_resources
        .current_layer_alpha_mask_resource_heap_frame_plan()
        .ok_or_else(|| {
            "scene layer alpha-mask runtime requires cold-prepared alpha-mask resource heap"
                .to_owned()
        })?
        .descriptor_set_bindings
        .iter()
        .map(|binding| binding.descriptor_set_index)
        .collect::<Vec<_>>();
    if descriptor_set_indices.is_empty() {
        return Err(
            "scene layer alpha-mask runtime has tokenized layers but no alpha-mask descriptor sets"
                .to_owned(),
        );
    }
    let mut bind_infos = Vec::with_capacity(descriptor_set_indices.len());
    for descriptor_set_index in descriptor_set_indices {
        bind_infos
            .push(frame_resources.layer_alpha_mask_resource_heap_bind_info(descriptor_set_index)?);
    }
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan::from_runtime_and_bind_infos(
        runtime, bind_infos,
    )
}

fn token_command_bind_plans(
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
    binds: &[NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan],
) -> Result<Vec<NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan>, String> {
    runtime
        .commands
        .iter()
        .enumerate()
        .map(|(command_index, command)| {
            let requirement = bind_requirement_for_operation(command.operation)?;
            let matched_descriptor_set_indices =
                matched_descriptor_set_indices(command.object, requirement, binds);
            validate_token_command_bind_matches(
                command_index,
                command.object,
                command.operation,
                requirement,
                &matched_descriptor_set_indices,
            )?;
            let mut command_order = vec!["read_layer_alpha_mask_token_command"];
            match requirement {
                NativeVulkanSceneLayerAlphaMaskBindRequirement::TokenProgramNoResourceBind => {
                    command_order.push("dispatch_token_program_without_descriptor_heap_bind");
                }
                NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4 => {
                    command_order.push("bind_clippingmaskimage4_resource_heap_before_mask_draw");
                }
                NativeVulkanSceneLayerAlphaMaskBindRequirement::FlatTextureCopyBackSeparateDrawResourceBind => {
                    command_order.push("bind_flattexture_copy_back_resource_heap_before_minimalalpha_draw");
                }
                NativeVulkanSceneLayerAlphaMaskBindRequirement::GeneratedClippingTarget => {
                    command_order.push("bind_generated_clippingtarget_resource_heap_before_draw");
                }
            }
            Ok(NativeVulkanSceneLayerAlphaMaskTokenCommandResourceBindPlan {
                command_index,
                object: command.object,
                operation: command.operation,
                target: command.target,
                source: command.source,
                requirement,
                matched_bind_count: matched_descriptor_set_indices.len(),
                matched_descriptor_set_indices,
                command_order,
            })
        })
        .collect()
}

fn bind_requirement_for_operation(
    operation: SceneLayerCompositorOperation,
) -> Result<NativeVulkanSceneLayerAlphaMaskBindRequirement, String> {
    match operation {
        SceneLayerCompositorOperation::TokenProgramDispatch => {
            Ok(NativeVulkanSceneLayerAlphaMaskBindRequirement::TokenProgramNoResourceBind)
        }
        SceneLayerCompositorOperation::DrawClippingMask => {
            Ok(NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4)
        }
        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask => {
            Ok(NativeVulkanSceneLayerAlphaMaskBindRequirement::FlatTextureCopyBackSeparateDrawResourceBind)
        }
        SceneLayerCompositorOperation::DrawGeneratedClippingTarget => {
            Ok(NativeVulkanSceneLayerAlphaMaskBindRequirement::GeneratedClippingTarget)
        }
        _ => Err(format!(
            "scene layer alpha-mask resource bind lowering received unsupported operation {operation:?}"
        )),
    }
}

fn matched_descriptor_set_indices(
    object: SceneObjectId,
    requirement: NativeVulkanSceneLayerAlphaMaskBindRequirement,
    binds: &[NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan],
) -> Vec<usize> {
    binds
        .iter()
        .filter(|bind| bind.object == object)
        .filter(|bind| match requirement {
            NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4 => matches!(
                bind.role,
                NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::ClippingMaskImage4 { .. }
            ),
            NativeVulkanSceneLayerAlphaMaskBindRequirement::GeneratedClippingTarget => {
                bind.role
                    == NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::GeneratedClippingTarget
            }
            NativeVulkanSceneLayerAlphaMaskBindRequirement::FlatTextureCopyBackSeparateDrawResourceBind => {
                bind.role == NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::FlatTextureCopyBack
            }
            NativeVulkanSceneLayerAlphaMaskBindRequirement::TokenProgramNoResourceBind => false,
        })
        .map(|bind| bind.descriptor_set_index)
        .collect()
}

fn validate_token_command_bind_matches(
    command_index: usize,
    object: SceneObjectId,
    operation: SceneLayerCompositorOperation,
    requirement: NativeVulkanSceneLayerAlphaMaskBindRequirement,
    matched_descriptor_set_indices: &[usize],
) -> Result<(), String> {
    match requirement {
        NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4 => {
            if matched_descriptor_set_indices.is_empty() {
                return Err(format!(
                    "scene layer alpha-mask command {command_index} object {object:?} {:?} requires at least one clippingmaskimage4 descriptor heap bind",
                    operation
                ));
            }
        }
        NativeVulkanSceneLayerAlphaMaskBindRequirement::GeneratedClippingTarget => {
            if matched_descriptor_set_indices.len() != 1 {
                return Err(format!(
                    "scene layer alpha-mask command {command_index} object {object:?} {:?} requires exactly one generated CLIPPINGTARGET descriptor heap bind, got {}",
                    operation,
                    matched_descriptor_set_indices.len()
                ));
            }
        }
        NativeVulkanSceneLayerAlphaMaskBindRequirement::FlatTextureCopyBackSeparateDrawResourceBind => {
            if matched_descriptor_set_indices.len() != 1 {
                return Err(format!(
                    "scene layer alpha-mask command {command_index} object {object:?} {:?} requires exactly one flattexture copy-back descriptor heap bind, got {}",
                    operation,
                    matched_descriptor_set_indices.len()
                ));
            }
        }
        NativeVulkanSceneLayerAlphaMaskBindRequirement::TokenProgramNoResourceBind => {}
    }
    Ok(())
}

fn copy_back_draw_bind_plans(
    copy_back_draws: &[NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan],
    binds: &[NativeVulkanSceneLayerAlphaMaskResourceBindCommandPlan],
) -> Result<Vec<NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan>, String> {
    copy_back_draws
        .iter()
        .enumerate()
        .map(|(copy_back_draw_index, draw)| {
            let matching_binds = binds
                .iter()
                .enumerate()
                .filter(|(_, bind)| {
                    bind.object == draw.object
                        && bind.role
                            == NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::FlatTextureCopyBack
                })
                .collect::<Vec<_>>();
            if matching_binds.len() != 1 {
                return Err(format!(
                    "scene layer alpha-mask copy-back draw command {} object {:?} requires exactly one FlatTextureCopyBack heap bind, got {}",
                    draw.command_index,
                    draw.object,
                    matching_binds.len()
                ));
            }
            let (bind_index, bind) = matching_binds[0];
            if bind.shader != draw.shader {
                return Err(format!(
                    "scene layer alpha-mask copy-back draw command {} object {:?} shader mismatch: draw {}, heap {}",
                    draw.command_index, draw.object, draw.shader, bind.shader
                ));
            }
            Ok(NativeVulkanSceneLayerAlphaMaskCopyBackDrawResourceBindPlan {
                copy_back_draw_index,
                command_index: draw.command_index,
                object: draw.object,
                shader: draw.shader,
                texture_slot: draw.texture_slot,
                texture_source: draw.texture_source,
                bind_index,
                descriptor_set_index: bind.descriptor_set_index,
                resource_set_index: bind.bind.resource_set_index,
                base_resource_descriptor_index: bind.bind.base_resource_descriptor_index,
                base_sampler_descriptor_index: bind.bind.base_sampler_descriptor_index,
                command_order: [
                    "read_flattexture_copy_back_draw_resource",
                    "select_flattexture_copy_back_heap_bind",
                    "bind_flattexture_copy_back_resource_heap",
                    "draw_minimalalpha_copy_back",
                ],
            })
        })
        .collect()
}

fn alpha_mask_operation_for_descriptor_role(
    role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
) -> SceneLayerCompositorOperation {
    match role {
        NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::ClippingMaskImage4 { .. } => {
            SceneLayerCompositorOperation::DrawClippingMask
        }
        NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::GeneratedClippingTarget => {
            SceneLayerCompositorOperation::DrawGeneratedClippingTarget
        }
        NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::FlatTextureCopyBack => {
            SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneGraphTarget, SceneLayerCompositorBlendKey, SceneLayerCompositorCommand,
        SceneLayerCompositorCondition, SceneLayerCompositorEntry, SceneLayerCompositorLayer,
        SceneLayerCompositorPlan, SceneLayerCompositorRoute, SceneResourceId,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::{
        NativeVulkanSceneLayerAlphaMaskDescriptorSource,
        NativeVulkanSceneLayerAlphaMaskTargetBinding,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::{
        NativeVulkanSceneLayerAlphaMaskResourceSetBinding,
        NativeVulkanSceneLayerAlphaMaskResourceSetKey,
    };
    use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;
    use vulkanalia::vk;
    use vulkanalia::vk::HasBuilder;

    #[test]
    fn alpha_mask_resource_bind_runtime_plan_matches_token_commands() {
        let runtime = runtime_plan();
        let bind_infos = vec![
            alpha_mask_bind_info(
                0,
                NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::ClippingMaskImage4 {
                    clipping_record_index: 0,
                },
                "we/clippingmaskimage4",
                vec![(0, SceneResourceId(9)), (1, SceneResourceId(12))],
            ),
            alpha_mask_bind_info(
                1,
                NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::GeneratedClippingTarget,
                "we/genericimage4",
                vec![(0, SceneResourceId(9))],
            ),
            alpha_mask_bind_info_from_sources(
                2,
                NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::FlatTextureCopyBack,
                "util/minimalalpha",
                vec![(
                    0,
                    NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                        SceneGraphTarget::FullAlphaMaskIntermediate,
                    ),
                )],
            ),
        ];

        let plan =
            NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan::from_runtime_and_bind_infos(
                &runtime, bind_infos,
            )
            .expect("resource bind runtime plan");

        assert_eq!(plan.descriptor_set_count, 3);
        assert_eq!(plan.resource_heap_bind_count, 3);
        assert_eq!(plan.clippingmaskimage4_bind_count, 1);
        assert_eq!(plan.generated_clippingtarget_bind_count, 1);
        assert_eq!(plan.flattexture_copy_back_bind_count, 1);
        assert_eq!(plan.token_command_count, 5);
        assert_eq!(plan.token_command_resource_bind_count, 4);
        assert_eq!(plan.draw_clipping_mask_command_bind_count, 2);
        assert_eq!(plan.generated_clippingtarget_command_bind_count, 1);
        assert_eq!(plan.copy_back_command_count, 1);
        assert_eq!(plan.copy_back_draw_resource_count, 1);
        assert_eq!(plan.copy_back_draw_bind_count, 1);
        assert_eq!(
            plan.binds[0].operation,
            SceneLayerCompositorOperation::DrawClippingMask
        );
        assert_eq!(
            plan.binds[1].operation,
            SceneLayerCompositorOperation::DrawGeneratedClippingTarget
        );
        assert_eq!(
            plan.binds[2].operation,
            SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
        );
        assert_eq!(plan.binds[0].bind.texture_count, 2);
        assert_eq!(
            plan.token_commands[1].requirement,
            NativeVulkanSceneLayerAlphaMaskBindRequirement::ClippingMaskImage4
        );
        assert_eq!(
            plan.token_commands[1].matched_descriptor_set_indices,
            vec![0]
        );
        assert_eq!(
            plan.token_commands[3].requirement,
            NativeVulkanSceneLayerAlphaMaskBindRequirement::FlatTextureCopyBackSeparateDrawResourceBind
        );
        assert_eq!(
            plan.token_commands[3].matched_descriptor_set_indices,
            vec![2]
        );
        assert_eq!(
            plan.token_commands[4].requirement,
            NativeVulkanSceneLayerAlphaMaskBindRequirement::GeneratedClippingTarget
        );
        assert_eq!(
            plan.token_commands[4].matched_descriptor_set_indices,
            vec![1]
        );
        assert_eq!(plan.copy_back_draws[0].shader, "util/minimalalpha");
        assert_eq!(
            plan.copy_back_draws[0].texture_source,
            NativeVulkanSceneTextureDescriptorSource::GraphTarget(
                SceneGraphTarget::FullAlphaMaskIntermediate
            )
        );
        assert_eq!(plan.copy_back_draw_binds[0].copy_back_draw_index, 0);
        assert_eq!(plan.copy_back_draw_binds[0].command_index, 3);
        assert_eq!(plan.copy_back_draw_binds[0].bind_index, 2);
        assert_eq!(plan.copy_back_draw_binds[0].descriptor_set_index, 2);
        assert_eq!(plan.copy_back_draw_binds[0].resource_set_index, 2);
        assert_eq!(
            plan.copy_back_draw_binds[0].base_resource_descriptor_index,
            4
        );
        assert_eq!(
            plan.copy_back_draw_binds[0].base_sampler_descriptor_index,
            4
        );
        assert_eq!(plan.copy_back_draw_binds[0].shader, "util/minimalalpha");
        assert_eq!(
            plan.copy_back_draw_binds[0].texture_source,
            NativeVulkanSceneTextureDescriptorSource::GraphTarget(
                SceneGraphTarget::FullAlphaMaskIntermediate
            )
        );
        assert_eq!(
            plan.copy_back_draw_binds[0].command_order,
            [
                "read_flattexture_copy_back_draw_resource",
                "select_flattexture_copy_back_heap_bind",
                "bind_flattexture_copy_back_resource_heap",
                "draw_minimalalpha_copy_back"
            ]
        );
        assert_eq!(plan.copy_back_pipelines.pipeline_count, 1);
        assert_eq!(plan.copy_back_pipelines.texture_slot_mask, 1);
        assert_eq!(plan.copy_back_pipelines.keys[0].shader, "util/minimalalpha");
        assert_eq!(
            plan.copy_back_pipelines.keys[0].shader_mapping,
            "VK_EXT_descriptor_heap set0.binding0.g_Texture0 -> alpha-mask-copy-back-resource-set2-resource4-sampler4"
        );
        assert_eq!(
            plan.command_order,
            [
                "read_current_alpha_mask_resource_heap_plan",
                "resolve_descriptor_set_bind_info",
                "classify_alpha_mask_descriptor_heap_bind",
                "match_resource_binds_to_token_commands",
                "require_heap_bind_for_tokenized_mask_draws",
                "lower_flattexture_copy_back_to_minimalalpha_draw_resource",
                "pair_flattexture_copy_back_draws_with_heap_binds",
                "derive_flattexture_copy_back_pipeline_mapping",
                "preserve_flattexture_copy_back_as_blend_key_0x100_draw"
            ]
        );
    }

    #[test]
    fn alpha_mask_resource_bind_runtime_plan_rejects_missing_generated_target_bind() {
        let runtime = runtime_plan();
        let bind_infos = vec![
            alpha_mask_bind_info(
                0,
                NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::ClippingMaskImage4 {
                    clipping_record_index: 0,
                },
                "we/clippingmaskimage4",
                vec![(0, SceneResourceId(9)), (1, SceneResourceId(12))],
            ),
            alpha_mask_bind_info_from_sources(
                1,
                NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::FlatTextureCopyBack,
                "util/minimalalpha",
                vec![(
                    0,
                    NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                        SceneGraphTarget::FullAlphaMaskIntermediate,
                    ),
                )],
            ),
        ];

        let err =
            NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan::from_runtime_and_bind_infos(
                &runtime, bind_infos,
            )
            .expect_err("missing generated target bind must fail");

        assert!(err.contains("requires exactly one generated CLIPPINGTARGET descriptor heap bind"));
    }

    #[test]
    fn alpha_mask_resource_bind_runtime_plan_rejects_missing_copy_back_bind() {
        let runtime = runtime_plan();
        let bind_infos = vec![
            alpha_mask_bind_info(
                0,
                NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::ClippingMaskImage4 {
                    clipping_record_index: 0,
                },
                "we/clippingmaskimage4",
                vec![(0, SceneResourceId(9)), (1, SceneResourceId(12))],
            ),
            alpha_mask_bind_info(
                1,
                NativeVulkanSceneLayerAlphaMaskDescriptorSetRole::GeneratedClippingTarget,
                "we/genericimage4",
                vec![(0, SceneResourceId(9))],
            ),
        ];

        let err =
            NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan::from_runtime_and_bind_infos(
                &runtime, bind_infos,
            )
            .expect_err("missing copy-back bind must fail");

        assert!(err.contains("requires exactly one flattexture copy-back descriptor heap bind"));
    }

    fn runtime_plan() -> NativeVulkanSceneLayerAlphaMaskRuntimePlan {
        NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
            &layer_compositor(),
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            |target| {
                Ok(NativeVulkanSceneLayerAlphaMaskTargetBinding {
                    target,
                    format: vk::Format::R8_UNORM,
                    width: 1920,
                    height: 1080,
                })
            },
        )
        .expect("runtime plan")
    }

    fn layer_compositor() -> SceneLayerCompositorPlan {
        SceneLayerCompositorPlan {
            layer_count: 1,
            command_count: 5,
            object_final_layer_count: 0,
            tokenized_layer_count: 1,
            layers: vec![SceneLayerCompositorLayer {
                object: SceneObjectId(77),
                route: SceneLayerCompositorRoute::DirectSwapchain,
                uses_tokenized_subdraw: true,
                commands: vec![
                    command(
                        SceneLayerCompositorEntry::TokenizedCompositeEntry52,
                        SceneLayerCompositorOperation::TokenProgramDispatch,
                        SceneLayerCompositorCondition::Always,
                        None,
                        SceneLayerCompositorTarget::LayerTarget490,
                        SceneLayerCompositorBlendKey::Inherit,
                    ),
                    command(
                        SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
                        SceneLayerCompositorOperation::DrawClippingMask,
                        SceneLayerCompositorCondition::Token1OrToken2FirstPair,
                        None,
                        SceneLayerCompositorTarget::FullAlphaMask,
                        SceneLayerCompositorBlendKey::Inherit,
                    ),
                    command(
                        SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
                        SceneLayerCompositorOperation::DrawClippingMask,
                        SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask,
                        None,
                        SceneLayerCompositorTarget::FullAlphaMaskIntermediate,
                        SceneLayerCompositorBlendKey::Inherit,
                    ),
                    command(
                        SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed,
                        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                        SceneLayerCompositorCondition::Token2AfterIntermediateMask,
                        Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
                        SceneLayerCompositorTarget::FullAlphaMask,
                        SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
                    ),
                    command(
                        SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53,
                        SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
                        SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
                        Some(SceneLayerCompositorTarget::FullAlphaMask),
                        SceneLayerCompositorTarget::LayerTarget490,
                        SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0,
                    ),
                ],
            }],
            command_order: [
                "preserve_scene_object_order",
                "classify_object_final_routes",
                "model_vtable_32_normal_render_entry",
                "model_vtable_50_clear_prep_entry",
                "model_vtable_52_53_tokenized_subdraw_entries",
                "model_wrapper_0xd8_0x128_state_keys",
                "model_flattexture_intermediate_copy_back_bit_0x100",
                "model_vtable_51_full_layer_composite_entry",
                "lower_layer_routes_to_scene_graph_passes",
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
    ) -> SceneLayerCompositorCommand {
        SceneLayerCompositorCommand {
            entry,
            operation,
            condition,
            source,
            target,
            blend_key,
        }
    }

    fn alpha_mask_bind_info(
        descriptor_set_index: usize,
        role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
        shader: &'static str,
        textures: Vec<(u32, SceneResourceId)>,
    ) -> NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
        let resource_set = NativeVulkanSceneLayerAlphaMaskResourceSetKey {
            shader: shader.to_owned(),
            bindings: textures
                .iter()
                .map(
                    |(slot, resource)| NativeVulkanSceneLayerAlphaMaskResourceSetBinding {
                        slot: *slot,
                        source: NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                            *resource,
                        ),
                    },
                )
                .collect(),
        };
        alpha_mask_bind_info_for_resource_set(descriptor_set_index, role, shader, resource_set)
    }

    fn alpha_mask_bind_info_from_sources(
        descriptor_set_index: usize,
        role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
        shader: &'static str,
        textures: Vec<(u32, NativeVulkanSceneLayerAlphaMaskDescriptorSource)>,
    ) -> NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
        let resource_set = NativeVulkanSceneLayerAlphaMaskResourceSetKey {
            shader: shader.to_owned(),
            bindings: textures
                .iter()
                .map(
                    |(slot, source)| NativeVulkanSceneLayerAlphaMaskResourceSetBinding {
                        slot: *slot,
                        source: *source,
                    },
                )
                .collect(),
        };
        alpha_mask_bind_info_for_resource_set(descriptor_set_index, role, shader, resource_set)
    }

    fn alpha_mask_bind_info_for_resource_set(
        descriptor_set_index: usize,
        role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
        shader: &'static str,
        resource_set: NativeVulkanSceneLayerAlphaMaskResourceSetKey,
    ) -> NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
        let texture_count = resource_set.bindings.len();
        let shader_mappings = resource_set
            .bindings
            .iter()
            .enumerate()
            .map(|(ordinal, binding)| {
                let slot = binding.slot;
                format!(
                    "set0.binding{slot}.g_Texture{slot} -> alpha-mask-resource-set-offset{ordinal}"
                )
            })
            .collect();
        NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo {
            descriptor_set_index,
            object: SceneObjectId(77),
            puppet: ScenePuppetId(5),
            shader: shader.to_owned(),
            role,
            resource_set_index: descriptor_set_index,
            resource_set,
            base_resource_descriptor_index: descriptor_set_index.saturating_mul(2),
            base_sampler_descriptor_index: descriptor_set_index.saturating_mul(2),
            resource_descriptor_count: texture_count,
            texture_count,
            shader_mappings,
            resource_bind: vk::BindHeapInfoEXT::builder().build(),
            sampler_bind: vk::BindHeapInfoEXT::builder().build(),
        }
    }
}
