//! Typed render-state values shared by scene storage and RenderingDevice plans.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenePipelineBlend {
    Normal,
    Translucent,
    Additive,
    Disabled,
    AlphaToCoverage,
}

impl ScenePipelineBlend {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::Translucent => 1,
            Self::Additive => 2,
            Self::Disabled => 3,
            Self::AlphaToCoverage => 4,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Normal),
            1 => Some(Self::Translucent),
            2 => Some(Self::Additive),
            3 => Some(Self::Disabled),
            4 => Some(Self::AlphaToCoverage),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneCompositeBlend {
    Alpha,
    Normal,
    Additive,
    Multiply,
    Screen,
    Max,
    Modulate,
    HslColor,
    AlphaToCoverage,
}

impl SceneCompositeBlend {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Alpha => 0,
            Self::Normal => 1,
            Self::Additive => 2,
            Self::Multiply => 3,
            Self::Screen => 4,
            Self::Max => 5,
            Self::Modulate => 6,
            Self::HslColor => 7,
            Self::AlphaToCoverage => 8,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Alpha),
            1 => Some(Self::Normal),
            2 => Some(Self::Additive),
            3 => Some(Self::Multiply),
            4 => Some(Self::Screen),
            5 => Some(Self::Max),
            6 => Some(Self::Modulate),
            7 => Some(Self::HslColor),
            8 => Some(Self::AlphaToCoverage),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneDepthTest {
    Disabled,
    Enabled,
}

impl SceneDepthTest {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Disabled => 0,
            Self::Enabled => 1,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Disabled),
            1 => Some(Self::Enabled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneCullMode {
    None,
    Normal,
}

impl SceneCullMode {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Normal => 1,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Normal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneColorWriteMask {
    Rgb,
    Rgba,
}

impl SceneColorWriteMask {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Rgb => 0,
            Self::Rgba => 1,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Rgb),
            1 => Some(Self::Rgba),
            _ => None,
        }
    }
}
