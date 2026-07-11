//! Wallpaper Engine `.tex` parsing and GPU upload lowering.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/exe/texture-and-format.md`
//! - `reverse-engineered/tools/parse_tex.py`
//! - `reverse-engineered/tools/audit_texb_legacy_payloads.py`

use std::fmt;

use crate::engine::scene::SceneTextureFormat;

pub(super) mod block_compression;
mod container;
mod decoded_image;
mod lz4_block;

use container::{TexEncodedLevel, parse_tex_container};
use decoded_image::decode_image_level;
use lz4_block::decode_lz4_block;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexMetadata {
    pub texv_tag: String,
    pub texi_tag: String,
    pub texb_tag: String,
    pub runtime_format: u32,
    pub payload_format: u32,
    pub sampler_flags: u32,
    pub width: u32,
    pub height: u32,
    pub storage_width: u32,
    pub storage_height: u32,
    pub mip_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexUpload {
    pub metadata: TexMetadata,
    pub format: SceneTextureFormat,
    pub mips: Vec<TexUploadMip>,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TexUploadMip {
    pub width: u32,
    pub height: u32,
    pub payload_offset: u64,
    pub payload_len: u64,
}

pub fn parse_tex_metadata(data: &[u8]) -> Result<TexMetadata, TexParseError> {
    Ok(parse_tex_container(data)?.metadata)
}

pub fn decode_tex_upload(data: &[u8]) -> Result<TexUpload, TexParseError> {
    let container = parse_tex_container(data)?;
    let mut payload = Vec::new();
    let mut mips = Vec::with_capacity(container.levels.len());
    for level in container.levels {
        let prepared = prepare_level(container.metadata.runtime_format, &level)?;
        let offset = payload.len() as u64;
        payload.extend_from_slice(&prepared.bytes);
        mips.push(TexUploadMip {
            width: prepared.width,
            height: prepared.height,
            payload_offset: offset,
            payload_len: prepared.bytes.len() as u64,
        });
    }
    let mut metadata = container.metadata;
    let first = mips
        .first()
        .ok_or(TexParseError::InvalidPayload("texture has no mip levels"))?;
    metadata.storage_width = first.width;
    metadata.storage_height = first.height;
    metadata.mip_count = mips.len() as u32;
    Ok(TexUpload {
        format: scene_texture_format(metadata.runtime_format)?,
        metadata,
        mips,
        payload,
    })
}

struct DecodedLevel {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

fn prepare_level(
    runtime_format: u32,
    level: &TexEncodedLevel<'_>,
) -> Result<DecodedLevel, TexParseError> {
    let decoded = match level.compression {
        0 => level.payload.to_vec(),
        1 => decode_lz4_block(level.payload, level.decoded_size as usize)?,
        value => return Err(TexParseError::UnsupportedCompression(value)),
    };
    if is_image_payload(&decoded) {
        return decode_image_level(runtime_format, &decoded);
    }
    let expected = expected_level_bytes(runtime_format, level.width, level.height)?;
    if decoded.len() != expected {
        return Err(TexParseError::InvalidLevelSize {
            runtime_format,
            width: level.width,
            height: level.height,
            expected,
            actual: decoded.len(),
        });
    }
    Ok(DecodedLevel {
        width: level.width,
        height: level.height,
        bytes: decoded,
    })
}

fn is_image_payload(payload: &[u8]) -> bool {
    payload.starts_with(b"\x89PNG\r\n\x1a\n")
        || payload.starts_with(b"\xff\xd8")
        || payload.starts_with(b"RIFF") && payload.get(8..12) == Some(b"WEBP")
}

fn expected_level_bytes(
    runtime_format: u32,
    width: u32,
    height: u32,
) -> Result<usize, TexParseError> {
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or(TexParseError::OffsetOverflow)?;
    match runtime_format {
        0 | 1 | 2 | 3 | 5 => pixels.checked_mul(4).ok_or(TexParseError::OffsetOverflow),
        8 => pixels.checked_mul(2).ok_or(TexParseError::OffsetOverflow),
        9 => Ok(pixels),
        4 | 6 | 12 => block_compressed_bytes(width, height, 16),
        7 => block_compressed_bytes(width, height, 8),
        value => Err(TexParseError::UnsupportedRuntimeFormat(value)),
    }
}

fn block_compressed_bytes(
    width: u32,
    height: u32,
    block_bytes: usize,
) -> Result<usize, TexParseError> {
    let blocks_wide = width.max(1).div_ceil(4) as usize;
    let blocks_high = height.max(1).div_ceil(4) as usize;
    blocks_wide
        .checked_mul(blocks_high)
        .and_then(|blocks| blocks.checked_mul(block_bytes))
        .ok_or(TexParseError::OffsetOverflow)
}

fn scene_texture_format(runtime_format: u32) -> Result<SceneTextureFormat, TexParseError> {
    match runtime_format {
        0 | 1 | 2 | 3 | 5 => Ok(SceneTextureFormat::Rgba8Unorm),
        4 => Ok(SceneTextureFormat::Bc3UnormBlock),
        6 => Ok(SceneTextureFormat::Bc2UnormBlock),
        7 => Ok(SceneTextureFormat::Bc1RgbaUnormBlock),
        8 => Ok(SceneTextureFormat::Rg8Unorm),
        9 => Ok(SceneTextureFormat::R8Unorm),
        12 => Ok(SceneTextureFormat::Bc7UnormBlock),
        value => Err(TexParseError::UnsupportedRuntimeFormat(value)),
    }
}

#[derive(Debug)]
pub enum TexParseError {
    Truncated(&'static str),
    OffsetOverflow,
    InvalidTag {
        field: &'static str,
        value: String,
    },
    MissingTexb,
    UnsupportedTexb(String),
    InvalidPayload(&'static str),
    UnsupportedCompression(u32),
    UnsupportedRuntimeFormat(u32),
    InvalidLevelSize {
        runtime_format: u32,
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    Lz4(String),
    Image(String),
    BlockCompression(String),
}

impl fmt::Display for TexParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(field) => write!(f, "truncated .tex while reading {field}"),
            Self::OffsetOverflow => f.write_str(".tex offset overflow"),
            Self::InvalidTag { field, value } => {
                write!(f, "invalid .tex {field} tag {value:?}")
            }
            Self::MissingTexb => f.write_str(".tex has no TEXB payload tag"),
            Self::UnsupportedTexb(tag) => write!(f, "unsupported .tex payload tag {tag}"),
            Self::InvalidPayload(message) => write!(f, "invalid .tex payload: {message}"),
            Self::UnsupportedCompression(value) => {
                write!(f, "unsupported .tex compression mode {value}")
            }
            Self::UnsupportedRuntimeFormat(value) => {
                write!(f, "unsupported .tex runtime format {value}")
            }
            Self::InvalidLevelSize {
                runtime_format,
                width,
                height,
                expected,
                actual,
            } => write!(
                f,
                ".tex runtime format {runtime_format} level {width}x{height} has {actual} bytes, expected {expected}"
            ),
            Self::Lz4(message) => write!(f, "invalid .tex LZ4 block: {message}"),
            Self::Image(message) => write!(f, "invalid .tex image level: {message}"),
            Self::BlockCompression(message) => {
                write!(f, ".tex block compression failed: {message}")
            }
        }
    }
}

impl std::error::Error for TexParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_texi_fields_and_texb0004_raw_level() {
        let pixels = [1, 2, 3, 4, 5, 6, 7, 8];
        let bytes = texb0004(8, 2, 2, 2, 1, &[(0, pixels.as_slice())]);

        let upload = decode_tex_upload(&bytes).expect("tex upload");

        assert_eq!(upload.metadata.runtime_format, 8);
        assert_eq!(upload.metadata.payload_format, 2);
        assert_eq!(upload.metadata.sampler_flags, 10);
        assert_eq!(upload.metadata.mip_count, 1);
        assert_eq!(upload.mips[0].payload_len, 8);
        assert_eq!(upload.payload, pixels);
    }

    #[test]
    fn texb0004_uses_authored_level_count_instead_of_texi_tail_byte() {
        let first = [255; 4 * 4 * 4];
        let second = [127; 2 * 2 * 4];
        let bytes = texb0004(
            0,
            2,
            4,
            4,
            222,
            &[(0, first.as_slice()), (0, second.as_slice())],
        );

        let metadata = parse_tex_metadata(&bytes).expect("metadata");

        assert_eq!(metadata.mip_count, 2);
        assert_eq!(metadata.sampler_flags, 2);
    }

    #[test]
    fn texb0004_zero_encoding_with_distinct_sizes_is_lz4() {
        // One terminal LZ4 sequence containing four literal RGBA bytes.
        let encoded = [0x40, 10, 20, 30, 40];
        let bytes = texb0004_with_decoded_sizes(0, 0, 1, 1, 1, &[(0, 4, encoded.as_slice())]);

        let upload = decode_tex_upload(&bytes).expect("TEXB0004 LZ4 upload");

        assert_eq!(upload.payload, [10, 20, 30, 40]);
    }

    fn texb0004(
        runtime_format: u32,
        payload_format: u32,
        width: u32,
        height: u32,
        tail_byte: u8,
        levels: &[(u32, &[u8])],
    ) -> Vec<u8> {
        let levels = levels
            .iter()
            .map(|(compression, payload)| (*compression, payload.len() as u32, *payload))
            .collect::<Vec<_>>();
        texb0004_with_decoded_sizes(
            runtime_format,
            payload_format,
            width,
            height,
            tail_byte,
            &levels,
        )
    }

    fn texb0004_with_decoded_sizes(
        runtime_format: u32,
        payload_format: u32,
        width: u32,
        height: u32,
        tail_byte: u8,
        levels: &[(u32, u32, &[u8])],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TEXV0005\0TEXI0001\0");
        for value in [runtime_format, payload_format, width, height, width, height] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.push(tail_byte);
        bytes.push(0xff);
        bytes.extend_from_slice(b"TEXB0004\0");
        for value in [1, u32::MAX, 0, levels.len() as u32, width, height] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for (index, (compression, decoded_size, payload)) in levels.iter().enumerate() {
            if index != 0 {
                bytes.extend_from_slice(&(width >> index).max(1).to_le_bytes());
                bytes.extend_from_slice(&(height >> index).max(1).to_le_bytes());
            }
            bytes.extend_from_slice(&compression.to_le_bytes());
            bytes.extend_from_slice(&decoded_size.to_le_bytes());
            bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            bytes.extend_from_slice(payload);
        }
        bytes
    }
}
