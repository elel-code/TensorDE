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
    CopyTarget,
    SwapTargetReferences,
    VideoSample,
    Particle,
    TextPath,
    SceneComposite,
    MeshVisiblePrefix,
    MeshClippingMask,
    MeshClippedTarget,
    MeshVisibleRemainder,
    DebugEvidence,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderPassDrawPrimitive {
    None,
    ObjectMesh,
    FullscreenTriangle,
    ObjectUvSupportQuad,
    ParticleBillboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderPassEffectVisibilityPolicy {
    None,
    Passthrough,
    WaterWavesStages,
    FlatRoundedMask,
    MaterialStages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderPassEffectVisibility {
    pub binding_start: u32,
    pub binding_count: u32,
    pub policy: RenderPassEffectVisibilityPolicy,
}

impl RenderPassEffectVisibility {
    pub const NONE: Self = Self {
        binding_start: u32::MAX,
        binding_count: 0,
        policy: RenderPassEffectVisibilityPolicy::None,
    };

    pub const fn passthrough(binding_start: u32, binding_count: u32) -> Self {
        Self {
            binding_start,
            binding_count,
            policy: RenderPassEffectVisibilityPolicy::Passthrough,
        }
    }

    pub const fn waterwaves_stages(binding_start: u32, binding_count: u32) -> Self {
        Self {
            binding_start,
            binding_count,
            policy: RenderPassEffectVisibilityPolicy::WaterWavesStages,
        }
    }

    pub const fn flat_rounded_mask(binding_start: u32) -> Self {
        Self {
            binding_start,
            binding_count: 1,
            policy: RenderPassEffectVisibilityPolicy::FlatRoundedMask,
        }
    }

    pub const fn material_stages(binding_start: u32, binding_count: u32) -> Self {
        Self {
            binding_start,
            binding_count,
            policy: RenderPassEffectVisibilityPolicy::MaterialStages,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderPassNode {
    pub id: u32,
    pub role: RenderPassRole,
    pub draw_primitive: RenderPassDrawPrimitive,
    pub object_index: Option<usize>,
    pub material_index: Option<usize>,
    pub pass_index: u32,
    pub shader: Option<String>,
    pub target: RenderTargetRole,
    pub target_name: Option<String>,
    pub target_extent: Option<[u32; 2]>,
    pub target_format: Option<String>,
    pub bindings: Vec<TextureBindingRole>,
    pub effect_visibility: RenderPassEffectVisibility,
    pub state: PassState,
}
