use vulkanalia::{prelude::v1_4::*, vk};

use super::CommandEncoder;
use crate::{
    Buffer, BufferUsages, Error, Extent2D, Extent3D, Image, Origin2D, Origin3D, Result,
    TextureLayout, TextureSubresourceLayers, TextureSubresourceRange,
};

mod validation;

use validation::{
    lower_color_image_copy, validate_buffer_copy, validate_buffer_image_copy,
    validate_buffer_image_resources, validate_color_clear, validate_image_blit,
    validate_image_blit_resources, validate_image_copy, validate_image_copy_resources,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferCopy {
    pub source_offset: u64,
    pub destination_offset: u64,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferImageCopy {
    pub buffer_offset: u64,
    /// Texels per row; zero requests tightly packed Vulkan semantics.
    pub buffer_row_length: u32,
    /// Texel rows per image; zero requests tightly packed Vulkan semantics.
    pub buffer_image_height: u32,
    pub image_subresource: TextureSubresourceLayers,
    pub image_offset: Origin3D,
    pub image_extent: Extent3D,
}

/// Typed color-image upload region for one buffer-to-image copy.
///
/// The destination is a color subresource. Row and image height values of
/// zero retain Vulkan's tightly-packed semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorBufferImageCopy {
    pub buffer_offset: u64,
    pub buffer_row_length: u32,
    pub buffer_image_height: u32,
    pub destination_mip_level: u32,
    pub destination_base_array_layer: u32,
    pub destination_origin: Origin2D,
    pub extent: Extent2D,
    pub layer_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageCopy {
    pub source_subresource: TextureSubresourceLayers,
    pub source_offset: Origin3D,
    pub destination_subresource: TextureSubresourceLayers,
    pub destination_offset: Origin3D,
    pub extent: Extent3D,
}

/// Backend-neutral color-image copy over explicit mip and array-layer ranges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorImageCopy {
    pub source_mip_level: u32,
    pub source_base_array_layer: u32,
    pub source_origin: Origin2D,
    pub destination_mip_level: u32,
    pub destination_base_array_layer: u32,
    pub destination_origin: Origin2D,
    pub extent: Extent2D,
    pub layer_count: u32,
}

/// Source and destination boxes for one device-side image scale operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageBlit {
    pub source_subresource: TextureSubresourceLayers,
    pub source_offsets: [Origin3D; 2],
    pub destination_subresource: TextureSubresourceLayers,
    pub destination_offsets: [Origin3D; 2],
}

/// Reconstruction filter used by [`CommandEncoder::blit_image`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageBlitFilter {
    Nearest,
    Linear,
}

impl ImageBlitFilter {
    const fn to_vk(self) -> vk::Filter {
        match self {
            Self::Nearest => vk::Filter::NEAREST,
            Self::Linear => vk::Filter::LINEAR,
        }
    }
}

impl CommandEncoder {
    /// Clears every color mip/layer of a renderer-owned image in a typed
    /// transfer or general layout.
    pub fn clear_color_image_all(
        &mut self,
        image: &Image,
        layout: crate::TextureLayout,
        color: [f32; 4],
    ) -> Result<()> {
        if !matches!(
            layout,
            crate::TextureLayout::TransferDestination | crate::TextureLayout::General
        ) {
            return Err(Error::Validation(
                "color image clear requires TransferDestination or General layout".into(),
            ));
        }
        let ranges = [image.full_subresource_range(crate::TextureAspects::COLOR)];
        unsafe { self.clear_color_image(image, layout, color, &ranges) }
    }

    /// Records a small inline buffer update without allocating staging memory.
    /// This is intended for infrequent values no larger than 64 KiB; repeated
    /// or large writes should use `UploadBelt`.
    ///
    /// # Safety
    ///
    /// The caller must provide the transfer-to-consumer barrier. The encoder
    /// retains the destination through submission completion.
    pub unsafe fn update_buffer(
        &mut self,
        destination: &Buffer,
        offset: u64,
        data: &[u8],
    ) -> Result<()> {
        if !destination.belongs_to(&self.owner) {
            return Err(Error::Validation(
                "updated buffer was created by a different Device".into(),
            ));
        }
        if !destination.usage().contains(BufferUsages::COPY_DESTINATION) {
            return Err(Error::Validation(
                "updated buffer is missing TRANSFER_DST usage".into(),
            ));
        }
        let size = u64::try_from(data.len())
            .map_err(|_| Error::Validation("buffer update size exceeds u64".into()))?;
        if size == 0 || size > 65_536 || !size.is_multiple_of(4) || !offset.is_multiple_of(4) {
            return Err(Error::Validation(
                "buffer update must contain 4-byte-aligned data no larger than 64 KiB".into(),
            ));
        }
        if offset
            .checked_add(size)
            .is_none_or(|end| end > destination.size())
        {
            return Err(Error::Validation(
                "buffer update exceeds the destination buffer".into(),
            ));
        }
        unsafe {
            self.owner
                .device
                .cmd_update_buffer(self.raw(), destination.raw(), offset, data)
        };
        self.retain_resource(destination);
        Ok(())
    }

    /// Records device-side buffer copies without CPU readback.
    ///
    /// # Safety
    ///
    /// The render graph must transition both buffers for transfer access. The
    /// encoder retains both buffers through submission completion.
    pub unsafe fn copy_buffer_to_buffer(
        &mut self,
        source: &Buffer,
        destination: &Buffer,
        regions: &[BufferCopy],
    ) -> Result<()> {
        if !source.belongs_to(&self.owner) || !destination.belongs_to(&self.owner) {
            return Err(Error::Validation(
                "copy buffers were created by a different Device".into(),
            ));
        }
        if !source.usage().contains(BufferUsages::COPY_SOURCE)
            || !destination.usage().contains(BufferUsages::COPY_DESTINATION)
        {
            return Err(Error::Validation(
                "buffer copy requires TRANSFER_SRC and TRANSFER_DST usage".into(),
            ));
        }
        let mut copies = Vec::with_capacity(regions.len());
        for region in regions {
            validate_buffer_copy(source, destination, *region)?;
            copies.push(
                vk::BufferCopy2::builder()
                    .src_offset(region.source_offset)
                    .dst_offset(region.destination_offset)
                    .size(region.size)
                    .build(),
            );
        }
        if copies.is_empty() {
            return Ok(());
        }
        let copy = vk::CopyBufferInfo2::builder()
            .src_buffer(source.raw())
            .dst_buffer(destination.raw())
            .regions(&copies);
        unsafe { self.owner.device.cmd_copy_buffer2(self.raw(), &copy) };
        self.retain_resource(source);
        self.retain_resource(destination);
        Ok(())
    }

    /// Records an upload/readback-buffer copy into an image.
    ///
    /// # Safety
    ///
    /// The graph must transition `image` to `layout` with transfer-write access,
    /// and source bytes must satisfy Vulkan's texel/block packing requirements.
    /// Both resources are retained automatically until submission completes.
    pub unsafe fn copy_buffer_to_image(
        &mut self,
        source: &Buffer,
        image: &Image,
        layout: TextureLayout,
        regions: &[BufferImageCopy],
    ) -> Result<()> {
        validate_buffer_image_resources(self, source, image, layout)?;
        let mut copies = Vec::with_capacity(regions.len());
        for region in regions {
            validate_buffer_image_copy(source, image, *region)?;
            copies.push(
                vk::BufferImageCopy2::builder()
                    .buffer_offset(region.buffer_offset)
                    .buffer_row_length(region.buffer_row_length)
                    .buffer_image_height(region.buffer_image_height)
                    .image_subresource(region.image_subresource.to_vk())
                    .image_offset(region.image_offset.to_vk())
                    .image_extent(region.image_extent.to_vk())
                    .build(),
            );
        }
        if copies.is_empty() {
            return Ok(());
        }
        let copy = vk::CopyBufferToImageInfo2::builder()
            .src_buffer(source.raw())
            .dst_image(image.raw())
            .dst_image_layout(layout.to_vk())
            .regions(&copies);
        unsafe {
            self.owner
                .device
                .cmd_copy_buffer_to_image2(self.raw(), &copy)
        };
        self.retain_resource(source);
        self.retain_resource(image);
        Ok(())
    }

    /// Records color-image uploads without exposing Vulkan copy structs or
    /// layouts. The destination must already be in the transfer-destination
    /// state.
    ///
    /// # Safety
    ///
    /// The caller must transition `image` to [`TextureLayout::TransferDestination`]
    /// and ensure that source bytes obey the selected format's packing rules.
    pub unsafe fn copy_buffer_to_color_image(
        &mut self,
        source: &Buffer,
        image: &Image,
        regions: &[ColorBufferImageCopy],
    ) -> Result<()> {
        let regions = regions
            .iter()
            .copied()
            .map(lower_color_buffer_image_copy)
            .collect::<Result<Vec<_>>>()?;
        unsafe {
            self.copy_buffer_to_image(source, image, TextureLayout::TransferDestination, &regions)
        }
    }

    /// Records exact-format image copies without CPU readback.
    ///
    /// # Safety
    ///
    /// The caller must transition both images to the declared transfer layouts
    /// and keep them live until submission completes.
    pub unsafe fn copy_image_to_image(
        &mut self,
        source: &Image,
        source_layout: TextureLayout,
        destination: &Image,
        destination_layout: TextureLayout,
        regions: &[ImageCopy],
    ) -> Result<()> {
        validate_image_copy_resources(
            self,
            source,
            source_layout,
            destination,
            destination_layout,
        )?;
        let mut copies = Vec::with_capacity(regions.len());
        for region in regions {
            validate_image_copy(source, destination, *region)?;
            copies.push(
                vk::ImageCopy2::builder()
                    .src_subresource(region.source_subresource.to_vk())
                    .src_offset(region.source_offset.to_vk())
                    .dst_subresource(region.destination_subresource.to_vk())
                    .dst_offset(region.destination_offset.to_vk())
                    .extent(region.extent.to_vk())
                    .build(),
            );
        }
        if copies.is_empty() {
            return Ok(());
        }
        let copy = vk::CopyImageInfo2::builder()
            .src_image(source.raw())
            .src_image_layout(source_layout.to_vk())
            .dst_image(destination.raw())
            .dst_image_layout(destination_layout.to_vk())
            .regions(&copies);
        unsafe { self.owner.device.cmd_copy_image2(self.raw(), &copy) };
        self.retain_resource(source);
        self.retain_resource(destination);
        Ok(())
    }

    /// Records exact-format color image copies without exposing backend types.
    ///
    /// # Safety
    ///
    /// The graph must transition the source and destination images to the
    /// supplied transfer layouts before this command.
    pub unsafe fn copy_color_image_to_image(
        &mut self,
        source: &Image,
        source_layout: TextureLayout,
        destination: &Image,
        destination_layout: TextureLayout,
        regions: &[ColorImageCopy],
    ) -> Result<()> {
        if source_layout != TextureLayout::TransferSource
            || destination_layout != TextureLayout::TransferDestination
        {
            return Err(Error::Validation(
                "color image copy requires TransferSource and TransferDestination layouts".into(),
            ));
        }
        let regions = regions
            .iter()
            .copied()
            .map(lower_color_image_copy)
            .collect::<Result<Vec<_>>>()?;
        unsafe {
            self.copy_image_to_image(
                source,
                source_layout,
                destination,
                destination_layout,
                &regions,
            )
        }
    }

    /// Clears color subresources without a render pass.
    ///
    /// # Safety
    ///
    /// The caller must transition `image` to `layout` with transfer-write
    /// access. The encoder retains the image through submission completion.
    pub unsafe fn clear_color_image(
        &mut self,
        image: &Image,
        layout: TextureLayout,
        color: [f32; 4],
        ranges: &[TextureSubresourceRange],
    ) -> Result<()> {
        validate_color_clear(self, image, layout, color, ranges)?;
        if ranges.is_empty() {
            return Ok(());
        }
        let color = vk::ClearColorValue { float32: color };
        unsafe {
            self.owner.device.cmd_clear_color_image(
                self.raw(),
                image.raw(),
                layout.to_vk(),
                &color,
                &ranges
                    .iter()
                    .copied()
                    .map(TextureSubresourceRange::to_vk)
                    .collect::<Vec<_>>(),
            )
        };
        self.retain_resource(image);
        Ok(())
    }

    /// Scales or copies image regions entirely on the device.
    ///
    /// # Safety
    ///
    /// The caller must transition the source and destination to the declared
    /// transfer layouts. Format capabilities must support the selected filter.
    pub unsafe fn blit_image(
        &mut self,
        source: &Image,
        source_layout: TextureLayout,
        destination: &Image,
        destination_layout: TextureLayout,
        regions: &[ImageBlit],
        filter: ImageBlitFilter,
    ) -> Result<()> {
        validate_image_blit_resources(
            self,
            source,
            source_layout,
            destination,
            destination_layout,
        )?;
        let mut blits = Vec::with_capacity(regions.len());
        for region in regions {
            validate_image_blit(source, destination, *region)?;
            blits.push(
                vk::ImageBlit2::builder()
                    .src_subresource(region.source_subresource.to_vk())
                    .src_offsets(region.source_offsets.map(Origin3D::to_vk))
                    .dst_subresource(region.destination_subresource.to_vk())
                    .dst_offsets(region.destination_offsets.map(Origin3D::to_vk))
                    .build(),
            );
        }
        if blits.is_empty() {
            return Ok(());
        }
        let blit = vk::BlitImageInfo2::builder()
            .src_image(source.raw())
            .src_image_layout(source_layout.to_vk())
            .dst_image(destination.raw())
            .dst_image_layout(destination_layout.to_vk())
            .regions(&blits)
            .filter(filter.to_vk());
        unsafe { self.owner.device.cmd_blit_image2(self.raw(), &blit) };
        self.retain_resource(source);
        self.retain_resource(destination);
        Ok(())
    }
}

fn lower_color_buffer_image_copy(copy: ColorBufferImageCopy) -> Result<BufferImageCopy> {
    if copy.extent.is_empty() || copy.layer_count == 0 {
        return Err(Error::Validation(
            "color buffer-image copy extent and layer count must be non-zero".into(),
        ));
    }
    if copy.destination_origin.x < 0 || copy.destination_origin.y < 0 {
        return Err(Error::Validation(
            "color buffer-image copy origin must be non-negative".into(),
        ));
    }
    Ok(BufferImageCopy {
        buffer_offset: copy.buffer_offset,
        buffer_row_length: copy.buffer_row_length,
        buffer_image_height: copy.buffer_image_height,
        image_subresource: TextureSubresourceLayers::color(
            copy.destination_mip_level,
            copy.destination_base_array_layer,
            copy.layer_count,
        ),
        image_offset: Origin3D::new(copy.destination_origin.x, copy.destination_origin.y, 0),
        image_extent: Extent3D::new(copy.extent.width, copy.extent.height, 1),
    })
}

#[cfg(test)]
mod tests;
