//! Core types and constants for the Gilder wallpaper package format.

pub mod format;
pub mod manifest;
pub mod package;
pub mod path;
pub mod scene_lite;

pub use format::{
    DIRECTORY_EXTENSION, FORMAT_NAME, FORMAT_VERSION, MANIFEST_FILE, MANIFEST_TOML_FILE,
    PACKAGE_EXTENSION, WallpaperKind,
};
pub use manifest::{
    FitMode, Manifest, ManifestError, PlaylistConditions, PlaylistItem, PlaylistLocalTimeCondition,
    PlaylistPowerCondition, PlaylistSelection, PlaylistWeekday, RuntimePolicy, Transition,
    WallpaperEntry,
};
pub use package::{
    ManifestParseError, PackageArchiveError, PackageIdentity, PackageLoadError, WallpaperPackage,
    load_gwp, load_gwpdir, pack_gwp, unpack_gwp,
};
pub use path::{PackagePath, PackagePathError};
pub use scene_lite::{
    SceneLiteAnimatedProperty, SceneLiteAnimation, SceneLiteCurve, SceneLiteDocument,
    SceneLiteError, SceneLiteKeyframe, SceneLiteLayer, SceneLiteLayerKind,
    SceneLitePropertyBinding, SceneLiteSnapshot, SceneLiteSnapshotLayer, SceneLiteTextAlign,
    SceneLiteTransform,
};
