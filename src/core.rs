//! Core types and constants for the Gilder wallpaper package format.

pub mod format;
pub mod manifest;
pub mod package;
pub mod path;

pub use format::{
    DIRECTORY_EXTENSION, FORMAT_NAME, FORMAT_VERSION, MANIFEST_FILE, PACKAGE_EXTENSION,
    WallpaperKind,
};
pub use manifest::{FitMode, Manifest, ManifestError, RuntimePolicy, WallpaperEntry};
pub use package::{
    PackageArchiveError, PackageIdentity, PackageLoadError, WallpaperPackage, load_gwp,
    load_gwpdir, pack_gwp, unpack_gwp,
};
pub use path::{PackagePath, PackagePathError};
