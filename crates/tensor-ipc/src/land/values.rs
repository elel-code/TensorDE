use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum LayoutKind {
    #[default]
    #[serde(rename = "scrolling-1d")]
    Scrolling1D,
    #[serde(rename = "spatial-2d")]
    Spatial2D,
    #[serde(rename = "master-stack")]
    MasterStack,
}

impl LayoutKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scrolling1D => "scrolling-1d",
            Self::Spatial2D => "spatial-2d",
            Self::MasterStack => "master-stack",
        }
    }
}

impl FromStr for LayoutKind {
    type Err = ParseLayoutError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "scrolling-1d" => Ok(Self::Scrolling1D),
            "spatial-2d" => Ok(Self::Spatial2D),
            "master-stack" => Ok(Self::MasterStack),
            _ => Err(ParseLayoutError(value.to_owned())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown layout '{0}'; expected scrolling-1d, spatial-2d, or master-stack")]
pub struct ParseLayoutError(String);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigDiagnosticCategory {
    Syntax,
    Type,
    Policy,
    Io,
}

/// Source-free reload failure metadata suitable for a bounded IPC event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConfigDiagnosticMetadata {
    pub category: ConfigDiagnosticCategory,
    pub path: String,
    pub error_code: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub summary: String,
    pub validation_command: String,
}
