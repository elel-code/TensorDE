//! Resource residency metadata derived from engine-owned scene resources.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`

use std::mem::size_of;

use serde::Serialize;

use super::{SceneGeometryId, ScenePuppetId, SceneResource, SceneResourceId};
use crate::core::scene::{SceneMeshVertex, ScenePuppetTransform};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SceneResourceResidencyPlan {
    pub resources: Vec<SceneResidentResource>,
}

impl SceneResourceResidencyPlan {
    pub fn from_resources(resources: &[SceneResource]) -> Self {
        Self {
            resources: resources.iter().map(SceneResidentResource::from).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SceneResidentResource {
    Texture(SceneTextureResidency),
    Buffer(SceneBufferResidency),
    MeshGeometry(SceneMeshResidency),
    PuppetRig(ScenePuppetRigResidency),
}

impl From<&SceneResource> for SceneResidentResource {
    fn from(resource: &SceneResource) -> Self {
        match resource {
            SceneResource::Texture {
                id, width, height, ..
            } => Self::Texture(SceneTextureResidency {
                id: *id,
                width: *width,
                height: *height,
            }),
            SceneResource::Buffer { id, bytes } => Self::Buffer(SceneBufferResidency {
                id: *id,
                bytes: *bytes,
            }),
            SceneResource::MeshGeometry {
                id,
                source_record,
                vertices,
                indices,
            } => Self::MeshGeometry(SceneMeshResidency {
                id: *id,
                source_record: *source_record,
                vertex_count: vertices.len().min(u32::MAX as usize) as u32,
                index_count: indices.len().min(u32::MAX as usize) as u32,
                vertex_bytes: scene_residency_bytes::<SceneMeshVertex>(vertices.len()),
                index_bytes: scene_residency_bytes::<u32>(indices.len()),
            }),
            SceneResource::PuppetRig {
                id,
                source_record,
                skin,
                clips,
                layers,
                clipping_records,
            } => Self::PuppetRig(ScenePuppetRigResidency {
                id: *id,
                source_record: *source_record,
                bone_count: skin
                    .as_ref()
                    .map(|skin| skin.bones.len().min(u32::MAX as usize) as u32)
                    .unwrap_or_default(),
                skin_vertex_count: skin
                    .as_ref()
                    .map(|skin| skin.vertices.len().min(u32::MAX as usize) as u32)
                    .unwrap_or_default(),
                attachment_count: skin
                    .as_ref()
                    .map(|skin| skin.attachments.len().min(u32::MAX as usize) as u32)
                    .unwrap_or_default(),
                clip_count: clips.len().min(u32::MAX as usize) as u32,
                clip_bone_count: clips
                    .iter()
                    .map(|clip| clip.bones.len())
                    .sum::<usize>()
                    .min(u32::MAX as usize) as u32,
                clip_frame_count: clips
                    .iter()
                    .flat_map(|clip| &clip.bones)
                    .map(|bone| bone.frames.len())
                    .sum::<usize>()
                    .min(u32::MAX as usize) as u32,
                clip_frame_bytes: scene_residency_bytes::<ScenePuppetTransform>(
                    clips
                        .iter()
                        .flat_map(|clip| &clip.bones)
                        .map(|bone| bone.frames.len())
                        .sum(),
                ),
                layer_count: layers.len().min(u32::MAX as usize) as u32,
                clipping_record_count: clipping_records.len().min(u32::MAX as usize) as u32,
                clipping_bone_count: clipping_records
                    .iter()
                    .map(|record| record.bones.len())
                    .sum::<usize>()
                    .min(u32::MAX as usize) as u32,
                clipping_frame_key_count: clipping_records
                    .iter()
                    .map(|record| record.frame_keys.len())
                    .sum::<usize>()
                    .min(u32::MAX as usize) as u32,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneTextureResidency {
    pub id: SceneResourceId,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneBufferResidency {
    pub id: SceneResourceId,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneMeshResidency {
    pub id: SceneGeometryId,
    pub source_record: u32,
    pub vertex_count: u32,
    pub index_count: u32,
    pub vertex_bytes: u64,
    pub index_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScenePuppetRigResidency {
    pub id: ScenePuppetId,
    pub source_record: u32,
    pub bone_count: u32,
    pub skin_vertex_count: u32,
    pub attachment_count: u32,
    pub clip_count: u32,
    pub clip_bone_count: u32,
    pub clip_frame_count: u32,
    pub clip_frame_bytes: u64,
    pub layer_count: u32,
    pub clipping_record_count: u32,
    pub clipping_bone_count: u32,
    pub clipping_frame_key_count: u32,
}

fn scene_residency_bytes<T>(count: usize) -> u64 {
    count
        .checked_mul(size_of::<T>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scene::{ScenePuppetAnimationBone, ScenePuppetAnimationClip};

    #[test]
    fn residency_plan_keeps_mesh_and_puppet_payload_out_of_draw_graph() {
        let resources = vec![
            SceneResource::MeshGeometry {
                id: SceneGeometryId(4),
                source_record: 12,
                vertices: vec![SceneMeshVertex::default(); 3],
                indices: vec![0, 1, 2],
            },
            SceneResource::PuppetRig {
                id: ScenePuppetId(7),
                source_record: 8,
                skin: None,
                clips: vec![ScenePuppetAnimationClip {
                    id: 1,
                    name: None,
                    fps: 30.0,
                    frame_count: 2,
                    looping: true,
                    bones: vec![ScenePuppetAnimationBone {
                        frames: vec![ScenePuppetTransform::default(); 2],
                    }],
                }],
                layers: Vec::new(),
                clipping_records: Vec::new(),
            },
        ];

        let plan = SceneResourceResidencyPlan::from_resources(&resources);
        assert_eq!(plan.resources.len(), 2);
        assert!(matches!(
            plan.resources[0],
            SceneResidentResource::MeshGeometry(SceneMeshResidency {
                id: SceneGeometryId(4),
                vertex_count: 3,
                index_count: 3,
                ..
            })
        ));
        assert!(matches!(
            plan.resources[1],
            SceneResidentResource::PuppetRig(ScenePuppetRigResidency {
                id: ScenePuppetId(7),
                clip_count: 1,
                clip_bone_count: 1,
                clip_frame_count: 2,
                ..
            })
        ));
    }
}
