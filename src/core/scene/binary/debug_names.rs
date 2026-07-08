use super::{
    SCENE_BINARY_DEBUG_NAME_RECORD_SIZE, SceneBinaryChunkKind, SceneBinaryError, read_u32,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneBinaryDebugNameRecord {
    pub id: u32,
    pub kind: u32,
    pub offset: u32,
    pub length: u32,
}

pub struct SceneBinaryDebugNames<'a> {
    records: &'a [u8],
    strings: &'a [u8],
    record_count: usize,
}

impl<'a> SceneBinaryDebugNames<'a> {
    pub(super) fn new(record_count: u32, payload: &'a [u8]) -> Result<Self, SceneBinaryError> {
        let record_bytes = usize::try_from(record_count)
            .ok()
            .and_then(|count| count.checked_mul(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE))
            .ok_or(SceneBinaryError::InvalidRecordPayload {
                kind: SceneBinaryChunkKind::DebugNames,
                record_size: SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
                record_count,
                length: payload.len(),
            })?;
        if payload.len() < record_bytes {
            return Err(SceneBinaryError::InvalidRecordPayload {
                kind: SceneBinaryChunkKind::DebugNames,
                record_size: SCENE_BINARY_DEBUG_NAME_RECORD_SIZE,
                record_count,
                length: payload.len(),
            });
        }
        let (records, strings) = payload.split_at(record_bytes);
        Ok(Self {
            records,
            strings,
            record_count: record_count as usize,
        })
    }

    pub fn len(&self) -> usize {
        self.record_count
    }

    pub fn is_empty(&self) -> bool {
        self.record_count == 0
    }

    pub fn record(&self, id: u32) -> Result<Option<SceneBinaryDebugNameRecord>, SceneBinaryError> {
        let Some(start) = usize::try_from(id)
            .ok()
            .and_then(|index| index.checked_mul(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE))
        else {
            return Ok(None);
        };
        let Some(end) = start.checked_add(SCENE_BINARY_DEBUG_NAME_RECORD_SIZE) else {
            return Ok(None);
        };
        let Some(bytes) = self.records.get(start..end) else {
            return Ok(None);
        };
        let record = decode_debug_name_record(bytes)?;
        Ok(Some(record))
    }

    pub fn name(&self, id: u32) -> Result<Option<&'a str>, SceneBinaryError> {
        let Some(record) = self.record(id)? else {
            return Ok(None);
        };
        let start =
            usize::try_from(record.offset).map_err(|_| SceneBinaryError::NameOutOfBounds {
                id,
                offset: record.offset,
                length: record.length,
                string_table_len: self.strings.len(),
            })?;
        let length =
            usize::try_from(record.length).map_err(|_| SceneBinaryError::NameOutOfBounds {
                id,
                offset: record.offset,
                length: record.length,
                string_table_len: self.strings.len(),
            })?;
        let end = start
            .checked_add(length)
            .ok_or(SceneBinaryError::NameOutOfBounds {
                id,
                offset: record.offset,
                length: record.length,
                string_table_len: self.strings.len(),
            })?;
        let Some(bytes) = self.strings.get(start..end) else {
            return Err(SceneBinaryError::NameOutOfBounds {
                id,
                offset: record.offset,
                length: record.length,
                string_table_len: self.strings.len(),
            });
        };
        std::str::from_utf8(bytes)
            .map(Some)
            .map_err(|_| SceneBinaryError::InvalidNameUtf8 { id })
    }
}

pub(crate) fn decode_debug_name_record(
    bytes: &[u8],
) -> Result<SceneBinaryDebugNameRecord, SceneBinaryError> {
    Ok(SceneBinaryDebugNameRecord {
        id: read_u32(bytes, 0)?,
        kind: read_u32(bytes, 4)?,
        offset: read_u32(bytes, 8)?,
        length: read_u32(bytes, 12)?,
    })
}
