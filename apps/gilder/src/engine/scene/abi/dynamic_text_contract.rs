//! Typed retained glyph-atlas records for script-mutated text.

use serde::{Deserialize, Serialize};

use super::{SceneObjectHandle, SceneResourceId};

pub const SCENE_DYNAMIC_TEXT_MAX_GLYPHS: u32 = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneTextHorizontalAlign {
    Left,
    Center,
    Right,
}

impl SceneTextHorizontalAlign {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Left => 0,
            Self::Center => 1,
            Self::Right => 2,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Left),
            1 => Some(Self::Center),
            2 => Some(Self::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneTextVerticalAlign {
    Top,
    Center,
    Bottom,
}

impl SceneTextVerticalAlign {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Top => 0,
            Self::Center => 1,
            Self::Bottom => 2,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Top),
            1 => Some(Self::Center),
            2 => Some(Self::Bottom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneDynamicTextRecord {
    pub object: SceneObjectHandle,
    pub font_resource: SceneResourceId,
    pub atlas_resource: SceneResourceId,
    pub glyph_start: u32,
    pub glyph_count: u32,
    pub max_glyph_count: u32,
    pub pixels_per_em: f32,
    pub spacing: [f32; 2],
    pub padding: [f32; 2],
    pub horizontal_align: SceneTextHorizontalAlign,
    pub vertical_align: SceneTextVerticalAlign,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneDynamicTextGlyphRecord {
    pub codepoint: u32,
    /// Atlas UV bounds ordered as left, top, right, bottom.
    pub atlas_uv: [f32; 4],
    /// Glyph ink bounds relative to its authored baseline position.
    pub plane_bounds: [f32; 4],
}
