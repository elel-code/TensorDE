//! GPU upload payload planning for native Vulkan scene resources.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use crate::core::scene::{
    SceneMeshSkin, SceneMeshSkinBone, SceneMeshSkinVertex, SceneMeshVertex,
    ScenePuppetAnimationClip, ScenePuppetTransform,
};
use crate::engine::scene_engine::{
    SCENE_GPU_MESH_INDEX_BYTES, SCENE_GPU_MESH_VERTEX_BYTES, SCENE_GPU_PARENT_NONE,
    SCENE_GPU_PUPPET_BONE_BYTES, SCENE_GPU_PUPPET_CLIP_FRAME_BYTES,
    SCENE_GPU_PUPPET_SKIN_VERTEX_BYTES, SceneResource,
};

use super::resource_storage::{
    NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRequirement,
    NativeVulkanSceneGpuBufferRole, NativeVulkanSceneResourceStorage,
};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct NativeVulkanSceneGpuUploadPlan {
    uploads: Vec<NativeVulkanSceneGpuBufferUpload>,
}

impl NativeVulkanSceneGpuUploadPlan {
    pub fn from_resources(
        resources: &[SceneResource],
    ) -> Result<Self, NativeVulkanSceneGpuUploadError> {
        let mut uploads = Vec::new();
        for resource in resources {
            match resource {
                SceneResource::Texture { .. } | SceneResource::Buffer { .. } => {}
                SceneResource::MeshGeometry {
                    id,
                    vertices,
                    indices,
                    ..
                } => {
                    let owner = NativeVulkanSceneGpuBufferOwner::MeshGeometry(*id);
                    push_upload(
                        &mut uploads,
                        owner,
                        NativeVulkanSceneGpuBufferRole::MeshVertex,
                        vertices.len(),
                        SCENE_GPU_MESH_VERTEX_BYTES,
                        mesh_vertex_payload(owner, vertices)?,
                    )?;
                    push_upload(
                        &mut uploads,
                        owner,
                        NativeVulkanSceneGpuBufferRole::MeshIndex,
                        indices.len(),
                        SCENE_GPU_MESH_INDEX_BYTES,
                        mesh_index_payload(owner, indices)?,
                    )?;
                }
                SceneResource::PuppetRig {
                    id, skin, clips, ..
                } => {
                    let owner = NativeVulkanSceneGpuBufferOwner::PuppetRig(*id);
                    if let Some(skin) = skin {
                        push_upload(
                            &mut uploads,
                            owner,
                            NativeVulkanSceneGpuBufferRole::PuppetBone,
                            skin.bones.len(),
                            SCENE_GPU_PUPPET_BONE_BYTES,
                            puppet_bone_payload(owner, skin)?,
                        )?;
                        push_upload(
                            &mut uploads,
                            owner,
                            NativeVulkanSceneGpuBufferRole::PuppetSkinVertex,
                            skin.vertices.len(),
                            SCENE_GPU_PUPPET_SKIN_VERTEX_BYTES,
                            puppet_skin_vertex_payload(owner, skin)?,
                        )?;
                    }
                    let clip_frame_count = clips
                        .iter()
                        .flat_map(|clip| &clip.bones)
                        .map(|bone| bone.frames.len())
                        .sum();
                    push_upload(
                        &mut uploads,
                        owner,
                        NativeVulkanSceneGpuBufferRole::PuppetClipFrame,
                        clip_frame_count,
                        SCENE_GPU_PUPPET_CLIP_FRAME_BYTES,
                        puppet_clip_frame_payload(owner, clips)?,
                    )?;
                }
            }
        }
        Ok(Self { uploads })
    }

    pub fn from_resident_resources(
        storage: &NativeVulkanSceneResourceStorage,
        resources: &[SceneResource],
    ) -> Result<Self, NativeVulkanSceneGpuUploadError> {
        let active = storage
            .gpu_buffer_requirements()
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut pending = active.clone();
        let mut resident_uploads = Vec::new();

        for upload in Self::from_resources(resources)?.uploads {
            if active.contains(&upload.requirement) {
                pending.remove(&upload.requirement);
                resident_uploads.push(upload);
            }
        }

        if let Some(requirement) = pending.into_iter().next() {
            return Err(NativeVulkanSceneGpuUploadError::MissingResidentPayload { requirement });
        }

        Ok(Self {
            uploads: resident_uploads,
        })
    }

    pub fn uploads(&self) -> &[NativeVulkanSceneGpuBufferUpload] {
        &self.uploads
    }

    pub fn into_uploads(self) -> Vec<NativeVulkanSceneGpuBufferUpload> {
        self.uploads
    }

    #[cfg(test)]
    pub(in crate::renderer::native_vulkan) fn from_uploads_for_test(
        uploads: Vec<NativeVulkanSceneGpuBufferUpload>,
    ) -> Self {
        Self { uploads }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct NativeVulkanSceneGpuBufferUpload {
    pub requirement: NativeVulkanSceneGpuBufferRequirement,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeVulkanSceneGpuUploadError {
    NonFiniteFloat {
        owner: NativeVulkanSceneGpuBufferOwner,
        role: NativeVulkanSceneGpuBufferRole,
        field: &'static str,
        element: usize,
    },
    IndexTooLarge {
        owner: NativeVulkanSceneGpuBufferOwner,
        role: NativeVulkanSceneGpuBufferRole,
        field: &'static str,
        value: usize,
    },
    ByteCountOverflow {
        owner: NativeVulkanSceneGpuBufferOwner,
        role: NativeVulkanSceneGpuBufferRole,
        count: usize,
        record_bytes: u64,
    },
    UploadSizeMismatch {
        requirement: NativeVulkanSceneGpuBufferRequirement,
        actual: u64,
    },
    MissingResidentPayload {
        requirement: NativeVulkanSceneGpuBufferRequirement,
    },
}

impl fmt::Display for NativeVulkanSceneGpuUploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteFloat {
                owner,
                role,
                field,
                element,
            } => write!(
                f,
                "non-finite scene GPU upload float at {owner:?} {role:?} {field}[{element}]"
            ),
            Self::IndexTooLarge {
                owner,
                role,
                field,
                value,
            } => write!(
                f,
                "scene GPU upload index is too large at {owner:?} {role:?} {field}: {value}"
            ),
            Self::ByteCountOverflow {
                owner,
                role,
                count,
                record_bytes,
            } => write!(
                f,
                "scene GPU upload byte count overflow at {owner:?} {role:?}: {count} records * {record_bytes} bytes"
            ),
            Self::UploadSizeMismatch {
                requirement,
                actual,
            } => write!(
                f,
                "scene GPU upload size mismatch for {requirement:?}: actual {actual} bytes"
            ),
            Self::MissingResidentPayload { requirement } => {
                write!(f, "missing resident scene GPU payload for {requirement:?}")
            }
        }
    }
}

impl Error for NativeVulkanSceneGpuUploadError {}

fn mesh_vertex_payload(
    owner: NativeVulkanSceneGpuBufferOwner,
    vertices: &[SceneMeshVertex],
) -> Result<Vec<u8>, NativeVulkanSceneGpuUploadError> {
    let role = NativeVulkanSceneGpuBufferRole::MeshVertex;
    let mut payload = Vec::with_capacity(payload_capacity(
        owner,
        role,
        vertices.len(),
        SCENE_GPU_MESH_VERTEX_BYTES,
    )?);
    for (element, vertex) in vertices.iter().enumerate() {
        push_f32(&mut payload, owner, role, "x", element, vertex.x)?;
        push_f32(&mut payload, owner, role, "y", element, vertex.y)?;
        push_f32(&mut payload, owner, role, "u", element, vertex.u)?;
        push_f32(&mut payload, owner, role, "v", element, vertex.v)?;
        push_f32(
            &mut payload,
            owner,
            role,
            "opacity",
            element,
            vertex.opacity,
        )?;
    }
    Ok(payload)
}

fn mesh_index_payload(
    owner: NativeVulkanSceneGpuBufferOwner,
    indices: &[u32],
) -> Result<Vec<u8>, NativeVulkanSceneGpuUploadError> {
    let role = NativeVulkanSceneGpuBufferRole::MeshIndex;
    let mut payload = Vec::with_capacity(payload_capacity(
        owner,
        role,
        indices.len(),
        SCENE_GPU_MESH_INDEX_BYTES,
    )?);
    for index in indices {
        push_u32(&mut payload, *index);
    }
    Ok(payload)
}

fn puppet_bone_payload(
    owner: NativeVulkanSceneGpuBufferOwner,
    skin: &SceneMeshSkin,
) -> Result<Vec<u8>, NativeVulkanSceneGpuUploadError> {
    let role = NativeVulkanSceneGpuBufferRole::PuppetBone;
    let mut payload = Vec::with_capacity(payload_capacity(
        owner,
        role,
        skin.bones.len(),
        SCENE_GPU_PUPPET_BONE_BYTES,
    )?);
    for (element, bone) in skin.bones.iter().enumerate() {
        push_puppet_bone(&mut payload, owner, role, element, bone)?;
    }
    Ok(payload)
}

fn puppet_skin_vertex_payload(
    owner: NativeVulkanSceneGpuBufferOwner,
    skin: &SceneMeshSkin,
) -> Result<Vec<u8>, NativeVulkanSceneGpuUploadError> {
    let role = NativeVulkanSceneGpuBufferRole::PuppetSkinVertex;
    let mut payload = Vec::with_capacity(payload_capacity(
        owner,
        role,
        skin.vertices.len(),
        SCENE_GPU_PUPPET_SKIN_VERTEX_BYTES,
    )?);
    for (element, vertex) in skin.vertices.iter().enumerate() {
        push_skin_vertex(&mut payload, owner, role, element, vertex)?;
    }
    Ok(payload)
}

fn puppet_clip_frame_payload(
    owner: NativeVulkanSceneGpuBufferOwner,
    clips: &[ScenePuppetAnimationClip],
) -> Result<Vec<u8>, NativeVulkanSceneGpuUploadError> {
    let role = NativeVulkanSceneGpuBufferRole::PuppetClipFrame;
    let frame_count = clips
        .iter()
        .flat_map(|clip| &clip.bones)
        .map(|bone| bone.frames.len())
        .sum();
    let mut payload = Vec::with_capacity(payload_capacity(
        owner,
        role,
        frame_count,
        SCENE_GPU_PUPPET_CLIP_FRAME_BYTES,
    )?);
    let mut element = 0;
    for clip in clips {
        for bone in &clip.bones {
            for frame in &bone.frames {
                push_transform(&mut payload, owner, role, element, frame)?;
                element += 1;
            }
        }
    }
    Ok(payload)
}

fn push_puppet_bone(
    payload: &mut Vec<u8>,
    owner: NativeVulkanSceneGpuBufferOwner,
    role: NativeVulkanSceneGpuBufferRole,
    element: usize,
    bone: &SceneMeshSkinBone,
) -> Result<(), NativeVulkanSceneGpuUploadError> {
    let parent = match bone.parent {
        Some(parent) => checked_gpu_index(owner, role, "parent", parent)?,
        None => SCENE_GPU_PARENT_NONE,
    };
    push_u32(payload, parent);
    push_u32(payload, 0);
    push_u32(payload, 0);
    push_u32(payload, 0);
    push_transform(payload, owner, role, element, &bone.bind)
}

fn push_skin_vertex(
    payload: &mut Vec<u8>,
    owner: NativeVulkanSceneGpuBufferOwner,
    role: NativeVulkanSceneGpuBufferRole,
    element: usize,
    vertex: &SceneMeshSkinVertex,
) -> Result<(), NativeVulkanSceneGpuUploadError> {
    for (slot, bone_index) in vertex.bone_indices.iter().enumerate() {
        let bone_index = checked_gpu_index(
            owner,
            role,
            match slot {
                0 => "bone_indices.x",
                1 => "bone_indices.y",
                2 => "bone_indices.z",
                _ => "bone_indices.w",
            },
            *bone_index,
        )?;
        push_u32(payload, bone_index);
    }
    for (slot, weight) in vertex.weights.iter().enumerate() {
        push_f32(
            payload,
            owner,
            role,
            match slot {
                0 => "weights.x",
                1 => "weights.y",
                2 => "weights.z",
                _ => "weights.w",
            },
            element,
            *weight,
        )?;
    }
    Ok(())
}

fn push_transform(
    payload: &mut Vec<u8>,
    owner: NativeVulkanSceneGpuBufferOwner,
    role: NativeVulkanSceneGpuBufferRole,
    element: usize,
    transform: &ScenePuppetTransform,
) -> Result<(), NativeVulkanSceneGpuUploadError> {
    push_f32(
        payload,
        owner,
        role,
        "translation.x",
        element,
        transform.translation[0],
    )?;
    push_f32(
        payload,
        owner,
        role,
        "translation.y",
        element,
        transform.translation[1],
    )?;
    push_f32(
        payload,
        owner,
        role,
        "translation.z",
        element,
        transform.translation[2],
    )?;
    push_f32(payload, owner, role, "opacity", element, transform.opacity)?;
    push_f32(
        payload,
        owner,
        role,
        "rotation.x",
        element,
        transform.rotation[0],
    )?;
    push_f32(
        payload,
        owner,
        role,
        "rotation.y",
        element,
        transform.rotation[1],
    )?;
    push_f32(
        payload,
        owner,
        role,
        "rotation.z",
        element,
        transform.rotation[2],
    )?;
    push_f32(payload, owner, role, "reserved.rotation", element, 0.0)?;
    push_f32(payload, owner, role, "scale.x", element, transform.scale[0])?;
    push_f32(payload, owner, role, "scale.y", element, transform.scale[1])?;
    push_f32(payload, owner, role, "scale.z", element, transform.scale[2])?;
    push_f32(payload, owner, role, "reserved.scale", element, 0.0)?;
    Ok(())
}

fn push_upload(
    uploads: &mut Vec<NativeVulkanSceneGpuBufferUpload>,
    owner: NativeVulkanSceneGpuBufferOwner,
    role: NativeVulkanSceneGpuBufferRole,
    count: usize,
    record_bytes: u64,
    payload: Vec<u8>,
) -> Result<(), NativeVulkanSceneGpuUploadError> {
    if count == 0 {
        return Ok(());
    }

    let expected = payload_bytes(owner, role, count, record_bytes)?;
    let actual = u64::try_from(payload.len()).map_err(|_| {
        NativeVulkanSceneGpuUploadError::ByteCountOverflow {
            owner,
            role,
            count,
            record_bytes,
        }
    })?;
    let requirement = NativeVulkanSceneGpuBufferRequirement {
        owner,
        role,
        bytes: expected,
        usage: role.usage(),
    };
    if actual != expected {
        return Err(NativeVulkanSceneGpuUploadError::UploadSizeMismatch {
            requirement,
            actual,
        });
    }
    uploads.push(NativeVulkanSceneGpuBufferUpload {
        requirement,
        payload,
    });
    Ok(())
}

fn payload_capacity(
    owner: NativeVulkanSceneGpuBufferOwner,
    role: NativeVulkanSceneGpuBufferRole,
    count: usize,
    record_bytes: u64,
) -> Result<usize, NativeVulkanSceneGpuUploadError> {
    let record_bytes = usize::try_from(record_bytes).map_err(|_| {
        NativeVulkanSceneGpuUploadError::ByteCountOverflow {
            owner,
            role,
            count,
            record_bytes,
        }
    })?;
    count
        .checked_mul(record_bytes)
        .ok_or(NativeVulkanSceneGpuUploadError::ByteCountOverflow {
            owner,
            role,
            count,
            record_bytes: record_bytes as u64,
        })
}

fn payload_bytes(
    owner: NativeVulkanSceneGpuBufferOwner,
    role: NativeVulkanSceneGpuBufferRole,
    count: usize,
    record_bytes: u64,
) -> Result<u64, NativeVulkanSceneGpuUploadError> {
    u64::try_from(payload_capacity(owner, role, count, record_bytes)?).map_err(|_| {
        NativeVulkanSceneGpuUploadError::ByteCountOverflow {
            owner,
            role,
            count,
            record_bytes,
        }
    })
}

fn push_f32(
    payload: &mut Vec<u8>,
    owner: NativeVulkanSceneGpuBufferOwner,
    role: NativeVulkanSceneGpuBufferRole,
    field: &'static str,
    element: usize,
    value: f64,
) -> Result<(), NativeVulkanSceneGpuUploadError> {
    let value = value as f32;
    if !value.is_finite() {
        return Err(NativeVulkanSceneGpuUploadError::NonFiniteFloat {
            owner,
            role,
            field,
            element,
        });
    }
    payload.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

fn push_u32(payload: &mut Vec<u8>, value: u32) {
    payload.extend_from_slice(&value.to_le_bytes());
}

fn checked_gpu_index(
    owner: NativeVulkanSceneGpuBufferOwner,
    role: NativeVulkanSceneGpuBufferRole,
    field: &'static str,
    value: usize,
) -> Result<u32, NativeVulkanSceneGpuUploadError> {
    if value >= SCENE_GPU_PARENT_NONE as usize {
        return Err(NativeVulkanSceneGpuUploadError::IndexTooLarge {
            owner,
            role,
            field,
            value,
        });
    }
    Ok(value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scene::{SceneMeshSkin, ScenePuppetAnimationBone, ScenePuppetAnimationClip};
    use crate::engine::scene_engine::{
        SceneGeometryId, SceneMeshResidency, ScenePuppetId, ScenePuppetRigResidency,
        SceneResidentResource, SceneResourceResidencyPlan,
    };

    #[test]
    fn upload_plan_packs_mesh_payload_as_gpu_f32_vertices_and_u32_indices() {
        let resources = vec![SceneResource::MeshGeometry {
            id: SceneGeometryId(4),
            source_record: 9,
            vertices: vec![
                SceneMeshVertex {
                    x: 1.25,
                    y: -2.5,
                    u: 0.25,
                    v: 0.75,
                    opacity: 0.5,
                },
                SceneMeshVertex {
                    x: 3.0,
                    y: 4.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 0],
        }];

        let plan = NativeVulkanSceneGpuUploadPlan::from_resources(&resources).unwrap();

        assert_eq!(plan.uploads().len(), 2);
        let vertex_upload = &plan.uploads()[0];
        assert_eq!(
            vertex_upload.requirement,
            NativeVulkanSceneGpuBufferRequirement {
                owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(SceneGeometryId(4)),
                role: NativeVulkanSceneGpuBufferRole::MeshVertex,
                bytes: 40,
                usage: NativeVulkanSceneGpuBufferRole::MeshVertex.usage(),
            }
        );
        assert_eq!(read_f32(&vertex_upload.payload, 0), 1.25);
        assert_eq!(read_f32(&vertex_upload.payload, 4), -2.5);
        assert_eq!(read_f32(&vertex_upload.payload, 8), 0.25);
        assert_eq!(read_f32(&vertex_upload.payload, 12), 0.75);
        assert_eq!(read_f32(&vertex_upload.payload, 16), 0.5);

        let index_upload = &plan.uploads()[1];
        assert_eq!(index_upload.requirement.bytes, 12);
        assert_eq!(read_u32(&index_upload.payload, 0), 0);
        assert_eq!(read_u32(&index_upload.payload, 4), 1);
        assert_eq!(read_u32(&index_upload.payload, 8), 0);
    }

    #[test]
    fn upload_plan_packs_puppet_payload_as_aligned_storage_records() {
        let transform = ScenePuppetTransform {
            translation: [1.0, 2.0, 3.0],
            rotation: [10.0, 20.0, 30.0],
            scale: [1.5, 2.5, 3.5],
            opacity: 0.25,
        };
        let resources = vec![SceneResource::PuppetRig {
            id: ScenePuppetId(8),
            source_record: 3,
            skin: Some(SceneMeshSkin {
                bones: vec![SceneMeshSkinBone {
                    parent: None,
                    bind: transform,
                }],
                vertices: vec![SceneMeshSkinVertex {
                    bone_indices: [0, 1, 2, 3],
                    weights: [0.1, 0.2, 0.3, 0.4],
                }],
                attachments: Vec::new(),
            }),
            clips: vec![ScenePuppetAnimationClip {
                id: 2,
                name: None,
                fps: 30.0,
                frame_count: 1,
                looping: true,
                bones: vec![ScenePuppetAnimationBone {
                    frames: vec![transform],
                }],
            }],
            layers: Vec::new(),
            clipping_records: Vec::new(),
        }];

        let plan = NativeVulkanSceneGpuUploadPlan::from_resources(&resources).unwrap();

        assert_eq!(plan.uploads().len(), 3);
        let bone_upload = &plan.uploads()[0];
        assert_eq!(bone_upload.requirement.bytes, 64);
        assert_eq!(read_u32(&bone_upload.payload, 0), SCENE_GPU_PARENT_NONE);
        assert_eq!(read_f32(&bone_upload.payload, 16), 1.0);
        assert_eq!(read_f32(&bone_upload.payload, 28), 0.25);
        assert_eq!(read_f32(&bone_upload.payload, 32), 10.0);
        assert_eq!(read_f32(&bone_upload.payload, 48), 1.5);

        let skin_upload = &plan.uploads()[1];
        assert_eq!(skin_upload.requirement.bytes, 32);
        assert_eq!(read_u32(&skin_upload.payload, 12), 3);
        assert!((read_f32(&skin_upload.payload, 28) - 0.4).abs() < f32::EPSILON);

        let clip_upload = &plan.uploads()[2];
        assert_eq!(clip_upload.requirement.bytes, 48);
        assert_eq!(read_f32(&clip_upload.payload, 0), 1.0);
        assert_eq!(read_f32(&clip_upload.payload, 12), 0.25);
        assert_eq!(read_f32(&clip_upload.payload, 32), 1.5);
    }

    #[test]
    fn resident_upload_plan_requires_storage_payload_match() {
        let resources = vec![SceneResource::MeshGeometry {
            id: SceneGeometryId(4),
            source_record: 9,
            vertices: vec![SceneMeshVertex::default(); 2],
            indices: vec![0, 1, 0],
        }];
        let mut storage = NativeVulkanSceneResourceStorage::default();
        storage.sync_residency_plan(&SceneResourceResidencyPlan {
            resources: vec![SceneResidentResource::MeshGeometry(SceneMeshResidency {
                id: SceneGeometryId(4),
                source_record: 9,
                vertex_count: 2,
                index_count: 3,
                vertex_bytes: 40,
                index_bytes: 12,
            })],
        });

        let plan =
            NativeVulkanSceneGpuUploadPlan::from_resident_resources(&storage, &resources).unwrap();

        assert_eq!(plan.uploads().len(), 2);
    }

    #[test]
    fn resident_upload_plan_rejects_missing_payload_for_active_storage() {
        let mut storage = NativeVulkanSceneResourceStorage::default();
        storage.sync_residency_plan(&SceneResourceResidencyPlan {
            resources: vec![SceneResidentResource::PuppetRig(ScenePuppetRigResidency {
                id: ScenePuppetId(5),
                source_record: 2,
                bone_count: 1,
                bone_bytes: 64,
                skin_vertex_count: 0,
                skin_vertex_bytes: 0,
                attachment_count: 0,
                clip_count: 0,
                clip_bone_count: 0,
                clip_frame_count: 0,
                clip_frame_bytes: 0,
                layer_count: 0,
                clipping_record_count: 0,
                clipping_bone_count: 0,
                clipping_frame_key_count: 0,
            })],
        });

        let err = NativeVulkanSceneGpuUploadPlan::from_resident_resources(&storage, &[])
            .expect_err("active storage without payload must fail");

        assert!(matches!(
            err,
            NativeVulkanSceneGpuUploadError::MissingResidentPayload {
                requirement: NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::PuppetRig(ScenePuppetId(5)),
                    role: NativeVulkanSceneGpuBufferRole::PuppetBone,
                    bytes: 64,
                    ..
                }
            }
        ));
    }

    #[test]
    fn upload_plan_rejects_non_finite_gpu_floats() {
        let resources = vec![SceneResource::MeshGeometry {
            id: SceneGeometryId(4),
            source_record: 9,
            vertices: vec![SceneMeshVertex {
                x: f64::INFINITY,
                ..SceneMeshVertex::default()
            }],
            indices: Vec::new(),
        }];

        let err = NativeVulkanSceneGpuUploadPlan::from_resources(&resources)
            .expect_err("non-finite vertex data must fail");

        assert!(matches!(
            err,
            NativeVulkanSceneGpuUploadError::NonFiniteFloat {
                owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(SceneGeometryId(4)),
                role: NativeVulkanSceneGpuBufferRole::MeshVertex,
                field: "x",
                element: 0,
            }
        ));
    }

    #[test]
    fn upload_plan_rejects_none_sentinel_as_real_gpu_index() {
        let resources = vec![SceneResource::PuppetRig {
            id: ScenePuppetId(8),
            source_record: 3,
            skin: Some(SceneMeshSkin {
                bones: vec![SceneMeshSkinBone {
                    parent: None,
                    bind: ScenePuppetTransform::default(),
                }],
                vertices: vec![SceneMeshSkinVertex {
                    bone_indices: [SCENE_GPU_PARENT_NONE as usize, 0, 0, 0],
                    weights: [1.0, 0.0, 0.0, 0.0],
                }],
                attachments: Vec::new(),
            }),
            clips: Vec::new(),
            layers: Vec::new(),
            clipping_records: Vec::new(),
        }];

        let err = NativeVulkanSceneGpuUploadPlan::from_resources(&resources)
            .expect_err("none sentinel cannot be a real bone index");

        assert!(matches!(
            err,
            NativeVulkanSceneGpuUploadError::IndexTooLarge {
                owner: NativeVulkanSceneGpuBufferOwner::PuppetRig(ScenePuppetId(8)),
                role: NativeVulkanSceneGpuBufferRole::PuppetSkinVertex,
                field: "bone_indices.x",
                value,
            } if value == SCENE_GPU_PARENT_NONE as usize
        ));
    }

    fn read_f32(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn read_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }
}
