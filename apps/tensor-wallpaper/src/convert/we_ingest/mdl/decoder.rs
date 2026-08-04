//! Bounds-checked MDL byte cursor used by the cold parser.

use crate::engine::scene::abi::SceneVec3;

use super::MdlParseError;

pub(super) struct MdlDecoder<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) offset: usize,
}

impl<'a> MdlDecoder<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn bytes(
        &mut self,
        len: usize,
        field: &'static str,
    ) -> Result<&'a [u8], MdlParseError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(MdlParseError::UnexpectedEof(field))?;
        if end > self.bytes.len() {
            return Err(MdlParseError::UnexpectedEof(field));
        }
        let out = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(out)
    }

    pub(super) fn checked_count(
        &self,
        count: u32,
        minimum_item_bytes: usize,
        field: &'static str,
    ) -> Result<usize, MdlParseError> {
        let count = count as usize;
        let required = count
            .checked_mul(minimum_item_bytes)
            .ok_or(MdlParseError::UnexpectedEof(field))?;
        if required > self.bytes.len().saturating_sub(self.offset) {
            return Err(MdlParseError::UnexpectedEof(field));
        }
        Ok(count)
    }

    pub(super) fn skip_zero_padding(&mut self, limit: usize) {
        let limit = limit.min(self.bytes.len());
        while self.offset < limit && self.bytes[self.offset] == 0 {
            self.offset += 1;
        }
    }

    pub(super) fn u32(&mut self, field: &'static str) -> Result<u32, MdlParseError> {
        let bytes = self.bytes(4, field)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(super) fn u64(&mut self, field: &'static str) -> Result<u64, MdlParseError> {
        let bytes = self.bytes(8, field)?;
        Ok(u64::from_le_bytes(
            bytes.try_into().expect("eight-byte slice"),
        ))
    }

    pub(super) fn u8(&mut self, field: &'static str) -> Result<u8, MdlParseError> {
        Ok(self.bytes(1, field)?[0])
    }

    pub(super) fn u16(&mut self, field: &'static str) -> Result<u16, MdlParseError> {
        let bytes = self.bytes(2, field)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub(super) fn i32(&mut self, field: &'static str) -> Result<i32, MdlParseError> {
        let bytes = self.bytes(4, field)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(super) fn f32(&mut self, field: &'static str) -> Result<f32, MdlParseError> {
        let bytes = self.bytes(4, field)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(super) fn vec3(&mut self, field: &'static str) -> Result<SceneVec3, MdlParseError> {
        Ok(SceneVec3 {
            x: self.f32(field)?,
            y: self.f32(field)?,
            z: self.f32(field)?,
        })
    }

    pub(super) fn c_string(&mut self, field: &'static str) -> Result<String, MdlParseError> {
        let start = self.offset;
        let tail = self
            .bytes
            .get(start..)
            .ok_or(MdlParseError::UnexpectedEof(field))?;
        let len = tail
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(MdlParseError::UnexpectedEof(field))?;
        let value =
            std::str::from_utf8(&tail[..len]).map_err(|_| MdlParseError::InvalidUtf8String {
                field,
                offset: start,
            })?;
        self.offset = start + len + 1;
        Ok(value.replace('\\', "/"))
    }
}

pub(super) fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

pub(super) fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
