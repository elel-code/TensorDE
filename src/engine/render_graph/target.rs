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
