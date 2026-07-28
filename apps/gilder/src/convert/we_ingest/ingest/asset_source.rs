//! Wallpaper Engine project, package, and installed built-in asset resolution.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use super::utility_layer::builtin_utility_asset;
use super::{WeIngestError, normalize_we_path};
use crate::convert::we_ingest::ir::WeIrResourceSource;
use crate::convert::we_ingest::pkg::ScenePackage;

const BUILTIN_ASSET_ROOT_ENV: &str = "GILDER_WALLPAPER_ENGINE_ASSET_ROOT";

#[derive(Debug, Clone)]
pub(super) struct WeAsset {
    pub(super) bytes: Vec<u8>,
    pub(super) source: WeIrResourceSource,
}

#[derive(Debug, Clone)]
pub(super) struct WeAssetSource {
    root: PathBuf,
    package: Option<ScenePackage>,
    builtin_roots: Vec<PathBuf>,
}

impl WeAssetSource {
    pub(super) fn open(root: PathBuf) -> Result<Self, WeIngestError> {
        let pkg_path = root.join("scene.pkg");
        let package = if pkg_path.is_file() {
            Some(ScenePackage::from_path(&pkg_path)?)
        } else {
            None
        };
        let builtin_roots = discover_builtin_asset_roots(&root);
        Ok(Self {
            root,
            package,
            builtin_roots,
        })
    }

    pub(super) fn read_required_asset(
        &self,
        path: impl AsRef<str>,
    ) -> Result<WeAsset, WeIngestError> {
        let path = normalize_we_path(path.as_ref());
        self.read_optional_asset(&path)?
            .ok_or(WeIngestError::MissingAsset(path))
    }

    pub(super) fn read_optional_asset(
        &self,
        path: impl AsRef<str>,
    ) -> Result<Option<WeAsset>, WeIngestError> {
        let path = normalize_we_path(path.as_ref());
        validate_relative_we_path(&path)?;
        if let Some(asset) = read_file_asset(self.root.join(&path), WeIrResourceSource::LooseFile)?
        {
            return Ok(Some(asset));
        }
        if let Some(package) = &self.package
            && let Some(bytes) = package.entry_bytes(&path)
        {
            return Ok(Some(WeAsset {
                bytes: bytes.to_vec(),
                source: WeIrResourceSource::ScenePackage,
            }));
        }
        for root in &self.builtin_roots {
            if let Some(asset) = read_file_asset(root.join(&path), WeIrResourceSource::Builtin)? {
                return Ok(Some(asset));
            }
        }
        if let Some(bytes) = builtin_utility_asset(&path) {
            return Ok(Some(WeAsset {
                bytes: bytes.to_vec(),
                source: WeIrResourceSource::Builtin,
            }));
        }
        Ok(None)
    }
}

fn read_file_asset(
    path: PathBuf,
    source_kind: WeIrResourceSource,
) -> Result<Option<WeAsset>, WeIngestError> {
    if !path.is_file() {
        return Ok(None);
    }
    fs::read(&path)
        .map(|bytes| {
            Some(WeAsset {
                bytes,
                source: source_kind,
            })
        })
        .map_err(|source| WeIngestError::Io { path, source })
}

fn validate_relative_we_path(path: &str) -> Result<(), WeIngestError> {
    if Path::new(path).is_absolute()
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(WeIngestError::UnsafePath(path.to_owned()));
    }
    Ok(())
}

fn discover_builtin_asset_roots(project_root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = env::var_os(BUILTIN_ASSET_ROOT_ENV) {
        push_existing_unique(&mut roots, PathBuf::from(root));
    }
    for ancestor in project_root.ancestors() {
        // The Linux Wallpaper Engine distribution used by the native runtime carries Windows
        // system-font faces under `files/share/fonts`. Keep this as a built-in root so virtual
        // text resources such as `fonts/arial.ttf` are embedded into the resulting `.gscene`.
        push_existing_unique(&mut roots, ancestor.join("files/share"));
        if ancestor.file_name().and_then(|name| name.to_str()) == Some("steamapps") {
            push_existing_unique(&mut roots, ancestor.join("common/wallpaper_engine/assets"));
        }
    }
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        for candidate in [
            home.join(".local/share/Steam/steamapps/common/wallpaper_engine/assets"),
            home.join(".steam/steam/steamapps/common/wallpaper_engine/assets"),
        ] {
            push_existing_unique(&mut roots, candidate);
        }
    }
    if let Ok(current_dir) = env::current_dir() {
        push_existing_unique(
            &mut roots,
            current_dir.join("artifacts/wallpaper-engine-workshop/steamcmd-root/assets"),
        );
        push_existing_unique(
            &mut roots,
            current_dir.join("artifacts/wallpaper-engine-workshop/steamcmd-root/files/share"),
        );
    }
    roots
}

fn push_existing_unique(roots: &mut Vec<PathBuf>, candidate: PathBuf) {
    if candidate.is_dir() && !roots.contains(&candidate) {
        roots.push(candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_workshop_project_discovers_sibling_install_assets() {
        let root = Path::new("/steam/steamapps/workshop/content/431960/3742497499");
        let candidates = root
            .ancestors()
            .filter(|ancestor| {
                ancestor.file_name().and_then(|name| name.to_str()) == Some("steamapps")
            })
            .map(|steamapps| steamapps.join("common/wallpaper_engine/assets"))
            .collect::<Vec<_>>();
        assert_eq!(
            candidates,
            vec![PathBuf::from(
                "/steam/steamapps/common/wallpaper_engine/assets"
            )]
        );
    }
}
