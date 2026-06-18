use super::format::{FORMAT_VERSION, MANIFEST_FILE, MANIFEST_TOML_FILE};
use super::manifest::{Manifest, ManifestError};
use super::path::PackagePath;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

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

    let manifest = load_manifest(root)?;
    manifest
        .validate()
        .map_err(PackageLoadError::InvalidManifest)?;
    validate_referenced_resources(root, &manifest)?;

    Ok(WallpaperPackage {
        root: root.to_path_buf(),
        manifest,
    })
}

fn load_manifest(root: &Path) -> Result<Manifest, PackageLoadError> {
    let json_path = root.join(MANIFEST_FILE);
    let json_error = match read_manifest_file(&json_path, ManifestSyntax::Json) {
        Ok(manifest) => return Ok(manifest),
        Err(err) => err,
    };
    if !is_missing_manifest_file(&json_error) {
        return Err(json_error);
    }

    let toml_path = root.join(MANIFEST_TOML_FILE);
    let toml_error = match read_manifest_file(&toml_path, ManifestSyntax::Toml) {
        Ok(manifest) => return Ok(manifest),
        Err(err) => err,
    };
    if !is_missing_manifest_file(&toml_error) {
        return Err(toml_error);
    }

    Err(PackageLoadError::MissingManifest {
        json_path,
        toml_path,
    })
}

fn read_manifest_file(path: &Path, syntax: ManifestSyntax) -> Result<Manifest, PackageLoadError> {
    let contents = fs::read_to_string(path).map_err(|source| PackageLoadError::ReadManifest {
        path: path.to_path_buf(),
        source,
    })?;
    match syntax {
        ManifestSyntax::Json => {
            serde_json::from_str(&contents).map_err(|source| PackageLoadError::ParseManifest {
                path: path.to_path_buf(),
                source: ManifestParseError::Json(source),
            })
        }
        ManifestSyntax::Toml => {
            toml::from_str(&contents).map_err(|source| PackageLoadError::ParseManifest {
                path: path.to_path_buf(),
                source: ManifestParseError::Toml(source),
            })
        }
    }
}

fn is_missing_manifest_file(error: &PackageLoadError) -> bool {
    matches!(
        error,
        PackageLoadError::ReadManifest { source, .. }
            if source.kind() == io::ErrorKind::NotFound
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestSyntax {
    Json,
    Toml,
}

pub fn load_gwp(
    archive_path: impl AsRef<Path>,
    extract_dir: impl AsRef<Path>,
) -> Result<WallpaperPackage, PackageArchiveError> {
    unpack_gwp(archive_path, extract_dir.as_ref())?;
    load_gwpdir(extract_dir).map_err(PackageArchiveError::InvalidPackage)
}

pub fn pack_gwp(
    source_dir: impl AsRef<Path>,
    archive_path: impl AsRef<Path>,
) -> Result<(), PackageArchiveError> {
    let source_dir = source_dir.as_ref();
    let archive_path = archive_path.as_ref();
    let package = load_gwpdir(source_dir).map_err(PackageArchiveError::InvalidPackage)?;

    if let Some(parent) = archive_path.parent() {
        fs::create_dir_all(parent).map_err(PackageArchiveError::CreateDir)?;
    }
    let archive_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(archive_path)
        .map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                PackageArchiveError::ArchiveExists(archive_path.to_path_buf())
            } else {
                PackageArchiveError::CreateArchive(source)
            }
        })?;
    let mut writer = ZipWriter::new(archive_file);
    let stored = file_options(CompressionMethod::Stored);
    let deflated = file_options(CompressionMethod::Deflated);

    let manifest_json = serde_json::to_vec_pretty(&package.manifest)
        .map_err(PackageArchiveError::SerializeManifest)?;
    writer
        .start_file(MANIFEST_FILE, deflated)
        .map_err(PackageArchiveError::Zip)?;
    writer
        .write_all(&manifest_json)
        .map_err(PackageArchiveError::WriteFile)?;

    add_directory_to_zip(&mut writer, source_dir, source_dir, stored, deflated)?;
    writer.finish().map_err(PackageArchiveError::Zip)?;
    Ok(())
}

pub fn unpack_gwp(
    archive_path: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
) -> Result<(), PackageArchiveError> {
    let archive_path = archive_path.as_ref();
    let output_dir = output_dir.as_ref();
    if output_dir.exists()
        && fs::read_dir(output_dir)
            .map_err(PackageArchiveError::ReadDir)?
            .next()
            .is_some()
    {
        return Err(PackageArchiveError::OutputExists(output_dir.to_path_buf()));
    }
    fs::create_dir_all(output_dir).map_err(PackageArchiveError::CreateDir)?;

    let archive_file = File::open(archive_path).map_err(PackageArchiveError::OpenArchive)?;
    let mut archive = ZipArchive::new(archive_file).map_err(PackageArchiveError::Zip)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(PackageArchiveError::Zip)?;
        let enclosed_name = entry
            .enclosed_name()
            .ok_or_else(|| PackageArchiveError::UnsafeArchivePath(entry.name().to_owned()))?;
        let output_path = output_dir.join(enclosed_name);
        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(PackageArchiveError::CreateDir)?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(PackageArchiveError::CreateDir)?;
        }
        let mut output_file = File::create(&output_path).map_err(PackageArchiveError::WriteFile)?;
        io::copy(&mut entry, &mut output_file).map_err(PackageArchiveError::Copy)?;
    }

    load_gwpdir(output_dir).map_err(PackageArchiveError::InvalidPackage)?;
    Ok(())
}

fn file_options(compression_method: CompressionMethod) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(compression_method)
        .unix_permissions(0o644)
}

fn add_directory_to_zip(
    writer: &mut ZipWriter<File>,
    root: &Path,
    dir: &Path,
    directory_options: SimpleFileOptions,
    file_options: SimpleFileOptions,
) -> Result<(), PackageArchiveError> {
    let mut entries = fs::read_dir(dir)
        .map_err(PackageArchiveError::ReadDir)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(PackageArchiveError::ReadDirEntry)?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| PackageArchiveError::InvalidSourcePath(path.clone()))?;
        if relative == Path::new(MANIFEST_FILE) || relative == Path::new(MANIFEST_TOML_FILE) {
            continue;
        }
        let zip_name = path_to_zip_name(relative)?;
        if path.is_dir() {
            writer
                .add_directory(zip_name, directory_options)
                .map_err(PackageArchiveError::Zip)?;
            add_directory_to_zip(writer, root, &path, directory_options, file_options)?;
        } else if path.is_file() {
            let options = if should_store(&path) {
                file_options.compression_method(CompressionMethod::Stored)
            } else {
                file_options
            };
            writer
                .start_file(zip_name, options)
                .map_err(PackageArchiveError::Zip)?;
            let mut source_file = File::open(&path).map_err(PackageArchiveError::OpenFile)?;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = source_file
                    .read(&mut buffer)
                    .map_err(PackageArchiveError::ReadFile)?;
                if read == 0 {
                    break;
                }
                writer
                    .write_all(&buffer[..read])
                    .map_err(PackageArchiveError::WriteFile)?;
            }
        }
    }
    Ok(())
}

fn path_to_zip_name(path: &Path) -> Result<String, PackageArchiveError> {
    let name = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        Err(PackageArchiveError::InvalidZipPath(name))
    } else {
        Ok(name)
    }
}

fn should_store(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "avif"
                    | "webp"
                    | "jpg"
                    | "jpeg"
                    | "png"
                    | "gif"
                    | "mp4"
                    | "m4v"
                    | "webm"
                    | "mkv"
                    | "mov"
                    | "avi"
            )
        })
        .unwrap_or(false)
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
pub enum ManifestParseError {
    Json(serde_json::Error),
    Toml(toml::de::Error),
}

impl fmt::Display for ManifestParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(source) => write!(f, "JSON parse error: {source}"),
            Self::Toml(source) => write!(f, "TOML parse error: {source}"),
        }
    }
}

impl std::error::Error for ManifestParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(source) => Some(source),
            Self::Toml(source) => Some(source),
        }
    }
}

#[derive(Debug)]
pub enum PackageLoadError {
    ReadPackageRoot {
        path: PathBuf,
        source: io::Error,
    },
    NotDirectory(PathBuf),
    MissingManifest {
        json_path: PathBuf,
        toml_path: PathBuf,
    },
    ReadManifest {
        path: PathBuf,
        source: io::Error,
    },
    ParseManifest {
        path: PathBuf,
        source: ManifestParseError,
    },
    InvalidManifest(ManifestError),
    MissingResource {
        package_path: PackagePath,
        path: PathBuf,
    },
}

#[derive(Debug)]
pub enum PackageArchiveError {
    InvalidPackage(PackageLoadError),
    CreateDir(io::Error),
    ReadDir(io::Error),
    ReadDirEntry(io::Error),
    CreateArchive(io::Error),
    ArchiveExists(PathBuf),
    OpenArchive(io::Error),
    OpenFile(io::Error),
    ReadFile(io::Error),
    WriteFile(io::Error),
    SerializeManifest(serde_json::Error),
    Copy(io::Error),
    Zip(zip::result::ZipError),
    OutputExists(PathBuf),
    InvalidSourcePath(PathBuf),
    InvalidZipPath(String),
    UnsafeArchivePath(String),
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
            Self::MissingManifest {
                json_path,
                toml_path,
            } => write!(
                f,
                "package manifest not found; expected {} or {}",
                json_path.display(),
                toml_path.display()
            ),
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
            Self::NotDirectory(_) | Self::MissingManifest { .. } | Self::MissingResource { .. } => {
                None
            }
        }
    }
}

impl fmt::Display for PackageArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPackage(source) => write!(f, "invalid package: {source}"),
            Self::CreateDir(source) => write!(f, "failed to create directory: {source}"),
            Self::ReadDir(source) => write!(f, "failed to read directory: {source}"),
            Self::ReadDirEntry(source) => write!(f, "failed to read directory entry: {source}"),
            Self::CreateArchive(source) => write!(f, "failed to create archive: {source}"),
            Self::ArchiveExists(path) => write!(f, "archive already exists: {}", path.display()),
            Self::OpenArchive(source) => write!(f, "failed to open archive: {source}"),
            Self::OpenFile(source) => write!(f, "failed to open package file: {source}"),
            Self::ReadFile(source) => write!(f, "failed to read package file: {source}"),
            Self::WriteFile(source) => write!(f, "failed to write package archive: {source}"),
            Self::SerializeManifest(source) => {
                write!(
                    f,
                    "failed to serialize canonical package manifest: {source}"
                )
            }
            Self::Copy(source) => write!(f, "failed to copy archive entry: {source}"),
            Self::Zip(source) => write!(f, "zip error: {source}"),
            Self::OutputExists(path) => {
                write!(f, "output directory is not empty: {}", path.display())
            }
            Self::InvalidSourcePath(path) => {
                write!(f, "invalid package source path: {}", path.display())
            }
            Self::InvalidZipPath(path) => write!(f, "invalid zip entry path: {path}"),
            Self::UnsafeArchivePath(path) => write!(f, "unsafe zip entry path: {path}"),
        }
    }
}

impl std::error::Error for PackageArchiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidPackage(source) => Some(source),
            Self::CreateDir(source)
            | Self::ReadDir(source)
            | Self::ReadDirEntry(source)
            | Self::CreateArchive(source)
            | Self::OpenArchive(source)
            | Self::OpenFile(source)
            | Self::ReadFile(source)
            | Self::WriteFile(source)
            | Self::Copy(source) => Some(source),
            Self::SerializeManifest(source) => Some(source),
            Self::Zip(source) => Some(source),
            Self::OutputExists(_)
            | Self::ArchiveExists(_)
            | Self::InvalidSourcePath(_)
            | Self::InvalidZipPath(_)
            | Self::UnsafeArchivePath(_) => None,
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
    fn loads_valid_toml_gwpdir() {
        let package_dir = TestPackageDir::new("valid-toml");
        package_dir.write_file(
            MANIFEST_TOML_FILE,
            &toml_static_manifest("org.example.static-toml"),
        );
        package_dir.write_file("assets/wallpaper.svg", "<svg></svg>");
        package_dir.write_file("previews/thumbnail.svg", "<svg></svg>");

        let package = load_gwpdir(package_dir.path()).unwrap();
        assert_eq!(package.manifest.id, "org.example.static-toml");
    }

    #[test]
    fn prefers_json_manifest_when_both_manifest_formats_exist() {
        let package_dir = TestPackageDir::new("json-precedence");
        package_dir.write_file(
            MANIFEST_FILE,
            r##"
            {
              "format": "gilder.wallpaper",
              "format_version": 1,
              "id": "org.example.json",
              "version": "1.0.0",
              "title": "JSON",
              "kind": "static-image",
              "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.svg"
              }
            }
            "##,
        );
        package_dir.write_file(
            MANIFEST_TOML_FILE,
            &toml_static_manifest("org.example.toml"),
        );
        package_dir.write_file("assets/wallpaper.svg", "<svg></svg>");

        let package = load_gwpdir(package_dir.path()).unwrap();
        assert_eq!(package.manifest.id, "org.example.json");
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

    #[test]
    fn packs_and_unpacks_gwp_archive() {
        let package_dir = TestPackageDir::new("archive-source");
        package_dir.write_file(
            MANIFEST_FILE,
            r##"
            {
              "format": "gilder.wallpaper",
              "format_version": 1,
              "id": "org.example.archive",
              "version": "1.0.0",
              "title": "Archive",
              "kind": "static-image",
              "entry": {
                "type": "static-image",
                "source": "assets/wallpaper.svg"
              }
            }
            "##,
        );
        package_dir.write_file("assets/wallpaper.svg", "<svg></svg>");

        let archive = std::env::temp_dir().join(format!(
            "gilder-test-archive-{}-{}.gwp",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let unpacked = TestPackageDir::new("archive-unpacked");
        unpacked.remove();

        pack_gwp(package_dir.path(), &archive).unwrap();
        unpack_gwp(&archive, unpacked.path()).unwrap();

        let package = load_gwpdir(unpacked.path()).unwrap();
        assert_eq!(package.manifest.id, "org.example.archive");
        assert!(unpacked.path().join("assets/wallpaper.svg").exists());

        let _ = fs::remove_file(archive);
    }

    #[test]
    fn packs_toml_gwpdir_as_canonical_json_archive() {
        let package_dir = TestPackageDir::new("toml-archive-source");
        package_dir.write_file(
            MANIFEST_TOML_FILE,
            &toml_static_manifest("org.example.toml-archive"),
        );
        package_dir.write_file("assets/wallpaper.svg", "<svg></svg>");
        package_dir.write_file("previews/thumbnail.svg", "<svg></svg>");

        let archive = std::env::temp_dir().join(format!(
            "gilder-test-toml-archive-{}-{}.gwp",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let unpacked = TestPackageDir::new("toml-archive-unpacked");
        unpacked.remove();

        pack_gwp(package_dir.path(), &archive).unwrap();
        unpack_gwp(&archive, unpacked.path()).unwrap();

        let package = load_gwpdir(unpacked.path()).unwrap();
        assert_eq!(package.manifest.id, "org.example.toml-archive");
        assert!(unpacked.path().join(MANIFEST_FILE).exists());
        assert!(!unpacked.path().join(MANIFEST_TOML_FILE).exists());

        let _ = fs::remove_file(archive);
    }

    fn toml_static_manifest(id: &str) -> String {
        format!(
            r##"
            format = "gilder.wallpaper"
            format_version = 1
            id = "{id}"
            version = "1.0.0"
            title = "TOML Static"
            kind = "static-image"

            [preview]
            thumbnail = "previews/thumbnail.svg"

            [entry]
            type = "static-image"
            source = "assets/wallpaper.svg"
            fit = "cover"
            "##
        )
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

        fn remove(&self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    impl Drop for TestPackageDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
