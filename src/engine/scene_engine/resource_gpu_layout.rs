//! Stable GPU layouts for engine-owned scene resources.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/storage/`

use super::we::WE_VEC4_BYTES;

pub const SCENE_GPU_PARENT_NONE: u32 = u32::MAX;

pub const SCENE_GPU_MESH_VERTEX_BYTES: u64 = 20;
pub const SCENE_GPU_MESH_INDEX_BYTES: u64 = 4;
pub const SCENE_GPU_PUPPET_TRANSFORM_BYTES: u64 = 48;
pub const SCENE_GPU_PUPPET_BONE_BYTES: u64 = 64;
pub const SCENE_GPU_PUPPET_SKIN_VERTEX_BYTES: u64 = 32;
pub const SCENE_GPU_PUPPET_CLIP_FRAME_BYTES: u64 = SCENE_GPU_PUPPET_TRANSFORM_BYTES;
pub const SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES: u64 = WE_VEC4_BYTES * 3;

pub fn scene_gpu_record_bytes(count: usize, record_bytes: u64) -> u64 {
    u64::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(record_bytes))
        .unwrap_or(u64::MAX)
}
