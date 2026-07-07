//! Flattexture copy-back facts for WE layer alpha-mask token commands.
//!
//! References:
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/materials/util/flattexture.json`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.frag`
//! - `artifacts/wallpaper-engine-workshop/steamcmd-root/assets/shaders/minimalalpha.vert`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneGraphResourceRole, SceneGraphTarget, SceneLayerCompositorBlendKey,
    SceneLayerCompositorOperation, SceneObjectId,
};

use super::{
    NativeVulkanSceneLayerAlphaMaskCommandPlan, NativeVulkanSceneLayerAlphaMaskCopyMethod,
    NativeVulkanSceneLayerAlphaMaskRuntimePlan,
};
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::{
    NativeVulkanSceneTextureDescriptorSource, NativeVulkanSceneTextureDescriptorVkFormat,
};

pub(in crate::renderer::native_vulkan) const ALPHA_MASK_FLATTEXTURE_MATERIAL: &str =
    "materials/util/flattexture.json";
pub(in crate::renderer::native_vulkan) const ALPHA_MASK_FLATTEXTURE_SHADER: &str =
    "util/minimalalpha";
pub(in crate::renderer::native_vulkan) const ALPHA_MASK_FLATTEXTURE_TEXTURE_SLOT: u32 = 0;
pub(in crate::renderer::native_vulkan) const ALPHA_MASK_FLATTEXTURE_ALPHA: f32 = 1.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan {
    pub command_index: usize,
    pub object: SceneObjectId,
    pub operation: SceneLayerCompositorOperation,
    pub material: &'static str,
    pub shader: &'static str,
    pub source: SceneGraphTarget,
    pub target: SceneGraphTarget,
    pub texture_slot: u32,
    pub texture_role: SceneGraphResourceRole,
    pub texture_source: NativeVulkanSceneTextureDescriptorSource,
    pub target_format: NativeVulkanSceneTextureDescriptorVkFormat,
    pub alpha_uniform: NativeVulkanSceneLayerAlphaMaskCopyBackAlphaUniform,
    pub blend_key: SceneLayerCompositorBlendKey,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCopyBackAlphaUniform {
    pub name: &'static str,
    pub value_bits: u32,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_copy_back_draws(
    runtime: &NativeVulkanSceneLayerAlphaMaskRuntimePlan,
) -> Result<Vec<NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan>, String> {
    runtime
        .commands
        .iter()
        .enumerate()
        .filter(|(_, command)| {
            command.copy_method
                == NativeVulkanSceneLayerAlphaMaskCopyMethod::FlatTextureDrawDestColorBlendKey0x100
        })
        .map(|(command_index, command)| copy_back_draw_from_command(command_index, command))
        .collect()
}

fn copy_back_draw_from_command(
    command_index: usize,
    command: &NativeVulkanSceneLayerAlphaMaskCommandPlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan, String> {
    if command.operation != SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask {
        return Err(format!(
            "scene layer alpha-mask copy-back command {command_index} must be CopyIntermediateToFullAlphaMask, got {:?}",
            command.operation
        ));
    }
    if command.source_graph_target != Some(SceneGraphTarget::FullAlphaMaskIntermediate)
        || command.target_graph_target != Some(SceneGraphTarget::FullAlphaMask)
    {
        return Err(format!(
            "scene layer alpha-mask copy-back command {command_index} must read FullAlphaMaskIntermediate and write FullAlphaMask, got source {:?} target {:?}",
            command.source_graph_target, command.target_graph_target
        ));
    }
    if command.blend_key != SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100 {
        return Err(format!(
            "scene layer alpha-mask copy-back command {command_index} must keep blend-key bit 0x100, got {:?}",
            command.blend_key
        ));
    }
    Ok(NativeVulkanSceneLayerAlphaMaskCopyBackDrawPlan {
        command_index,
        object: command.object,
        operation: command.operation,
        material: ALPHA_MASK_FLATTEXTURE_MATERIAL,
        shader: ALPHA_MASK_FLATTEXTURE_SHADER,
        source: SceneGraphTarget::FullAlphaMaskIntermediate,
        target: SceneGraphTarget::FullAlphaMask,
        texture_slot: ALPHA_MASK_FLATTEXTURE_TEXTURE_SLOT,
        texture_role: SceneGraphResourceRole::shader_texture(ALPHA_MASK_FLATTEXTURE_TEXTURE_SLOT),
        texture_source: NativeVulkanSceneTextureDescriptorSource::GraphTarget(
            SceneGraphTarget::FullAlphaMaskIntermediate,
        ),
        target_format: NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm,
        alpha_uniform: NativeVulkanSceneLayerAlphaMaskCopyBackAlphaUniform {
            name: "g_Alpha",
            value_bits: ALPHA_MASK_FLATTEXTURE_ALPHA.to_bits(),
        },
        blend_key: SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
        command_order: [
            "load_materials_util_flattexture_json",
            "select_util_minimalalpha_shader",
            "bind_g_Texture0_to_full_alpha_mask_intermediate",
            "set_g_Alpha_to_1",
            "toggle_wrapper_blend_key_0x100",
            "draw_target_like_flattexture_copy_back",
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneLayerCompositorBlendKey, SceneLayerCompositorCommand, SceneLayerCompositorCondition,
        SceneLayerCompositorEntry, SceneLayerCompositorLayer, SceneLayerCompositorPlan,
        SceneLayerCompositorRoute, SceneLayerCompositorTarget,
    };
    use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::NativeVulkanSceneLayerAlphaMaskTargetBinding;
    use vulkanalia::vk;

    #[test]
    fn copy_back_plan_tracks_flattexture_minimalalpha_draw() {
        let runtime = runtime_plan();

        let copy_backs =
            native_vulkan_plan_scene_layer_alpha_mask_copy_back_draws(&runtime).unwrap();

        assert_eq!(copy_backs.len(), 1);
        let copy_back = &copy_backs[0];
        assert_eq!(copy_back.material, "materials/util/flattexture.json");
        assert_eq!(copy_back.shader, "util/minimalalpha");
        assert_eq!(
            copy_back.source,
            SceneGraphTarget::FullAlphaMaskIntermediate
        );
        assert_eq!(copy_back.target, SceneGraphTarget::FullAlphaMask);
        assert_eq!(copy_back.texture_slot, 0);
        assert_eq!(
            copy_back.texture_source,
            NativeVulkanSceneTextureDescriptorSource::GraphTarget(
                SceneGraphTarget::FullAlphaMaskIntermediate
            )
        );
        assert_eq!(
            copy_back.blend_key,
            SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100
        );
        assert_eq!(copy_back.alpha_uniform.name, "g_Alpha");
        assert_eq!(copy_back.alpha_uniform.value_bits, 1.0f32.to_bits());
        assert_eq!(
            copy_back.command_order,
            [
                "load_materials_util_flattexture_json",
                "select_util_minimalalpha_shader",
                "bind_g_Texture0_to_full_alpha_mask_intermediate",
                "set_g_Alpha_to_1",
                "toggle_wrapper_blend_key_0x100",
                "draw_target_like_flattexture_copy_back"
            ]
        );
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
            command_count: 1,
            object_final_layer_count: 0,
            tokenized_layer_count: 1,
            layers: vec![SceneLayerCompositorLayer {
                object: SceneObjectId(77),
                route: SceneLayerCompositorRoute::DirectSwapchain,
                uses_tokenized_subdraw: true,
                commands: vec![SceneLayerCompositorCommand {
                    entry: SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed,
                    operation: SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                    condition: SceneLayerCompositorCondition::Token2AfterIntermediateMask,
                    source: Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
                    target: SceneLayerCompositorTarget::FullAlphaMask,
                    blend_key: SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
                }],
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
}
