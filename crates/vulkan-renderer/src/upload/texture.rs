use vulkanalia::vk;

use crate::{BufferImageCopy, Error, Image, Result};

/// Texel-block geometry and byte size for an image format.
///
/// Uncompressed formats use a 1x1 block. Compressed formats use their native
/// block extent. Explicit construction supports formats not yet listed by
/// [`TexelBlockLayout::for_format`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TexelBlockLayout {
    pub width: u32,
    pub height: u32,
    pub bytes: u32,
}

impl TexelBlockLayout {
    pub const R8: Self = Self::new(1, 1, 1);
    pub const RGBA8: Self = Self::new(1, 1, 4);
    pub const BC1: Self = Self::new(4, 4, 8);
    pub const BC2_TO_BC7: Self = Self::new(4, 4, 16);

    pub const fn new(width: u32, height: u32, bytes: u32) -> Self {
        Self {
            width,
            height,
            bytes,
        }
    }

    /// Returns standard block metadata for common color formats used by UI,
    /// scene, and texture-compression pipelines.
    pub fn for_format(format: vk::Format) -> Option<Self> {
        if matches!(
            format,
            vk::Format::R8_UNORM
                | vk::Format::R8_SNORM
                | vk::Format::R8_USCALED
                | vk::Format::R8_SSCALED
                | vk::Format::R8_UINT
                | vk::Format::R8_SINT
                | vk::Format::R8_SRGB
        ) {
            return Some(Self::R8);
        }
        if matches!(
            format,
            vk::Format::R8G8B8A8_UNORM
                | vk::Format::R8G8B8A8_SNORM
                | vk::Format::R8G8B8A8_USCALED
                | vk::Format::R8G8B8A8_SSCALED
                | vk::Format::R8G8B8A8_UINT
                | vk::Format::R8G8B8A8_SINT
                | vk::Format::R8G8B8A8_SRGB
                | vk::Format::B8G8R8A8_UNORM
                | vk::Format::B8G8R8A8_SNORM
                | vk::Format::B8G8R8A8_USCALED
                | vk::Format::B8G8R8A8_SSCALED
                | vk::Format::B8G8R8A8_UINT
                | vk::Format::B8G8R8A8_SINT
                | vk::Format::B8G8R8A8_SRGB
                | vk::Format::A8B8G8R8_UNORM_PACK32
                | vk::Format::A8B8G8R8_SNORM_PACK32
                | vk::Format::A8B8G8R8_USCALED_PACK32
                | vk::Format::A8B8G8R8_SSCALED_PACK32
                | vk::Format::A8B8G8R8_UINT_PACK32
                | vk::Format::A8B8G8R8_SINT_PACK32
                | vk::Format::A8B8G8R8_SRGB_PACK32
        ) {
            return Some(Self::RGBA8);
        }
        if matches!(
            format,
            vk::Format::BC1_RGB_UNORM_BLOCK
                | vk::Format::BC1_RGB_SRGB_BLOCK
                | vk::Format::BC1_RGBA_UNORM_BLOCK
                | vk::Format::BC1_RGBA_SRGB_BLOCK
                | vk::Format::BC4_UNORM_BLOCK
                | vk::Format::BC4_SNORM_BLOCK
        ) {
            return Some(Self::BC1);
        }
        if matches!(
            format,
            vk::Format::BC2_UNORM_BLOCK
                | vk::Format::BC2_SRGB_BLOCK
                | vk::Format::BC3_UNORM_BLOCK
                | vk::Format::BC3_SRGB_BLOCK
                | vk::Format::BC5_UNORM_BLOCK
                | vk::Format::BC5_SNORM_BLOCK
                | vk::Format::BC6H_UFLOAT_BLOCK
                | vk::Format::BC6H_SFLOAT_BLOCK
                | vk::Format::BC7_UNORM_BLOCK
                | vk::Format::BC7_SRGB_BLOCK
        ) {
            return Some(Self::BC2_TO_BC7);
        }
        None
    }

    fn validate(self) -> Result<()> {
        if self.width == 0 || self.height == 0 || self.bytes == 0 {
            return Err(Error::Validation(
                "texel block dimensions and byte size must be non-zero".into(),
            ));
        }
        Ok(())
    }
}

/// CPU byte layout for one image upload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageDataLayout {
    /// Distance in bytes between adjacent texel-block rows.
    pub bytes_per_row: u64,
    /// Distance between array layers or depth slices, expressed in texel rows.
    pub rows_per_image: u32,
}

impl ImageDataLayout {
    /// Builds a tightly packed layout for `extent` and `block`.
    pub fn tightly_packed(extent: vk::Extent3D, block: TexelBlockLayout) -> Result<Self> {
        block.validate()?;
        let blocks_wide = div_ceil(u64::from(extent.width), u64::from(block.width));
        let block_rows = div_ceil(u64::from(extent.height), u64::from(block.height));
        let bytes_per_row = blocks_wide
            .checked_mul(u64::from(block.bytes))
            .ok_or_else(|| Error::Validation("image row byte size overflows".into()))?;
        let rows_per_image = block_rows
            .checked_mul(u64::from(block.height))
            .and_then(|rows| u32::try_from(rows).ok())
            .ok_or_else(|| Error::Validation("image row count overflows".into()))?;
        Ok(Self {
            bytes_per_row,
            rows_per_image,
        })
    }
}

/// Strict description of one CPU-to-image copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageUpload {
    pub data_layout: ImageDataLayout,
    pub texel_block: TexelBlockLayout,
    pub image_subresource: vk::ImageSubresourceLayers,
    pub image_offset: vk::Offset3D,
    pub image_extent: vk::Extent3D,
}

pub(crate) struct ValidatedImageUpload {
    pub copy: BufferImageCopy,
    pub required_bytes: u64,
}

impl ImageUpload {
    pub(crate) fn validate(self, image: &Image, data_len: usize) -> Result<ValidatedImageUpload> {
        self.texel_block.validate()?;
        if TexelBlockLayout::for_format(image.format())
            .is_some_and(|expected| expected != self.texel_block)
        {
            return Err(Error::Validation(
                "texel block layout does not match the image format".into(),
            ));
        }
        validate_subresource_and_extent(image, self)?;
        let block = self.texel_block;
        let row_blocks = self
            .data_layout
            .bytes_per_row
            .checked_div(u64::from(block.bytes))
            .filter(|_| {
                self.data_layout
                    .bytes_per_row
                    .is_multiple_of(u64::from(block.bytes))
            })
            .ok_or_else(|| {
                Error::Validation(
                    "image bytes_per_row is not a whole number of texel blocks".into(),
                )
            })?;
        let row_texels = row_blocks
            .checked_mul(u64::from(block.width))
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| Error::Validation("image buffer row length overflows u32".into()))?;
        if row_texels < self.image_extent.width {
            return Err(Error::Validation(
                "image bytes_per_row is smaller than the copied width".into(),
            ));
        }
        if self.data_layout.rows_per_image < self.image_extent.height
            || !self.data_layout.rows_per_image.is_multiple_of(block.height)
        {
            return Err(Error::Validation(
                "image rows_per_image is too small or not block aligned".into(),
            ));
        }
        let required_bytes = required_footprint(self)?;
        let available = u64::try_from(data_len)
            .map_err(|_| Error::Validation("image upload data length exceeds u64".into()))?;
        if available < required_bytes {
            return Err(Error::Validation(format!(
                "image upload requires {required_bytes} bytes but received {available}"
            )));
        }
        Ok(ValidatedImageUpload {
            copy: BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: row_texels,
                buffer_image_height: self.data_layout.rows_per_image,
                image_subresource: self.image_subresource,
                image_offset: self.image_offset,
                image_extent: self.image_extent,
            },
            required_bytes,
        })
    }
}

fn validate_subresource_and_extent(image: &Image, upload: ImageUpload) -> Result<()> {
    let block = upload.texel_block;
    if upload.image_subresource.aspect_mask.is_empty()
        || upload.image_subresource.layer_count == 0
        || upload
            .image_subresource
            .base_array_layer
            .checked_add(upload.image_subresource.layer_count)
            .is_none_or(|end| end > image.array_layers())
    {
        return Err(Error::Validation(
            "image upload subresource layers are invalid".into(),
        ));
    }
    if upload.image_extent.width == 0
        || upload.image_extent.height == 0
        || upload.image_extent.depth == 0
    {
        return Err(Error::Validation(
            "image upload extent must be non-empty".into(),
        ));
    }
    if upload.image_offset.x < 0 || upload.image_offset.y < 0 || upload.image_offset.z < 0 {
        return Err(Error::Validation(
            "image upload offsets must be non-negative".into(),
        ));
    }
    if !(upload.image_offset.x as u32).is_multiple_of(block.width)
        || !(upload.image_offset.y as u32).is_multiple_of(block.height)
    {
        return Err(Error::Validation(
            "image upload offsets must align to the texel block extent".into(),
        ));
    }
    let mip = upload.image_subresource.mip_level;
    if mip >= image.mip_levels() {
        return Err(Error::Validation(
            "image upload mip level is invalid".into(),
        ));
    }
    let extent = image.extent();
    let mip_extent = vk::Extent3D {
        width: (extent.width >> mip).max(1),
        height: (extent.height >> mip).max(1),
        depth: (extent.depth >> mip).max(1),
    };
    let end_x = (upload.image_offset.x as u32)
        .checked_add(upload.image_extent.width)
        .ok_or_else(|| Error::Validation("image upload X range overflows".into()))?;
    let end_y = (upload.image_offset.y as u32)
        .checked_add(upload.image_extent.height)
        .ok_or_else(|| Error::Validation("image upload Y range overflows".into()))?;
    let end_z = (upload.image_offset.z as u32)
        .checked_add(upload.image_extent.depth)
        .ok_or_else(|| Error::Validation("image upload Z range overflows".into()))?;
    if end_x > mip_extent.width || end_y > mip_extent.height || end_z > mip_extent.depth {
        return Err(Error::Validation(
            "image upload exceeds the selected mip extent".into(),
        ));
    }
    if end_x != mip_extent.width && !upload.image_extent.width.is_multiple_of(block.width) {
        return Err(Error::Validation(
            "compressed upload width must be block aligned except at the mip edge".into(),
        ));
    }
    if end_y != mip_extent.height && !upload.image_extent.height.is_multiple_of(block.height) {
        return Err(Error::Validation(
            "compressed upload height must be block aligned except at the mip edge".into(),
        ));
    }
    match image.image_type() {
        vk::ImageType::_3D
            if upload.image_subresource.base_array_layer != 0
                || upload.image_subresource.layer_count != 1 =>
        {
            return Err(Error::Validation(
                "3D image uploads require base array layer zero and one layer".into(),
            ));
        }
        vk::ImageType::_1D if upload.image_extent.height != 1 => {
            return Err(Error::Validation(
                "1D image uploads require a height of one".into(),
            ));
        }
        vk::ImageType::_1D | vk::ImageType::_2D if upload.image_extent.depth != 1 => {
            return Err(Error::Validation(
                "1D and 2D image uploads require a depth of one".into(),
            ));
        }
        _ => {}
    }
    Ok(())
}

fn required_footprint(upload: ImageUpload) -> Result<u64> {
    let block = upload.texel_block;
    let width_blocks = div_ceil(u64::from(upload.image_extent.width), u64::from(block.width));
    let height_blocks = div_ceil(
        u64::from(upload.image_extent.height),
        u64::from(block.height),
    );
    let row_bytes = width_blocks
        .checked_mul(u64::from(block.bytes))
        .ok_or_else(|| Error::Validation("image row footprint overflows".into()))?;
    let block_rows_per_image = u64::from(upload.data_layout.rows_per_image / block.height);
    let bytes_per_image = upload
        .data_layout
        .bytes_per_row
        .checked_mul(block_rows_per_image)
        .ok_or_else(|| Error::Validation("image slice footprint overflows".into()))?;
    let images = u64::from(
        upload
            .image_extent
            .depth
            .max(upload.image_subresource.layer_count),
    );
    let preceding_images = images
        .checked_sub(1)
        .and_then(|count| count.checked_mul(bytes_per_image))
        .ok_or_else(|| Error::Validation("image array footprint overflows".into()))?;
    let preceding_rows = height_blocks
        .checked_sub(1)
        .and_then(|count| count.checked_mul(upload.data_layout.bytes_per_row))
        .ok_or_else(|| Error::Validation("image row footprint overflows".into()))?;
    preceding_images
        .checked_add(preceding_rows)
        .and_then(|size| size.checked_add(row_bytes))
        .ok_or_else(|| Error::Validation("image upload footprint overflows".into()))
}

const fn div_ceil(value: u64, divisor: u64) -> u64 {
    value.div_ceil(divisor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r8_atlas_layout_is_tightly_packed() {
        let extent = vk::Extent3D {
            width: 37,
            height: 19,
            depth: 1,
        };
        assert_eq!(
            ImageDataLayout::tightly_packed(extent, TexelBlockLayout::R8).unwrap(),
            ImageDataLayout {
                bytes_per_row: 37,
                rows_per_image: 19,
            }
        );
    }

    #[test]
    fn rgba8_and_bc_footprints_exclude_trailing_row_padding() {
        let rgba = ImageUpload {
            data_layout: ImageDataLayout {
                bytes_per_row: 256,
                rows_per_image: 2,
            },
            texel_block: TexelBlockLayout::RGBA8,
            image_subresource: layers(1),
            image_offset: vk::Offset3D::default(),
            image_extent: vk::Extent3D {
                width: 3,
                height: 2,
                depth: 1,
            },
        };
        assert_eq!(required_footprint(rgba).unwrap(), 268);

        let bc = ImageUpload {
            data_layout: ImageDataLayout {
                bytes_per_row: 16,
                rows_per_image: 8,
            },
            texel_block: TexelBlockLayout::BC1,
            image_subresource: layers(1),
            image_offset: vk::Offset3D::default(),
            image_extent: vk::Extent3D {
                width: 7,
                height: 7,
                depth: 1,
            },
        };
        assert_eq!(required_footprint(bc).unwrap(), 32);
    }

    #[test]
    fn common_ui_formats_have_standard_metadata() {
        assert_eq!(
            TexelBlockLayout::for_format(vk::Format::R8_UNORM),
            Some(TexelBlockLayout::R8)
        );
        assert_eq!(
            TexelBlockLayout::for_format(vk::Format::R8G8B8A8_UNORM),
            Some(TexelBlockLayout::RGBA8)
        );
        assert_eq!(
            TexelBlockLayout::for_format(vk::Format::B8G8R8A8_UNORM),
            Some(TexelBlockLayout::RGBA8)
        );
    }

    fn layers(count: u32) -> vk::ImageSubresourceLayers {
        vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: count,
        }
    }
}
