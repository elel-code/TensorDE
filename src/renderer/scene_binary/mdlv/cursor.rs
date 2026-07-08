//! Bounded MDLV byte cursor.
//!
//! Reference:
//! - `reverse-engineered/docs/mdl-format.md`

use std::path::Path;

use crate::renderer::RendererPlanError;

use super::error::mdlv_error;

pub(super) struct BinarySceneMdlvCursor<'a> {
    bytes: &'a [u8],
    path: &'a Path,
    position: usize,
}

impl<'a> BinarySceneMdlvCursor<'a> {
    pub(super) fn new(bytes: &'a [u8], path: &'a Path) -> Self {
        Self {
            bytes,
            path,
            position: 0,
        }
    }

    pub(super) fn skip(&mut self, bytes: usize) -> Result<(), RendererPlanError> {
        self.position = self
            .position
            .checked_add(bytes)
            .ok_or_else(|| mdlv_error(self.path, "MDLV cursor overflow"))?;
        if self.position > self.bytes.len() {
            return Err(mdlv_error(self.path, "MDLV cursor moved past EOF"));
        }
        Ok(())
    }

    pub(super) fn take_u32(&mut self, field: &str) -> Result<u32, RendererPlanError> {
        let end = self
            .position
            .checked_add(4)
            .ok_or_else(|| mdlv_error(self.path, "MDLV u32 cursor overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is out of bounds")))?;
        self.position = end;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    pub(super) fn take_u64(&mut self, field: &str) -> Result<u64, RendererPlanError> {
        let end = self
            .position
            .checked_add(8)
            .ok_or_else(|| mdlv_error(self.path, "MDLV u64 cursor overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is out of bounds")))?;
        self.position = end;
        Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
    }

    pub(super) fn take_u8(&mut self, field: &str) -> Result<u8, RendererPlanError> {
        let byte = self
            .bytes
            .get(self.position)
            .copied()
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is out of bounds")))?;
        self.position += 1;
        Ok(byte)
    }

    pub(super) fn take_bytes(
        &mut self,
        count: usize,
        field: &str,
    ) -> Result<Vec<u8>, RendererPlanError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| mdlv_error(self.path, "MDLV byte block cursor overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is out of bounds")))?;
        self.position = end;
        Ok(bytes.to_vec())
    }

    pub(super) fn take_byte_block(&mut self, field: &str) -> Result<&'a [u8], RendererPlanError> {
        let count = self.take_u32(field)? as usize;
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| mdlv_error(self.path, "MDLV byte block cursor overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is out of bounds")))?;
        self.position = end;
        Ok(bytes)
    }

    pub(super) fn skip_byte_block(&mut self, field: &str) -> Result<(), RendererPlanError> {
        let count = self.take_u32(field)? as usize;
        self.skip(count)
    }

    pub(super) fn take_u32_list(&mut self, field: &str) -> Result<Vec<u32>, RendererPlanError> {
        let count = self.take_u32(field)? as usize;
        if count > 1_000_000 {
            return Err(mdlv_error(
                self.path,
                &format!("{field} count is unreasonable"),
            ));
        }
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.take_u32(field)?);
        }
        Ok(values)
    }

    pub(super) fn take_c_string(&mut self, field: &str) -> Result<&'a str, RendererPlanError> {
        let relative_end = self.bytes[self.position..]
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| mdlv_error(self.path, &format!("{field} is not NUL-terminated")))?;
        let end = self.position + relative_end;
        let value = std::str::from_utf8(&self.bytes[self.position..end])
            .map_err(|_| mdlv_error(self.path, &format!("{field} is not valid UTF-8")))?;
        self.position = end + 1;
        Ok(value)
    }
}
