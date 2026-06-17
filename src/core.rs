//! Core types and constants for the Gilder wallpaper package format.

pub mod format;
pub mod manifest;
pub mod package;
pub mod path;

pub use format::{
    WallpaperKind, DIRECTORY_EXTENSION, FORMAT_NAME, FORMAT_VERSION, MANIFEST_FILE,
    PACKAGE_EXTENSION,
};
pub use manifest::{FitMode, Manifest, ManifestError, RuntimePolicy, WallpaperEntry};
pub use package::{load_gwpdir, PackageIdentity, PackageLoadError, WallpaperPackage};
pub use path::{PackagePath, PackagePathError};
