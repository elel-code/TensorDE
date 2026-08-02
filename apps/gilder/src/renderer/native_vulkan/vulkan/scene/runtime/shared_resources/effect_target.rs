//! Shared color-target ownership for authored effect graphs.

use vulkan_renderer::{
    Extent2D, Extent3D, Image, ImageDescriptor, ImageDimension, ImageTiling, ImageView,
    MemoryAllocator, MemoryLocation, SampleCount, TextureFormat, TextureLayout, TextureState,
    TextureUsages, UploadBatch,
};

use super::super::effect_target::SceneEffectTargetImagePlan;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) struct SharedSceneEffectTargetDescriptor
{
    pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) physical_slot: u32,
    pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) format: TextureFormat,
    pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) extent: Extent2D,
    pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) input_attachment_required: bool,
}

impl SharedSceneEffectTargetDescriptor {
    pub(super) fn from_plan(plan: &SceneEffectTargetImagePlan) -> Self {
        Self {
            physical_slot: plan.physical_slot,
            format: plan.format,
            extent: plan.extent,
            input_attachment_required: plan.input_attachment_required,
        }
    }
}

pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) struct SharedSceneEffectTargetResource
{
    pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) descriptor:
        SharedSceneEffectTargetDescriptor,
    pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) image: Image,
    pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) view: ImageView,
}

pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) struct SharedSceneEffectTargetResources
{
    pub targets: Vec<SharedSceneEffectTargetResource>,
}

impl SharedSceneEffectTargetResources {
    pub(super) fn create(
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        descriptors: &[SharedSceneEffectTargetDescriptor],
    ) -> Result<Self, String> {
        let mut targets = Vec::with_capacity(descriptors.len());
        for descriptor in descriptors.iter().copied() {
            if descriptor.extent.is_empty() {
                return Err(format!(
                    "scene effect target physical slot {} has an empty extent",
                    descriptor.physical_slot
                ));
            }
            let mut usage = TextureUsages::COLOR_ATTACHMENT
                | TextureUsages::SAMPLED
                | TextureUsages::COPY_SOURCE
                | TextureUsages::COPY_DESTINATION;
            if descriptor.input_attachment_required {
                usage |= TextureUsages::INPUT_ATTACHMENT;
            }
            let image = allocator
                .create_image(&ImageDescriptor {
                    label: Some(format!(
                        "gilder-scene-effect-target-slot-{}",
                        descriptor.physical_slot
                    )),
                    dimension: ImageDimension::D2,
                    format: descriptor.format,
                    extent: Extent3D::new(descriptor.extent.width, descriptor.extent.height, 1),
                    mip_levels: 1,
                    array_layers: 1,
                    samples: SampleCount::One,
                    tiling: ImageTiling::Optimal,
                    usage,
                    memory: MemoryLocation::Device,
                })
                .map_err(|error| {
                    format!(
                        "create scene effect target slot {}: {error}",
                        descriptor.physical_slot
                    )
                })?;
            uploads
                .encoder_mut()
                .transition_image(
                    &image,
                    TextureState::Undefined,
                    TextureState::TransferDestination,
                )
                .map_err(|error| {
                    format!(
                        "transition scene effect target slot {} for clear: {error}",
                        descriptor.physical_slot
                    )
                })?;
            uploads
                .encoder_mut()
                .clear_color_image_all(&image, TextureLayout::TransferDestination, [0.0; 4])
                .map_err(|error| {
                    format!(
                        "clear scene effect target slot {}: {error}",
                        descriptor.physical_slot
                    )
                })?;
            uploads
                .encoder_mut()
                .transition_image(
                    &image,
                    TextureState::TransferDestination,
                    TextureState::FragmentSampledRead,
                )
                .map_err(|error| {
                    format!(
                        "transition scene effect target slot {} for sampling: {error}",
                        descriptor.physical_slot
                    )
                })?;
            let view = image
                .create_color_view(Some(format!(
                    "gilder-scene-effect-target-view-slot-{}",
                    descriptor.physical_slot
                )))
                .map_err(|error| {
                    format!(
                        "create scene effect target view slot {}: {error}",
                        descriptor.physical_slot
                    )
                })?;
            targets.push(SharedSceneEffectTargetResource {
                descriptor,
                image,
                view,
            });
        }
        Ok(Self { targets })
    }

    pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) fn target(
        &self,
        physical_slot: u32,
    ) -> Option<&SharedSceneEffectTargetResource> {
        self.targets
            .iter()
            .find(|target| target.descriptor.physical_slot == physical_slot)
    }

    pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) fn allocation_bytes(&self) -> u64 {
        self.targets
            .iter()
            .map(|target| target.image.allocation_size())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{SceneRenderTargetKind, SceneStringId};

    #[test]
    fn effect_target_descriptor_keeps_typed_format_extent_and_local_read_usage() {
        let descriptor = SharedSceneEffectTargetDescriptor {
            physical_slot: 7,
            format: TextureFormat::Rg16Float,
            extent: Extent2D::new(320, 180),
            input_attachment_required: true,
        };
        assert_eq!(descriptor.format, TextureFormat::Rg16Float);
        assert_eq!(descriptor.extent, Extent2D::new(320, 180));
        assert!(descriptor.input_attachment_required);
    }

    #[test]
    fn effect_target_descriptor_is_lowered_exactly_from_the_retained_plan() {
        let plan = SceneEffectTargetImagePlan {
            physical_slot: 9,
            graph_index: 2,
            target: SceneRenderTargetKind::NamedFbo,
            target_name: SceneStringId(4),
            format: TextureFormat::Rgba16Float,
            extent: Extent2D::new(640, 360),
            batch_field_count: 1,
            batch_atlas_columns: 1,
            batch_atlas_rows: 1,
            persistent_across_frames: true,
            aliased_logical_target_count: 2,
            input_attachment_required: true,
        };

        assert_eq!(
            SharedSceneEffectTargetDescriptor::from_plan(&plan),
            SharedSceneEffectTargetDescriptor {
                physical_slot: 9,
                format: TextureFormat::Rgba16Float,
                extent: Extent2D::new(640, 360),
                input_attachment_required: true,
            }
        );
    }
}
