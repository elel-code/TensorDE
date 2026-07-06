//! Native `.gtex` texture metadata used by `.gscn` resource ingest.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/exe/texture-and-format.md`
//! - `references/godot/servers/rendering/storage/texture_storage.h`
//! - `references/godot/servers/rendering/rendering_device.h`

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

use crate::engine::scene_engine::SceneTextureFormat;
use crate::renderer::RendererPlanError;

const GILDER_SCENE_TEXTURE_MAGIC: &[u8; 8] = b"GDTEX002";
const GILDER_SCENE_TEXTURE_HEADER_BYTES: usize = 32;
const GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK: u32 = 1;
const GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK: u32 = 3;
const GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK: u32 = 7;
const GILDER_SCENE_TEXTURE_FORMAT_R8_UNORM: u32 = 9;
const GILDER_SCENE_TEXTURE_FORMAT_R8G8B8A8_UNORM: u32 = 37;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BinarySceneTextureMetadata {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: SceneTextureFormat,
    pub(super) mip_count: u32,
    pub(super) payload_bytes: u64,
}

pub(super) fn binary_scene_texture_metadata(
    path: &Path,
) -> Result<Option<BinarySceneTextureMetadata>, RendererPlanError> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gtex"))
    {
        return Ok(None);
    }
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(RendererPlanError::PackageLoad(format!(
                "failed to open native gtex {}: {err}",
                path.display()
            )));
        }
    };
    let mut header = [0u8; GILDER_SCENE_TEXTURE_HEADER_BYTES];
    file.read_exact(&mut header).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to read native gtex header {}: {err}",
            path.display()
        ))
    })?;
    binary_scene_texture_metadata_from_header(path, &header).map(Some)
}

fn binary_scene_texture_metadata_from_header(
    path: &Path,
    header: &[u8; GILDER_SCENE_TEXTURE_HEADER_BYTES],
) -> Result<BinarySceneTextureMetadata, RendererPlanError> {
    if header.get(0..8) != Some(GILDER_SCENE_TEXTURE_MAGIC.as_slice()) {
        return Err(RendererPlanError::PackageLoad(format!(
            "{} is not a native GDTEX002 texture",
            path.display()
        )));
    }
    let width = u32::from_le_bytes(header[8..12].try_into().expect("gtex width bytes"));
    let height = u32::from_le_bytes(header[12..16].try_into().expect("gtex height bytes"));
    let format_id = u32::from_le_bytes(header[16..20].try_into().expect("gtex format bytes"));
    let mip_count = u32::from_le_bytes(header[20..24].try_into().expect("gtex mip bytes"));
    let payload_bytes = u64::from_le_bytes(header[24..32].try_into().expect("gtex payload bytes"));
    if width == 0 || height == 0 {
        return Err(RendererPlanError::PackageLoad(format!(
            "{} has invalid zero-sized native texture metadata {width}x{height}",
            path.display()
        )));
    }
    if mip_count == 0 {
        return Err(RendererPlanError::PackageLoad(format!(
            "{} has invalid zero mip native texture metadata",
            path.display()
        )));
    }
    let format = scene_texture_format(format_id).ok_or_else(|| {
        RendererPlanError::PackageLoad(format!(
            "{} has unsupported native gtex format id {format_id}",
            path.display()
        ))
    })?;
    Ok(BinarySceneTextureMetadata {
        width,
        height,
        format,
        mip_count,
        payload_bytes,
    })
}

fn scene_texture_format(format: u32) -> Option<SceneTextureFormat> {
    match format {
        GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK => {
            Some(SceneTextureFormat::Bc1RgbaUnormBlock)
        }
        GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK => Some(SceneTextureFormat::Bc3UnormBlock),
        GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK => Some(SceneTextureFormat::Bc7UnormBlock),
        GILDER_SCENE_TEXTURE_FORMAT_R8_UNORM => Some(SceneTextureFormat::R8Unorm),
        GILDER_SCENE_TEXTURE_FORMAT_R8G8B8A8_UNORM => Some(SceneTextureFormat::R8G8B8A8Unorm),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reads_native_gtex_header_metadata_without_payload_copy() {
        let root = unique_test_dir("gilder-gtex-metadata");
        fs::create_dir_all(&root).expect("test dir");
        let path = root.join("eye.gtex");
        write_gtex_header(
            &path,
            663,
            230,
            GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK,
            1,
            155_520,
        );

        let metadata = binary_scene_texture_metadata(&path)
            .expect("metadata result")
            .expect("metadata");

        assert_eq!(metadata.width, 663);
        assert_eq!(metadata.height, 230);
        assert_eq!(metadata.format, SceneTextureFormat::Bc7UnormBlock);
        assert_eq!(metadata.mip_count, 1);
        assert_eq!(metadata.payload_bytes, 155_520);
        fs::remove_dir_all(root).expect("remove test dir");
    }

    #[test]
    fn maps_uncompressed_native_gtex_format_ids() {
        let header = gtex_header(32, 16, GILDER_SCENE_TEXTURE_FORMAT_R8G8B8A8_UNORM, 6, 2732);
        let metadata = binary_scene_texture_metadata_from_header(Path::new("atlas.gtex"), &header)
            .expect("metadata");

        assert_eq!(metadata.format, SceneTextureFormat::R8G8B8A8Unorm);
        assert_eq!(metadata.mip_count, 6);
        assert_eq!(metadata.payload_bytes, 2732);
    }

    #[test]
    fn missing_gtex_keeps_metadata_absent_for_pure_fact_tests() {
        let metadata = binary_scene_texture_metadata(Path::new("/tmp/gilder-missing-test.gtex"))
            .expect("metadata result");

        assert_eq!(metadata, None);
    }

    fn write_gtex_header(
        path: &Path,
        width: u32,
        height: u32,
        format: u32,
        mip_count: u32,
        payload_bytes: u64,
    ) {
        let mut file = fs::File::create(path).expect("create gtex");
        file.write_all(&gtex_header(
            width,
            height,
            format,
            mip_count,
            payload_bytes,
        ))
        .expect("write gtex header");
    }

    fn gtex_header(
        width: u32,
        height: u32,
        format: u32,
        mip_count: u32,
        payload_bytes: u64,
    ) -> [u8; GILDER_SCENE_TEXTURE_HEADER_BYTES] {
        let mut header = [0u8; GILDER_SCENE_TEXTURE_HEADER_BYTES];
        header[0..8].copy_from_slice(GILDER_SCENE_TEXTURE_MAGIC);
        header[8..12].copy_from_slice(&width.to_le_bytes());
        header[12..16].copy_from_slice(&height.to_le_bytes());
        header[16..20].copy_from_slice(&format.to_le_bytes());
        header[20..24].copy_from_slice(&mip_count.to_le_bytes());
        header[24..32].copy_from_slice(&payload_bytes.to_le_bytes());
        header
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }
}
