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

/// The coordinate space that owns a target's allocation extent.
///
/// A target can be attached to the live physical scene surface, or to the
/// authored source extent of the graph owner. The target name is deliberately
/// not part of this decision: identical effect target names are scoped by
/// their graph owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderTargetExtentDomain {
    PhysicalSurface,
    OwnerAuthored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderTargetSpec {
    pub role: RenderTargetRole,
    pub name: String,
    pub format: String,
    pub extent_domain: RenderTargetExtentDomain,
    pub width_divisor_milli: u32,
    pub height_divisor_milli: u32,
}
