//! Resource residency metadata derived from engine-owned scene resources.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`

use serde::Serialize;

use super::{
    SCENE_GPU_MESH_INDEX_BYTES, SCENE_GPU_MESH_VERTEX_BYTES, SCENE_GPU_PUPPET_ACTIVE_SOURCE_BYTES,
    SCENE_GPU_PUPPET_BONE_BYTES, SCENE_GPU_PUPPET_CLIP_FRAME_BYTES,
    SCENE_GPU_PUPPET_CLIPPING_BONE_INDEX_BYTES, SCENE_GPU_PUPPET_CLIPPING_FRAME_KEY_BYTES,
    SCENE_GPU_PUPPET_CLIPPING_RECORD_BYTES, SCENE_GPU_PUPPET_SKIN_VERTEX_BYTES, SceneGeometryId,
    ScenePuppetId, SceneResource, SceneResourceId, SceneTextureFormat, scene_gpu_record_bytes,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
                id,
                width,
                height,
                format,
                mip_count,
                payload_bytes,
                ..
            } => Self::Texture(SceneTextureResidency {
                id: *id,
                width: *width,
                height: *height,
                format: *format,
                mip_count: *mip_count,
                payload_bytes: *payload_bytes,
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
                vertex_bytes: scene_gpu_record_bytes(vertices.len(), SCENE_GPU_MESH_VERTEX_BYTES),
                index_bytes: scene_gpu_record_bytes(indices.len(), SCENE_GPU_MESH_INDEX_BYTES),
            }),
            SceneResource::PuppetRig {
                id,
                source_record,
                skin,
                clips,
                layers,
                clipping,
            } => Self::PuppetRig(ScenePuppetRigResidency {
                id: *id,
                source_record: *source_record,
                bone_count: skin
                    .as_ref()
                    .map(|skin| skin.bones.len().min(u32::MAX as usize) as u32)
                    .unwrap_or_default(),
                bone_bytes: scene_residency_bytes(
                    skin.as_ref()
                        .map(|skin| skin.bones.len())
                        .unwrap_or_default(),
                    SCENE_GPU_PUPPET_BONE_BYTES,
                ),
                skin_vertex_count: skin
                    .as_ref()
                    .map(|skin| skin.vertices.len().min(u32::MAX as usize) as u32)
                    .unwrap_or_default(),
                skin_vertex_bytes: scene_residency_bytes(
                    skin.as_ref()
                        .map(|skin| skin.vertices.len())
                        .unwrap_or_default(),
                    SCENE_GPU_PUPPET_SKIN_VERTEX_BYTES,
                ),
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
                clip_frame_bytes: scene_residency_bytes(
                    clips
                        .iter()
                        .flat_map(|clip| &clip.bones)
                        .map(|bone| bone.frames.len())
                        .sum(),
                    SCENE_GPU_PUPPET_CLIP_FRAME_BYTES,
                ),
                layer_count: layers.len().min(u32::MAX as usize) as u32,
                clipping_record_count: clipping.records.len().min(u32::MAX as usize) as u32,
                clipping_record_bytes: scene_residency_bytes(
                    clipping.records.len(),
                    SCENE_GPU_PUPPET_CLIPPING_RECORD_BYTES,
                ),
                clipping_bone_count: clipping.bone_indices.len().min(u32::MAX as usize) as u32,
                clipping_bone_bytes: scene_residency_bytes(
                    clipping.bone_indices.len(),
                    SCENE_GPU_PUPPET_CLIPPING_BONE_INDEX_BYTES,
                ),
                clipping_frame_key_count: clipping.frame_keys.len().min(u32::MAX as usize) as u32,
                clipping_frame_key_bytes: scene_residency_bytes(
                    clipping.frame_keys.len(),
                    SCENE_GPU_PUPPET_CLIPPING_FRAME_KEY_BYTES,
                ),
                active_source_count: clipping.active_sources.len().min(u32::MAX as usize) as u32,
                active_source_bytes: scene_residency_bytes(
                    clipping.active_sources.len(),
                    SCENE_GPU_PUPPET_ACTIVE_SOURCE_BYTES,
                ),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneTextureResidency {
    pub id: SceneResourceId,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<SceneTextureFormat>,
    pub mip_count: Option<u32>,
    pub payload_bytes: Option<u64>,
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
    pub bone_bytes: u64,
    pub skin_vertex_count: u32,
    pub skin_vertex_bytes: u64,
    pub attachment_count: u32,
    pub clip_count: u32,
    pub clip_bone_count: u32,
    pub clip_frame_count: u32,
    pub clip_frame_bytes: u64,
    pub layer_count: u32,
    pub clipping_record_count: u32,
    pub clipping_record_bytes: u64,
    pub clipping_bone_count: u32,
    pub clipping_bone_bytes: u64,
    pub clipping_frame_key_count: u32,
    pub clipping_frame_key_bytes: u64,
    pub active_source_count: u32,
    pub active_source_bytes: u64,
}

fn scene_residency_bytes(count: usize, record_bytes: u64) -> u64 {
    scene_gpu_record_bytes(count, record_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scene::SceneMeshPuppetClippingRecord;
    use crate::core::scene::{SceneMeshVertex, ScenePuppetTransform};
    use crate::core::scene::{ScenePuppetAnimationBone, ScenePuppetAnimationClip};
    use crate::engine::scene_engine::{
        ScenePuppetClippingActiveSource, ScenePuppetClippingProgram,
    };

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
                clipping: Default::default(),
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
                vertex_bytes: 60,
                index_bytes: 12,
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
                clip_frame_bytes: 96,
                ..
            })
        ));
    }

    #[test]
    fn residency_plan_preserves_native_texture_metadata() {
        let resources = vec![SceneResource::Texture {
            id: SceneResourceId(9),
            source: "assets/eye.gtex".into(),
            width: Some(663),
            height: Some(230),
            format: Some(SceneTextureFormat::Bc7UnormBlock),
            mip_count: Some(1),
            payload_bytes: Some(155_520),
        }];

        let plan = SceneResourceResidencyPlan::from_resources(&resources);

        assert_eq!(
            plan.resources,
            vec![SceneResidentResource::Texture(SceneTextureResidency {
                id: SceneResourceId(9),
                width: Some(663),
                height: Some(230),
                format: Some(SceneTextureFormat::Bc7UnormBlock),
                mip_count: Some(1),
                payload_bytes: Some(155_520),
            })]
        );
    }

    #[test]
    fn residency_plan_counts_puppet_clipping_gpu_buffers() {
        let mut clipping =
            ScenePuppetClippingProgram::from_source_records(vec![SceneMeshPuppetClippingRecord {
                source_name: Some("eye-right".to_owned()),
                mask: "masks/clipping_mask_eye".to_owned(),
                mask_resource: Some("assets/clipping-mask.gtex".to_owned()),
                duration_frames: 1680,
                flags: 1,
                bones: vec![42, 43],
                frame_keys: vec![0, 1, 2],
            }]);
        clipping
            .active_sources
            .push(ScenePuppetClippingActiveSource {
                source_name: "eye-right".to_owned(),
                scalar_bits: 1.0f32.to_bits(),
                source_scale: 6,
                flags: 2,
                transform_index: 4,
                parameter0: -1.0,
                parameter1: 0.5,
            });

        let resources = vec![SceneResource::PuppetRig {
            id: ScenePuppetId(7),
            source_record: 8,
            skin: None,
            clips: Vec::new(),
            layers: Vec::new(),
            clipping,
        }];

        let plan = SceneResourceResidencyPlan::from_resources(&resources);

        assert!(matches!(
            plan.resources.as_slice(),
            [SceneResidentResource::PuppetRig(ScenePuppetRigResidency {
                clipping_record_count: 1,
                clipping_record_bytes: 48,
                clipping_bone_count: 2,
                clipping_bone_bytes: 8,
                clipping_frame_key_count: 3,
                clipping_frame_key_bytes: 12,
                active_source_count: 1,
                active_source_bytes: 64,
                ..
            })]
        ));
    }
}
