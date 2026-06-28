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
pub(super) enum SceneWeTexPayload<'a> {
    Image(SceneWeTexImage),
    Video(SceneWeTexVideo<'a>),
}

#[derive(Debug, Clone, Copy)]
struct SceneWeTexBlock {
    width: u32,
    height: u32,
    declared_size: u32,
    encoded_size: u32,
    payload_offset: usize,
}

#[cfg(test)]
pub(super) fn decode_we_tex_image(bytes: &[u8]) -> Result<SceneWeTexImage, String> {
    match decode_we_tex_payload(bytes)? {
        SceneWeTexPayload::Image(image) => Ok(image),
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
    let block_marker = find_bytes(bytes, b"TEXB0004")
        .ok_or_else(|| "unsupported .tex payload; missing TEXB0004 block".to_owned())?;
    let width = read_u32_le_at(bytes, block_marker + 25)
        .ok_or_else(|| "truncated TEXB0004 block width".to_owned())?;
    let height = read_u32_le_at(bytes, block_marker + 29)
        .ok_or_else(|| "truncated TEXB0004 block height".to_owned())?;
    if width == 0 || height == 0 {
        return Err("TEXB0004 block has zero dimensions".to_owned());
    }
    let declared_size = read_u32_le_at(bytes, block_marker + 37)
        .ok_or_else(|| "truncated TEXB0004 decoded size".to_owned())?;
    let encoded_size = read_u32_le_at(bytes, block_marker + 41)
        .ok_or_else(|| "truncated TEXB0004 encoded size".to_owned())?;
    Ok(SceneWeTexBlock {
        width,
        height,
        declared_size,
        encoded_size,
        payload_offset: block_marker + 45,
    })
}

fn we_tex_block_payload(bytes: &[u8], block: SceneWeTexBlock) -> Result<&[u8], String> {
    let encoded_size = usize::try_from(block.encoded_size)
        .map_err(|_| "TEXB0004 encoded size does not fit this platform".to_owned())?;
    let payload_end = block
        .payload_offset
        .checked_add(encoded_size)
        .ok_or_else(|| "TEXB0004 encoded payload range overflowed".to_owned())?;
    bytes
        .get(block.payload_offset..payload_end)
        .ok_or_else(|| "truncated TEXB0004 encoded payload".to_owned())
}

fn decode_we_tex_image_payload(
    block: SceneWeTexBlock,
    payload: &[u8],
) -> Result<SceneWeTexImage, String> {
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
