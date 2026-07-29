//! Typed RenderingDevice graph records shared by planning and native backends.

use serde::{Deserialize, Serialize};

use super::super::abi::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDevicePassNode {
    pub graph_index: u32,
    pub graph_activation_policy: SceneRenderGraphActivationPolicy,
    pub pass_record_index: u32,
    pub pass_id: u32,
    pub role: SceneRenderPassKind,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub binding_start: u32,
    pub binding_count: u32,
    pub effect_binding_start: u32,
    pub effect_binding_count: u32,
    pub effect_visibility_policy: SceneRenderEffectVisibilityPolicy,
    pub mesh_draw_start: u32,
    pub mesh_draw_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceTargetAllocation {
    pub graph_index: u32,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub first_write_pass_id: u32,
    pub last_use_pass_id: u32,
    pub physical_slot: u32,
    /// Non-zero dimensions select a graph-local authored-texture target.
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderingDeviceImageAccess {
    /// A sampled image plus a sampler; UV/filter/mip semantics are retained.
    SampledImage,
    /// An exact-pixel dynamic-rendering local read; no sampler is involved.
    InputAttachment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceSampledBinding {
    pub pass_node_index: u32,
    pub graph_index: u32,
    pub mesh_draw_start: u32,
    pub mesh_draw_count: u32,
    pub kind: SceneRenderBindingKind,
    pub slot: u32,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub access: SceneRenderingDeviceImageAccess,
}

impl SceneRenderingDeviceSampledBinding {
    pub fn logical_target(self) -> Option<(u32, SceneRenderTargetKind, SceneStringId)> {
        match self.kind {
            SceneRenderBindingKind::PreviousGraphTarget
            | SceneRenderBindingKind::GraphTarget
            | SceneRenderBindingKind::NamedFboBind
            | SceneRenderBindingKind::EffectTarget => {
                Some((self.graph_index, self.target, self.target_name))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceMaterialSampledBinding {
    pub draw_index: u32,
    pub slot: u32,
    pub resource: SceneResourceId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderingDeviceMeshDraw {
    pub primitive: SceneRenderingDeviceDrawPrimitive,
    /// Shader selected by the actual render pass. Synthetic composites can differ from
    /// the authored material's first pass.
    pub shader_key: SceneStringId,
    pub mesh_index: u32,
    pub resolved_object_index: u32,
    /// Authored object-to-scene transform after semantic animation and pointer parallax.
    pub render_world_matrix: [[f32; 4]; 4],
    /// Final object-to-clip transform for the current scene projection.
    pub clip_transform: [[f32; 4]; 4],
    pub authored_source_extent: [f32; 2],
    pub skinning_palette_start: u32,
    pub skinning_palette_count: u32,
    pub resolved_color: SceneVec3,
    pub resolved_alpha: f32,
    pub apply_resolved_visual: bool,
    /// Layer in a scene-level GPU effect batch, or `INVALID_OBJECT_ID` for ordinary draws.
    pub effect_batch_atlas_tile: u32,
    /// Column/row count of the scene-level 2D effect atlas; `[0, 0]` means no batch.
    pub effect_batch_atlas_grid: [u32; 2],
    pub effect_binding_start: u32,
    pub effect_binding_count: u32,
    pub effect_visibility_policy: SceneRenderEffectVisibilityPolicy,
    pub resolved_effect_visibility_mask: u32,
    pub object: SceneObjectHandle,
    pub material: SceneMaterialHandle,
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
    pub instance_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneRenderingDeviceDrawPrimitive {
    ObjectMesh,
    FullscreenTriangle,
    ObjectUvSupportQuad,
    ParticleBillboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderingDevicePuppetBonePalette {
    pub object: SceneObjectHandle,
    pub puppet_index: u32,
    pub bone_matrix_start: u32,
    pub bone_matrix_count: u32,
    pub resolved_visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneRenderingDevicePuppetBoneMatrix {
    pub puppet_index: u32,
    pub bone_index: u32,
    pub parent_index: i32,
    pub matrix: [[f32; 4]; 4],
    pub alpha: f32,
}
