//! Shared image/view ownership for cold scene texture uploads.

use std::collections::BTreeSet;

use vulkan_renderer::{
    Extent3D, Image, ImageDescriptor, ImageDimension, ImageTiling, ImageUpload, ImageView,
    MemoryAllocator, MemoryLocation, Queue, SampleCount, SamplerAddressMode, SamplerDescriptor,
    SamplerFilterMode, TextureFormat, TextureLayout, TextureState, TextureUsages, UploadBatch,
};

use crate::engine::scene::{
    SceneResourceId, SceneStorage, SceneTextureFormat, SceneTextureMipRecord, SceneTextureRecord,
    SceneTextureSamplerAddressMode, SceneTextureSamplerFilter,
};

use super::super::sampled_binding::{SceneSampledImageBindingPlan, SceneSampledImageSource};
use super::record_cold_upload;

pub(in crate::renderer::rendering_device::scene_present::scene::runtime) struct SharedSceneTextureResource {
    pub resource: Option<SceneResourceId>,
    pub image: Image,
    pub view: ImageView,
    pub sampler: SamplerDescriptor,
}

pub(in crate::renderer::rendering_device::scene_present::scene::runtime) struct SharedSceneTextureResources {
    pub white_fallback: Option<SharedSceneTextureResource>,
    pub textures: Vec<SharedSceneTextureResource>,
}

impl SharedSceneTextureResources {
    pub(super) fn create(
        allocator: &MemoryAllocator,
        uploads: &mut UploadBatch<'_>,
        queue: &Queue,
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
            .then(|| create_white_fallback(allocator, uploads, queue))
            .transpose()?;
        let mut textures = Vec::with_capacity(resource_ids.len());
        for resource in resource_ids {
            textures.push(create_texture(
                allocator, uploads, queue, storage, resource,
            )?);
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

    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn allocation_bytes(
        &self,
    ) -> u64 {
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
    queue: &Queue,
) -> Result<SharedSceneTextureResource, String> {
    let image = create_sampled_image(
        allocator,
        "tensor-wallpaper-scene-white-fallback",
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
        Extent3D::new(1, 1, 1),
        &[255, 255, 255, 255],
        queue,
    )?;
    finish_texture_upload(uploads, image, None, SamplerDescriptor::linear_clamp())
}

fn create_texture(
    allocator: &MemoryAllocator,
    uploads: &mut UploadBatch<'_>,
    queue: &Queue,
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
    let storage_extent = texture_mip_storage_image_extent(texture, 0);
    let image = create_sampled_image(
        allocator,
        "tensor-wallpaper-scene-material-texture",
        format,
        storage_extent.width,
        storage_extent.height,
        texture.mip_count,
    )?;
    for (level, mip) in storage.texture_mips(texture).iter().enumerate() {
        let mip_level = u32::try_from(level).map_err(|_| "scene texture mip level exceeds u32")?;
        upload_mip(
            uploads,
            &image,
            format,
            mip_level,
            texture_mip_image_extent(texture, mip, mip_level)?,
            Extent3D::new(mip.width, mip.height, 1),
            storage.texture_mip_payload(mip),
            queue,
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
    image_extent: Extent3D,
    source_storage_extent: Extent3D,
    data: &[u8],
    queue: &Queue,
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
    let upload = ImageUpload::color_mip_2d_with_source_storage_extent(
        format,
        image_extent,
        source_storage_extent,
        mip_level,
    )
    .map_err(|error| format!("describe scene texture mip {mip_level}: {error}"))?;
    record_cold_upload(uploads, queue, |uploads| unsafe {
        uploads.write_image_data(image, TextureLayout::TransferDestination, upload, data)
    })
    .map_err(|error| format!("upload scene texture mip {mip_level}: {error}"))?;
    Ok(())
}

fn texture_mip_image_extent(
    texture: &SceneTextureRecord,
    mip: &SceneTextureMipRecord,
    mip_level: u32,
) -> Result<Extent3D, String> {
    let image_extent = texture_mip_storage_image_extent(texture, mip_level);
    if mip.width < image_extent.width || mip.height < image_extent.height {
        return Err(format!(
            "scene texture mip {mip_level} source storage {}x{} is smaller than storage image {}x{}",
            mip.width, mip.height, image_extent.width, image_extent.height
        ));
    }
    Ok(image_extent)
}

fn texture_mip_storage_image_extent(texture: &SceneTextureRecord, mip_level: u32) -> Extent3D {
    Extent3D::new(
        texture
            .storage_width
            .checked_shr(mip_level)
            .unwrap_or(0)
            .max(1),
        texture
            .storage_height
            .checked_shr(mip_level)
            .unwrap_or(0)
            .max(1),
        1,
    )
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
        .create_color_view(Some("tensor-wallpaper-scene-material-texture-view".into()))
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

    #[test]
    fn padded_bc_source_mip_uploads_to_its_storage_image_extent() {
        let texture = SceneTextureRecord {
            resource: SceneResourceId(7),
            format: SceneTextureFormat::Bc7UnormBlock,
            source_runtime_format: 0,
            payload_format: 0,
            sampler_filter: SceneTextureSamplerFilter::Anisotropic8,
            sampler_address_mode: SceneTextureSamplerAddressMode::Repeat,
            width: 1287,
            height: 1080,
            storage_width: 1287,
            storage_height: 1080,
            mip_start: 0,
            mip_count: 3,
            texv_tag: crate::engine::scene::SceneStringId::NONE,
            texb_tag: crate::engine::scene::SceneStringId::NONE,
            sequence_tag: crate::engine::scene::SceneStringId::NONE,
            sequence_cell_width: 0,
            sequence_cell_height: 0,
            sequence_frame_start: 0,
            sequence_frame_count: 0,
            payload_offset: 0,
            payload_len: 0,
            alpha_coverage_rows: [0; crate::engine::scene::SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
        };
        let mip = SceneTextureMipRecord {
            width: 324,
            height: 272,
            payload_offset: 0,
            payload_len: 81 * 68 * 16,
        };

        assert_eq!(
            texture_mip_image_extent(&texture, &mip, 2).unwrap(),
            Extent3D::new(321, 270, 1)
        );
    }

    #[test]
    fn legacy_image_payload_uses_storage_image_extent_without_rewriting_logical_extent() {
        let texture = SceneTextureRecord {
            resource: SceneResourceId(96),
            format: SceneTextureFormat::Bc7UnormBlock,
            source_runtime_format: 0,
            payload_format: 2,
            sampler_filter: SceneTextureSamplerFilter::Anisotropic8,
            sampler_address_mode: SceneTextureSamplerAddressMode::ClampToEdge,
            width: 1024,
            height: 1024,
            storage_width: 1000,
            storage_height: 1000,
            mip_start: 0,
            mip_count: 4,
            texv_tag: crate::engine::scene::SceneStringId::NONE,
            texb_tag: crate::engine::scene::SceneStringId::NONE,
            sequence_tag: crate::engine::scene::SceneStringId::NONE,
            sequence_cell_width: 0,
            sequence_cell_height: 0,
            sequence_frame_start: 0,
            sequence_frame_count: 0,
            payload_offset: 0,
            payload_len: 0,
            alpha_coverage_rows: [0; crate::engine::scene::SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
        };
        let mip = SceneTextureMipRecord {
            width: 1000,
            height: 1000,
            payload_offset: 0,
            payload_len: 250 * 250 * 16,
        };

        assert_eq!(texture.width, 1024);
        assert_eq!(texture.height, 1024);
        assert_eq!(
            texture_mip_image_extent(&texture, &mip, 0).unwrap(),
            Extent3D::new(1000, 1000, 1)
        );
    }
}
