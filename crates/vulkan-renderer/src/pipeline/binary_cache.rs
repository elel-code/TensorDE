use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use vulkanalia::vk;

use super::binary::{PipelineBinaryArchive, PipelineBinaryBlob, validate_archive};
use crate::{Backend, Error, Result};

const CACHE_MAGIC: &[u8; 8] = b"GPBIN001";
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BINARY_COUNT: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipelineBinaryCacheIdentity {
    pub device_uuid: [u8; vk::UUID_SIZE],
    pub driver_uuid: [u8; vk::UUID_SIZE],
    pub driver_version: u32,
}

impl PipelineBinaryCacheIdentity {
    pub fn from_device(device: &Backend) -> Self {
        let info = device.device_info();
        Self {
            device_uuid: info.device_uuid,
            driver_uuid: info.driver_uuid,
            driver_version: info.driver_version,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PipelineBinaryArchiveCache {
    root: PathBuf,
    identity: PipelineBinaryCacheIdentity,
}

impl PipelineBinaryArchiveCache {
    pub fn new(root: impl Into<PathBuf>, identity: PipelineBinaryCacheIdentity) -> Self {
        Self {
            root: root.into(),
            identity,
        }
    }

    pub fn load(&self, pipeline_key: &[u8]) -> Result<Option<PipelineBinaryArchive>> {
        let path = self.archive_path(pipeline_key)?;
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(cache_io_error("inspect", &path, error)),
        };
        if metadata.len() > MAX_ARCHIVE_BYTES {
            return Err(Error::Validation(format!(
                "pipeline binary cache archive {} exceeds {MAX_ARCHIVE_BYTES} bytes",
                path.display()
            )));
        }
        let bytes = fs::read(&path).map_err(|error| cache_io_error("read", &path, error))?;
        decode_archive(&bytes, self.identity, pipeline_key).map(Some)
    }

    pub fn store(&self, pipeline_key: &[u8], archive: &PipelineBinaryArchive) -> Result<()> {
        validate_archive(archive)?;
        let path = self.archive_path(pipeline_key)?;
        let parent = path.parent().ok_or_else(|| {
            Error::Validation("pipeline binary cache archive has no parent directory".into())
        })?;
        fs::create_dir_all(parent)
            .map_err(|error| cache_io_error("create directory for", parent, error))?;
        let bytes = encode_archive(self.identity, pipeline_key, archive)?;
        let temporary = path.with_extension(format!("gpb.tmp-{}", std::process::id()));
        let write_result = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &path)
        })();
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary);
            return Err(cache_io_error("write", &path, error));
        }
        Ok(())
    }

    fn archive_path(&self, pipeline_key: &[u8]) -> Result<PathBuf> {
        validate_pipeline_key(pipeline_key)?;
        let identity = format!(
            "{}-{}-{:08x}",
            hex(&self.identity.device_uuid),
            hex(&self.identity.driver_uuid),
            self.identity.driver_version
        );
        Ok(self
            .root
            .join("v1")
            .join(identity)
            .join(format!("{}.gpb", hex(pipeline_key))))
    }
}

fn encode_archive(
    identity: PipelineBinaryCacheIdentity,
    pipeline_key: &[u8],
    archive: &PipelineBinaryArchive,
) -> Result<Vec<u8>> {
    validate_pipeline_key(pipeline_key)?;
    validate_archive(archive)?;
    if archive.binaries.len() > MAX_BINARY_COUNT {
        return Err(Error::Validation(format!(
            "pipeline binary cache archive has too many binaries: {}",
            archive.binaries.len()
        )));
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(CACHE_MAGIC);
    bytes.extend_from_slice(&identity.device_uuid);
    bytes.extend_from_slice(&identity.driver_uuid);
    bytes.extend_from_slice(&identity.driver_version.to_le_bytes());
    push_len(&mut bytes, pipeline_key.len())?;
    bytes.extend_from_slice(pipeline_key);
    push_len(&mut bytes, archive.binaries.len())?;
    for binary in &archive.binaries {
        push_len(&mut bytes, binary.key.len())?;
        let data_len = u64::try_from(binary.data.len())
            .map_err(|_| Error::Validation("pipeline binary payload length exceeds u64".into()))?;
        bytes.extend_from_slice(&data_len.to_le_bytes());
        bytes.extend_from_slice(&binary.key);
        bytes.extend_from_slice(&binary.data);
    }
    if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
        return Err(Error::Validation(format!(
            "pipeline binary cache archive exceeds {MAX_ARCHIVE_BYTES} bytes"
        )));
    }
    Ok(bytes)
}

fn decode_archive(
    bytes: &[u8],
    identity: PipelineBinaryCacheIdentity,
    pipeline_key: &[u8],
) -> Result<PipelineBinaryArchive> {
    let mut input = bytes;
    require_bytes(&mut input, CACHE_MAGIC, "magic")?;
    require_bytes(&mut input, &identity.device_uuid, "device UUID")?;
    require_bytes(&mut input, &identity.driver_uuid, "driver UUID")?;
    require_bytes(
        &mut input,
        &identity.driver_version.to_le_bytes(),
        "driver version",
    )?;
    let stored_key_len = take_u32(&mut input, "pipeline key length")? as usize;
    let stored_key = take(&mut input, stored_key_len, "pipeline key")?;
    if stored_key != pipeline_key {
        return Err(Error::Validation(
            "pipeline binary cache pipeline key does not match its lookup key".into(),
        ));
    }
    let binary_count = take_u32(&mut input, "binary count")? as usize;
    if binary_count == 0 || binary_count > MAX_BINARY_COUNT {
        return Err(Error::Validation(format!(
            "pipeline binary cache has invalid binary count {binary_count}"
        )));
    }
    let mut binaries = Vec::with_capacity(binary_count);
    for _ in 0..binary_count {
        let key_len = take_u32(&mut input, "binary key length")? as usize;
        let data_len = take_u64(&mut input, "binary payload length")?;
        let data_len = usize::try_from(data_len).map_err(|_| {
            Error::Validation("pipeline binary cache payload exceeds host address space".into())
        })?;
        let key = take(&mut input, key_len, "binary key")?.to_vec();
        let data = take(&mut input, data_len, "binary payload")?.to_vec();
        binaries.push(PipelineBinaryBlob { key, data });
    }
    if !input.is_empty() {
        return Err(Error::Validation(
            "pipeline binary cache has trailing bytes".into(),
        ));
    }
    let archive = PipelineBinaryArchive { binaries };
    validate_archive(&archive)?;
    Ok(archive)
}

fn validate_pipeline_key(key: &[u8]) -> Result<()> {
    if key.is_empty() || key.len() > vk::MAX_PIPELINE_BINARY_KEY_SIZE_KHR {
        return Err(Error::Validation(format!(
            "pipeline cache has invalid pipeline key size {}",
            key.len()
        )));
    }
    Ok(())
}

fn push_len(output: &mut Vec<u8>, len: usize) -> Result<()> {
    let len = u32::try_from(len)
        .map_err(|_| Error::Validation("pipeline binary cache field exceeds u32".into()))?;
    output.extend_from_slice(&len.to_le_bytes());
    Ok(())
}

fn take<'a>(input: &mut &'a [u8], len: usize, label: &str) -> Result<&'a [u8]> {
    if input.len() < len {
        return Err(Error::Validation(format!(
            "pipeline binary cache is truncated at {label}"
        )));
    }
    let (value, rest) = input.split_at(len);
    *input = rest;
    Ok(value)
}

fn take_u32(input: &mut &[u8], label: &str) -> Result<u32> {
    let bytes: [u8; 4] = take(input, 4, label)?.try_into().expect("fixed u32 width");
    Ok(u32::from_le_bytes(bytes))
}

fn take_u64(input: &mut &[u8], label: &str) -> Result<u64> {
    let bytes: [u8; 8] = take(input, 8, label)?.try_into().expect("fixed u64 width");
    Ok(u64::from_le_bytes(bytes))
}

fn require_bytes(input: &mut &[u8], expected: &[u8], label: &str) -> Result<()> {
    if take(input, expected.len(), label)? != expected {
        return Err(Error::Validation(format!(
            "pipeline binary cache {label} mismatch"
        )));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn cache_io_error(operation: &str, path: &Path, error: std::io::Error) -> Error {
    Error::Validation(format!(
        "{operation} pipeline binary cache {}: {error}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> PipelineBinaryCacheIdentity {
        PipelineBinaryCacheIdentity {
            device_uuid: [1; vk::UUID_SIZE],
            driver_uuid: [2; vk::UUID_SIZE],
            driver_version: 3,
        }
    }

    fn archive() -> PipelineBinaryArchive {
        PipelineBinaryArchive {
            binaries: vec![PipelineBinaryBlob {
                key: vec![4, 5],
                data: vec![6, 7, 8],
            }],
        }
    }

    #[test]
    fn archive_round_trip_binds_identity_and_full_pipeline_key() {
        let bytes = encode_archive(identity(), &[9, 10], &archive()).unwrap();
        assert_eq!(
            decode_archive(&bytes, identity(), &[9, 10]).unwrap(),
            archive()
        );
        assert!(decode_archive(&bytes, identity(), &[9, 11]).is_err());
        let mut other = identity();
        other.driver_version += 1;
        assert!(decode_archive(&bytes, other, &[9, 10]).is_err());
    }

    #[test]
    fn archive_decoder_rejects_truncation_and_trailing_data() {
        let bytes = encode_archive(identity(), &[9], &archive()).unwrap();
        assert!(decode_archive(&bytes[..bytes.len() - 1], identity(), &[9]).is_err());
        let mut trailing = bytes;
        trailing.push(0);
        assert!(decode_archive(&trailing, identity(), &[9]).is_err());
    }
}
