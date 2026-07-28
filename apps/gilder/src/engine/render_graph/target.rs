use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderTargetRole {
    SceneColor,
    Swapchain,
    ImageLocalMain,
    ImageLocalSub,
    NamedFbo,
    FirstClassEffectTarget,
    VideoExternalImage,
    Temporary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderTargetSpec {
    pub role: RenderTargetRole,
    pub name: String,
    pub format: String,
    pub width_divisor_milli: u32,
    pub height_divisor_milli: u32,
}
