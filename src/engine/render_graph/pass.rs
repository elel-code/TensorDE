use serde::{Deserialize, Serialize};

use super::binding::TextureBindingRole;
use super::state::PassState;
use super::target::RenderTargetRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderPassRole {
    Clear,
    BaseMaterial,
    EffectMaterial,
    ColorBlendPassthrough,
    VideoSample,
    Particle,
    TextPath,
    SceneComposite,
    DebugEvidence,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderPassNode {
    pub id: u32,
    pub role: RenderPassRole,
    pub object_index: Option<usize>,
    pub pass_index: u32,
    pub shader: Option<String>,
    pub target: RenderTargetRole,
    pub target_name: Option<String>,
    pub bindings: Vec<TextureBindingRole>,
    pub state: PassState,
}
