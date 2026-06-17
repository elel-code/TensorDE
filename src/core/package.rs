use super::format::{FORMAT_VERSION, MANIFEST_FILE};
use super::manifest::{Manifest, ManifestError};
use super::path::PackagePath;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageIdentity {
    pub id: String,
    pub version: String,
    pub format_version: u32,
}

impl PackageIdentity {
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            format_version: FORMAT_VERSION,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WallpaperPackage {
    pub root: PathBuf,
    pub manifest: Manifest,
}

pub fn load_gwpdir(root: impl AsRef<Path>) -> Result<WallpaperPackage, PackageLoadError> {
    let root = root.as_ref();
    let metadata = fs::metadata(root).map_err(|source| PackageLoadError::ReadPackageRoot {
        path: root.to_path_buf(),
        source,
    })?;
    if !metadata.is_dir() {
        return Err(PackageLoadError::NotDirectory(root.to_path_buf()));
    }

    let manifest_path = root.join(MANIFEST_FILE);
    let manifest_json =
        fs::read_to_string(&manifest_path).map_err(|source| PackageLoadError::ReadManifest {
            path: manifest_path.clone(),
            source,
        })?;
    let manifest: Manifest =
        serde_json::from_str(&manifest_json).map_err(|source| PackageLoadError::ParseManifest {
            path: manifest_path,
            source,
        })?;
    manifest
        .validate()
        .map_err(PackageLoadError::InvalidManifest)?;
    validate_referenced_resources(root, &manifest)?;

    Ok(WallpaperPackage {
        root: root.to_path_buf(),
        manifest,
    })
}

fn validate_referenced_resources(root: &Path, manifest: &Manifest) -> Result<(), PackageLoadError> {
    for package_path in manifest
        .referenced_paths()
        .map_err(PackageLoadError::InvalidManifest)?
    {
        let path = package_path.join_to(root);
        if !path.exists() {
            return Err(PackageLoadError::MissingResource { package_path, path });
        }
    }
    Ok(())
}

#[derive(Debug)]
pub enum PackageLoadError {
    ReadPackageRoot {
        path: PathBuf,
        source: io::Error,
    },
    NotDirectory(PathBuf),
    ReadManifest {
        path: PathBuf,
        source: io::Error,
    },
    ParseManifest {
        path: PathBuf,
        source: serde_json::Error,
    },
    InvalidManifest(ManifestError),
    MissingResource {
        package_path: PackagePath,
        path: PathBuf,
    },
}

impl fmt::Display for PackageLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadPackageRoot { path, source } => {
                write!(
                    f,
                    "failed to read package root {}: {source}",
                    path.display()
                )
            }
            Self::NotDirectory(path) => {
                write!(f, "package root is not a directory: {}", path.display())
            }
            Self::ReadManifest { path, source } => {
                write!(f, "failed to read manifest {}: {source}", path.display())
            }
            Self::ParseManifest { path, source } => {
                write!(f, "failed to parse manifest {}: {source}", path.display())
            }
            Self::InvalidManifest(source) => write!(f, "invalid manifest: {source}"),
            Self::MissingResource { package_path, path } => write!(
                f,
                "manifest references missing resource {} at {}",
                package_path,
                path.display()
            ),
        }
    }
}

impl std::error::Error for PackageLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadPackageRoot { source, .. } | Self::ReadManifest { source, .. } => {
                Some(source)
            }
            Self::ParseManifest { source, .. } => Some(source),
            Self::InvalidManifest(source) => Some(source),
            Self::NotDirectory(_) | Self::MissingResource { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn package_identity_uses_current_format_version() {
        let identity = PackageIdentity::new("org.example.wallpaper", "0.1.0");
        assert_eq!(identity.format_version, FORMAT_VERSION);
    }

    #[test]
    fn loads_valid_gwpdir() {
        let package_dir = TestPackageDir::new("valid");
        package_dir.write_file(
            MANIFEST_FILE,
            r##"
            {
              "format": "gilder.wallpaper",
              "format_version": 1,
              "id": "org.example.static",
              "version": "1.0.0",
              "title": "Example Static",
              "kind": "static-image",
              "preview": {
                "thumbnail": "previews/thumbnail.svg"
              },
              "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.svg"
              }
            }
            "##,
        );
        package_dir.write_file("assets/wallpaper.svg", "<svg></svg>");
        package_dir.write_file("previews/thumbnail.svg", "<svg></svg>");

        let package = load_gwpdir(package_dir.path()).unwrap();
        assert_eq!(package.manifest.id, "org.example.static");
    }

    #[test]
    fn rejects_gwpdir_with_missing_resource() {
        let package_dir = TestPackageDir::new("missing-resource");
        package_dir.write_file(
            MANIFEST_FILE,
            r#"
            {
              "format": "gilder.wallpaper",
              "format_version": 1,
              "id": "org.example.missing",
              "version": "1.0.0",
              "title": "Missing",
              "kind": "static-image",
              "entry": {
                "type": "static-image",
                "source": "assets/missing.svg"
              }
            }
            "#,
        );

        assert!(matches!(
            load_gwpdir(package_dir.path()),
            Err(PackageLoadError::MissingResource { .. })
        ));
    }

    struct TestPackageDir {
        path: PathBuf,
    }

    impl TestPackageDir {
        fn new(name: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("gilder-test-{name}-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write_file(&self, relative_path: &str, contents: &str) {
            let path = self.path.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TestPackageDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
