use vulkanalia::{prelude::v1_4::*, vk};

use super::CommandEncoder;
use crate::{Buffer, Error, Image, Result};

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
    pub image_subresource: vk::ImageSubresourceLayers,
    pub image_offset: vk::Offset3D,
    pub image_extent: vk::Extent3D,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageCopy {
    pub source_subresource: vk::ImageSubresourceLayers,
    pub source_offset: vk::Offset3D,
    pub destination_subresource: vk::ImageSubresourceLayers,
    pub destination_offset: vk::Offset3D,
    pub extent: vk::Extent3D,
}

/// Source and destination boxes for one device-side image scale operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageBlit {
    pub source_subresource: vk::ImageSubresourceLayers,
    pub source_offsets: [vk::Offset3D; 2],
    pub destination_subresource: vk::ImageSubresourceLayers,
    pub destination_offsets: [vk::Offset3D; 2],
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
        if !destination
            .usage()
            .contains(vk::BufferUsageFlags::TRANSFER_DST)
        {
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
        if !source.usage().contains(vk::BufferUsageFlags::TRANSFER_SRC)
            || !destination
                .usage()
                .contains(vk::BufferUsageFlags::TRANSFER_DST)
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
        layout: vk::ImageLayout,
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
                    .image_subresource(region.image_subresource)
                    .image_offset(region.image_offset)
                    .image_extent(region.image_extent)
                    .build(),
            );
        }
        if copies.is_empty() {
            return Ok(());
        }
        let copy = vk::CopyBufferToImageInfo2::builder()
            .src_buffer(source.raw())
            .dst_image(image.raw())
            .dst_image_layout(layout)
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

    /// Records exact-format image copies without CPU readback.
    ///
    /// # Safety
    ///
    /// The caller must transition both images to the declared transfer layouts
    /// and keep them live until submission completes.
    pub unsafe fn copy_image_to_image(
        &mut self,
        source: &Image,
        source_layout: vk::ImageLayout,
        destination: &Image,
        destination_layout: vk::ImageLayout,
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
                    .src_subresource(region.source_subresource)
                    .src_offset(region.source_offset)
                    .dst_subresource(region.destination_subresource)
                    .dst_offset(region.destination_offset)
                    .extent(region.extent)
                    .build(),
            );
        }
        if copies.is_empty() {
            return Ok(());
        }
        let copy = vk::CopyImageInfo2::builder()
            .src_image(source.raw())
            .src_image_layout(source_layout)
            .dst_image(destination.raw())
            .dst_image_layout(destination_layout)
            .regions(&copies);
        unsafe { self.owner.device.cmd_copy_image2(self.raw(), &copy) };
        self.retain_resource(source);
        self.retain_resource(destination);
        Ok(())
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
        layout: vk::ImageLayout,
        color: [f32; 4],
        ranges: &[vk::ImageSubresourceRange],
    ) -> Result<()> {
        validate_color_clear(self, image, layout, color, ranges)?;
        if ranges.is_empty() {
            return Ok(());
        }
        let color = vk::ClearColorValue { float32: color };
        unsafe {
            self.owner
                .device
                .cmd_clear_color_image(self.raw(), image.raw(), layout, &color, ranges)
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
        source_layout: vk::ImageLayout,
        destination: &Image,
        destination_layout: vk::ImageLayout,
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
                    .src_subresource(region.source_subresource)
                    .src_offsets(region.source_offsets)
                    .dst_subresource(region.destination_subresource)
                    .dst_offsets(region.destination_offsets)
                    .build(),
            );
        }
        if blits.is_empty() {
            return Ok(());
        }
        let blit = vk::BlitImageInfo2::builder()
            .src_image(source.raw())
            .src_image_layout(source_layout)
            .dst_image(destination.raw())
            .dst_image_layout(destination_layout)
            .regions(&blits)
            .filter(filter.to_vk());
        unsafe { self.owner.device.cmd_blit_image2(self.raw(), &blit) };
        self.retain_resource(source);
        self.retain_resource(destination);
        Ok(())
    }
}

fn validate_color_clear(
    encoder: &CommandEncoder,
    image: &Image,
    layout: vk::ImageLayout,
    color: [f32; 4],
    ranges: &[vk::ImageSubresourceRange],
) -> Result<()> {
    if !image.belongs_to(&encoder.owner) {
        return Err(Error::Validation(
            "cleared image was created by a different Device".into(),
        ));
    }
    if !image.usage().contains(vk::ImageUsageFlags::TRANSFER_DST)
        || !matches!(
            layout,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL | vk::ImageLayout::GENERAL
        )
    {
        return Err(Error::Validation(
            "color clear requires TRANSFER_DST usage and destination/general layout".into(),
        ));
    }
    if color.iter().any(|channel| !channel.is_finite()) {
        return Err(Error::Validation(
            "floating-point clear color must contain finite values".into(),
        ));
    }
    for range in ranges {
        if range.aspect_mask != vk::ImageAspectFlags::COLOR
            || range.level_count == 0
            || range.layer_count == 0
            || range
                .base_mip_level
                .checked_add(range.level_count)
                .is_none_or(|end| end > image.mip_levels())
            || range
                .base_array_layer
                .checked_add(range.layer_count)
                .is_none_or(|end| end > image.array_layers())
        {
            return Err(Error::Validation(
                "color clear subresource range is invalid".into(),
            ));
        }
    }
    Ok(())
}

fn validate_image_blit_resources(
    encoder: &CommandEncoder,
    source: &Image,
    source_layout: vk::ImageLayout,
    destination: &Image,
    destination_layout: vk::ImageLayout,
) -> Result<()> {
    validate_image_copy_resources(
        encoder,
        source,
        source_layout,
        destination,
        destination_layout,
    )?;
    if source.sample_count() != vk::SampleCountFlags::_1
        || destination.sample_count() != vk::SampleCountFlags::_1
    {
        return Err(Error::Validation(
            "image blits require single-sampled images".into(),
        ));
    }
    Ok(())
}

fn validate_image_blit(source: &Image, destination: &Image, blit: ImageBlit) -> Result<()> {
    validate_blit_box(source, blit.source_subresource, blit.source_offsets)?;
    validate_blit_box(
        destination,
        blit.destination_subresource,
        blit.destination_offsets,
    )?;
    if blit.source_subresource.layer_count != blit.destination_subresource.layer_count
        || blit.source_subresource.aspect_mask != blit.destination_subresource.aspect_mask
    {
        return Err(Error::Validation(
            "image blit source and destination layers/aspects must match".into(),
        ));
    }
    Ok(())
}

fn validate_blit_box(
    image: &Image,
    subresource: vk::ImageSubresourceLayers,
    offsets: [vk::Offset3D; 2],
) -> Result<()> {
    if subresource.aspect_mask != vk::ImageAspectFlags::COLOR
        || subresource.layer_count == 0
        || subresource.mip_level >= image.mip_levels()
        || subresource
            .base_array_layer
            .checked_add(subresource.layer_count)
            .is_none_or(|end| end > image.array_layers())
    {
        return Err(Error::Validation(
            "image blit subresource is invalid".into(),
        ));
    }
    let base = image.extent();
    let mip = subresource.mip_level;
    let extent = vk::Extent3D {
        width: (base.width >> mip).max(1),
        height: (base.height >> mip).max(1),
        depth: (base.depth >> mip).max(1),
    };
    for offset in offsets {
        if offset.x < 0
            || offset.y < 0
            || offset.z < 0
            || offset.x as u32 > extent.width
            || offset.y as u32 > extent.height
            || offset.z as u32 > extent.depth
        {
            return Err(Error::Validation(
                "image blit offset exceeds the selected mip extent".into(),
            ));
        }
    }
    if offsets[0].x == offsets[1].x || offsets[0].y == offsets[1].y || offsets[0].z == offsets[1].z
    {
        return Err(Error::Validation(
            "image blit boxes must have non-zero extent".into(),
        ));
    }
    Ok(())
}

fn validate_image_copy_resources(
    encoder: &CommandEncoder,
    source: &Image,
    source_layout: vk::ImageLayout,
    destination: &Image,
    destination_layout: vk::ImageLayout,
) -> Result<()> {
    if !source.belongs_to(&encoder.owner) || !destination.belongs_to(&encoder.owner) {
        return Err(Error::Validation(
            "image copy resources were created by a different Device".into(),
        ));
    }
    if source.raw() == destination.raw() {
        return Err(Error::Validation(
            "same-image copies are outside the standard transfer path".into(),
        ));
    }
    if !source.usage().contains(vk::ImageUsageFlags::TRANSFER_SRC)
        || !destination
            .usage()
            .contains(vk::ImageUsageFlags::TRANSFER_DST)
    {
        return Err(Error::Validation(
            "image copy requires TRANSFER_SRC and TRANSFER_DST usage".into(),
        ));
    }
    if !matches!(
        source_layout,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL | vk::ImageLayout::GENERAL
    ) || !matches!(
        destination_layout,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL | vk::ImageLayout::GENERAL
    ) {
        return Err(Error::Validation(
            "image copy layouts must be transfer-optimal or general".into(),
        ));
    }
    if source.format() != destination.format()
        || source.sample_count() != destination.sample_count()
        || source.image_type() != destination.image_type()
    {
        return Err(Error::Validation(
            "standard image copies require equal formats, samples, and image types".into(),
        ));
    }
    Ok(())
}

fn validate_image_copy(source: &Image, destination: &Image, copy: ImageCopy) -> Result<()> {
    validate_copy_subresource(
        source,
        copy.source_subresource,
        copy.source_offset,
        copy.extent,
    )?;
    validate_copy_subresource(
        destination,
        copy.destination_subresource,
        copy.destination_offset,
        copy.extent,
    )?;
    if copy.source_subresource.layer_count != copy.destination_subresource.layer_count
        || copy.source_subresource.aspect_mask != copy.destination_subresource.aspect_mask
    {
        return Err(Error::Validation(
            "image copy source and destination layers/aspects must match".into(),
        ));
    }
    Ok(())
}

fn validate_copy_subresource(
    image: &Image,
    subresource: vk::ImageSubresourceLayers,
    offset: vk::Offset3D,
    extent: vk::Extent3D,
) -> Result<()> {
    if subresource.aspect_mask.is_empty()
        || subresource.layer_count == 0
        || subresource.mip_level >= image.mip_levels()
        || subresource
            .base_array_layer
            .checked_add(subresource.layer_count)
            .is_none_or(|end| end > image.array_layers())
        || extent.width == 0
        || extent.height == 0
        || extent.depth == 0
        || offset.x < 0
        || offset.y < 0
        || offset.z < 0
    {
        return Err(Error::Validation(
            "image copy subresource, offset, or extent is invalid".into(),
        ));
    }
    let base = image.extent();
    let mip = subresource.mip_level;
    let mip_extent = vk::Extent3D {
        width: (base.width >> mip).max(1),
        height: (base.height >> mip).max(1),
        depth: (base.depth >> mip).max(1),
    };
    if (offset.x as u32)
        .checked_add(extent.width)
        .is_none_or(|end| end > mip_extent.width)
        || (offset.y as u32)
            .checked_add(extent.height)
            .is_none_or(|end| end > mip_extent.height)
        || (offset.z as u32)
            .checked_add(extent.depth)
            .is_none_or(|end| end > mip_extent.depth)
    {
        return Err(Error::Validation(
            "image copy region exceeds the selected mip extent".into(),
        ));
    }
    Ok(())
}

fn validate_buffer_copy(source: &Buffer, destination: &Buffer, copy: BufferCopy) -> Result<()> {
    if copy.size == 0 {
        return Err(Error::Validation(
            "buffer copy size must be non-zero".into(),
        ));
    }
    let source_end = copy
        .source_offset
        .checked_add(copy.size)
        .ok_or_else(|| Error::Validation("source buffer copy range overflows".into()))?;
    let destination_end = copy
        .destination_offset
        .checked_add(copy.size)
        .ok_or_else(|| Error::Validation("destination buffer copy range overflows".into()))?;
    if source_end > source.size() || destination_end > destination.size() {
        return Err(Error::Validation(
            "buffer copy range exceeds a buffer".into(),
        ));
    }
    if !copy.source_offset.is_multiple_of(4)
        || !copy.destination_offset.is_multiple_of(4)
        || !copy.size.is_multiple_of(4)
    {
        return Err(Error::Validation(
            "buffer copy offsets and size must be multiples of four bytes".into(),
        ));
    }
    if source.raw() == destination.raw()
        && copy.source_offset < destination_end
        && copy.destination_offset < source_end
    {
        return Err(Error::Validation(
            "overlapping copies within one buffer are forbidden".into(),
        ));
    }
    Ok(())
}

fn validate_buffer_image_resources(
    encoder: &CommandEncoder,
    source: &Buffer,
    image: &Image,
    layout: vk::ImageLayout,
) -> Result<()> {
    if !source.belongs_to(&encoder.owner) || !image.belongs_to(&encoder.owner) {
        return Err(Error::Validation(
            "buffer/image copy resources were created by a different Device".into(),
        ));
    }
    if !source.usage().contains(vk::BufferUsageFlags::TRANSFER_SRC)
        || !image.usage().contains(vk::ImageUsageFlags::TRANSFER_DST)
    {
        return Err(Error::Validation(
            "buffer-to-image copy requires TRANSFER_SRC and TRANSFER_DST usage".into(),
        ));
    }
    if image.sample_count() != vk::SampleCountFlags::_1 {
        return Err(Error::Validation(
            "buffer-to-image copies require a single-sampled image".into(),
        ));
    }
    if !matches!(
        layout,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL | vk::ImageLayout::GENERAL
    ) {
        return Err(Error::Validation(
            "buffer-to-image copy layout must be TRANSFER_DST_OPTIMAL or GENERAL".into(),
        ));
    }
    Ok(())
}

fn validate_buffer_image_copy(source: &Buffer, image: &Image, copy: BufferImageCopy) -> Result<()> {
    if copy.buffer_offset >= source.size() {
        return Err(Error::Validation(
            "buffer-to-image source offset is outside the buffer".into(),
        ));
    }
    if !copy.buffer_offset.is_multiple_of(4) {
        return Err(Error::Validation(
            "buffer-to-image source offset must be a multiple of four bytes".into(),
        ));
    }
    if copy.image_extent.width == 0 || copy.image_extent.height == 0 || copy.image_extent.depth == 0
    {
        return Err(Error::Validation(
            "buffer-to-image extent must be non-empty".into(),
        ));
    }
    if copy.image_subresource.aspect_mask.is_empty()
        || copy.image_subresource.layer_count == 0
        || copy.image_subresource.mip_level >= image.mip_levels()
        || copy
            .image_subresource
            .base_array_layer
            .checked_add(copy.image_subresource.layer_count)
            .is_none_or(|end| end > image.array_layers())
    {
        return Err(Error::Validation(
            "buffer-to-image subresource range is invalid".into(),
        ));
    }
    if copy.image_offset.x < 0 || copy.image_offset.y < 0 || copy.image_offset.z < 0 {
        return Err(Error::Validation(
            "buffer-to-image offsets must be non-negative".into(),
        ));
    }
    let mip = copy.image_subresource.mip_level;
    let extent = image.extent();
    let mip_extent = vk::Extent3D {
        width: (extent.width >> mip).max(1),
        height: (extent.height >> mip).max(1),
        depth: (extent.depth >> mip).max(1),
    };
    let end_x = (copy.image_offset.x as u32).checked_add(copy.image_extent.width);
    let end_y = (copy.image_offset.y as u32).checked_add(copy.image_extent.height);
    let end_z = (copy.image_offset.z as u32).checked_add(copy.image_extent.depth);
    if end_x.is_none_or(|end| end > mip_extent.width)
        || end_y.is_none_or(|end| end > mip_extent.height)
        || end_z.is_none_or(|end| end > mip_extent.depth)
    {
        return Err(Error::Validation(
            "buffer-to-image copy exceeds the selected mip extent".into(),
        ));
    }
    if copy.buffer_row_length != 0 && copy.buffer_row_length < copy.image_extent.width {
        return Err(Error::Validation(
            "buffer row length is smaller than the copied image width".into(),
        ));
    }
    if copy.buffer_image_height != 0 && copy.buffer_image_height < copy.image_extent.height {
        return Err(Error::Validation(
            "buffer image height is smaller than the copied image height".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
