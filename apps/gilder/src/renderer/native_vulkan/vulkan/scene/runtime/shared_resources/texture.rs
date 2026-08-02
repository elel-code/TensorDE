//! Shared image/view ownership for cold scene texture uploads.

use std::collections::BTreeSet;

use vulkan_renderer::{
    Extent3D, Image, ImageDescriptor, ImageDimension, ImageTiling, ImageUpload, ImageView,
    MemoryAllocator, MemoryLocation, SampleCount, SamplerAddressMode, SamplerDescriptor,
    SamplerFilterMode, TextureFormat, TextureLayout, TextureState, TextureUsages, UploadBatch,
};

use crate::engine::scene::{
    SceneResourceId, SceneStorage, SceneTextureFormat, SceneTextureSamplerAddressMode,
    SceneTextureSamplerFilter,
};

use super::super::sampled_binding::{SceneSampledImageBindingPlan, SceneSampledImageSource};

pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) struct SharedSceneTextureResource {
    pub resource: Option<SceneResourceId>,
    pub image: Image,
    pub view: ImageView,
    pub sampler: SamplerDescriptor,
}

pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) struct SharedSceneTextureResources {
    pub white_fallback: Option<SharedSceneTextureResource>,
    pub textures: Vec<SharedSceneTextureResource>,
}

impl SharedSceneTextureResources {
    pub(super) fn create(
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        storage: &SceneStorage,
        binding_cycle: &[SceneSampledImageBindingPlan],
    ) -> Result<Self, String> {
        let resource_ids = binding_cycle
            .iter()
            .flat_map(|plan| plan.sources.iter())
            .filter_map(|source| match source {
                SceneSampledImageSource::SceneTexture { resource } => Some(*resource),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let white_required = binding_cycle
            .iter()
            .flat_map(|plan| plan.sources.iter())
            .any(|source| *source == SceneSampledImageSource::FallbackWhite);
        let white_fallback = white_required
            .then(|| create_white_fallback(allocator, uploads))
            .transpose()?;
        let mut textures = Vec::with_capacity(resource_ids.len());
        for resource in resource_ids {
            textures.push(create_texture(allocator, uploads, storage, resource)?);
        }
        Ok(Self {
            white_fallback,
            textures,
        })
    }

    pub(super) fn texture(&self, resource: SceneResourceId) -> Option<&SharedSceneTextureResource> {
        self.textures
            .iter()
            .find(|texture| texture.resource == Some(resource))
    }

    pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) fn allocation_bytes(&self) -> u64 {
        self.white_fallback
            .iter()
            .chain(&self.textures)
            .map(|texture| texture.image.allocation_size())
            .sum()
    }
}

fn create_white_fallback(
    allocator: &MemoryAllocator,
    uploads: &mut UploadBatch<'_>,
) -> Result<SharedSceneTextureResource, String> {
    let image = create_sampled_image(
        allocator,
        "gilder-scene-white-fallback",
        TextureFormat::Rgba8Unorm,
        1,
        1,
        1,
    )?;
    upload_mip(
        uploads,
        &image,
        TextureFormat::Rgba8Unorm,
        0,
        Extent3D::new(1, 1, 1),
        &[255, 255, 255, 255],
    )?;
    finish_texture_upload(uploads, image, None, SamplerDescriptor::linear_clamp())
}

fn create_texture(
    allocator: &MemoryAllocator,
    uploads: &mut UploadBatch<'_>,
    storage: &SceneStorage,
    resource: SceneResourceId,
) -> Result<SharedSceneTextureResource, String> {
    let texture = storage.texture(resource).ok_or_else(|| {
        format!(
            "scene material texture resource {} has no texture record",
            resource.0
        )
    })?;
    let format = texture_format(texture.format);
    let image = create_sampled_image(
        allocator,
        "gilder-scene-material-texture",
        format,
        texture.width,
        texture.height,
        texture.mip_count,
    )?;
    for (level, mip) in storage.texture_mips(texture).iter().enumerate() {
        upload_mip(
            uploads,
            &image,
            format,
            u32::try_from(level).map_err(|_| "scene texture mip level exceeds u32")?,
            Extent3D::new(mip.width, mip.height, 1),
            storage.texture_mip_payload(mip),
        )?;
    }
    finish_texture_upload(
        uploads,
        image,
        Some(resource),
        sampler_descriptor(texture.sampler_filter, texture.sampler_address_mode),
    )
}

fn create_sampled_image(
    allocator: &MemoryAllocator,
    label: &str,
    format: TextureFormat,
    width: u32,
    height: u32,
    mip_levels: u32,
) -> Result<Image, String> {
    allocator
        .create_image(&ImageDescriptor {
            label: Some(label.into()),
            dimension: ImageDimension::D2,
            format,
            extent: Extent3D::new(width, height, 1),
            mip_levels,
            array_layers: 1,
            samples: SampleCount::One,
            tiling: ImageTiling::Optimal,
            usage: TextureUsages::COPY_DESTINATION | TextureUsages::SAMPLED,
            memory: MemoryLocation::Device,
        })
        .map_err(|error| format!("create scene texture {label:?}: {error}"))
}

fn upload_mip(
    uploads: &mut UploadBatch<'_>,
    image: &Image,
    format: TextureFormat,
    mip_level: u32,
    extent: Extent3D,
    data: &[u8],
) -> Result<(), String> {
    if mip_level == 0 {
        uploads
            .encoder_mut()
            .transition_image(
                image,
                TextureState::Undefined,
                TextureState::TransferDestination,
            )
            .map_err(|error| format!("transition scene texture for upload: {error}"))?;
    }
    let upload = ImageUpload::color_mip_2d_tightly_packed(format, extent, mip_level)
        .map_err(|error| format!("describe scene texture mip {mip_level}: {error}"))?;
    unsafe {
        uploads
            .write_image_data(image, TextureLayout::TransferDestination, upload, data)
            .map_err(|error| format!("upload scene texture mip {mip_level}: {error}"))?;
    }
    Ok(())
}

fn finish_texture_upload(
    uploads: &mut UploadBatch<'_>,
    image: Image,
    resource: Option<SceneResourceId>,
    sampler: SamplerDescriptor,
) -> Result<SharedSceneTextureResource, String> {
    uploads
        .encoder_mut()
        .transition_image(
            &image,
            TextureState::TransferDestination,
            TextureState::FragmentSampledRead,
        )
        .map_err(|error| format!("transition scene texture for sampling: {error}"))?;
    let view = image
        .create_color_view(Some("gilder-scene-material-texture-view".into()))
        .map_err(|error| format!("create scene texture view: {error}"))?;
    Ok(SharedSceneTextureResource {
        resource,
        image,
        view,
        sampler,
    })
}

const fn texture_format(format: SceneTextureFormat) -> TextureFormat {
    match format {
        SceneTextureFormat::Rgba8Unorm => TextureFormat::Rgba8Unorm,
        SceneTextureFormat::Rg8Unorm => TextureFormat::Rg8Unorm,
        SceneTextureFormat::R8Unorm => TextureFormat::R8Unorm,
        SceneTextureFormat::Bc1RgbaUnormBlock => TextureFormat::Bc1RgbaUnorm,
        SceneTextureFormat::Bc2UnormBlock => TextureFormat::Bc2RgbaUnorm,
        SceneTextureFormat::Bc3UnormBlock => TextureFormat::Bc3RgbaUnorm,
        SceneTextureFormat::Bc4UnormBlock => TextureFormat::Bc4RUnorm,
        SceneTextureFormat::Bc5UnormBlock => TextureFormat::Bc5RgUnorm,
        SceneTextureFormat::Bc7UnormBlock => TextureFormat::Bc7RgbaUnorm,
    }
}

fn sampler_descriptor(
    filter: SceneTextureSamplerFilter,
    address: SceneTextureSamplerAddressMode,
) -> SamplerDescriptor {
    let max_anisotropy_x1 = if filter == SceneTextureSamplerFilter::Anisotropic8 {
        8
    } else {
        1
    };
    let filter_mode = match filter {
        SceneTextureSamplerFilter::Point => SamplerFilterMode::Nearest,
        SceneTextureSamplerFilter::Linear | SceneTextureSamplerFilter::Anisotropic8 => {
            SamplerFilterMode::Linear
        }
    };
    let address = match address {
        SceneTextureSamplerAddressMode::Repeat => SamplerAddressMode::Repeat,
        SceneTextureSamplerAddressMode::ClampToEdge => SamplerAddressMode::ClampToEdge,
        SceneTextureSamplerAddressMode::ClampToTransparentBlackBorder => {
            SamplerAddressMode::ClampToBorder
        }
    };
    SamplerDescriptor {
        mag_filter: filter_mode,
        min_filter: filter_mode,
        mipmap_filter: filter_mode,
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        max_anisotropy_x1,
        ..SamplerDescriptor::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_texture_formats_keep_exact_gpu_block_formats() {
        assert_eq!(
            texture_format(SceneTextureFormat::Rg8Unorm),
            TextureFormat::Rg8Unorm
        );
        assert_eq!(
            texture_format(SceneTextureFormat::Bc7UnormBlock),
            TextureFormat::Bc7RgbaUnorm
        );
    }

    #[test]
    fn authored_sampler_policy_keeps_point_border_and_anisotropic_eight() {
        let point = sampler_descriptor(
            SceneTextureSamplerFilter::Point,
            SceneTextureSamplerAddressMode::ClampToTransparentBlackBorder,
        );
        assert_eq!(point.min_filter, SamplerFilterMode::Nearest);
        assert_eq!(point.address_mode_u, SamplerAddressMode::ClampToBorder);
        let anisotropic = sampler_descriptor(
            SceneTextureSamplerFilter::Anisotropic8,
            SceneTextureSamplerAddressMode::Repeat,
        );
        assert_eq!(anisotropic.max_anisotropy_x1, 8);
    }
}
