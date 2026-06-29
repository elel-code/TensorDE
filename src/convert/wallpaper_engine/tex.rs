#[derive(Debug, Clone, Copy)]
pub(super) struct SceneWeModelFrameSize {
    pub(super) width: u32,
    pub(super) height: u32,
}

#[derive(Debug, Clone)]
pub(super) struct SceneWeTexImage {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneWeTexVideo<'a> {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) extension: &'static str,
    pub(super) payload: &'a [u8],
}

#[derive(Debug, Clone)]
pub(super) struct SceneWeTexBlockCompressedImage<'a> {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: SceneWeTexBlockCompressedFormat,
    pub(super) payload: Cow<'a, [u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneWeTexBlockCompressedFormat {
    Bc1RgbaUnormBlock,
    Bc3UnormBlock,
    Bc7UnormBlock,
}

impl SceneWeTexBlockCompressedFormat {
    pub(super) fn gtex_format(self) -> u32 {
        match self {
            Self::Bc1RgbaUnormBlock => {
                super::gtex::GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK
            }
            Self::Bc3UnormBlock => super::gtex::GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK,
            Self::Bc7UnormBlock => super::gtex::GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Bc1RgbaUnormBlock => "BC1_RGBA_UNORM_BLOCK",
            Self::Bc3UnormBlock => "BC3_UNORM_BLOCK",
            Self::Bc7UnormBlock => "BC7_UNORM_BLOCK",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum SceneWeTexPayload<'a> {
    Image(SceneWeTexImage),
    BlockCompressedImage(SceneWeTexBlockCompressedImage<'a>),
    Video(SceneWeTexVideo<'a>),
}

#[derive(Debug, Clone, Copy)]
struct SceneWeTexBlock {
    format: SceneWeTextureFormat,
    container: SceneWeTexContainer,
    free_image_format: u32,
    width: u32,
    height: u32,
    compression: u32,
    declared_size: u32,
    encoded_size: u32,
    payload_offset: usize,
}

const WE_FREE_IMAGE_FORMAT_PNG: u32 = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneWeTexContainer {
    Texb0003,
    Texb0004,
}

impl SceneWeTexContainer {
    fn marker(self) -> &'static [u8; 8] {
        match self {
            Self::Texb0003 => b"TEXB0003",
            Self::Texb0004 => b"TEXB0004",
        }
    }

    fn mip_width_offset(self) -> usize {
        match self {
            Self::Texb0003 => 21,
            Self::Texb0004 => 25,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneWeTextureFormat {
    Argb8888,
    Dxt5,
    Dxt3,
    Dxt1,
    Bc7,
    Other(u32),
}

impl SceneWeTextureFormat {
    fn block_compressed_format(self) -> Option<SceneWeTexBlockCompressedFormat> {
        match self {
            Self::Dxt1 => Some(SceneWeTexBlockCompressedFormat::Bc1RgbaUnormBlock),
            Self::Dxt5 => Some(SceneWeTexBlockCompressedFormat::Bc3UnormBlock),
            Self::Bc7 => Some(SceneWeTexBlockCompressedFormat::Bc7UnormBlock),
            Self::Argb8888 | Self::Dxt3 | Self::Other(_) => None,
        }
    }
}

#[cfg(test)]
pub(super) fn decode_we_tex_image(bytes: &[u8]) -> Result<SceneWeTexImage, String> {
    match decode_we_tex_payload(bytes)? {
        SceneWeTexPayload::Image(image) => Ok(image),
        SceneWeTexPayload::BlockCompressedImage(image) => Err(format!(
            "TEXB0004 payload is a {} GPU texture, not an RGBA image",
            image.format.label()
        )),
        SceneWeTexPayload::Video(_) => {
            Err("TEXB0004 payload is a video stream, not an RGBA image".to_owned())
        }
    }
}

pub(super) fn decode_we_tex_payload(bytes: &[u8]) -> Result<SceneWeTexPayload<'_>, String> {
    let block = we_tex_block(bytes)?;
    let payload = we_tex_block_payload(bytes, block)?;
    if block.declared_size == 0
        && let Some(extension) = we_tex_video_extension(payload)
    {
        return Ok(SceneWeTexPayload::Video(SceneWeTexVideo {
            width: block.width,
            height: block.height,
            extension,
            payload,
        }));
    }
    if let Some(format) = block.format.block_compressed_format() {
        let payload = decode_we_tex_block_compressed_payload(block, format, payload)?;
        return Ok(SceneWeTexPayload::BlockCompressedImage(
            SceneWeTexBlockCompressedImage {
                width: block.width,
                height: block.height,
                format,
                payload,
            },
        ));
    }
    if let Some(image) = decode_we_tex_embedded_image_payload(block, payload)? {
        return Ok(SceneWeTexPayload::Image(image));
    }
    let image = decode_we_tex_image_payload(block, payload)?;
    Ok(SceneWeTexPayload::Image(image))
}

pub(super) fn rgba_len(width: u32, height: u32) -> Result<usize, String> {
    usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "RGBA texture size overflowed".to_owned())
}

fn we_tex_block(bytes: &[u8]) -> Result<SceneWeTexBlock, String> {
    if !bytes.starts_with(b"TEXV0005\0TEXI0001\0") {
        return Err("unsupported .tex header; expected TEXV0005/TEXI0001".to_owned());
    }
    let format = read_u32_le_at(bytes, 18)
        .ok_or_else(|| "truncated TEXI0001 texture format".to_owned())
        .map(scene_we_texture_format)?;
    let (container, block_marker) = we_tex_container(bytes)
        .ok_or_else(|| "unsupported .tex payload; missing TEXB0003/TEXB0004 block".to_owned())?;
    let free_image_format = read_u32_le_at(bytes, block_marker + 13)
        .ok_or_else(|| format!("truncated {} free image format", container.label()))?;
    let width_offset = block_marker + container.mip_width_offset();
    let height_offset = width_offset + 4;
    let compression_offset = width_offset + 8;
    let declared_size_offset = width_offset + 12;
    let encoded_size_offset = width_offset + 16;
    let payload_offset = width_offset + 20;
    let width = read_u32_le_at(bytes, width_offset)
        .ok_or_else(|| format!("truncated {} block width", container.label()))?;
    let height = read_u32_le_at(bytes, height_offset)
        .ok_or_else(|| format!("truncated {} block height", container.label()))?;
    if width == 0 || height == 0 {
        return Err(format!("{} block has zero dimensions", container.label()));
    }
    let compression = read_u32_le_at(bytes, compression_offset)
        .ok_or_else(|| format!("truncated {} compression", container.label()))?;
    let declared_size = read_u32_le_at(bytes, declared_size_offset)
        .ok_or_else(|| format!("truncated {} decoded size", container.label()))?;
    let encoded_size = read_u32_le_at(bytes, encoded_size_offset)
        .ok_or_else(|| format!("truncated {} encoded size", container.label()))?;
    Ok(SceneWeTexBlock {
        format,
        container,
        free_image_format,
        width,
        height,
        compression,
        declared_size,
        encoded_size,
        payload_offset,
    })
}

impl SceneWeTexContainer {
    fn label(self) -> &'static str {
        match self {
            Self::Texb0003 => "TEXB0003",
            Self::Texb0004 => "TEXB0004",
        }
    }
}

fn we_tex_container(bytes: &[u8]) -> Option<(SceneWeTexContainer, usize)> {
    [SceneWeTexContainer::Texb0004, SceneWeTexContainer::Texb0003]
        .into_iter()
        .filter_map(|container| {
            find_bytes(bytes, container.marker()).map(|offset| (container, offset))
        })
        .min_by_key(|(_, offset)| *offset)
}

fn scene_we_texture_format(value: u32) -> SceneWeTextureFormat {
    match value {
        0 => SceneWeTextureFormat::Argb8888,
        4 => SceneWeTextureFormat::Dxt5,
        6 => SceneWeTextureFormat::Dxt3,
        7 => SceneWeTextureFormat::Dxt1,
        12 => SceneWeTextureFormat::Bc7,
        other => SceneWeTextureFormat::Other(other),
    }
}

fn we_tex_block_payload(bytes: &[u8], block: SceneWeTexBlock) -> Result<&[u8], String> {
    let encoded_size = usize::try_from(block.encoded_size).map_err(|_| {
        format!(
            "{} encoded size does not fit this platform",
            block.container.label()
        )
    })?;
    let payload_end = block
        .payload_offset
        .checked_add(encoded_size)
        .ok_or_else(|| {
            format!(
                "{} encoded payload range overflowed",
                block.container.label()
            )
        })?;
    bytes
        .get(block.payload_offset..payload_end)
        .ok_or_else(|| format!("truncated {} encoded payload", block.container.label()))
}

fn decode_we_tex_block_compressed_payload<'a>(
    block: SceneWeTexBlock,
    format: SceneWeTexBlockCompressedFormat,
    payload: &'a [u8],
) -> Result<Cow<'a, [u8]>, String> {
    let expected_len = usize::try_from(super::gtex::bc_payload_len(
        format.gtex_format(),
        block.width,
        block.height,
    )?)
    .map_err(|_| {
        format!(
            "{} {} payload length exceeds usize",
            block.container.label(),
            format.label()
        )
    })?;
    match block.compression {
        0 => {
            if payload.len() != expected_len {
                return Err(format!(
                    "TEXB0004 {} payload has {} bytes, expected {expected_len}",
                    format.label(),
                    payload.len()
                ));
            }
            Ok(Cow::Borrowed(payload))
        }
        1 => {
            if usize::try_from(block.declared_size).ok() != Some(expected_len) {
                return Err(format!(
                    "TEXB0004 {} decoded size {} does not match {}x{} block payload",
                    format.label(),
                    block.declared_size,
                    block.width,
                    block.height
                ));
            }
            let decoded = lz4_block_decode(payload, expected_len)?;
            Ok(Cow::Owned(decoded))
        }
        other => Err(format!(
            "TEXB0004 {} uses unsupported mip compression {other}",
            format.label()
        )),
    }
}

fn decode_we_tex_embedded_image_payload(
    block: SceneWeTexBlock,
    payload: &[u8],
) -> Result<Option<SceneWeTexImage>, String> {
    if block.format != SceneWeTextureFormat::Argb8888
        || block.free_image_format != WE_FREE_IMAGE_FORMAT_PNG
    {
        return Ok(None);
    }
    if block.compression != 0 || block.declared_size != 0 {
        return Err(format!(
            "TEXB0004 PNG payload uses unsupported mip compression {} and decoded size {}",
            block.compression, block.declared_size
        ));
    }
    let mut image = super::gtex::read_png_bytes_as_rgba(payload).map_err(|err| {
        format!(
            "{} PNG payload could not be decoded: {err}",
            block.container.label()
        )
    })?;
    if image.width != block.width || image.height != block.height {
        return Err(format!(
            "TEXB0004 PNG payload decoded to {}x{}, expected {}x{}",
            image.width, image.height, block.width, block.height
        ));
    }
    super::gtex::flip_rgba_rows_vertically(&mut image.rgba, image.width, image.height)?;
    Ok(Some(image))
}

fn decode_we_tex_image_payload(
    block: SceneWeTexBlock,
    payload: &[u8],
) -> Result<SceneWeTexImage, String> {
    if block.format != SceneWeTextureFormat::Argb8888 {
        return Err(format!(
            "TEXB0004 texture format {:?} is not an RGBA image payload",
            block.format
        ));
    }
    let expected_len = rgba_len(block.width, block.height)?;
    if usize::try_from(block.declared_size).ok() != Some(expected_len) {
        return Err(format!(
            "TEXB0004 decoded size {} does not match {}x{} RGBA",
            block.declared_size, block.width, block.height
        ));
    }
    let rgba = lz4_block_decode(payload, expected_len)?;
    Ok(SceneWeTexImage {
        width: block.width,
        height: block.height,
        rgba,
    })
}

fn we_tex_video_extension(payload: &[u8]) -> Option<&'static str> {
    if payload.len() >= 12 && payload.get(4..8) == Some(&b"ftyp"[..]) {
        return Some("mp4");
    }
    if payload.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        return Some("webm");
    }
    if payload.len() >= 12
        && payload.get(0..4) == Some(&b"RIFF"[..])
        && payload.get(8..12) == Some(&b"AVI "[..])
    {
        return Some("avi");
    }
    None
}

fn read_u32_le_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let bytes = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn lz4_block_decode(input: &[u8], expected_len: usize) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(expected_len);
    let mut cursor = 0usize;
    while cursor < input.len() {
        let token = input[cursor];
        cursor += 1;

        let literal_len = lz4_read_len(input, &mut cursor, usize::from(token >> 4))?;
        let literal_end = cursor
            .checked_add(literal_len)
            .ok_or_else(|| "LZ4 literal range overflowed".to_owned())?;
        if literal_end > input.len() {
            return Err("LZ4 literal run exceeds input".to_owned());
        }
        output.extend_from_slice(&input[cursor..literal_end]);
        cursor = literal_end;
        if cursor >= input.len() {
            break;
        }

        let offset_end = cursor
            .checked_add(2)
            .ok_or_else(|| "LZ4 offset range overflowed".to_owned())?;
        let offset = input
            .get(cursor..offset_end)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u16::from_le_bytes)
            .ok_or_else(|| "truncated LZ4 match offset".to_owned())?;
        cursor = offset_end;
        let offset = usize::from(offset);
        if offset == 0 || offset > output.len() {
            return Err("invalid LZ4 match offset".to_owned());
        }

        let match_len = lz4_read_len(input, &mut cursor, usize::from(token & 0x0f))?
            .checked_add(4)
            .ok_or_else(|| "LZ4 match length overflowed".to_owned())?;
        for _ in 0..match_len {
            let index = output
                .len()
                .checked_sub(offset)
                .ok_or_else(|| "invalid LZ4 back-reference".to_owned())?;
            let byte = *output
                .get(index)
                .ok_or_else(|| "invalid LZ4 back-reference index".to_owned())?;
            output.push(byte);
        }
        if output.len() > expected_len {
            return Err("LZ4 output exceeds declared decoded size".to_owned());
        }
    }
    if output.len() != expected_len {
        return Err(format!(
            "LZ4 output length {} does not match declared decoded size {expected_len}",
            output.len()
        ));
    }
    Ok(output)
}

fn lz4_read_len(input: &[u8], cursor: &mut usize, initial: usize) -> Result<usize, String> {
    let mut length = initial;
    if initial != 15 {
        return Ok(length);
    }
    loop {
        let byte = *input
            .get(*cursor)
            .ok_or_else(|| "truncated LZ4 extended length".to_owned())?;
        *cursor += 1;
        length = length
            .checked_add(usize::from(byte))
            .ok_or_else(|| "LZ4 extended length overflowed".to_owned())?;
        if byte != 255 {
            break;
        }
    }
    Ok(length)
}
use std::borrow::Cow;
