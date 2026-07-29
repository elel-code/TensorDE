//! Strict Wallpaper Engine ingest failures.

use std::fmt;
use std::path::PathBuf;

use super::super::pkg::ScenePackageError;
use super::super::tex::TexParseError;

#[derive(Debug)]
pub enum WeIngestError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Package(ScenePackageError),
    Json {
        path: String,
        source: serde_json::Error,
    },
    Tex {
        path: String,
        source: TexParseError,
    },
    MissingAsset(String),
    UnsafePath(String),
    UnsupportedProjectType {
        wallpaper_type: String,
    },
    InvalidProject(String),
    Script {
        object: u32,
        message: String,
    },
    ShaderCompile {
        program: String,
        stage: &'static str,
        message: String,
    },
}

impl fmt::Display for WeIngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "failed to read {}: {source}", path.display()),
            Self::Package(err) => write!(f, "{err}"),
            Self::Json { path, source } => write!(f, "failed to parse WE JSON {path}: {source}"),
            Self::Tex { path, source } => write!(f, "failed to parse WE texture {path}: {source}"),
            Self::MissingAsset(path) => write!(f, "missing WE asset {path}"),
            Self::UnsafePath(path) => write!(f, "unsafe WE asset path {path}"),
            Self::UnsupportedProjectType { wallpaper_type } => {
                write!(
                    f,
                    "Wallpaper Engine type {wallpaper_type:?} is not a scene wallpaper"
                )
            }
            Self::InvalidProject(message) => write!(f, "invalid WE project: {message}"),
            Self::Script { object, message } => {
                write!(f, "invalid SceneScript on object {object}: {message}")
            }
            Self::ShaderCompile {
                program,
                stage,
                message,
            } => write!(
                f,
                "failed to cold-compile authored {stage} shader {program}: {message}"
            ),
        }
    }
}

impl std::error::Error for WeIngestError {}

impl From<ScenePackageError> for WeIngestError {
    fn from(value: ScenePackageError) -> Self {
        Self::Package(value)
    }
}
