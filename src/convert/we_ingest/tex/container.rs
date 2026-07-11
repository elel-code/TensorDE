//! WE TEXV/TEXI/TEXB container parsing.

use super::{TexMetadata, TexParseError, is_image_payload};

pub(super) struct TexContainer<'a> {
    pub metadata: TexMetadata,
    pub levels: Vec<TexEncodedLevel<'a>>,
}

pub(super) struct TexEncodedLevel<'a> {
    pub width: u32,
    pub height: u32,
    pub compression: u32,
    pub decoded_size: u32,
    pub payload: &'a [u8],
}

pub(super) fn parse_tex_container(data: &[u8]) -> Result<TexContainer<'_>, TexParseError> {
    let mut cursor = Cursor::new(data);
    let texv_tag = cursor.tag("TEXV")?;
    require_tag("TEXV", &texv_tag)?;
    let texi_tag = cursor.tag("TEXI")?;
    require_tag("TEXI", &texi_tag)?;
    let runtime_format = cursor.u32("TEXI runtime format")?;
    let payload_format = cursor.u32("TEXI payload format")?;
    let width = cursor.u32("TEXI width")?;
    let height = cursor.u32("TEXI height")?;
    let storage_width = cursor.u32("TEXI storage width")?;
    let storage_height = cursor.u32("TEXI storage height")?;
    let texb_offset = find_tag(data, b"TEXB", cursor.offset).ok_or(TexParseError::MissingTexb)?;
    cursor.offset = texb_offset;
    let texb_tag = cursor.tag("TEXB")?;
    require_tag("TEXB", &texb_tag)?;

    let (levels, sampler_flags) = match texb_tag.as_str() {
        "TEXB0001" => (parse_texb0001(&mut cursor)?, payload_format),
        "TEXB0002" | "TEXB0003" => (parse_legacy_levels(data, cursor.offset)?, payload_format),
        "TEXB0004" => parse_texb0004(&mut cursor, payload_format)?,
        _ => return Err(TexParseError::UnsupportedTexb(texb_tag)),
    };
    if levels.is_empty() {
        return Err(TexParseError::InvalidPayload("TEXB has no levels"));
    }
    Ok(TexContainer {
        metadata: TexMetadata {
            texv_tag,
            texi_tag,
            texb_tag,
            runtime_format,
            payload_format,
            sampler_flags,
            width,
            height,
            storage_width,
            storage_height,
            mip_count: levels.len() as u32,
        },
        levels,
    })
}

fn parse_texb0001<'a>(cursor: &mut Cursor<'a>) -> Result<Vec<TexEncodedLevel<'a>>, TexParseError> {
    let _flags = cursor.u32("TEXB0001 flags")?;
    let level_count = cursor.u32("TEXB0001 level count")?;
    let mut levels = Vec::with_capacity(level_count as usize);
    for _ in 0..level_count {
        let width = cursor.u32("TEXB0001 level width")?;
        let height = cursor.u32("TEXB0001 level height")?;
        let encoded_size = cursor.u32("TEXB0001 level size")?;
        let payload = cursor.bytes(encoded_size as usize, "TEXB0001 level payload")?;
        levels.push(TexEncodedLevel {
            width,
            height,
            compression: 0,
            decoded_size: encoded_size,
            payload,
        });
    }
    Ok(levels)
}

fn parse_texb0004<'a>(
    cursor: &mut Cursor<'a>,
    payload_format: u32,
) -> Result<(Vec<TexEncodedLevel<'a>>, u32), TexParseError> {
    let group_count = cursor.u32("TEXB0004 group count")?;
    let _storage_code = cursor.u32("TEXB0004 storage code")?;
    let resource_map_count = cursor.u32("TEXB0004 resource map count")?;
    let level_count = cursor.u32("TEXB0004 first group level count")?;
    if group_count == 0 || resource_map_count != 0 || level_count == 0 {
        return Err(TexParseError::InvalidPayload(
            "unsupported TEXB0004 resource group layout",
        ));
    }
    let mut width = cursor.u32("TEXB0004 first level width")?;
    let mut height = cursor.u32("TEXB0004 first level height")?;
    let mut levels = Vec::with_capacity(level_count as usize);
    for level_index in 0..level_count {
        if level_index != 0 {
            width = cursor.u32("TEXB0004 level width")?;
            height = cursor.u32("TEXB0004 level height")?;
        }
        levels.push(parse_texb0004_level(cursor, width, height)?);
    }
    let sampler_flags = payload_format | u32::from(level_count < 2) * 0x8;
    Ok((levels, sampler_flags))
}

fn parse_texb0004_level<'a>(
    cursor: &mut Cursor<'a>,
    width: u32,
    height: u32,
) -> Result<TexEncodedLevel<'a>, TexParseError> {
    let stored_encoding = cursor.u32("TEXB0004 level encoding")?;
    let decoded_size = cursor.u32("TEXB0004 level decoded size")?;
    let encoded_size = cursor.u32("TEXB0004 level encoded size")?;
    let payload = cursor.bytes(encoded_size as usize, "TEXB0004 level payload")?;
    // TEXB0004 RGBA pattern/noise resources use encoding 0 for both stored
    // data and LZ4. A nonzero decoded size that differs from the stored size
    // is the unambiguous LZ4 discriminator. Encoding 1 is also observed on R8
    // masks and retains the legacy LZ4 meaning.
    let compression = if stored_encoding == 0 && decoded_size != 0 && decoded_size != encoded_size {
        1
    } else {
        stored_encoding
    };
    if compression == 0 && decoded_size != 0 && decoded_size != encoded_size {
        return Err(TexParseError::InvalidPayload(
            "stored TEXB0004 level decoded and encoded sizes differ",
        ));
    }
    Ok(TexEncodedLevel {
        width,
        height,
        compression,
        decoded_size,
        payload,
    })
}

fn parse_legacy_levels(
    data: &[u8],
    start: usize,
) -> Result<Vec<TexEncodedLevel<'_>>, TexParseError> {
    let probe = data
        .get(start..)
        .ok_or(TexParseError::Truncated("legacy TEXB prefix"))?;
    if probe.len() < 40 {
        return Err(TexParseError::Truncated("legacy TEXB prefix"));
    }
    let words = (0..8)
        .map(|index| read_u32_at(data, start + index * 4))
        .collect::<Result<Vec<_>, _>>()?;
    let image_probe = data.get(start + 32..start + 40).unwrap_or_default();
    let mut cursor = Cursor::at(
        data,
        if words[1] == u32::MAX || words[5] == 0 && words[6] == 0 && is_image_payload(image_probe) {
            start + 12
        } else {
            start + 8
        },
    );
    let data_end = find_tag(data, b"TEXS", cursor.offset).unwrap_or(data.len());
    let mut levels = Vec::new();
    while cursor.offset < data_end {
        if data_end.saturating_sub(cursor.offset) < 20 {
            return Err(TexParseError::Truncated("legacy TEXB level header"));
        }
        let width = cursor.u32("legacy level width")?;
        let height = cursor.u32("legacy level height")?;
        let level = parse_legacy_level(&mut cursor, width, height)?;
        if cursor.offset > data_end {
            return Err(TexParseError::Truncated("legacy TEXB level payload"));
        }
        levels.push(level);
    }
    Ok(levels)
}

fn parse_legacy_level<'a>(
    cursor: &mut Cursor<'a>,
    width: u32,
    height: u32,
) -> Result<TexEncodedLevel<'a>, TexParseError> {
    let compression = cursor.u32("legacy TEXB level compression")?;
    let decoded_size = cursor.u32("legacy TEXB level decoded size")?;
    let encoded_size = cursor.u32("legacy TEXB level encoded size")?;
    let payload = cursor.bytes(encoded_size as usize, "legacy TEXB level payload")?;
    if compression == 0 && decoded_size != 0 && decoded_size != encoded_size {
        return Err(TexParseError::InvalidPayload(
            "stored level decoded and encoded sizes differ",
        ));
    }
    Ok(TexEncodedLevel {
        width,
        height,
        compression,
        decoded_size,
        payload,
    })
}

fn require_tag(field: &'static str, tag: &str) -> Result<(), TexParseError> {
    if tag.starts_with(field) {
        Ok(())
    } else {
        Err(TexParseError::InvalidTag {
            field,
            value: tag.to_owned(),
        })
    }
}

fn read_u32_at(data: &[u8], offset: usize) -> Result<u32, TexParseError> {
    let bytes = data
        .get(offset..offset.saturating_add(4))
        .ok_or(TexParseError::Truncated("u32"))?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("u32 slice")))
}

fn find_tag(data: &[u8], prefix: &[u8], start: usize) -> Option<usize> {
    data.get(start..)?
        .windows(prefix.len())
        .position(|window| window == prefix)
        .map(|relative| start + relative)
}

struct Cursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn at(data: &'a [u8], offset: usize) -> Self {
        Self { data, offset }
    }

    fn tag(&mut self, field: &'static str) -> Result<String, TexParseError> {
        let tail = self
            .data
            .get(self.offset..)
            .ok_or(TexParseError::Truncated(field))?;
        let len = tail
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(TexParseError::Truncated(field))?;
        let bytes = self.bytes(len + 1, field)?;
        Ok(String::from_utf8_lossy(&bytes[..len]).into_owned())
    }

    fn bytes(&mut self, len: usize, field: &'static str) -> Result<&'a [u8], TexParseError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(TexParseError::OffsetOverflow)?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or(TexParseError::Truncated(field))?;
        self.offset = end;
        Ok(bytes)
    }

    fn u32(&mut self, field: &'static str) -> Result<u32, TexParseError> {
        Ok(u32::from_le_bytes(
            self.bytes(4, field)?.try_into().expect("u32 slice"),
        ))
    }
}
