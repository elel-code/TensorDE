//! Bounded catalog change detection for the launcher runtime.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{LauncherCatalogError, LauncherConfig};

use super::LauncherCatalog;

/// Cold catalog data plus a bounded filesystem fingerprint for live refresh.
///
/// The watcher deliberately does not retain open directory handles or create a
/// background task. A caller can check it from its own Compio event loop and
/// keep input/render work on the same completion runtime.
#[derive(Debug)]
pub struct LauncherCatalogWatcher {
    config: LauncherConfig,
    catalog: LauncherCatalog,
    fingerprint: CatalogFingerprint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CatalogFingerprint(Vec<FingerprintEntry>);

#[derive(Clone, Debug, Eq, PartialEq)]
struct FingerprintEntry {
    path: PathBuf,
    modified: Option<SystemTime>,
    length: u64,
    directory: bool,
}

impl LauncherCatalogWatcher {
    pub fn start(config: LauncherConfig) -> Result<Self, LauncherCatalogError> {
        let catalog = LauncherCatalog::discover(&config)?;
        let fingerprint = CatalogFingerprint::capture(&config)?;
        Ok(Self {
            config,
            catalog,
            fingerprint,
        })
    }

    pub fn catalog(&self) -> &LauncherCatalog {
        &self.catalog
    }

    /// Re-scan only after the bounded directory/file fingerprint changes.
    ///
    /// A failed refresh leaves both the previous catalog and fingerprint in
    /// place, so a transient filesystem error cannot discard usable results.
    pub fn refresh_if_changed(&mut self) -> Result<bool, LauncherCatalogError> {
        let next_fingerprint = CatalogFingerprint::capture(&self.config)?;
        if next_fingerprint == self.fingerprint {
            return Ok(false);
        }
        let next_catalog = LauncherCatalog::discover(&self.config)?;
        self.catalog = next_catalog;
        self.fingerprint = next_fingerprint;
        Ok(true)
    }
}

impl CatalogFingerprint {
    fn capture(config: &LauncherConfig) -> Result<Self, LauncherCatalogError> {
        let limit = config
            .max_catalog_entries
            .saturating_mul(2)
            .saturating_add(config.application_directories.len())
            .max(config.application_directories.len());
        let mut entries = Vec::new();
        for root in &config.application_directories {
            collect_fingerprint(root, &mut entries, limit)?;
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Self(entries))
    }
}

fn collect_fingerprint(
    path: &Path,
    entries: &mut Vec<FingerprintEntry>,
    limit: usize,
) -> Result<(), LauncherCatalogError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(LauncherCatalogError::Metadata {
                path: path.to_owned(),
                source,
            });
        }
    };
    if entries.len() == limit {
        return Err(LauncherCatalogError::FingerprintLimit { limit });
    }
    let directory = metadata.is_dir();
    entries.push(FingerprintEntry {
        path: path.to_owned(),
        modified: metadata.modified().ok(),
        length: metadata.len(),
        directory,
    });
    if !directory {
        return Ok(());
    }
    let mut children = fs::read_dir(path)
        .map_err(|source| LauncherCatalogError::Metadata {
            path: path.to_owned(),
            source,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| LauncherCatalogError::Metadata {
            path: path.to_owned(),
            source,
        })?;
    children.sort_by_key(|entry| entry.path());
    for child in children {
        let child_path = child.path();
        if child.file_type().is_ok_and(|kind| kind.is_dir())
            || child_path.extension().is_some_and(|ext| ext == "desktop")
        {
            collect_fingerprint(&child_path, entries, limit)?;
        }
    }
    Ok(())
}
