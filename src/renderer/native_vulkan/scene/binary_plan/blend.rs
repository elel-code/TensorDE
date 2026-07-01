use crate::core::scene::binary::{SceneBinaryEffectPassRecord, SceneBinaryMaterialPassRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryBlendState {
    pub(in crate::renderer::native_vulkan::scene) blending_name: u32,
    pub(in crate::renderer::native_vulkan::scene) mode: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryMaterialPassState {
    pub(in crate::renderer::native_vulkan::scene) blend: NativeVulkanSceneBinaryBlendState,
    pub(in crate::renderer::native_vulkan::scene) alpha_texture_slot: u32,
    pub(in crate::renderer::native_vulkan::scene) alpha_texture_mode: u16,
    pub(in crate::renderer::native_vulkan::scene) depth_test: u16,
    pub(in crate::renderer::native_vulkan::scene) depth_write: u16,
    pub(in crate::renderer::native_vulkan::scene) cull_mode: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::scene) struct NativeVulkanSceneBinaryEffectPassState {
    pub(in crate::renderer::native_vulkan::scene) blending_name: u32,
    pub(in crate::renderer::native_vulkan::scene) depth_test: u16,
    pub(in crate::renderer::native_vulkan::scene) depth_write: u16,
    pub(in crate::renderer::native_vulkan::scene) cull_mode: u16,
}

pub(super) fn native_vulkan_scene_binary_material_pass_state(
    material: SceneBinaryMaterialPassRecord,
) -> NativeVulkanSceneBinaryMaterialPassState {
    NativeVulkanSceneBinaryMaterialPassState {
        blend: NativeVulkanSceneBinaryBlendState {
            blending_name: material.blending_name,
            mode: material.blend_mode,
        },
        alpha_texture_slot: material.alpha_texture_slot,
        alpha_texture_mode: material.alpha_texture_mode,
        depth_test: material.depth_test,
        depth_write: material.depth_write,
        cull_mode: material.cull_mode,
    }
}

pub(super) fn native_vulkan_scene_binary_effect_pass_state(
    effect_pass: SceneBinaryEffectPassRecord,
) -> NativeVulkanSceneBinaryEffectPassState {
    NativeVulkanSceneBinaryEffectPassState {
        blending_name: effect_pass.blending_name,
        depth_test: effect_pass.depth_test,
        depth_write: effect_pass.depth_write,
        cull_mode: effect_pass.cull_mode,
    }
}
