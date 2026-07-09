//! Wallpaper Engine `scene.pkg` reader.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-pkg-format.md`
//! - `reverse-engineered/tools/unpack_pkg.py`

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenePackage {
    magic: String,
    entries: BTreeMap<String, ScenePackageEntry>,
    data: Vec<u8>,
}

impl ScenePackage {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ScenePackageError> {
        let path = path.as_ref();
        let data = fs::read(path).map_err(|source| ScenePackageError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(data)
    }

    pub fn parse(data: Vec<u8>) -> Result<Self, ScenePackageError> {
        let mut cursor = Cursor::new(&data);
        let magic_len = cursor.u32()?;
        if magic_len != 8 {
            return Err(ScenePackageError::InvalidMagicLength(magic_len));
        }
        let magic = lossy_string(cursor.bytes(8)?);
        if magic != "PKGV0023" && magic != "PKGV0024" {
            return Err(ScenePackageError::InvalidMagic(magic));
        }
        let file_count = cursor.u32()? as usize;
        let mut pending = Vec::with_capacity(file_count);
        for _ in 0..file_count {
            let filename_len = cursor.u32()? as usize;
            let filename = normalize_pkg_path(&lossy_string(cursor.bytes(filename_len)?));
            let data_offset = cursor.u32()? as usize;
            let data_size = cursor.u32()? as usize;
            pending.push((filename, data_offset, data_size));
        }
        let data_section_start = cursor.offset;
        let mut entries = BTreeMap::new();
        for (path, data_offset, data_size) in pending {
            let start = data_section_start
                .checked_add(data_offset)
                .ok_or(ScenePackageError::OffsetOverflow)?;
            let end = start
                .checked_add(data_size)
                .ok_or(ScenePackageError::OffsetOverflow)?;
            if end > data.len() {
                return Err(ScenePackageError::EntryOutOfBounds {
                    path,
                    offset: data_offset,
                    size: data_size,
                    data_section_start,
                    archive_len: data.len(),
                });
            }
            entries.insert(
                path,
                ScenePackageEntry {
                    offset: start,
                    len: data_size,
                },
            );
        }
        Ok(Self {
            magic,
            entries,
            data,
        })
    }

    pub fn magic(&self) -> &str {
        &self.magic
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(&normalize_pkg_path(path))
    }

    pub fn entry_bytes(&self, path: &str) -> Option<&[u8]> {
        let path = normalize_pkg_path(path);
        let entry = self.entries.get(&path)?;
        self.data.get(entry.offset..entry.offset + entry.len)
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenePackageEntry {
    pub offset: usize,
    pub len: usize,
}

#[derive(Debug)]
pub enum ScenePackageError {
    Read {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    Truncated(&'static str),
    InvalidMagicLength(u32),
    InvalidMagic(String),
    OffsetOverflow,
    EntryOutOfBounds {
        path: String,
        offset: usize,
        size: usize,
        data_section_start: usize,
        archive_len: usize,
    },
}

impl fmt::Display for ScenePackageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read scene.pkg {}: {source}", path.display())
            }
            Self::Truncated(field) => write!(f, "truncated scene.pkg while reading {field}"),
            Self::InvalidMagicLength(value) => {
                write!(f, "invalid scene.pkg magic length {value}, expected 8")
            }
            Self::InvalidMagic(value) => write!(f, "unsupported scene.pkg magic {value}"),
            Self::OffsetOverflow => f.write_str("scene.pkg offset overflow"),
            Self::EntryOutOfBounds {
                path,
                offset,
                size,
                data_section_start,
                archive_len,
            } => write!(
                f,
                "scene.pkg entry {path} out of bounds: data_start={data_section_start} offset={offset} size={size} archive_len={archive_len}"
            ),
        }
    }
}

impl std::error::Error for ScenePackageError {}

fn normalize_pkg_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_owned()
}

fn lossy_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

struct Cursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], ScenePackageError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(ScenePackageError::OffsetOverflow)?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or(ScenePackageError::Truncated("field"))?;
        self.offset = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32, ScenePackageError> {
        let bytes = self.bytes(4)?;
        Ok(u32::from_le_bytes(bytes.try_into().expect("u32 slice")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pkg_offsets_relative_to_data_section() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(b"PKGV0024");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(b"scene.json");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(b"{}");

        let pkg = ScenePackage::parse(bytes).expect("pkg");
        assert_eq!(pkg.magic(), "PKGV0024");
        assert_eq!(pkg.entry_bytes("scene.json"), Some(&b"{}"[..]));
    }
}
