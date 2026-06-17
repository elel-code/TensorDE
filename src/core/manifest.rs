use super::format::{WallpaperKind, FORMAT_NAME, FORMAT_VERSION};
use super::path::PackagePath;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub id: String,
    pub version: String,
    pub title: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default = "unknown_license")]
    pub license: String,
    pub kind: WallpaperKind,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub preview: Preview,
    pub entry: WallpaperEntry,
    #[serde(default)]
    pub variants: Vec<Variant>,
    #[serde(default)]
    pub properties: BTreeMap<String, PropertySpec>,
    #[serde(default)]
    pub runtime: RuntimePolicy,
}

impl Manifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.format != FORMAT_NAME {
            return Err(ManifestError::InvalidFormat {
                expected: FORMAT_NAME,
                actual: self.format.clone(),
            });
        }
        if self.format_version != FORMAT_VERSION {
            return Err(ManifestError::UnsupportedVersion {
                supported: FORMAT_VERSION,
                actual: self.format_version,
            });
        }
        validate_required_text("id", &self.id)?;
        validate_required_text("version", &self.version)?;
        validate_required_text("title", &self.title)?;

        let entry_kind = self.entry.kind();
        if self.kind != entry_kind {
            return Err(ManifestError::KindMismatch {
                manifest: self.kind,
                entry: entry_kind,
            });
        }

        self.entry.validate()?;
        for variant in &self.variants {
            variant.validate()?;
        }
        for (name, property) in &self.properties {
            validate_required_text("property name", name)?;
            property.validate(name)?;
        }

        Ok(())
    }

    pub fn referenced_paths(&self) -> Result<Vec<PackagePath>, ManifestError> {
        let mut paths = Vec::new();
        if let Some(path) = &self.preview.thumbnail {
            paths.push(path.clone());
        }
        if let Some(path) = &self.preview.poster {
            paths.push(path.clone());
        }
        self.entry.push_referenced_paths(&mut paths)?;
        for variant in &self.variants {
            paths.push(variant.source.clone());
        }
        Ok(paths)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    pub thumbnail: Option<PackagePath>,
    pub poster: Option<PackagePath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum WallpaperEntry {
    StaticImage {
        source: PackagePath,
        #[serde(default)]
        fit: FitMode,
        #[serde(default)]
        background: Option<String>,
        #[serde(default)]
        orientation: Orientation,
    },
    Video {
        source: PackagePath,
        #[serde(default)]
        poster: Option<PackagePath>,
        #[serde(rename = "loop", default = "default_true")]
        loop_playback: bool,
        #[serde(default = "default_true")]
        muted: bool,
        #[serde(default)]
        fit: FitMode,
        #[serde(default)]
        max_fps: Option<u32>,
        #[serde(default)]
        start_offset_ms: u64,
    },
    Slideshow {
        sources: Vec<PackagePath>,
        interval_ms: u64,
        #[serde(default)]
        transition: Transition,
        #[serde(default)]
        fit: FitMode,
    },
    Web {
        root: PackagePath,
        index: PackagePath,
        #[serde(default)]
        fallback: Option<PackagePath>,
        #[serde(default)]
        max_fps: Option<u32>,
    },
    SceneLite {
        #[serde(default)]
        source: Option<PackagePath>,
        #[serde(default)]
        fallback: Option<PackagePath>,
        #[serde(default)]
        max_fps: Option<u32>,
    },
}

impl WallpaperEntry {
    pub fn kind(&self) -> WallpaperKind {
        match self {
            Self::StaticImage { .. } => WallpaperKind::StaticImage,
            Self::Video { .. } => WallpaperKind::Video,
            Self::Slideshow { .. } => WallpaperKind::Slideshow,
            Self::Web { .. } => WallpaperKind::Web,
            Self::SceneLite { .. } => WallpaperKind::SceneLite,
        }
    }

    fn validate(&self) -> Result<(), ManifestError> {
        match self {
            Self::StaticImage { .. } => Ok(()),
            Self::Video { max_fps, .. } | Self::Web { max_fps, .. } => {
                validate_fps(*max_fps)?;
                Ok(())
            }
            Self::Slideshow {
                sources,
                interval_ms,
                ..
            } => {
                if sources.is_empty() {
                    return Err(ManifestError::InvalidEntry(
                        "slideshow must contain at least one source".to_owned(),
                    ));
                }
                if *interval_ms == 0 {
                    return Err(ManifestError::InvalidEntry(
                        "slideshow interval_ms must be greater than 0".to_owned(),
                    ));
                }
                Ok(())
            }
            Self::SceneLite { max_fps, .. } => {
                validate_fps(*max_fps)?;
                Ok(())
            }
        }
    }

    fn push_referenced_paths(&self, paths: &mut Vec<PackagePath>) -> Result<(), ManifestError> {
        match self {
            Self::StaticImage { source, .. } => paths.push(source.clone()),
            Self::Video { source, poster, .. } => {
                paths.push(source.clone());
                if let Some(path) = poster {
                    paths.push(path.clone());
                }
            }
            Self::Slideshow { sources, .. } => paths.extend(sources.iter().cloned()),
            Self::Web {
                root,
                index,
                fallback,
                ..
            } => {
                paths.push(root.clone());
                paths.push(root.join_package_path(index).map_err(|err| {
                    ManifestError::InvalidEntry(format!("invalid web index path: {err}"))
                })?);
                if let Some(path) = fallback {
                    paths.push(path.clone());
                }
            }
            Self::SceneLite {
                source, fallback, ..
            } => {
                if let Some(path) = source {
                    paths.push(path.clone());
                }
                if let Some(path) = fallback {
                    paths.push(path.clone());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FitMode {
    #[default]
    Cover,
    Contain,
    Stretch,
    Tile,
    Center,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Orientation {
    #[default]
    FromMetadata,
    Ignore,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transition {
    #[default]
    None,
    Crossfade,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
    pub id: String,
    pub source: PackagePath,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub scale: Option<Scale>,
}

impl Variant {
    fn validate(&self) -> Result<(), ManifestError> {
        validate_required_text("variant id", &self.id)?;
        if self.width == Some(0) || self.height == Some(0) {
            return Err(ManifestError::InvalidVariant {
                id: self.id.clone(),
                message: "width and height must be greater than 0".to_owned(),
            });
        }
        if self.scale == Some(Scale(0)) {
            return Err(ManifestError::InvalidVariant {
                id: self.id.clone(),
                message: "scale must be greater than 0".to_owned(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct Scale(u32);

impl TryFrom<f32> for Scale {
    type Error = String;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if value.is_finite() && value > 0.0 {
            Ok(Self((value * 1000.0).round() as u32))
        } else {
            Err("scale must be a finite number greater than 0".to_owned())
        }
    }
}

impl From<Scale> for f32 {
    fn from(value: Scale) -> Self {
        value.0 as f32 / 1000.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PropertySpec {
    Bool {
        #[serde(default)]
        default: Option<bool>,
    },
    Number {
        #[serde(default)]
        default: Option<f64>,
    },
    Range {
        min: f64,
        max: f64,
        #[serde(default)]
        step: Option<f64>,
        #[serde(default)]
        default: Option<f64>,
    },
    Choice {
        choices: Vec<String>,
        #[serde(default)]
        default: Option<String>,
    },
    Color {
        #[serde(default)]
        default: Option<String>,
    },
    Text {
        #[serde(default)]
        default: Option<String>,
    },
    File {
        #[serde(default)]
        default: Option<PackagePath>,
    },
}

impl PropertySpec {
    fn validate(&self, name: &str) -> Result<(), ManifestError> {
        match self {
            Self::Range {
                min,
                max,
                step,
                default,
            } => {
                if !min.is_finite() || !max.is_finite() || min >= max {
                    return Err(ManifestError::InvalidProperty {
                        name: name.to_owned(),
                        message: "range requires finite min < max".to_owned(),
                    });
                }
                if let Some(step) = step {
                    if !step.is_finite() || *step <= 0.0 {
                        return Err(ManifestError::InvalidProperty {
                            name: name.to_owned(),
                            message: "range step must be finite and greater than 0".to_owned(),
                        });
                    }
                }
                if let Some(default) = default {
                    if !default.is_finite() || default < min || default > max {
                        return Err(ManifestError::InvalidProperty {
                            name: name.to_owned(),
                            message: "range default must be inside min/max".to_owned(),
                        });
                    }
                }
            }
            Self::Choice { choices, default } => {
                if choices.is_empty() {
                    return Err(ManifestError::InvalidProperty {
                        name: name.to_owned(),
                        message: "choice requires at least one value".to_owned(),
                    });
                }
                if let Some(default) = default {
                    if !choices.contains(default) {
                        return Err(ManifestError::InvalidProperty {
                            name: name.to_owned(),
                            message: "choice default must be present in choices".to_owned(),
                        });
                    }
                }
            }
            Self::Number { default } => {
                if default.is_some_and(|value| !value.is_finite()) {
                    return Err(ManifestError::InvalidProperty {
                        name: name.to_owned(),
                        message: "number default must be finite".to_owned(),
                    });
                }
            }
            Self::Bool { .. } | Self::Color { .. } | Self::Text { .. } | Self::File { .. } => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePolicy {
    #[serde(default = "default_true")]
    pub pause_when_fullscreen: bool,
    #[serde(default)]
    pub pause_when_unfocused: bool,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default)]
    pub allow_audio: bool,
}

impl Default for RuntimePolicy {
    fn default() -> Self {
        Self {
            pause_when_fullscreen: true,
            pause_when_unfocused: false,
            allow_network: false,
            allow_audio: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManifestError {
    InvalidFormat {
        expected: &'static str,
        actual: String,
    },
    UnsupportedVersion {
        supported: u32,
        actual: u32,
    },
    MissingRequiredField(&'static str),
    KindMismatch {
        manifest: WallpaperKind,
        entry: WallpaperKind,
    },
    InvalidEntry(String),
    InvalidVariant {
        id: String,
        message: String,
    },
    InvalidProperty {
        name: String,
        message: String,
    },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat { expected, actual } => {
                write!(f, "invalid format {actual:?}, expected {expected:?}")
            }
            Self::UnsupportedVersion { supported, actual } => {
                write!(
                    f,
                    "unsupported format_version {actual}, supported version is {supported}"
                )
            }
            Self::MissingRequiredField(field) => {
                write!(f, "manifest field {field} must not be empty")
            }
            Self::KindMismatch { manifest, entry } => write!(
                f,
                "manifest kind {:?} does not match entry kind {:?}",
                manifest, entry
            ),
            Self::InvalidEntry(message) => write!(f, "invalid entry: {message}"),
            Self::InvalidVariant { id, message } => {
                write!(f, "invalid variant {id:?}: {message}")
            }
            Self::InvalidProperty { name, message } => {
                write!(f, "invalid property {name:?}: {message}")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

fn validate_required_text(field: &'static str, value: &str) -> Result<(), ManifestError> {
    if value.trim().is_empty() {
        Err(ManifestError::MissingRequiredField(field))
    } else {
        Ok(())
    }
}

fn validate_fps(max_fps: Option<u32>) -> Result<(), ManifestError> {
    if max_fps == Some(0) {
        Err(ManifestError::InvalidEntry(
            "max_fps must be greater than 0".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

fn unknown_license() -> String {
    "unknown".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates_static_manifest() {
        let json = r##"
        {
          "format": "gilder.wallpaper",
          "format_version": 1,
          "id": "org.example.static",
          "version": "1.0.0",
          "title": "Example Static",
          "kind": "static-image",
          "preview": {
            "thumbnail": "previews/thumbnail.svg",
            "poster": "previews/poster.svg"
          },
          "entry": {
            "type": "static-image",
            "source": "assets/wallpaper.svg",
            "fit": "cover",
            "background": "#000000"
          },
          "runtime": {
            "pause_when_fullscreen": true
          }
        }
        "##;
        let manifest: Manifest = serde_json::from_str(json).unwrap();
        manifest.validate().unwrap();
        assert_eq!(manifest.kind, WallpaperKind::StaticImage);
        assert_eq!(manifest.referenced_paths().unwrap().len(), 3);
    }

    #[test]
    fn rejects_kind_mismatch() {
        let json = r#"
        {
          "format": "gilder.wallpaper",
          "format_version": 1,
          "id": "org.example.mismatch",
          "version": "1.0.0",
          "title": "Mismatch",
          "kind": "video",
          "entry": {
            "type": "static-image",
            "source": "assets/wallpaper.png"
          }
        }
        "#;
        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::KindMismatch { .. })
        ));
    }

    #[test]
    fn rejects_invalid_property_schema() {
        let json = r#"
        {
          "format": "gilder.wallpaper",
          "format_version": 1,
          "id": "org.example.bad-property",
          "version": "1.0.0",
          "title": "Bad Property",
          "kind": "static-image",
          "entry": {
            "type": "static-image",
            "source": "assets/wallpaper.png"
          },
          "properties": {
            "fit": {
              "type": "choice",
              "default": "cover",
              "choices": []
            }
          }
        }
        "#;
        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::InvalidProperty { .. })
        ));
    }
}
