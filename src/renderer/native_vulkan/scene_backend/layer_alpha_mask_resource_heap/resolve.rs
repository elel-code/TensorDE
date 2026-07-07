//! WE layer alpha-mask sampled-image resolver.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/texture-and-format.md`

use vulkanalia::vk;

use crate::engine::scene_engine::{SceneGraphTarget, SceneResourceId};

use super::super::layer_alpha_mask_executor::NativeVulkanSceneLayerAlphaMaskDescriptorSource;
use super::super::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use super::super::texture_images::NativeVulkanSceneTextureImageBinding;

#[derive(Debug, Clone, Copy)]
pub(super) struct NativeVulkanSceneLayerAlphaMaskResolvedSampledImageBinding {
    pub source: NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
}

pub(super) fn resolve_alpha_mask_sampled_image_binding(
    source: NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    texture_image_binding: &impl Fn(
        SceneResourceId,
    ) -> Result<NativeVulkanSceneTextureImageBinding, String>,
    target_image_binding: &impl Fn(
        SceneGraphTarget,
    ) -> Result<NativeVulkanSceneOffscreenTargetBinding, String>,
) -> Result<NativeVulkanSceneLayerAlphaMaskResolvedSampledImageBinding, String> {
    match source {
        NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(resource) => {
            let binding = texture_image_binding(resource)?;
            if binding.resource != resource {
                return Err(format!(
                    "scene layer alpha-mask texture resolver returned {:?} for requested {:?}",
                    binding.resource, resource
                ));
            }
            Ok(NativeVulkanSceneLayerAlphaMaskResolvedSampledImageBinding {
                source,
                image: binding.image,
                view: binding.view,
                sampler: binding.sampler,
                format: binding.format,
                width: binding.width,
                height: binding.height,
                mip_count: binding.mip_count,
            })
        }
        NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(target) => {
            let binding = target_image_binding(target)?;
            if binding.target != target {
                return Err(format!(
                    "scene layer alpha-mask target resolver returned {:?} for requested {:?}",
                    binding.target, target
                ));
            }
            if matches!(
                target,
                SceneGraphTarget::FullAlphaMask | SceneGraphTarget::FullAlphaMaskIntermediate
            ) && binding.format != vk::Format::R8_UNORM
            {
                return Err(format!(
                    "scene layer alpha-mask graph target {target:?} must be R8_UNORM, got {:?}",
                    binding.format
                ));
            }
            Ok(NativeVulkanSceneLayerAlphaMaskResolvedSampledImageBinding {
                source,
                image: binding.image,
                view: binding.view,
                sampler: binding.sampler,
                format: binding.format,
                width: binding.width,
                height: binding.height,
                mip_count: 1,
            })
        }
    }
}
