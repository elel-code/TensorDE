use std::error::Error;
use std::fmt;

use super::SceneBinaryChunkKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneBinaryError {
    BufferTooSmall {
        needed: usize,
        actual: usize,
    },
    BadMagic {
        actual: [u8; 4],
    },
    UnsupportedVersion {
        version: u16,
    },
    UnsupportedEndian {
        endian: u8,
    },
    InvalidAlignment {
        alignment: u8,
    },
    InvalidChunkOrder {
        index: usize,
        expected: SceneBinaryChunkKind,
        actual: SceneBinaryChunkKind,
    },
    RequiredChunkCount {
        expected: usize,
        actual: usize,
    },
    DuplicateChunk {
        kind: SceneBinaryChunkKind,
    },
    MissingChunk {
        kind: SceneBinaryChunkKind,
    },
    UnknownChunk {
        code: u32,
    },
    UnknownRetainedOwnerKind {
        owner_kind: u16,
    },
    InvalidRecordPayload {
        kind: SceneBinaryChunkKind,
        record_size: usize,
        record_count: u32,
        length: usize,
    },
    RecordRangeOutOfBounds {
        kind: SceneBinaryChunkKind,
        first_record: u32,
        record_count: u32,
        chunk_record_count: u32,
    },
    NameOutOfBounds {
        id: u32,
        offset: u32,
        length: u32,
        string_table_len: usize,
    },
    InvalidNameUtf8 {
        id: u32,
    },
    ChunkTableOutOfBounds {
        offset: u64,
        count: u32,
        container_len: usize,
    },
    MisalignedChunk {
        kind: SceneBinaryChunkKind,
        offset: u64,
        alignment: u8,
    },
    ChunkOutOfBounds {
        kind: SceneBinaryChunkKind,
        offset: u64,
        length: u64,
        container_len: usize,
    },
    ChunkOverlap {
        previous: SceneBinaryChunkKind,
        current: SceneBinaryChunkKind,
    },
    StreamIo {
        operation: &'static str,
        message: String,
    },
}

impl fmt::Display for SceneBinaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooSmall { needed, actual } => {
                write!(f, "scene binary buffer is {actual} bytes; needs {needed}")
            }
            Self::BadMagic { actual } => write!(f, "invalid scene binary magic {actual:?}"),
            Self::UnsupportedVersion { version } => {
                write!(f, "unsupported scene binary version {version}")
            }
            Self::UnsupportedEndian { endian } => {
                write!(f, "unsupported scene binary endian policy {endian}")
            }
            Self::InvalidAlignment { alignment } => {
                write!(f, "invalid scene binary alignment {alignment}")
            }
            Self::InvalidChunkOrder {
                index,
                expected,
                actual,
            } => write!(
                f,
                "scene binary chunk {index} is {}; expected {}",
                actual.label(),
                expected.label()
            ),
            Self::RequiredChunkCount { expected, actual } => write!(
                f,
                "scene binary has {actual} required chunk families; expected {expected}"
            ),
            Self::DuplicateChunk { kind } => {
                write!(f, "duplicate scene binary chunk {}", kind.label())
            }
            Self::MissingChunk { kind } => {
                write!(f, "missing scene binary chunk {}", kind.label())
            }
            Self::UnknownChunk { code } => write!(f, "unknown scene binary chunk code {code:#x}"),
            Self::UnknownRetainedOwnerKind { owner_kind } => {
                write!(f, "unknown scene binary retained owner kind {owner_kind}")
            }
            Self::InvalidRecordPayload {
                kind,
                record_size,
                record_count,
                length,
            } => write!(
                f,
                "scene binary chunk {} has {length} payload bytes; expected {} records of {record_size} bytes",
                kind.label(),
                record_count
            ),
            Self::RecordRangeOutOfBounds {
                kind,
                first_record,
                record_count,
                chunk_record_count,
            } => write!(
                f,
                "scene binary chunk {} record range {}..{} exceeds {} records",
                kind.label(),
                first_record,
                first_record.saturating_add(*record_count),
                chunk_record_count
            ),
            Self::NameOutOfBounds {
                id,
                offset,
                length,
                string_table_len,
            } => write!(
                f,
                "scene binary debug name {id} offset {offset} length {length} exceeds {string_table_len} string bytes"
            ),
            Self::InvalidNameUtf8 { id } => {
                write!(f, "scene binary debug name {id} is not valid UTF-8")
            }
            Self::ChunkTableOutOfBounds {
                offset,
                count,
                container_len,
            } => write!(
                f,
                "scene binary chunk table offset {offset} count {count} exceeds {container_len} bytes"
            ),
            Self::MisalignedChunk {
                kind,
                offset,
                alignment,
            } => write!(
                f,
                "scene binary chunk {} offset {offset} is not aligned to {alignment}",
                kind.label()
            ),
            Self::ChunkOutOfBounds {
                kind,
                offset,
                length,
                container_len,
            } => write!(
                f,
                "scene binary chunk {} offset {offset} length {length} exceeds {container_len} bytes",
                kind.label()
            ),
            Self::ChunkOverlap { previous, current } => write!(
                f,
                "scene binary chunk {} overlaps {}",
                current.label(),
                previous.label()
            ),
            Self::StreamIo { operation, message } => {
                write!(f, "scene binary {operation} failed: {message}")
            }
        }
    }
}

impl Error for SceneBinaryError {}
