//! Effect sampled-image resolver for descriptor-heap resource sets.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/texture-and-format.md`

use vulkanalia::vk;

use crate::engine::scene_engine::{SceneGraphTarget, SceneResourceId, SceneTextureFormat};

use super::super::effect_descriptors::NativeVulkanSceneEffectTextureDescriptorBinding;
use super::super::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use super::super::texture_descriptors::{
    NativeVulkanSceneTextureDescriptorFormat, NativeVulkanSceneTextureDescriptorSource,
};
use super::super::texture_images::NativeVulkanSceneTextureImageBinding;

#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanSceneEffectResolvedSampledImageBinding {
    pub source: NativeVulkanSceneTextureDescriptorSource,
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
}

pub(super) fn resolve_effect_sampled_image_binding(
    descriptor: &NativeVulkanSceneEffectTextureDescriptorBinding,
    texture_image_binding: &impl Fn(
        SceneResourceId,
    ) -> Result<NativeVulkanSceneTextureImageBinding, String>,
    target_image_binding: &impl Fn(
        SceneGraphTarget,
    ) -> Result<NativeVulkanSceneOffscreenTargetBinding, String>,
) -> Result<NativeVulkanSceneEffectResolvedSampledImageBinding, String> {
    match descriptor.source {
        NativeVulkanSceneTextureDescriptorSource::ResidentTexture(resource) => {
            let binding = texture_image_binding(resource)?;
            Ok(NativeVulkanSceneEffectResolvedSampledImageBinding {
                source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(binding.resource),
                image: binding.image,
                view: binding.view,
                sampler: binding.sampler,
                format: binding.format,
                width: binding.width,
                height: binding.height,
                mip_count: binding.mip_count,
            })
        }
        NativeVulkanSceneTextureDescriptorSource::GraphTarget(target) => {
            let binding = target_image_binding(target)?;
            Ok(NativeVulkanSceneEffectResolvedSampledImageBinding {
                source: NativeVulkanSceneTextureDescriptorSource::GraphTarget(binding.target),
                image: binding.image,
                view: binding.view,
                sampler: binding.sampler,
                format: binding.format,
                width: binding.width,
                height: binding.height,
                mip_count: 1,
            })
        }
        NativeVulkanSceneTextureDescriptorSource::PreviousFramebuffer { .. }
        | NativeVulkanSceneTextureDescriptorSource::Scene { .. } => Err(format!(
            "scene effect resource heap cannot resolve external sampled source {:?} without a retained image resolver",
            descriptor.source
        )),
    }
}

pub(super) fn validate_effect_texture_binding(
    descriptor: &NativeVulkanSceneEffectTextureDescriptorBinding,
    binding: NativeVulkanSceneEffectResolvedSampledImageBinding,
) -> Result<(), String> {
    if descriptor.source != binding.source {
        return Err(format!(
            "scene effect resource heap texture resolver returned {:?} for descriptor {:?}",
            binding.source, descriptor.source
        ));
    }
    validate_effect_descriptor_u32(descriptor.width, binding.width, descriptor.source, "width")?;
    validate_effect_descriptor_u32(
        descriptor.height,
        binding.height,
        descriptor.source,
        "height",
    )?;
    validate_effect_descriptor_u32(
        descriptor.mip_count,
        binding.mip_count,
        descriptor.source,
        "mip count",
    )?;
    let expected_format = descriptor_vk_format(&descriptor.format).ok_or_else(|| {
        format!(
            "scene effect resource heap texture {:?} descriptor format {:?} cannot map to vk::Format",
            descriptor.source, descriptor.format
        )
    })?;
    if expected_format != binding.format {
        return Err(format!(
            "scene effect resource heap texture {:?} descriptor format {:?} does not match retained image {:?}",
            descriptor.source, expected_format, binding.format
        ));
    }
    Ok(())
}

fn validate_effect_descriptor_u32(
    descriptor_value: u32,
    binding_value: u32,
    source: NativeVulkanSceneTextureDescriptorSource,
    label: &'static str,
) -> Result<(), String> {
    if descriptor_value == binding_value {
        Ok(())
    } else {
        Err(format!(
            "scene effect resource heap texture {source:?} descriptor {label} {descriptor_value} does not match retained image {binding_value}"
        ))
    }
}

fn descriptor_vk_format(format: &NativeVulkanSceneTextureDescriptorFormat) -> Option<vk::Format> {
    match format {
        NativeVulkanSceneTextureDescriptorFormat::SceneTexture(format) => {
            Some(scene_texture_vk_format(*format))
        }
        NativeVulkanSceneTextureDescriptorFormat::VkFormat(format) => Some(format.to_vk_format()),
    }
}

fn scene_texture_vk_format(format: SceneTextureFormat) -> vk::Format {
    match format {
        SceneTextureFormat::Bc1RgbaUnormBlock => vk::Format::BC1_RGBA_UNORM_BLOCK,
        SceneTextureFormat::Bc3UnormBlock => vk::Format::BC3_UNORM_BLOCK,
        SceneTextureFormat::Bc7UnormBlock => vk::Format::BC7_UNORM_BLOCK,
        SceneTextureFormat::R8Unorm => vk::Format::R8_UNORM,
        SceneTextureFormat::R8G8B8A8Unorm => vk::Format::R8G8B8A8_UNORM,
    }
}
