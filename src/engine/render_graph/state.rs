use serde::{Deserialize, Serialize};

use crate::core::SceneBlendMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PipelineBlendMode {
    Normal,
    Translucent,
    Additive,
    Disabled,
    AlphaToCoverage,
}

impl PipelineBlendMode {
    pub fn from_we_material_blending(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "translucent" | "alpha" => Self::Translucent,
            "additive" | "add" => Self::Additive,
            "disabled" | "opaque" => Self::Disabled,
            "alphatocoverage" | "alpha-to-coverage" => Self::AlphaToCoverage,
            _ => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShaderBlendMode {
    Normal,
    Darken,
    Multiply,
    ColorBurn,
    Subtract,
    Min,
    Lighten,
    Screen,
    ColorDodge,
    Add,
    Max,
    Overlay,
    SoftLight,
    HardLight,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    Difference,
    Exclusion,
    Reflect,
    Glow,
    Phoenix,
    Average,
    Negation,
    Hue,
    Saturation,
    HslColor,
    Luminosity,
    Tint,
    LinearDodge,
    Modulate,
}

impl ShaderBlendMode {
    pub fn from_we_blendmode(value: i64) -> Self {
        match value {
            1 => Self::Darken,
            2 => Self::Multiply,
            3 => Self::ColorBurn,
            4 | 20 => Self::Subtract,
            5 => Self::Min,
            6 => Self::Lighten,
            7 => Self::Screen,
            8 => Self::ColorDodge,
            9 => Self::Add,
            10 => Self::Max,
            11 => Self::Overlay,
            12 => Self::SoftLight,
            13 => Self::HardLight,
            14 => Self::VividLight,
            15 => Self::LinearLight,
            16 => Self::PinLight,
            17 => Self::HardMix,
            18 => Self::Difference,
            19 => Self::Exclusion,
            21 => Self::Reflect,
            22 => Self::Glow,
            23 => Self::Phoenix,
            24 => Self::Average,
            25 => Self::Negation,
            26 => Self::Hue,
            27 => Self::Saturation,
            28 => Self::HslColor,
            29 => Self::Luminosity,
            30 => Self::Tint,
            31 => Self::LinearDodge,
            32 => Self::Modulate,
            _ => Self::Normal,
        }
    }

    pub fn requires_framebuffer_sample(self) -> bool {
        !matches!(self, Self::Normal)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DepthTestMode {
    Disabled,
    Less,
    LessEqual,
    Equal,
    NotEqual,
    Greater,
    Never,
}

impl DepthTestMode {
    pub fn from_we_depthtest(value: Option<&str>) -> Self {
        match value.unwrap_or("disabled").to_ascii_lowercase().as_str() {
            "less" => Self::Less,
            "lessequal" | "lessorequal" => Self::LessEqual,
            "equal" => Self::Equal,
            "notequal" => Self::NotEqual,
            "greater" => Self::Greater,
            "never" => Self::Never,
            _ => Self::Disabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CullMode {
    None,
    Front,
    Back,
}

impl CullMode {
    pub fn from_we_cullmode(value: Option<&str>) -> Self {
        match value.unwrap_or("nocull").to_ascii_lowercase().as_str() {
            "front" => Self::Front,
            "back" => Self::Back,
            _ => Self::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassState {
    pub pipeline_blend: PipelineBlendMode,
    pub scene_blend: SceneBlendMode,
    pub shader_blend: Option<ShaderBlendMode>,
    pub depth_test: DepthTestMode,
    pub depth_write: bool,
    pub cull_mode: CullMode,
}

impl Default for PassState {
    fn default() -> Self {
        Self {
            pipeline_blend: PipelineBlendMode::Normal,
            scene_blend: SceneBlendMode::Alpha,
            shader_blend: None,
            depth_test: DepthTestMode::Disabled,
            depth_write: false,
            cull_mode: CullMode::None,
        }
    }
}
