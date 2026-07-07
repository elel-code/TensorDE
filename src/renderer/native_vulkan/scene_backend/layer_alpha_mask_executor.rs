//! WE layer alpha-mask token executor planning for Vulkan.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use serde::Serialize;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneGraphTarget, SceneLayerCompositorBlendKey, SceneLayerCompositorCommand,
    SceneLayerCompositorCondition, SceneLayerCompositorEntry, SceneLayerCompositorOperation,
    SceneLayerCompositorPlan, SceneLayerCompositorTarget, SceneObjectId,
};

use super::frame_resources::NativeVulkanSceneFrameResources;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRuntimePlan {
    pub tokenized_layer_count: usize,
    pub command_count: usize,
    pub required_target_count: usize,
    pub target_scope_count: usize,
    pub alpha_mask_attachment_write_count: usize,
    pub alpha_mask_shader_sample_count: usize,
    pub token_program_dispatch_count: usize,
    pub draw_clipping_mask_count: usize,
    pub draw_style_copy_back_count: usize,
    pub generated_clipping_target_draw_count: usize,
    pub transfer_copy_count: usize,
    pub targets: Vec<NativeVulkanSceneLayerAlphaMaskTargetPlan>,
    pub commands: Vec<NativeVulkanSceneLayerAlphaMaskCommandPlan>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTargetBinding {
    pub target: SceneGraphTarget,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTargetPlan {
    pub target: SceneGraphTarget,
    pub format: &'static str,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCommandPlan {
    pub object: SceneObjectId,
    pub entry: SceneLayerCompositorEntry,
    pub operation: SceneLayerCompositorOperation,
    pub condition: SceneLayerCompositorCondition,
    pub source: Option<SceneLayerCompositorTarget>,
    pub target: SceneLayerCompositorTarget,
    pub source_graph_target: Option<SceneGraphTarget>,
    pub target_graph_target: Option<SceneGraphTarget>,
    pub access: NativeVulkanSceneLayerAlphaMaskAccess,
    pub copy_method: NativeVulkanSceneLayerAlphaMaskCopyMethod,
    pub blend_key: SceneLayerCompositorBlendKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskAccess {
    TokenProgram,
    AlphaMaskAttachmentWrite,
    AlphaMaskSampleAndAttachmentWrite,
    FullMaskSampleForGeneratedTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskCopyMethod {
    None,
    FlatTextureDrawDestColorBlendKey0x100,
}

impl NativeVulkanSceneLayerAlphaMaskRuntimePlan {
    pub(in crate::renderer::native_vulkan) fn from_layer_compositor<TargetBindingForGraph>(
        layer_compositor: &SceneLayerCompositorPlan,
        swapchain_extent: vk::Extent2D,
        mut target_binding: TargetBindingForGraph,
    ) -> Result<Self, String>
    where
        TargetBindingForGraph:
            FnMut(SceneGraphTarget) -> Result<NativeVulkanSceneLayerAlphaMaskTargetBinding, String>,
    {
        if layer_compositor.tokenized_layer_count == 0 {
            return Ok(Self::empty());
        }
        if swapchain_extent.width == 0 || swapchain_extent.height == 0 {
            return Err(
                "scene layer alpha-mask executor requires non-zero swapchain extent".into(),
            );
        }

        let full = validate_alpha_mask_target_binding(
            target_binding(SceneGraphTarget::FullAlphaMask)?,
            SceneGraphTarget::FullAlphaMask,
            swapchain_extent,
        )?;
        let intermediate = validate_alpha_mask_target_binding(
            target_binding(SceneGraphTarget::FullAlphaMaskIntermediate)?,
            SceneGraphTarget::FullAlphaMaskIntermediate,
            swapchain_extent,
        )?;
        if full.width != intermediate.width || full.height != intermediate.height {
            return Err(format!(
                "scene layer alpha-mask targets require matching extents: full {}x{}, intermediate {}x{}",
                full.width, full.height, intermediate.width, intermediate.height
            ));
        }

        let mut commands = Vec::new();
        for layer in layer_compositor
            .layers
            .iter()
            .filter(|layer| layer.uses_tokenized_subdraw)
        {
            for command in layer
                .commands
                .iter()
                .filter(|command| layer_alpha_mask_executor_command(command))
            {
                commands.push(layer_alpha_mask_command_plan(layer.object, *command)?);
            }
        }

        let token_program_dispatch_count = commands
            .iter()
            .filter(|command| {
                command.operation == SceneLayerCompositorOperation::TokenProgramDispatch
            })
            .count();
        let draw_clipping_mask_count = commands
            .iter()
            .filter(|command| command.operation == SceneLayerCompositorOperation::DrawClippingMask)
            .count();
        let draw_style_copy_back_count = commands
            .iter()
            .filter(|command| {
                command.copy_method
                    == NativeVulkanSceneLayerAlphaMaskCopyMethod::FlatTextureDrawDestColorBlendKey0x100
            })
            .count();
        let generated_clipping_target_draw_count = commands
            .iter()
            .filter(|command| {
                command.operation == SceneLayerCompositorOperation::DrawGeneratedClippingTarget
            })
            .count();
        let alpha_mask_shader_sample_count = commands
            .iter()
            .filter(|command| command.source_graph_target.is_some())
            .count();
        let alpha_mask_attachment_write_count = commands
            .iter()
            .filter(|command| {
                matches!(
                    command.access,
                    NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskAttachmentWrite
                        | NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskSampleAndAttachmentWrite
                )
            })
            .count();

        Ok(Self {
            tokenized_layer_count: layer_compositor.tokenized_layer_count,
            command_count: commands.len(),
            required_target_count: 2,
            target_scope_count: alpha_mask_attachment_write_count,
            alpha_mask_attachment_write_count,
            alpha_mask_shader_sample_count,
            token_program_dispatch_count,
            draw_clipping_mask_count,
            draw_style_copy_back_count,
            generated_clipping_target_draw_count,
            transfer_copy_count: 0,
            targets: vec![full, intermediate],
            commands,
            command_order: [
                "read_we_vtable_52_53_token_program",
                "validate_full_alpha_mask_targets_r8_half_extent",
                "lower_clippingmaskimage4_to_alpha_mask_attachment_writes",
                "lower_flattexture_copy_back_to_draw_blend_key_0x100",
                "preserve_generated_clippingtarget_full_mask_sample",
                "track_alpha_mask_usage_like_godot_rendering_device_graph",
            ],
        })
    }

    fn empty() -> Self {
        Self {
            tokenized_layer_count: 0,
            command_count: 0,
            required_target_count: 0,
            target_scope_count: 0,
            alpha_mask_attachment_write_count: 0,
            alpha_mask_shader_sample_count: 0,
            token_program_dispatch_count: 0,
            draw_clipping_mask_count: 0,
            draw_style_copy_back_count: 0,
            generated_clipping_target_draw_count: 0,
            transfer_copy_count: 0,
            targets: Vec::new(),
            commands: Vec::new(),
            command_order: [
                "read_we_vtable_52_53_token_program",
                "validate_full_alpha_mask_targets_r8_half_extent",
                "lower_clippingmaskimage4_to_alpha_mask_attachment_writes",
                "lower_flattexture_copy_back_to_draw_blend_key_0x100",
                "preserve_generated_clippingtarget_full_mask_sample",
                "track_alpha_mask_usage_like_godot_rendering_device_graph",
            ],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_runtime_frame(
    frame_resources: &NativeVulkanSceneFrameResources,
    layer_compositor: &SceneLayerCompositorPlan,
    swapchain_extent: vk::Extent2D,
) -> Result<NativeVulkanSceneLayerAlphaMaskRuntimePlan, String> {
    NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
        layer_compositor,
        swapchain_extent,
        |target| {
            let binding = frame_resources.offscreen_target_binding(target)?;
            Ok(NativeVulkanSceneLayerAlphaMaskTargetBinding {
                target: binding.target,
                format: binding.format,
                width: binding.width,
                height: binding.height,
            })
        },
    )
}

fn validate_alpha_mask_target_binding(
    binding: NativeVulkanSceneLayerAlphaMaskTargetBinding,
    target: SceneGraphTarget,
    swapchain_extent: vk::Extent2D,
) -> Result<NativeVulkanSceneLayerAlphaMaskTargetPlan, String> {
    if binding.target != target {
        return Err(format!(
            "scene layer alpha-mask executor requested {target:?} but resolver returned {:?}",
            binding.target
        ));
    }
    if binding.format != vk::Format::R8_UNORM {
        return Err(format!(
            "scene layer alpha-mask target {target:?} must be R8_UNORM, got {:?}",
            binding.format
        ));
    }
    let expected_width = swapchain_extent.width.saturating_add(1) / 2;
    let expected_height = swapchain_extent.height.saturating_add(1) / 2;
    if binding.width != expected_width || binding.height != expected_height {
        return Err(format!(
            "scene layer alpha-mask target {target:?} must be half-resolution {}x{}, got {}x{}",
            expected_width, expected_height, binding.width, binding.height
        ));
    }
    Ok(NativeVulkanSceneLayerAlphaMaskTargetPlan {
        target,
        format: "R8_UNORM",
        width: binding.width,
        height: binding.height,
        scale: 2,
    })
}

fn layer_alpha_mask_executor_command(command: &&SceneLayerCompositorCommand) -> bool {
    matches!(
        command.operation,
        SceneLayerCompositorOperation::TokenProgramDispatch
            | SceneLayerCompositorOperation::DrawClippingMask
            | SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
            | SceneLayerCompositorOperation::DrawGeneratedClippingTarget
    )
}

fn layer_alpha_mask_command_plan(
    object: SceneObjectId,
    command: SceneLayerCompositorCommand,
) -> Result<NativeVulkanSceneLayerAlphaMaskCommandPlan, String> {
    let source_graph_target = command.source.and_then(layer_alpha_mask_graph_target);
    let target_graph_target = layer_alpha_mask_graph_target(command.target);
    let (access, copy_method) = match command.operation {
        SceneLayerCompositorOperation::TokenProgramDispatch => (
            NativeVulkanSceneLayerAlphaMaskAccess::TokenProgram,
            NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
        ),
        SceneLayerCompositorOperation::DrawClippingMask => (
            NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskAttachmentWrite,
            NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
        ),
        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask => {
            if command.source != Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate)
                || command.target != SceneLayerCompositorTarget::FullAlphaMask
                || command.blend_key != SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100
            {
                return Err(format!(
                    "scene layer alpha-mask copy-back must be intermediate -> full with blend-key 0x100, got {:?}",
                    command
                ));
            }
            (
                NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskSampleAndAttachmentWrite,
                NativeVulkanSceneLayerAlphaMaskCopyMethod::FlatTextureDrawDestColorBlendKey0x100,
            )
        }
        SceneLayerCompositorOperation::DrawGeneratedClippingTarget => {
            if command.source != Some(SceneLayerCompositorTarget::FullAlphaMask) {
                return Err(format!(
                    "scene layer generated CLIPPINGTARGET draw must sample full alpha mask, got {:?}",
                    command.source
                ));
            }
            (
                NativeVulkanSceneLayerAlphaMaskAccess::FullMaskSampleForGeneratedTarget,
                NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
            )
        }
        _ => {
            return Err(format!(
                "scene layer alpha-mask executor received unsupported operation {:?}",
                command.operation
            ));
        }
    };

    Ok(NativeVulkanSceneLayerAlphaMaskCommandPlan {
        object,
        entry: command.entry,
        operation: command.operation,
        condition: command.condition,
        source: command.source,
        target: command.target,
        source_graph_target,
        target_graph_target,
        access,
        copy_method,
        blend_key: command.blend_key,
    })
}

fn layer_alpha_mask_graph_target(target: SceneLayerCompositorTarget) -> Option<SceneGraphTarget> {
    match target {
        SceneLayerCompositorTarget::FullAlphaMask => Some(SceneGraphTarget::FullAlphaMask),
        SceneLayerCompositorTarget::FullAlphaMaskIntermediate => {
            Some(SceneGraphTarget::FullAlphaMaskIntermediate)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{SceneLayerCompositorLayer, SceneLayerCompositorRoute};

    #[test]
    fn alpha_mask_executor_keeps_empty_layer_plan_targetless() {
        let plan = NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
            &SceneLayerCompositorPlan::empty(),
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            |_| panic!("empty alpha-mask plan must not resolve targets"),
        )
        .expect("empty alpha-mask executor plan");

        assert_eq!(plan.tokenized_layer_count, 0);
        assert_eq!(plan.command_count, 0);
        assert_eq!(plan.required_target_count, 0);
        assert_eq!(plan.transfer_copy_count, 0);
    }

    #[test]
    fn alpha_mask_executor_lowers_token_commands_to_draw_style_copy_back() {
        let layer_compositor = tokenized_layer_compositor();

        let plan = NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
            &layer_compositor,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            alpha_mask_binding,
        )
        .expect("alpha-mask executor plan");

        assert_eq!(plan.tokenized_layer_count, 1);
        assert_eq!(plan.required_target_count, 2);
        assert_eq!(plan.command_count, 5);
        assert_eq!(plan.token_program_dispatch_count, 1);
        assert_eq!(plan.draw_clipping_mask_count, 2);
        assert_eq!(plan.draw_style_copy_back_count, 1);
        assert_eq!(plan.generated_clipping_target_draw_count, 1);
        assert_eq!(plan.transfer_copy_count, 0);
        assert_eq!(plan.targets[0].width, 1920);
        assert_eq!(plan.targets[0].height, 1080);

        let copy_back = plan
            .commands
            .iter()
            .find(|command| {
                command.operation == SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
            })
            .expect("copy-back command");
        assert_eq!(
            copy_back.copy_method,
            NativeVulkanSceneLayerAlphaMaskCopyMethod::FlatTextureDrawDestColorBlendKey0x100
        );
        assert_eq!(
            copy_back.source_graph_target,
            Some(SceneGraphTarget::FullAlphaMaskIntermediate)
        );
        assert_eq!(
            copy_back.target_graph_target,
            Some(SceneGraphTarget::FullAlphaMask)
        );
        assert_eq!(
            copy_back.blend_key,
            SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100
        );
    }

    #[test]
    fn alpha_mask_executor_rejects_non_r8_targets() {
        let err = NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
            &tokenized_layer_compositor(),
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            |target| {
                let mut binding = alpha_mask_binding(target)?;
                if target == SceneGraphTarget::FullAlphaMask {
                    binding.format = vk::Format::R16G16B16A16_SFLOAT;
                }
                Ok(binding)
            },
        )
        .expect_err("alpha-mask target format must be R8");

        assert!(err.contains("must be R8_UNORM"));
    }

    #[test]
    fn alpha_mask_executor_rejects_transfer_copy_shaped_copy_back() {
        let mut layer_compositor = tokenized_layer_compositor();
        let copy_back = layer_compositor.layers[0]
            .commands
            .iter_mut()
            .find(|command| {
                command.operation == SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
            })
            .expect("copy-back command");
        copy_back.blend_key = SceneLayerCompositorBlendKey::Inherit;

        let err = NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
            &layer_compositor,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            alpha_mask_binding,
        )
        .expect_err("copy-back without blend-key 0x100 must fail");

        assert!(err.contains("blend-key 0x100"));
    }

    fn tokenized_layer_compositor() -> SceneLayerCompositorPlan {
        let commands = vec![
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
                SceneLayerCompositorBlendKey::Inherit,
            ),
        ];
        SceneLayerCompositorPlan {
            layer_count: 1,
            command_count: commands.len(),
            object_final_layer_count: 0,
            tokenized_layer_count: 1,
            layers: vec![SceneLayerCompositorLayer {
                object: SceneObjectId(77),
                route: SceneLayerCompositorRoute::DirectSwapchain,
                uses_tokenized_subdraw: true,
                commands,
            }],
            command_order: [
                "preserve_scene_object_order",
                "classify_object_final_routes",
                "model_vtable_32_normal_render_entry",
                "model_vtable_50_clear_prep_entry",
                "model_vtable_52_53_tokenized_subdraw_entries",
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

    fn alpha_mask_binding(
        target: SceneGraphTarget,
    ) -> Result<NativeVulkanSceneLayerAlphaMaskTargetBinding, String> {
        match target {
            SceneGraphTarget::FullAlphaMask | SceneGraphTarget::FullAlphaMaskIntermediate => {
                Ok(NativeVulkanSceneLayerAlphaMaskTargetBinding {
                    target,
                    format: vk::Format::R8_UNORM,
                    width: 1920,
                    height: 1080,
                })
            }
            target => Err(format!("unexpected alpha-mask target {target:?}")),
        }
    }
}
