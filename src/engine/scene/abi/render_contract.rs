use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneRenderBindingRecord {
    pub kind: SceneRenderBindingKind,
    pub slot: u32,
    pub target: SceneRenderTargetKind,
    pub name: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneUnsupportedRecord {
    pub object: SceneObjectHandle,
    pub pass_index: u32,
    pub feature: SceneStringId,
    pub expected_subsystem: SceneStringId,
    pub containment: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneImageTargetRecord {
    pub name: SceneStringId,
    pub role: SceneRenderTargetKind,
    pub format: SceneStringId,
    pub width_divisor_milli: u32,
    pub height_divisor_milli: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneShaderContractRecord {
    pub shader_key: SceneStringId,
    pub pipeline_key: SceneStringId,
    pub texture_slot_mask: u32,
    pub constant_start: u32,
    pub constant_count: u32,
    pub resource_heap_count: u32,
    pub sampler_heap_count: u32,
}
