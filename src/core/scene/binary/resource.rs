use super::{SceneBinaryError, read_u16, read_u32, write_u16, write_u32};

pub const SCENE_BINARY_RESOURCE_RECORD_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryResourceRecord {
    pub id_name: u32,
    pub source_name: u32,
    pub original_source_name: u32,
    pub role_name: u32,
    pub kind: u16,
    pub flags: u16,
    pub width: u32,
    pub height: u32,
    pub upload_hints: u32,
}

impl SceneBinaryResourceRecord {
    pub(super) fn encode(self, out: &mut Vec<u8>) {
        write_u32(out, self.id_name);
        write_u32(out, self.source_name);
        write_u32(out, self.original_source_name);
        write_u32(out, self.role_name);
        write_u16(out, self.kind);
        write_u16(out, self.flags);
        write_u32(out, self.width);
        write_u32(out, self.height);
        write_u32(out, self.upload_hints);
        debug_assert_eq!(SCENE_BINARY_RESOURCE_RECORD_SIZE, 32);
    }
}

pub(crate) fn decode_resource_record(
    bytes: &[u8],
) -> Result<SceneBinaryResourceRecord, SceneBinaryError> {
    Ok(SceneBinaryResourceRecord {
        id_name: read_u32(bytes, 0)?,
        source_name: read_u32(bytes, 4)?,
        original_source_name: read_u32(bytes, 8)?,
        role_name: read_u32(bytes, 12)?,
        kind: read_u16(bytes, 16)?,
        flags: read_u16(bytes, 18)?,
        width: read_u32(bytes, 20)?,
        height: read_u32(bytes, 24)?,
        upload_hints: read_u32(bytes, 28)?,
    })
}
