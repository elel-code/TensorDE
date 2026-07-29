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
    /// Slots consumed through ordinary sampler/image sampling.
    pub texture_slot_mask: u32,
    /// Slots consumed as exact-pixel dynamic-rendering input attachments.
    ///
    /// The masks are intentionally separate: an input attachment is an image
    /// resource, but it has no sampler and cannot be lowered as a sampled
    /// image without changing authored coordinate/filter semantics.
    pub input_attachment_slot_mask: u32,
    pub constant_start: u32,
    pub constant_count: u32,
    pub resource_heap_count: u32,
    pub sampler_heap_count: u32,
}

/// A single optimized SPIR-V stage embedded by the cold scene converter.
///
/// Source languages and compiler intermediates are intentionally absent from
/// this ABI. `spirv_start` and `spirv_count` address words in the document's
/// `shader_spirv` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneShaderProgramRecord {
    pub program_key: SceneStringId,
    pub stage: SceneShaderStage,
    pub entry_point: SceneStringId,
    pub spirv_start: u32,
    pub spirv_count: u32,
    pub binding_start: u32,
    pub binding_count: u32,
    pub stage_io_start: u32,
    pub stage_io_count: u32,
    pub uniform_buffer_start: u32,
    pub uniform_buffer_count: u32,
    pub push_constant_bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneShaderBindingRecord {
    pub kind: SceneShaderBindingKind,
    pub register: u32,
    pub descriptor_count: u32,
    pub push_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneShaderStageIoRecord {
    pub name: SceneStringId,
    pub direction: SceneShaderIoDirection,
    pub location: u32,
    pub scalar_type: SceneShaderScalarType,
    pub rows: u32,
    pub columns: u32,
    pub location_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneShaderUniformBufferRecord {
    pub name: SceneStringId,
    pub register: u32,
    pub byte_size: u32,
    pub member_start: u32,
    pub member_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneShaderUniformMemberRecord {
    pub name: SceneStringId,
    pub byte_offset: u32,
    pub byte_size: u32,
    pub scalar_type: SceneShaderScalarType,
    pub rows: u32,
    pub columns: u32,
    pub array_count: u32,
    pub array_stride: u32,
    pub matrix_stride: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneShaderIoDirection {
    Input,
    Output,
}

impl SceneShaderIoDirection {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Input => 1,
            Self::Output => 2,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Input),
            2 => Some(Self::Output),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneShaderScalarType {
    Bool,
    I32,
    U32,
    F32,
}

impl SceneShaderScalarType {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Bool => 1,
            Self::I32 => 2,
            Self::U32 => 3,
            Self::F32 => 4,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Bool),
            2 => Some(Self::I32),
            3 => Some(Self::U32),
            4 => Some(Self::F32),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneShaderStage {
    Vertex,
    Fragment,
    Compute,
}

impl SceneShaderStage {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Vertex => 1,
            Self::Fragment => 2,
            Self::Compute => 3,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Vertex),
            2 => Some(Self::Fragment),
            3 => Some(Self::Compute),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SceneShaderBindingKind {
    SampledImage,
    StorageImage,
    Sampler,
    UniformBuffer,
    StorageBuffer,
}

impl SceneShaderBindingKind {
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::SampledImage => 1,
            Self::StorageImage => 2,
            Self::Sampler => 3,
            Self::UniformBuffer => 4,
            Self::StorageBuffer => 5,
        }
    }

    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::SampledImage),
            2 => Some(Self::StorageImage),
            3 => Some(Self::Sampler),
            4 => Some(Self::UniformBuffer),
            5 => Some(Self::StorageBuffer),
            _ => None,
        }
    }
}
