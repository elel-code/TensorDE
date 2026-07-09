//! Wallpaper Engine `.tex` metadata parser.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/tools/parse_tex.py`
//! - `reverse-engineered/tools/audit_tex_format_inventory.py`

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexMetadata {
    pub texv_tag: String,
    pub texi_tag: String,
    pub texb_tag: String,
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub storage_width: u32,
    pub storage_height: u32,
    pub mip_count: u32,
}

pub fn parse_tex_metadata(data: &[u8]) -> Result<TexMetadata, TexParseError> {
    let mut cursor = Cursor::new(data);
    let texv_tag = read_tag(cursor.bytes(9)?)?;
    if !texv_tag.starts_with("TEXV") {
        return Err(TexParseError::InvalidTag {
            field: "TEXV",
            value: texv_tag,
        });
    }
    let texi_tag = read_tag(cursor.bytes(9)?)?;
    if !texi_tag.starts_with("TEXI") {
        return Err(TexParseError::InvalidTag {
            field: "TEXI",
            value: texi_tag,
        });
    }
    let _flags = cursor.u32()?;
    let format = cursor.u32()?;
    let width = cursor.u32()?;
    let height = cursor.u32()?;
    let storage_width = cursor.u32()?;
    let storage_height = cursor.u32()?;
    let _unk = cursor.u16()?;
    let mip_count = cursor.u8()? as u32;
    let _extra = cursor.u8()?;
    let texb_offset = find_tag(data, b"TEXB", cursor.offset).ok_or(TexParseError::MissingTexb)?;
    let texb_end = texb_offset
        .checked_add(9)
        .ok_or(TexParseError::OffsetOverflow)?;
    let texb_tag = read_tag(
        data.get(texb_offset..texb_end)
            .ok_or(TexParseError::Truncated("TEXB"))?,
    )?;
    Ok(TexMetadata {
        texv_tag,
        texi_tag,
        texb_tag,
        format,
        width,
        height,
        storage_width,
        storage_height,
        mip_count,
    })
}

#[derive(Debug)]
pub enum TexParseError {
    Truncated(&'static str),
    OffsetOverflow,
    InvalidTag { field: &'static str, value: String },
    MissingTexb,
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
        }
    }
}

impl std::error::Error for TexParseError {}

fn read_tag(bytes: &[u8]) -> Result<String, TexParseError> {
    let nul = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    Ok(String::from_utf8_lossy(&bytes[..nul]).into_owned())
}

fn find_tag(data: &[u8], tag_prefix: &[u8], start: usize) -> Option<usize> {
    data.get(start..)?
        .windows(tag_prefix.len())
        .position(|window| window == tag_prefix)
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

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], TexParseError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(TexParseError::OffsetOverflow)?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or(TexParseError::Truncated("field"))?;
        self.offset = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32, TexParseError> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().expect("u32 slice"),
        ))
    }

    fn u16(&mut self) -> Result<u16, TexParseError> {
        Ok(u16::from_le_bytes(
            self.bytes(2)?.try_into().expect("u16 slice"),
        ))
    }

    fn u8(&mut self) -> Result<u8, TexParseError> {
        Ok(*self
            .bytes(1)?
            .first()
            .ok_or(TexParseError::Truncated("u8"))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_texi_and_texb_tags() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TEXV0005\0");
        bytes.extend_from_slice(b"TEXI0001\0");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&64u32.to_le_bytes());
        bytes.extend_from_slice(&32u32.to_le_bytes());
        bytes.extend_from_slice(&64u32.to_le_bytes());
        bytes.extend_from_slice(&32u32.to_le_bytes());
        bytes.extend_from_slice(&0x00b2u16.to_le_bytes());
        bytes.push(3);
        bytes.push(0xff);
        bytes.extend_from_slice(b"TEXB0004\0");

        let meta = parse_tex_metadata(&bytes).expect("tex");
        assert_eq!(meta.format, 4);
        assert_eq!(meta.width, 64);
        assert_eq!(meta.mip_count, 3);
        assert_eq!(meta.texb_tag, "TEXB0004");
    }
}
