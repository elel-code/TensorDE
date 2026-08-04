#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// ABI record for a material and its pass range.
pub struct SceneMaterialRecord {
    pub id: SceneMaterialHandle,
    pub resource: SceneResourceId,
    pub pass_start: u32,
    pub pass_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMaterialPassRecord {
    pub material: SceneMaterialHandle,
    pub shader_key: SceneStringId,
    pub target: SceneStringId,
    pub texture_start: u32,
    pub texture_count: u32,
    pub constant_start: u32,
    pub constant_count: u32,
    pub pipeline_blend: ScenePipelineBlend,
    pub depth_test: SceneDepthTest,
    pub depth_write: bool,
    pub cull_mode: SceneCullMode,
    pub alpha_writing: SceneStringId,
    pub clear_target: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMaterialTextureRecord {
    pub slot: u32,
    pub resource: SceneResourceId,
    pub path: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMaterialConstantRecord {
    pub name: SceneStringId,
    pub value_json: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshRecord {
    pub object: SceneObjectHandle,
    pub material: SceneMaterialHandle,
    pub vertex_start: u32,
    pub vertex_count: u32,
    pub index_start: u32,
    pub index_count: u32,
    pub width: f32,
    pub height: f32,
    pub bounds_min: SceneVec3,
    pub bounds_max: SceneVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneMeshVertexRecord {
    pub position: SceneVec3,
    pub uv: [f32; 2],
    pub blend_indices: [u32; 4],
    pub blend_weights: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMeshSourceRecord {
    pub mesh: u32,
    pub source_index: u32,
    pub local_index_offset: u32,
    pub index_start: u32,
    pub index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMeshClippingSubdrawRecord {
    pub mesh: u32,
    pub source_qword: u64,
    pub mask: SceneStringId,
    pub mask_resource: SceneResourceId,
    pub raw_flags: u32,
    pub target_source_start: u32,
    pub target_source_count: u32,
    pub mask_source_start: u32,
    pub mask_source_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneMeshClippingSliceRole {
    VisiblePrefix,
    MaskProducer,
    ClippedTarget,
    VisibleRemainder,
}

impl SceneMeshClippingSliceRole {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::VisiblePrefix => 1,
            Self::MaskProducer => 2,
            Self::ClippedTarget => 3,
            Self::VisibleRemainder => 4,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::VisiblePrefix),
            2 => Some(Self::MaskProducer),
            3 => Some(Self::ClippedTarget),
            4 => Some(Self::VisibleRemainder),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMeshClippingSliceRecord {
    pub mesh: u32,
    pub subdraw: u32,
    pub role: SceneMeshClippingSliceRole,
    pub index_start: u32,
    pub index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenePuppetRecord {
    pub object: SceneObjectHandle,
    pub resource: SceneResourceId,
    pub mesh_start: u32,
    pub mesh_count: u32,
    pub bone_start: u32,
    pub bone_count: u32,
    pub attachment_start: u32,
    pub attachment_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetBoneRecord {
    pub puppet: u32,
    pub bone_index: u32,
    pub name: SceneStringId,
    pub simulation_type: i32,
    pub parent_index: i32,
    pub local_bind_matrix: [f32; 16],
    pub simulation_json: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScenePuppetAttachmentRecord {
    pub puppet: u32,
    pub bone_index: u32,
    pub name: SceneStringId,
    pub local_matrix: [f32; 16],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEffectRecord {
    pub id: SceneEffectHandle,
    pub resource: SceneResourceId,
    pub replacement_key: SceneStringId,
    pub pass_start: u32,
    pub pass_count: u32,
    pub fbo_start: u32,
    pub fbo_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEffectPassRecord {
    pub effect: SceneEffectHandle,
    pub pass_index: u32,
    pub material: SceneMaterialHandle,
    pub command: SceneStringId,
    pub source: SceneStringId,
    pub target: SceneStringId,
    pub binding_start: u32,
    pub binding_count: u32,
    pub combo_start: u32,
    pub combo_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEffectBindingRecord {
    pub slot: u32,
    pub target: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEffectComboRecord {
    pub name: SceneStringId,
    pub value: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SceneEffectFboRecord {
    pub name: SceneStringId,
    pub format: SceneStringId,
    pub scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderGraphRecord {
    pub object: SceneObjectHandle,
    pub activation_policy: SceneRenderGraphActivationPolicy,
    pub source_extent_domain: SceneRenderSourceExtentDomain,
    pub pass_start: u32,
    pub pass_count: u32,
    pub unsupported_start: u32,
    pub unsupported_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderPassRecord {
    pub id: u32,
    pub role: SceneRenderPassKind,
    pub draw_primitive: SceneRenderPassDrawPrimitive,
    pub object: SceneObjectHandle,
    pub material: SceneMaterialHandle,
    pub pass_index: u32,
    pub shader_key: SceneStringId,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub binding_start: u32,
    pub binding_count: u32,
    pub effect_binding_start: u32,
    pub effect_binding_count: u32,
    pub effect_visibility_policy: SceneRenderEffectVisibilityPolicy,
    pub pipeline_blend: ScenePipelineBlend,
    pub scene_blend: SceneCompositeBlend,
    pub depth_test: SceneDepthTest,
    pub depth_write: bool,
    pub cull_mode: SceneCullMode,
    pub color_write_mask: SceneColorWriteMask,
    pub clear_target: bool,
}
