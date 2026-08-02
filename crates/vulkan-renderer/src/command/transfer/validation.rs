use super::{
    Buffer, BufferCopy, BufferImageCopy, ColorImageCopy, CommandEncoder, Error, Image, ImageBlit,
    ImageCopy, Result,
};
use crate::{
    BufferUsages, Extent3D, Origin3D, SampleCount, TextureAspects, TextureLayout,
    TextureSubresourceLayers, TextureSubresourceRange, TextureUsages,
};

pub(super) fn lower_color_image_copy(copy: ColorImageCopy) -> Result<ImageCopy> {
    if copy.extent.is_empty() || copy.layer_count == 0 {
        return Err(Error::Validation(
            "color image copy extent and layer count must be non-zero".into(),
        ));
    }
    Ok(ImageCopy {
        source_subresource: TextureSubresourceLayers::color(
            copy.source_mip_level,
            copy.source_base_array_layer,
            copy.layer_count,
        ),
        source_offset: Origin3D::new(copy.source_origin.x, copy.source_origin.y, 0),
        destination_subresource: TextureSubresourceLayers::color(
            copy.destination_mip_level,
            copy.destination_base_array_layer,
            copy.layer_count,
        ),
        destination_offset: Origin3D::new(copy.destination_origin.x, copy.destination_origin.y, 0),
        extent: Extent3D::new(copy.extent.width, copy.extent.height, 1),
    })
}

pub(super) fn validate_color_clear(
    encoder: &CommandEncoder,
    image: &Image,
    layout: TextureLayout,
    color: [f32; 4],
    ranges: &[TextureSubresourceRange],
) -> Result<()> {
    if !image.belongs_to(&encoder.owner) {
        return Err(Error::Validation(
            "cleared image was created by a different Device".into(),
        ));
    }
    if !image.usage().contains(TextureUsages::COPY_DESTINATION)
        || !matches!(
            layout,
            TextureLayout::TransferDestination | TextureLayout::General
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
        if range.aspects != TextureAspects::COLOR
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

pub(super) fn validate_image_blit_resources(
    encoder: &CommandEncoder,
    source: &Image,
    source_layout: TextureLayout,
    destination: &Image,
    destination_layout: TextureLayout,
) -> Result<()> {
    validate_image_copy_resources(
        encoder,
        source,
        source_layout,
        destination,
        destination_layout,
    )?;
    if source.sample_count() != SampleCount::One || destination.sample_count() != SampleCount::One {
        return Err(Error::Validation(
            "image blits require single-sampled images".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_image_blit(
    source: &Image,
    destination: &Image,
    blit: ImageBlit,
) -> Result<()> {
    validate_blit_box(source, blit.source_subresource, blit.source_offsets)?;
    validate_blit_box(
        destination,
        blit.destination_subresource,
        blit.destination_offsets,
    )?;
    if blit.source_subresource.layer_count != blit.destination_subresource.layer_count
        || blit.source_subresource.aspects != blit.destination_subresource.aspects
    {
        return Err(Error::Validation(
            "image blit source and destination layers/aspects must match".into(),
        ));
    }
    Ok(())
}

fn validate_blit_box(
    image: &Image,
    subresource: TextureSubresourceLayers,
    offsets: [Origin3D; 2],
) -> Result<()> {
    if subresource.aspects != TextureAspects::COLOR
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
    let extent = Extent3D::new(
        (base.width >> mip).max(1),
        (base.height >> mip).max(1),
        (base.depth_or_layers >> mip).max(1),
    );
    for offset in offsets {
        if offset.x < 0
            || offset.y < 0
            || offset.z < 0
            || offset.x as u32 > extent.width
            || offset.y as u32 > extent.height
            || offset.z as u32 > extent.depth_or_layers
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

pub(super) fn validate_image_copy_resources(
    encoder: &CommandEncoder,
    source: &Image,
    source_layout: TextureLayout,
    destination: &Image,
    destination_layout: TextureLayout,
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
    if !source.usage().contains(TextureUsages::COPY_SOURCE)
        || !destination
            .usage()
            .contains(TextureUsages::COPY_DESTINATION)
    {
        return Err(Error::Validation(
            "image copy requires TRANSFER_SRC and TRANSFER_DST usage".into(),
        ));
    }
    if !matches!(
        source_layout,
        TextureLayout::TransferSource | TextureLayout::General
    ) || !matches!(
        destination_layout,
        TextureLayout::TransferDestination | TextureLayout::General
    ) {
        return Err(Error::Validation(
            "image copy layouts must be transfer-optimal or general".into(),
        ));
    }
    if source.format() != destination.format()
        || source.sample_count() != destination.sample_count()
        || source.dimension() != destination.dimension()
    {
        return Err(Error::Validation(
            "standard image copies require equal formats, samples, and image types".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_image_copy(
    source: &Image,
    destination: &Image,
    copy: ImageCopy,
) -> Result<()> {
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
        || copy.source_subresource.aspects != copy.destination_subresource.aspects
    {
        return Err(Error::Validation(
            "image copy source and destination layers/aspects must match".into(),
        ));
    }
    Ok(())
}

fn validate_copy_subresource(
    image: &Image,
    subresource: TextureSubresourceLayers,
    offset: Origin3D,
    extent: Extent3D,
) -> Result<()> {
    if subresource.aspects.is_empty()
        || subresource.layer_count == 0
        || subresource.mip_level >= image.mip_levels()
        || subresource
            .base_array_layer
            .checked_add(subresource.layer_count)
            .is_none_or(|end| end > image.array_layers())
        || extent.width == 0
        || extent.height == 0
        || extent.depth_or_layers == 0
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
    let mip_extent = Extent3D::new(
        (base.width >> mip).max(1),
        (base.height >> mip).max(1),
        (base.depth_or_layers >> mip).max(1),
    );
    if (offset.x as u32)
        .checked_add(extent.width)
        .is_none_or(|end| end > mip_extent.width)
        || (offset.y as u32)
            .checked_add(extent.height)
            .is_none_or(|end| end > mip_extent.height)
        || (offset.z as u32)
            .checked_add(extent.depth_or_layers)
            .is_none_or(|end| end > mip_extent.depth_or_layers)
    {
        return Err(Error::Validation(
            "image copy region exceeds the selected mip extent".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_buffer_copy(
    source: &Buffer,
    destination: &Buffer,
    copy: BufferCopy,
) -> Result<()> {
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

pub(super) fn validate_buffer_image_resources(
    encoder: &CommandEncoder,
    source: &Buffer,
    image: &Image,
    layout: TextureLayout,
) -> Result<()> {
    if !source.belongs_to(&encoder.owner) || !image.belongs_to(&encoder.owner) {
        return Err(Error::Validation(
            "buffer/image copy resources were created by a different Device".into(),
        ));
    }
    if !source.usage().contains(BufferUsages::COPY_SOURCE)
        || !image.usage().contains(TextureUsages::COPY_DESTINATION)
    {
        return Err(Error::Validation(
            "buffer-to-image copy requires TRANSFER_SRC and TRANSFER_DST usage".into(),
        ));
    }
    if image.sample_count() != SampleCount::One {
        return Err(Error::Validation(
            "buffer-to-image copies require a single-sampled image".into(),
        ));
    }
    if !matches!(
        layout,
        TextureLayout::TransferDestination | TextureLayout::General
    ) {
        return Err(Error::Validation(
            "buffer-to-image copy layout must be TRANSFER_DST_OPTIMAL or GENERAL".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_buffer_image_copy(
    source: &Buffer,
    image: &Image,
    copy: BufferImageCopy,
) -> Result<()> {
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
    if copy.image_extent.width == 0
        || copy.image_extent.height == 0
        || copy.image_extent.depth_or_layers == 0
    {
        return Err(Error::Validation(
            "buffer-to-image extent must be non-empty".into(),
        ));
    }
    if copy.image_subresource.aspects.is_empty()
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
    let mip_extent = Extent3D::new(
        (extent.width >> mip).max(1),
        (extent.height >> mip).max(1),
        (extent.depth_or_layers >> mip).max(1),
    );
    let end_x = (copy.image_offset.x as u32).checked_add(copy.image_extent.width);
    let end_y = (copy.image_offset.y as u32).checked_add(copy.image_extent.height);
    let end_z = (copy.image_offset.z as u32).checked_add(copy.image_extent.depth_or_layers);
    if end_x.is_none_or(|end| end > mip_extent.width)
        || end_y.is_none_or(|end| end > mip_extent.height)
        || end_z.is_none_or(|end| end > mip_extent.depth_or_layers)
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
