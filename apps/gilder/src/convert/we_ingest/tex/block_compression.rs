//! Convert decoded WE texture levels into GPU-native BC payloads.
//!
//! Color textures become BC7, one-channel masks become BC4, and two-channel
//! flow/normal data becomes BC5. Numeric LUT/phase resources stay lossless.

use std::path::Path;

use intel_tex_2::{RSurface, RgSurface, RgbaSurface, bc4, bc5, bc7};

use crate::engine::scene::SceneTextureFormat;

use super::{TexParseError, TexUpload, TexUploadMip};

pub(in crate::convert::we_ingest) fn transcode_texture_upload(
    path: &str,
    upload: TexUpload,
) -> Result<TexUpload, TexParseError> {
    let target_format = target_format(path, &upload);
    if target_format == upload.format {
        return Ok(upload);
    }

    let mut payload = Vec::new();
    let mut mips = Vec::with_capacity(upload.mips.len());
    for mip in &upload.mips {
        let source = mip_payload(&upload, mip)?;
        // BC images and BufferImageCopy extents must be multiples of the 4×4
        // block size. Compressors already pad source texels; the GPU storage
        // extent and each mip region must report that same padded size.
        // Logical width/height stay on metadata for g_TextureNResolution.zw UV.
        let (blocks, storage_width, storage_height) = match (upload.format, target_format) {
            (SceneTextureFormat::Rgba8Unorm, SceneTextureFormat::Bc7UnormBlock) => {
                compress_bc7(source, mip.width, mip.height)?
            }
            (SceneTextureFormat::R8Unorm, SceneTextureFormat::Bc4UnormBlock) => {
                compress_bc4(source, mip.width, mip.height)?
            }
            (SceneTextureFormat::Rg8Unorm, SceneTextureFormat::Bc5UnormBlock) => {
                compress_bc5(source, mip.width, mip.height)?
            }
            _ => {
                return Err(TexParseError::BlockCompression(format!(
                    "unsupported conversion {:?} -> {:?}",
                    upload.format, target_format
                )));
            }
        };
        let payload_offset = payload.len() as u64;
        payload.extend_from_slice(&blocks);
        mips.push(TexUploadMip {
            width: storage_width,
            height: storage_height,
            payload_offset,
            payload_len: blocks.len() as u64,
        });
    }

    let mut metadata = upload.metadata;
    if let Some(first) = mips.first() {
        metadata.storage_width = first.width;
        metadata.storage_height = first.height;
    }

    Ok(TexUpload {
        format: target_format,
        metadata,
        mips,
        payload,
    })
}

fn target_format(path: &str, upload: &TexUpload) -> SceneTextureFormat {
    if preserves_numeric_values(path, upload.metadata.payload_format) {
        return upload.format;
    }
    match upload.format {
        SceneTextureFormat::Rgba8Unorm => SceneTextureFormat::Bc7UnormBlock,
        SceneTextureFormat::R8Unorm => SceneTextureFormat::Bc4UnormBlock,
        SceneTextureFormat::Rg8Unorm => SceneTextureFormat::Bc5UnormBlock,
        format => format,
    }
}

fn preserves_numeric_values(path: &str, payload_format: u32) -> bool {
    if payload_format & 0x7ffff == 0x42 {
        return true;
    }
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let stem = Path::new(&normalized)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    is_numeric_utility_texture(&normalized)
        || normalized.contains("/lut/")
        || stem.contains("phase")
}

fn is_numeric_utility_texture(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized.starts_with("util/") || normalized.contains("/util/")
}

fn mip_payload<'a>(upload: &'a TexUpload, mip: &TexUploadMip) -> Result<&'a [u8], TexParseError> {
    let start = usize::try_from(mip.payload_offset).map_err(|_| TexParseError::OffsetOverflow)?;
    let len = usize::try_from(mip.payload_len).map_err(|_| TexParseError::OffsetOverflow)?;
    let end = start
        .checked_add(len)
        .ok_or(TexParseError::OffsetOverflow)?;
    upload
        .payload
        .get(start..end)
        .ok_or(TexParseError::InvalidPayload(
            "decoded mip range exceeds texture payload",
        ))
}

fn compress_bc7(
    source: &[u8],
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, u32, u32), TexParseError> {
    let (pixels, padded_width, padded_height) = pad_channels::<4>(source, width, height)?;
    let surface = RgbaSurface {
        data: &pixels,
        width: padded_width,
        height: padded_height,
        stride: padded_width * 4,
    };
    Ok((
        bc7::compress_blocks(&bc7::alpha_slow_settings(), &surface),
        padded_width,
        padded_height,
    ))
}

fn compress_bc4(
    source: &[u8],
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, u32, u32), TexParseError> {
    let (pixels, padded_width, padded_height) = pad_channels::<1>(source, width, height)?;
    Ok((
        bc4::compress_blocks(&RSurface {
            data: &pixels,
            width: padded_width,
            height: padded_height,
            stride: padded_width,
        }),
        padded_width,
        padded_height,
    ))
}

fn compress_bc5(
    source: &[u8],
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, u32, u32), TexParseError> {
    let (pixels, padded_width, padded_height) = pad_channels::<2>(source, width, height)?;
    Ok((
        bc5::compress_blocks(&RgSurface {
            data: &pixels,
            width: padded_width,
            height: padded_height,
            stride: padded_width * 2,
        }),
        padded_width,
        padded_height,
    ))
}

fn pad_channels<const CHANNELS: usize>(
    source: &[u8],
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, u32, u32), TexParseError> {
    if width == 0 || height == 0 {
        return Err(TexParseError::BlockCompression(
            "zero-sized texture level".to_owned(),
        ));
    }
    let expected = width as usize * height as usize * CHANNELS;
    if source.len() != expected {
        return Err(TexParseError::BlockCompression(format!(
            "{}x{}x{CHANNELS} source has {} bytes, expected {expected}",
            width,
            height,
            source.len()
        )));
    }
    let padded_width = width.next_multiple_of(4);
    let padded_height = height.next_multiple_of(4);
    let mut padded = vec![0; padded_width as usize * padded_height as usize * CHANNELS];
    for y in 0..padded_height {
        let source_y = y.min(height - 1);
        for x in 0..padded_width {
            let source_x = x.min(width - 1);
            let source_offset = (source_y as usize * width as usize + source_x as usize) * CHANNELS;
            let target_offset = (y as usize * padded_width as usize + x as usize) * CHANNELS;
            padded[target_offset..target_offset + CHANNELS]
                .copy_from_slice(&source[source_offset..source_offset + CHANNELS]);
        }
    }
    Ok((padded, padded_width, padded_height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_formats_choose_semantic_bc_targets() {
        let rgba = upload(SceneTextureFormat::Rgba8Unorm, 4, 4, 4 * 4 * 4);
        let mask = upload(SceneTextureFormat::R8Unorm, 4, 4, 4 * 4);
        let flow = upload(SceneTextureFormat::Rg8Unorm, 4, 4, 4 * 4 * 2);

        assert_eq!(
            transcode_texture_upload("materials/color.tex", rgba)
                .unwrap()
                .format,
            SceneTextureFormat::Bc7UnormBlock
        );
        assert_eq!(
            transcode_texture_upload("materials/masks/mask.tex", mask)
                .unwrap()
                .format,
            SceneTextureFormat::Bc4UnormBlock
        );
        assert_eq!(
            transcode_texture_upload("materials/flow.tex", flow)
                .unwrap()
                .format,
            SceneTextureFormat::Bc5UnormBlock
        );
    }

    #[test]
    fn four_channel_numeric_effect_textures_preserve_all_authored_channels() {
        let phase = upload(SceneTextureFormat::Rgba8Unorm, 4, 4, 4 * 4 * 4);
        let displacement = upload(SceneTextureFormat::Rgba8Unorm, 4, 4, 4 * 4 * 4);
        assert_eq!(
            transcode_texture_upload("materials/effects/waterflowphase.tex", phase)
                .unwrap()
                .format,
            SceneTextureFormat::Rgba8Unorm
        );
        let displacement =
            transcode_texture_upload("assets/materials/util/perlin_256.tex", displacement).unwrap();
        assert_eq!(displacement.format, SceneTextureFormat::Rgba8Unorm);
        assert_eq!(displacement.payload, vec![127; 4 * 4 * 4]);
    }

    #[test]
    fn bc_transcode_pads_storage_extent_and_keeps_logical_size() {
        let rgba = upload(SceneTextureFormat::Rgba8Unorm, 5, 3, 5 * 3 * 4);
        let bc7 = transcode_texture_upload("materials/color.tex", rgba).unwrap();
        assert_eq!(bc7.format, SceneTextureFormat::Bc7UnormBlock);
        assert_eq!(bc7.metadata.width, 5);
        assert_eq!(bc7.metadata.height, 3);
        assert_eq!(bc7.metadata.storage_width, 8);
        assert_eq!(bc7.metadata.storage_height, 4);
        assert_eq!(bc7.mips.len(), 1);
        assert_eq!(bc7.mips[0].width, 8);
        assert_eq!(bc7.mips[0].height, 4);
        assert_eq!(bc7.mips[0].payload_len, 16 * 2);
        assert_eq!(bc7.payload.len() as u64, bc7.mips[0].payload_len);

        let mask = upload(SceneTextureFormat::R8Unorm, 5, 3, 5 * 3);
        let bc4 = transcode_texture_upload("materials/masks/mask.tex", mask).unwrap();
        assert_eq!(bc4.metadata.width, 5);
        assert_eq!(bc4.metadata.height, 3);
        assert_eq!(bc4.metadata.storage_width, 8);
        assert_eq!(bc4.metadata.storage_height, 4);
        assert_eq!(bc4.mips[0].width, 8);
        assert_eq!(bc4.mips[0].height, 4);
        assert_eq!(bc4.mips[0].payload_len, 8 * 2);
    }

    fn upload(format: SceneTextureFormat, width: u32, height: u32, bytes: usize) -> TexUpload {
        TexUpload {
            metadata: super::super::TexMetadata {
                texv_tag: "TEXV0005".to_owned(),
                texi_tag: "TEXI0001".to_owned(),
                texb_tag: "TEXB0004".to_owned(),
                runtime_format: 0,
                payload_format: 0,
                sampler_seed: 0,
                sampler_filter: crate::engine::scene::SceneTextureSamplerFilter::Anisotropic8,
                sampler_address_mode: crate::engine::scene::SceneTextureSamplerAddressMode::Repeat,
                width,
                height,
                storage_width: width,
                storage_height: height,
                mip_count: 1,
            },
            format,
            mips: vec![TexUploadMip {
                width,
                height,
                payload_offset: 0,
                payload_len: bytes as u64,
            }],
            payload: vec![127; bytes],
        }
    }
}
