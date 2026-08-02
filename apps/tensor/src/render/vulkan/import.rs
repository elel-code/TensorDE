use std::{collections::HashMap, os::fd::AsFd};

use tensor_host::Fourcc;
use thiserror::Error;
use vulkan_renderer::vulkanalia::vk;
use vulkan_renderer::{
    Buffer, BufferDescriptor, BufferUsages, ColorBufferImageCopy, CommandEncoder,
    Device as RendererDevice, DmaBufImageDescriptor, DmaBufPlaneLayout, Error as RendererError,
    Extent2D, Extent3D, Image, ImageDescriptor, ImageDimension, ImageTiling, ImageView,
    ImageViewDescriptor, ImportedDmaBufImage, MemoryAllocator, MemoryLocation, ResourceBinding,
    SampleCount, SampledImageDescriptor, TextureUsages,
};

use crate::{ecs::SurfaceBufferId, render::Dmabuf};

use super::{texture_format_for_fourcc, vulkan_format_for_fourcc};

/// Imported client images are kept separate from compositor-owned output images.
/// The key is a compositor-assigned stable buffer identity, never a raw file
/// descriptor or an object ID that can be recycled by a client.
#[derive(Default)]
pub(super) struct ClientImageCache {
    active: HashMap<SurfaceBufferId, ImportedClientImage>,
    retired: Vec<ImportedClientImage>,
}

#[derive(Clone, Debug)]
pub(super) struct ClientImageInfo {
    backing: ClientImageBacking,
    pub(super) sampled_descriptor: SampledImageDescriptor,
    /// Imported dma-buf storage is owned by the non-Vulkan producer between
    /// submissions.  The frame executor performs an explicit FOREIGN acquire
    /// and release around every sampling pass.
    pub(super) foreign_owned: bool,
    /// The first FOREIGN acquire must pair `UNDEFINED` with the foreign queue
    /// family.  Once a queue submission succeeds, subsequent acquires use the
    /// preserved `GENERAL` layout instead.  This is intentionally a snapshot:
    /// the cache updates it only after queue submission, never while recording.
    pub(super) needs_initial_acquire: bool,
    /// A Tensor-owned SHM image has one pending staging copy after its CPU
    /// snapshot changes. dma-buf imports never carry an upload.
    pub(super) upload_pending: bool,
}

impl ClientImageInfo {
    pub(super) fn resource_binding(&self) -> ResourceBinding {
        self.backing.resource_binding()
    }

    pub(super) fn retain_for_submission(&self, encoder: &mut CommandEncoder) {
        self.backing.retain_for_submission(encoder);
    }

    pub(super) unsafe fn record_upload(
        &self,
        encoder: &mut CommandEncoder,
    ) -> Result<(), RendererError> {
        if !self.upload_pending {
            return Ok(());
        }
        let ClientImageBacking::Shm(image) = &self.backing else {
            return Err(RendererError::Validation(
                "only SHM client images may carry a pending upload".into(),
            ));
        };
        let extent = image.image.extent();
        unsafe {
            encoder.copy_buffer_to_color_image(
                &image.staging,
                &image.image,
                &[ColorBufferImageCopy {
                    buffer_offset: 0,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    destination_mip_level: 0,
                    destination_base_array_layer: 0,
                    destination_origin: vulkan_renderer::Origin2D::new(0, 0),
                    extent: Extent2D::new(extent.width, extent.height),
                    layer_count: 1,
                }],
            )
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ShmUploadTarget {
    pub(super) id: SurfaceBufferId,
    pub(super) size: tensor_util::Size,
    pub(super) format: Fourcc,
    pub(super) completed_timeline: u64,
}

impl ClientImageCache {
    pub(super) fn import<F: AsFd>(
        &mut self,
        id: SurfaceBufferId,
        device: &RendererDevice,
        dmabuf: &Dmabuf<F>,
    ) -> Result<(), ClientImportError> {
        if self.active.contains_key(&id) {
            return Ok(());
        }

        let image = ImportedClientImage::create_dmabuf(device, dmabuf)?;
        self.active.insert(id, image);
        Ok(())
    }

    pub(super) fn upload_shm(
        &mut self,
        allocator: &MemoryAllocator,
        target: ShmUploadTarget,
        fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    ) -> Result<(), ClientImportError> {
        let ShmUploadTarget {
            id,
            size,
            format,
            completed_timeline,
        } = target;
        let mut current = self.active.remove(&id);
        if let Some(image) = current.as_mut()
            && image.matches_shm(size, format)
            && image.last_use_timeline <= completed_timeline
        {
            let result = image.write_shm(fill);
            self.active
                .insert(id, current.expect("active SHM image was borrowed above"));
            return result;
        }

        let mut image = match ImportedClientImage::create_shm(allocator, size, format) {
            Ok(image) => image,
            Err(error) => {
                if let Some(current) = current {
                    self.active.insert(id, current);
                }
                return Err(error);
            }
        };
        if let Err(error) = image.write_shm(fill) {
            if let Some(current) = current {
                self.active.insert(id, current);
            }
            return Err(error);
        }
        if let Some(current) = current {
            self.retired.push(current);
        }
        self.active.insert(id, image);
        Ok(())
    }

    pub(super) fn release(&mut self, id: SurfaceBufferId) {
        if let Some(image) = self.active.remove(&id) {
            self.retired.push(image);
        }
    }

    pub(super) fn image_info(&self, id: SurfaceBufferId) -> Option<ClientImageInfo> {
        self.active.get(&id).map(|image| ClientImageInfo {
            backing: image.backing.clone(),
            sampled_descriptor: image.sampled_descriptor(),
            foreign_owned: image.foreign_owned,
            needs_initial_acquire: !image.initialized,
            upload_pending: image.upload_pending,
        })
    }

    /// Commit imported-image state after the queue accepted a frame.  This is
    /// deliberately separate from command recording: a failed queue submit
    /// must leave a fresh import on the `UNDEFINED` acquire path.
    pub(super) fn mark_submitted(
        &mut self,
        ids: impl IntoIterator<Item = SurfaceBufferId>,
        timeline: u64,
    ) {
        for id in ids {
            if let Some(image) = self.active.get_mut(&id) {
                image.initialized = true;
                image.upload_pending = false;
                image.last_use_timeline = image.last_use_timeline.max(timeline);
            }
        }
    }

    pub(super) fn retire_completed(&mut self, completed_timeline: u64) {
        let mut retained = Vec::with_capacity(self.retired.len());
        for image in self.retired.drain(..) {
            if image.last_use_timeline > completed_timeline {
                retained.push(image);
            }
        }
        self.retired = retained;
    }

    pub(super) fn destroy(&mut self) {
        self.active.clear();
        self.retired.clear();
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.active.len()
    }
}

#[derive(Clone, Debug)]
enum ClientImageBacking {
    DmaBuf(ImportedDmaBufImage),
    Shm(ShmClientImage),
}

impl ClientImageBacking {
    fn resource_binding(&self) -> ResourceBinding {
        match self {
            Self::DmaBuf(image) => image.resource_binding(),
            Self::Shm(image) => ResourceBinding::whole_color_image(&image.image),
        }
    }

    fn retain_for_submission(&self, encoder: &mut CommandEncoder) {
        match self {
            Self::DmaBuf(image) => encoder.retain_resource(image),
            Self::Shm(image) => {
                encoder.retain_resource(&image.image);
                encoder.retain_resource(&image.view);
                encoder.retain_resource(&image.staging);
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ShmClientImage {
    image: Image,
    view: ImageView,
    staging: Buffer,
    staging_len: usize,
}

struct ImportedClientImage {
    backing: ClientImageBacking,
    extent: Extent3D,
    format: Fourcc,
    foreign_owned: bool,
    upload_pending: bool,
    initialized: bool,
    last_use_timeline: u64,
}

impl ImportedClientImage {
    fn create_dmabuf<F: AsFd>(
        device: &RendererDevice,
        dmabuf: &Dmabuf<F>,
    ) -> Result<Self, ClientImportError> {
        let format = dmabuf.format;
        let host_code = format.code;
        let vulkan_format = vulkan_format_for_fourcc(host_code)
            .ok_or(ClientImportError::UnsupportedFourcc(host_code))?;
        let shape = validate_shape(dmabuf)?;
        let components = component_mapping(host_code);
        let fd = &dmabuf.planes.first().ok_or(ClientImportError::NoPlanes)?.fd;
        let shared_dma_buf = device
            .import_dma_buf_image(
                &DmaBufImageDescriptor {
                    label: Some("tensor-client-dmabuf".into()),
                    format: vulkan_format,
                    extent: vk::Extent2D {
                        width: shape.width,
                        height: shape.height,
                    },
                    modifier: format.modifier.raw(),
                    planes: vec![DmaBufPlaneLayout {
                        offset: u64::from(shape.offset),
                        row_pitch: u64::from(shape.stride),
                    }],
                    usage: vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC,
                    components,
                },
                fd,
            )
            .map_err(|source| ClientImportError::SharedDmaBuf(source.to_string()))?;

        Ok(Self {
            backing: ClientImageBacking::DmaBuf(shared_dma_buf),
            extent: Extent3D::new(shape.width, shape.height, 1),
            format: host_code,
            foreign_owned: true,
            upload_pending: false,
            initialized: false,
            last_use_timeline: 0,
        })
    }

    fn create_shm(
        allocator: &MemoryAllocator,
        size: tensor_util::Size,
        format: Fourcc,
    ) -> Result<Self, ClientImportError> {
        let texture_format = texture_format_for_fourcc(format)
            .ok_or(ClientImportError::UnsupportedFourcc(format))?;
        let extent = Extent3D::new(size.width, size.height, 1);
        if extent.is_empty() {
            return Err(ClientImportError::InvalidDimensions);
        }
        let image = allocator
            .create_image(&ImageDescriptor {
                label: Some("tensor-client-shm-image".into()),
                dimension: ImageDimension::D2,
                format: texture_format,
                extent,
                mip_levels: 1,
                array_layers: 1,
                samples: SampleCount::One,
                tiling: ImageTiling::Optimal,
                usage: TextureUsages::SAMPLED | TextureUsages::COPY_DESTINATION,
                memory: MemoryLocation::Device,
            })
            .map_err(|source| ClientImportError::SharedShm(source.to_string()))?;
        let view = image
            .create_view(&ImageViewDescriptor {
                label: Some("tensor-client-shm-view".into()),
                view_type: vk::ImageViewType::_2D,
                format: texture_format,
                components: component_mapping(format),
                subresource_range: image.full_subresource_range(vk::ImageAspectFlags::COLOR),
            })
            .map_err(|source| ClientImportError::SharedShm(source.to_string()))?;
        let staging_len = shm_staging_len(size)?;
        let staging = allocator
            .create_buffer(&BufferDescriptor {
                label: Some("tensor-client-shm-upload".into()),
                size: u64::try_from(staging_len)
                    .map_err(|_| ClientImportError::InvalidDimensions)?,
                usage: BufferUsages::COPY_SOURCE,
                memory: MemoryLocation::Upload,
            })
            .map_err(|source| ClientImportError::SharedShm(source.to_string()))?;
        Ok(Self {
            backing: ClientImageBacking::Shm(ShmClientImage {
                image,
                view,
                staging,
                staging_len,
            }),
            extent,
            format,
            foreign_owned: false,
            upload_pending: false,
            initialized: false,
            last_use_timeline: 0,
        })
    }

    fn matches_shm(&self, size: tensor_util::Size, format: Fourcc) -> bool {
        matches!(&self.backing, ClientImageBacking::Shm(_))
            && self.extent.width == size.width
            && self.extent.height == size.height
            && self.format == format
    }

    fn write_shm(
        &mut self,
        fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
    ) -> Result<(), ClientImportError> {
        let result = {
            let ClientImageBacking::Shm(image) = &self.backing else {
                return Err(ClientImportError::NotShmImage);
            };
            unsafe { image.staging.write_with(0, image.staging_len, fill) }
                .map_err(|source| ClientImportError::SharedShm(source.to_string()))?
        };
        if let Err(error) = result {
            self.upload_pending = false;
            return Err(ClientImportError::ShmSource(error));
        }
        self.upload_pending = true;
        Ok(())
    }

    fn sampled_descriptor(&self) -> SampledImageDescriptor {
        match &self.backing {
            ClientImageBacking::DmaBuf(image) => {
                SampledImageDescriptor::from_imported_dma_buf(image)
            }
            ClientImageBacking::Shm(image) => SampledImageDescriptor::from_image_view(&image.view),
        }
    }
}

fn component_mapping(format: Fourcc) -> vk::ComponentMapping {
    vk::ComponentMapping {
        r: vk::ComponentSwizzle::IDENTITY,
        g: vk::ComponentSwizzle::IDENTITY,
        b: vk::ComponentSwizzle::IDENTITY,
        a: if is_opaque(format) {
            vk::ComponentSwizzle::ONE
        } else {
            vk::ComponentSwizzle::IDENTITY
        },
    }
}

fn shm_staging_len(size: tensor_util::Size) -> Result<usize, ClientImportError> {
    usize::try_from(size.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .and_then(|stride| stride.checked_mul(usize::try_from(size.height).ok()?))
        .ok_or(ClientImportError::InvalidDimensions)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ImportShape {
    width: u32,
    height: u32,
    offset: u32,
    stride: u32,
}

fn validate_shape<F>(dmabuf: &Dmabuf<F>) -> Result<ImportShape, ClientImportError> {
    let width = dmabuf.size.width;
    let height = dmabuf.size.height;
    if width == 0 || height == 0 {
        return Err(ClientImportError::InvalidDimensions);
    }
    if dmabuf.planes.len() != 1 {
        return Err(ClientImportError::UnsupportedPlaneCount(
            dmabuf.planes.len(),
        ));
    }
    if dmabuf.format.modifier.is_invalid() {
        return Err(ClientImportError::ImplicitModifier);
    }
    let plane = dmabuf.planes.first().ok_or(ClientImportError::NoPlanes)?;
    let offset = plane.offset;
    let stride = plane.stride;
    if stride == 0 {
        return Err(ClientImportError::InvalidStride);
    }
    Ok(ImportShape {
        width,
        height,
        offset,
        stride,
    })
}

fn is_opaque(format: Fourcc) -> bool {
    matches!(
        format,
        Fourcc::XRGB8888 | Fourcc::XBGR8888 | Fourcc::XRGB2101010 | Fourcc::XBGR2101010
    )
}

#[derive(Debug, Error)]
pub(super) enum ClientImportError {
    #[error("DRM fourcc {0} has no Vulkan client-import format")]
    UnsupportedFourcc(Fourcc),
    #[error("client dma-buf dimensions must be positive and fit Vulkan's extent")]
    InvalidDimensions,
    #[error("client dma-buf has no planes")]
    NoPlanes,
    #[error("client dma-buf has unsupported plane count {0}; only one-plane RGB is enabled")]
    UnsupportedPlaneCount(usize),
    #[error("client dma-buf uses an implicit modifier")]
    ImplicitModifier,
    #[error("client dma-buf has a zero row stride")]
    InvalidStride,
    #[error("shared renderer failed to import the explicit-modifier client dma-buf: {0}")]
    SharedDmaBuf(String),
    #[error("shared renderer failed to create or update a client SHM image: {0}")]
    SharedShm(String),
    #[error("renderer image is not backed by SHM staging memory")]
    NotShmImage,
    #[error("failed to read client SHM pixels: {0}")]
    ShmSource(String),
}

#[cfg(test)]
mod tests;
