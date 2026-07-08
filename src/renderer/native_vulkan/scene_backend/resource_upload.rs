//! GPU upload payload planning for native Vulkan scene resources.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
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
    SCENE_GPU_LAYER_ALPHA_MASK_RT_METHOD8_MDLV_INDEX_BYTES, SCENE_GPU_MESH_INDEX_BYTES,
    SCENE_GPU_MESH_VERTEX_BYTES, SCENE_GPU_PARENT_NONE, SCENE_GPU_PUPPET_ACTIVE_SOURCE_BYTES,
    SCENE_GPU_PUPPET_BONE_BYTES, SCENE_GPU_PUPPET_CLIP_FRAME_BYTES,
    SCENE_GPU_PUPPET_CLIPPING_BONE_INDEX_BYTES, SCENE_GPU_PUPPET_CLIPPING_FRAME_KEY_BYTES,
    SCENE_GPU_PUPPET_CLIPPING_RECORD_BYTES, SCENE_GPU_PUPPET_SKIN_VERTEX_BYTES, SceneFramePlan,
    SceneLayerAlphaMaskRtMethod8MdlvGeometry, SceneLayerCompositorEntry,
    SceneLayerCompositorOperation, SceneLayerCompositorPlan, ScenePuppetClippingProgram,
    SceneResource, scene_stable_name_hash,
};

use super::layer_alpha_mask_executor::{
    FLATTEXTURE_COPY_BACK_VERTEX_COUNT, FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES,
    native_vulkan_scene_layer_alpha_mask_copy_back_fullscreen_triangle_payload,
    native_vulkan_scene_layer_alpha_mask_rt_method8_lower_aux_payload,
    native_vulkan_scene_layer_alpha_mask_rt_method8_materialize_index_slice,
};
use super::resource_storage::{
    NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRequirement,
    NativeVulkanSceneGpuBufferRole, NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice,
    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind,
    NativeVulkanSceneRenderStateUtilityGeometry, NativeVulkanSceneResourceStorage,
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
                SceneResource::Texture { .. }
                | SceneResource::Buffer { .. }
                | SceneResource::LayerAuxCompositeTargets { .. } => {}
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
                SceneResource::LayerAlphaMaskRtMethod8MdlvGeometry { geometry } => {
                    push_layer_alpha_mask_rt_method8_mdlv_geometry_uploads(&mut uploads, geometry)?;
                }
                SceneResource::PuppetRig {
                    id,
                    skin,
                    clips,
                    clipping,
                    ..
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
                    push_upload(
                        &mut uploads,
                        owner,
                        NativeVulkanSceneGpuBufferRole::PuppetClippingRecord,
                        clipping.records.len(),
                        SCENE_GPU_PUPPET_CLIPPING_RECORD_BYTES,
                        puppet_clipping_record_payload(owner, clipping)?,
                    )?;
                    push_upload(
                        &mut uploads,
                        owner,
                        NativeVulkanSceneGpuBufferRole::PuppetClippingBoneIndex,
                        clipping.bone_indices.len(),
                        SCENE_GPU_PUPPET_CLIPPING_BONE_INDEX_BYTES,
                        puppet_clipping_bone_index_payload(owner, clipping)?,
                    )?;
                    push_upload(
                        &mut uploads,
                        owner,
                        NativeVulkanSceneGpuBufferRole::PuppetClippingFrameKey,
                        clipping.frame_keys.len(),
                        SCENE_GPU_PUPPET_CLIPPING_FRAME_KEY_BYTES,
                        puppet_clipping_frame_key_payload(owner, clipping)?,
                    )?;
                    push_upload(
                        &mut uploads,
                        owner,
                        NativeVulkanSceneGpuBufferRole::PuppetActiveSource,
                        clipping.active_sources.len(),
                        SCENE_GPU_PUPPET_ACTIVE_SOURCE_BYTES,
                        puppet_active_source_payload(owner, clipping)?,
                    )?;
                }
            }
        }
        Ok(Self { uploads })
    }

    pub fn from_scene_frame(
        storage: &NativeVulkanSceneResourceStorage,
        resources: &[SceneResource],
        frame: &SceneFramePlan,
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

        push_frame_render_state_utility_uploads(&mut resident_uploads, &frame.layer_compositor)?;
        push_frame_layer_alpha_mask_rt_method8_mdlv_index_slice_uploads(
            &mut resident_uploads,
            resources,
            &frame.layer_compositor,
        )?;

        Ok(Self {
            uploads: resident_uploads,
        })
    }

    #[cfg(test)]
    fn from_resident_resources_for_test(
        storage: &NativeVulkanSceneResourceStorage,
        resources: &[SceneResource],
    ) -> Result<Self, NativeVulkanSceneGpuUploadError> {
        let frame = SceneFramePlan {
            residency: Default::default(),
            graph: crate::engine::scene_engine::SceneGraph { passes: Vec::new() },
            effect_pass_graph: crate::engine::scene_engine::SceneEffectPassGraphPlan::empty(),
            effect_uniforms: crate::engine::scene_engine::SceneEffectUniformFramePlan::empty(),
            final_compositor: crate::engine::scene_engine::SceneFinalCompositorPlan::empty(),
            layer_compositor: SceneLayerCompositorPlan::empty(),
        };
        Self::from_scene_frame(storage, resources, &frame)
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
    SemanticLowering {
        owner: NativeVulkanSceneGpuBufferOwner,
        role: NativeVulkanSceneGpuBufferRole,
        message: String,
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
            Self::SemanticLowering {
                owner,
                role,
                message,
            } => write!(
                f,
                "failed to lower scene GPU upload payload for {owner:?} {role:?}: {message}"
            ),
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

fn puppet_clipping_record_payload(
    owner: NativeVulkanSceneGpuBufferOwner,
    clipping: &ScenePuppetClippingProgram,
) -> Result<Vec<u8>, NativeVulkanSceneGpuUploadError> {
    let role = NativeVulkanSceneGpuBufferRole::PuppetClippingRecord;
    let mut payload = Vec::with_capacity(payload_capacity(
        owner,
        role,
        clipping.records.len(),
        SCENE_GPU_PUPPET_CLIPPING_RECORD_BYTES,
    )?);
    for record in &clipping.records {
        push_u64_words(&mut payload, record.source_name_hash);
        push_u32(&mut payload, record.duration_frames);
        push_u32(&mut payload, record.flags);
        push_u32(&mut payload, record.first_bone);
        push_u32(&mut payload, record.bone_count);
        push_u32(&mut payload, record.first_frame_key);
        push_u32(&mut payload, record.frame_key_count);
        push_u32(
            &mut payload,
            record.active_source_index.unwrap_or(SCENE_GPU_PARENT_NONE),
        );
        push_u32(
            &mut payload,
            record.mask_texture_index.unwrap_or(SCENE_GPU_PARENT_NONE),
        );
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 0);
    }
    Ok(payload)
}

fn puppet_clipping_bone_index_payload(
    owner: NativeVulkanSceneGpuBufferOwner,
    clipping: &ScenePuppetClippingProgram,
) -> Result<Vec<u8>, NativeVulkanSceneGpuUploadError> {
    let role = NativeVulkanSceneGpuBufferRole::PuppetClippingBoneIndex;
    let mut payload = Vec::with_capacity(payload_capacity(
        owner,
        role,
        clipping.bone_indices.len(),
        SCENE_GPU_PUPPET_CLIPPING_BONE_INDEX_BYTES,
    )?);
    for bone in &clipping.bone_indices {
        push_u32(&mut payload, *bone);
    }
    Ok(payload)
}

fn puppet_clipping_frame_key_payload(
    owner: NativeVulkanSceneGpuBufferOwner,
    clipping: &ScenePuppetClippingProgram,
) -> Result<Vec<u8>, NativeVulkanSceneGpuUploadError> {
    let role = NativeVulkanSceneGpuBufferRole::PuppetClippingFrameKey;
    let mut payload = Vec::with_capacity(payload_capacity(
        owner,
        role,
        clipping.frame_keys.len(),
        SCENE_GPU_PUPPET_CLIPPING_FRAME_KEY_BYTES,
    )?);
    for frame_key in &clipping.frame_keys {
        push_u32(&mut payload, *frame_key);
    }
    Ok(payload)
}

fn puppet_active_source_payload(
    owner: NativeVulkanSceneGpuBufferOwner,
    clipping: &ScenePuppetClippingProgram,
) -> Result<Vec<u8>, NativeVulkanSceneGpuUploadError> {
    let role = NativeVulkanSceneGpuBufferRole::PuppetActiveSource;
    let mut payload = Vec::with_capacity(payload_capacity(
        owner,
        role,
        clipping.active_sources.len(),
        SCENE_GPU_PUPPET_ACTIVE_SOURCE_BYTES,
    )?);
    for (element, source) in clipping.active_sources.iter().enumerate() {
        push_u64_words(&mut payload, source.source_id);
        push_u64_words(&mut payload, scene_stable_name_hash(&source.source_name));
        push_u32(&mut payload, source.scalar_bits);
        push_u32(&mut payload, source.source_scale);
        push_u32(&mut payload, source.flags);
        push_u32(&mut payload, source.transform_index);
        push_f32(
            &mut payload,
            owner,
            role,
            "parameter0",
            element,
            f64::from(source.parameter0),
        )?;
        push_f32(
            &mut payload,
            owner,
            role,
            "parameter1",
            element,
            f64::from(source.parameter1),
        )?;
        for _ in 0..6 {
            push_u32(&mut payload, 0);
        }
    }
    Ok(payload)
}

fn push_layer_alpha_mask_rt_method8_mdlv_geometry_uploads(
    uploads: &mut Vec<NativeVulkanSceneGpuBufferUpload>,
    geometry: &SceneLayerAlphaMaskRtMethod8MdlvGeometry,
) -> Result<(), NativeVulkanSceneGpuUploadError> {
    let owner = NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
            object: geometry.object,
            entry_owner_index: geometry.entry_owner_index,
        },
    );
    push_upload(
        uploads,
        owner,
        NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex,
        geometry.vertex_count as usize,
        u64::from(geometry.vertex_stride_bytes),
        geometry.vertex_payload.clone(),
    )?;
    push_upload(
        uploads,
        owner,
        NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex,
        geometry.index_count as usize,
        SCENE_GPU_LAYER_ALPHA_MASK_RT_METHOD8_MDLV_INDEX_BYTES,
        geometry.index_payload.clone(),
    )
}

fn push_frame_render_state_utility_uploads(
    uploads: &mut Vec<NativeVulkanSceneGpuBufferUpload>,
    layer_compositor: &SceneLayerCompositorPlan,
) -> Result<(), NativeVulkanSceneGpuUploadError> {
    if !layer_compositor_uses_flattexture_copy_back(layer_compositor) {
        return Ok(());
    }

    let owner = NativeVulkanSceneGpuBufferOwner::RenderStateUtility(
        NativeVulkanSceneRenderStateUtilityGeometry::LayerAlphaMaskCopyBackState48,
    );
    let role = NativeVulkanSceneGpuBufferRole::RenderStateFlatTextureVertex;
    let payload =
        native_vulkan_scene_layer_alpha_mask_copy_back_fullscreen_triangle_payload(false).bytes;
    push_upload(
        uploads,
        owner,
        role,
        FLATTEXTURE_COPY_BACK_VERTEX_COUNT as usize,
        u64::from(FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES),
        payload.to_vec(),
    )
}

fn push_frame_layer_alpha_mask_rt_method8_mdlv_index_slice_uploads(
    uploads: &mut Vec<NativeVulkanSceneGpuBufferUpload>,
    resources: &[SceneResource],
    layer_compositor: &SceneLayerCompositorPlan,
) -> Result<(), NativeVulkanSceneGpuUploadError> {
    let tokenized_objects = layer_compositor
        .layers
        .iter()
        .filter(|layer| layer.uses_tokenized_subdraw)
        .map(|layer| layer.object)
        .collect::<BTreeSet<_>>();
    if tokenized_objects.is_empty() {
        return Ok(());
    }

    for resource in resources {
        let SceneResource::LayerAlphaMaskRtMethod8MdlvGeometry { geometry } = resource else {
            continue;
        };
        if tokenized_objects.contains(&geometry.object) {
            push_layer_alpha_mask_rt_method8_mdlv_index_slice_uploads(uploads, geometry)?;
        }
    }
    Ok(())
}

fn push_layer_alpha_mask_rt_method8_mdlv_index_slice_uploads(
    uploads: &mut Vec<NativeVulkanSceneGpuBufferUpload>,
    geometry: &SceneLayerAlphaMaskRtMethod8MdlvGeometry,
) -> Result<(), NativeVulkanSceneGpuUploadError> {
    if geometry.source_records.is_empty() || geometry.subdraws.is_empty() {
        return Ok(());
    }

    let aux_payload = native_vulkan_scene_layer_alpha_mask_rt_method8_lower_aux_payload(geometry)
        .map_err(
        |message| NativeVulkanSceneGpuUploadError::SemanticLowering {
            owner: NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(
                NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
                    object: geometry.object,
                    entry_owner_index: geometry.entry_owner_index,
                },
            ),
            role: NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvSliceIndex,
            message,
        },
    )?;

    for (subdraw_index, subdraw) in geometry.subdraws.iter().enumerate() {
        if !subdraw.first_indices.is_empty() {
            push_layer_alpha_mask_rt_method8_mdlv_index_slice_upload(
                uploads,
                geometry,
                &aux_payload,
                subdraw_index,
                NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind::FirstListAppendToken0,
                &subdraw.first_indices,
                true,
            )?;
        }
        if !subdraw.second_indices.is_empty() {
            push_layer_alpha_mask_rt_method8_mdlv_index_slice_upload(
                uploads,
                geometry,
                &aux_payload,
                subdraw_index,
                NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind::SecondListNoToken,
                &subdraw.second_indices,
                false,
            )?;
        }
    }
    Ok(())
}

fn push_layer_alpha_mask_rt_method8_mdlv_index_slice_upload(
    uploads: &mut Vec<NativeVulkanSceneGpuBufferUpload>,
    geometry: &SceneLayerAlphaMaskRtMethod8MdlvGeometry,
    aux_payload: &super::layer_alpha_mask_executor::NativeVulkanSceneLayerAlphaMaskRtMethod8AuxPayloadLoweringPlan,
    subdraw_index: usize,
    kind: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind,
    payload_indices: &[u32],
    appends_token_zero: bool,
) -> Result<(), NativeVulkanSceneGpuUploadError> {
    let slice = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice {
        object: geometry.object,
        entry_owner_index: geometry.entry_owner_index,
        subdraw_index: subdraw_index.min(u32::MAX as usize) as u32,
        kind,
    };
    let owner = NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvIndexSlice(slice);
    let role = NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvSliceIndex;
    let payload_indices = payload_indices
        .iter()
        .map(|index| *index as usize)
        .collect::<Vec<_>>();
    let slice_plan = native_vulkan_scene_layer_alpha_mask_rt_method8_materialize_index_slice(
        geometry,
        aux_payload,
        slice.subdraw_index,
        &payload_indices,
        appends_token_zero,
    )
    .map_err(
        |message| NativeVulkanSceneGpuUploadError::SemanticLowering {
            owner,
            role,
            message,
        },
    )?;
    push_upload(
        uploads,
        owner,
        role,
        slice_plan.index_count as usize,
        SCENE_GPU_LAYER_ALPHA_MASK_RT_METHOD8_MDLV_INDEX_BYTES,
        slice_plan.index_payload,
    )
}

fn layer_compositor_uses_flattexture_copy_back(
    layer_compositor: &SceneLayerCompositorPlan,
) -> bool {
    layer_compositor.layers.iter().any(|layer| {
        layer.uses_tokenized_subdraw
            && layer.commands.iter().any(|command| {
                command.entry == SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed
                    && command.operation
                        == SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
            })
    })
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

fn push_u64_words(payload: &mut Vec<u8>, value: u64) {
    push_u32(payload, value as u32);
    push_u32(payload, (value >> 32) as u32);
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
    use super::super::resource_storage::NativeVulkanSceneGpuBufferUsage;
    use super::*;
    use crate::core::scene::{
        SceneMeshPuppetClippingActiveSource, SceneMeshPuppetClippingRecord, SceneMeshSkin,
        ScenePuppetAnimationBone, ScenePuppetAnimationClip,
    };
    use crate::engine::scene_engine::{
        SceneGeometryId, SceneLayerAlphaMaskRtMethod8MdlvGeometry,
        SceneLayerAlphaMaskRtMethod8MdlvSourceRecord, SceneLayerAlphaMaskRtMethod8MdlvSubdraw,
        SceneLayerCompositorBlendKey, SceneLayerCompositorCommand, SceneLayerCompositorCondition,
        SceneLayerCompositorLayer, SceneLayerCompositorRoute, SceneLayerCompositorTarget,
        SceneMeshResidency, SceneObjectId, ScenePuppetClippingProgram, ScenePuppetId,
        ScenePuppetRigResidency, SceneResidentResource, SceneResourceResidencyPlan,
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
            clipping: Default::default(),
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
    fn upload_plan_packs_puppet_clipping_program_storage_records() {
        let clipping = ScenePuppetClippingProgram::from_source_records(
            vec![SceneMeshPuppetClippingRecord {
                source_name: Some("eye-right".to_owned()),
                mask: "masks/clipping_mask_eye".to_owned(),
                mask_resource: Some("assets/clipping-mask.gtex".to_owned()),
                duration_frames: 1680,
                flags: 3,
                bones: vec![42, 43],
                frame_keys: vec![0, 1, 2],
            }],
            vec![SceneMeshPuppetClippingActiveSource {
                source_name: "eye-right".to_owned(),
                source_id: 0x1122_3344_5566_7788,
                scalar_bits: 1.0f32.to_bits(),
                source_scale: 6,
                flags: 2,
                transform_index: 4,
                parameter0: -1.0,
                parameter1: 0.5,
            }],
        );
        let resources = vec![SceneResource::PuppetRig {
            id: ScenePuppetId(8),
            source_record: 3,
            skin: None,
            clips: Vec::new(),
            layers: Vec::new(),
            clipping,
        }];

        let plan = NativeVulkanSceneGpuUploadPlan::from_resources(&resources).unwrap();

        assert_eq!(plan.uploads().len(), 4);
        let record_upload = &plan.uploads()[0];
        assert_eq!(
            record_upload.requirement.role,
            NativeVulkanSceneGpuBufferRole::PuppetClippingRecord
        );
        assert_eq!(record_upload.requirement.bytes, 48);
        let expected_hash = scene_stable_name_hash("eye-right");
        assert_eq!(read_u32(&record_upload.payload, 0), expected_hash as u32);
        assert_eq!(
            read_u32(&record_upload.payload, 4),
            (expected_hash >> 32) as u32
        );
        assert_eq!(read_u32(&record_upload.payload, 8), 1680);
        assert_eq!(read_u32(&record_upload.payload, 12), 3);
        assert_eq!(read_u32(&record_upload.payload, 16), 0);
        assert_eq!(read_u32(&record_upload.payload, 20), 2);
        assert_eq!(read_u32(&record_upload.payload, 24), 0);
        assert_eq!(read_u32(&record_upload.payload, 28), 3);
        assert_eq!(read_u32(&record_upload.payload, 32), 0);
        assert_eq!(read_u32(&record_upload.payload, 36), SCENE_GPU_PARENT_NONE);

        let bone_upload = &plan.uploads()[1];
        assert_eq!(
            bone_upload.requirement.role,
            NativeVulkanSceneGpuBufferRole::PuppetClippingBoneIndex
        );
        assert_eq!(read_u32(&bone_upload.payload, 0), 42);
        assert_eq!(read_u32(&bone_upload.payload, 4), 43);

        let frame_key_upload = &plan.uploads()[2];
        assert_eq!(
            frame_key_upload.requirement.role,
            NativeVulkanSceneGpuBufferRole::PuppetClippingFrameKey
        );
        assert_eq!(read_u32(&frame_key_upload.payload, 0), 0);
        assert_eq!(read_u32(&frame_key_upload.payload, 4), 1);
        assert_eq!(read_u32(&frame_key_upload.payload, 8), 2);

        let active_source_upload = &plan.uploads()[3];
        assert_eq!(
            active_source_upload.requirement.role,
            NativeVulkanSceneGpuBufferRole::PuppetActiveSource
        );
        assert_eq!(active_source_upload.requirement.bytes, 64);
        assert_eq!(read_u32(&active_source_upload.payload, 0), 0x5566_7788);
        assert_eq!(read_u32(&active_source_upload.payload, 4), 0x1122_3344);
        assert_eq!(
            read_u32(&active_source_upload.payload, 8),
            expected_hash as u32
        );
        assert_eq!(
            read_u32(&active_source_upload.payload, 12),
            (expected_hash >> 32) as u32
        );
        assert_eq!(
            read_u32(&active_source_upload.payload, 16),
            1.0f32.to_bits()
        );
        assert_eq!(read_u32(&active_source_upload.payload, 20), 6);
        assert_eq!(read_u32(&active_source_upload.payload, 24), 2);
        assert_eq!(read_u32(&active_source_upload.payload, 28), 4);
        assert_eq!(read_f32(&active_source_upload.payload, 32), -1.0);
        assert_eq!(read_f32(&active_source_upload.payload, 36), 0.5);
    }

    #[test]
    fn upload_plan_preserves_layer_alpha_mask_rt_method8_mdlv_raw_payloads() {
        let vertex_payload = (0..80).map(|value| value as u8).collect::<Vec<_>>();
        let index_payload = vec![0, 0, 2, 0, 1, 0, 1, 0, 2, 0, 3, 0];
        let resources = vec![SceneResource::LayerAlphaMaskRtMethod8MdlvGeometry {
            geometry: SceneLayerAlphaMaskRtMethod8MdlvGeometry {
                object: SceneObjectId(1530),
                entry_owner_index: 0,
                layout_key: 0x9,
                vertex_stride_bytes: 20,
                vertex_count: 4,
                index_count: 6,
                vertex_payload: vertex_payload.clone(),
                index_payload: index_payload.clone(),
                source_records: Vec::new(),
                subdraws: Vec::new(),
            },
        }];

        let plan = NativeVulkanSceneGpuUploadPlan::from_resources(&resources).unwrap();

        assert_eq!(plan.uploads().len(), 2);
        assert_eq!(
            plan.uploads()[0].requirement,
            NativeVulkanSceneGpuBufferRequirement {
                owner: NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(
                    NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
                        object: SceneObjectId(1530),
                        entry_owner_index: 0,
                    },
                ),
                role: NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex,
                bytes: 80,
                usage: NativeVulkanSceneGpuBufferUsage::Vertex,
            }
        );
        assert_eq!(plan.uploads()[0].payload, vertex_payload);
        assert_eq!(
            plan.uploads()[1].requirement.role,
            NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex
        );
        assert_eq!(plan.uploads()[1].requirement.bytes, 12);
        assert_eq!(plan.uploads()[1].payload, index_payload);
    }

    #[test]
    fn frame_upload_plan_materializes_rt_method8_mdlv_index_slices() {
        let geometry = SceneLayerAlphaMaskRtMethod8MdlvGeometry {
            object: SceneObjectId(1530),
            entry_owner_index: 0,
            layout_key: 0x9,
            vertex_stride_bytes: 20,
            vertex_count: 1,
            index_count: 8,
            vertex_payload: vec![0; 20],
            index_payload: vec![0, 0, 2, 0, 1, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0],
            source_records: vec![
                SceneLayerAlphaMaskRtMethod8MdlvSourceRecord {
                    source_index: 0,
                    local_offset: 0,
                    index_span_offset: 1,
                    index_span_count: 2,
                },
                SceneLayerAlphaMaskRtMethod8MdlvSourceRecord {
                    source_index: 1,
                    local_offset: 0,
                    index_span_offset: 4,
                    index_span_count: 2,
                },
                SceneLayerAlphaMaskRtMethod8MdlvSourceRecord {
                    source_index: 2,
                    local_offset: 0,
                    index_span_offset: 6,
                    index_span_count: 1,
                },
            ],
            subdraws: vec![SceneLayerAlphaMaskRtMethod8MdlvSubdraw {
                source_qword: 0x690,
                mask_resource: "masks/clipping_mask".to_owned(),
                raw_flags: 0,
                first_indices: vec![0, 1],
                second_indices: vec![2],
                link: u32::MAX,
            }],
        };
        let resources = vec![SceneResource::LayerAlphaMaskRtMethod8MdlvGeometry { geometry }];
        let frame = SceneFramePlan {
            residency: Default::default(),
            graph: crate::engine::scene_engine::SceneGraph { passes: Vec::new() },
            effect_pass_graph: crate::engine::scene_engine::SceneEffectPassGraphPlan::empty(),
            effect_uniforms: crate::engine::scene_engine::SceneEffectUniformFramePlan::empty(),
            final_compositor: crate::engine::scene_engine::SceneFinalCompositorPlan::empty(),
            layer_compositor: SceneLayerCompositorPlan {
                layer_count: 1,
                command_count: 1,
                object_final_layer_count: 0,
                tokenized_layer_count: 1,
                layers: vec![SceneLayerCompositorLayer {
                    object: SceneObjectId(1530),
                    route: SceneLayerCompositorRoute::DirectSwapchain,
                    uses_tokenized_subdraw: true,
                    has_active_aux_clear_target: false,
                    commands: Vec::new(),
                }],
                command_order: SceneLayerCompositorPlan::empty().command_order,
            },
        };

        let plan = NativeVulkanSceneGpuUploadPlan::from_scene_frame(
            &NativeVulkanSceneResourceStorage::default(),
            &resources,
            &frame,
        )
        .unwrap();

        assert_eq!(plan.uploads().len(), 2);
        let first = &plan.uploads()[0];
        assert_eq!(
            first.requirement.owner,
            NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvIndexSlice(
                NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice {
                    object: SceneObjectId(1530),
                    entry_owner_index: 0,
                    subdraw_index: 0,
                    kind: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind::FirstListAppendToken0,
                },
            )
        );
        assert_eq!(
            first.requirement.role,
            NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvSliceIndex
        );
        assert_eq!(
            first.requirement.usage,
            NativeVulkanSceneGpuBufferUsage::Index
        );
        assert_eq!(first.payload, vec![2, 0, 1, 0, 4, 0, 5, 0]);

        let second = &plan.uploads()[1];
        assert_eq!(
            second.requirement.owner,
            NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvIndexSlice(
                NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSlice {
                    object: SceneObjectId(1530),
                    entry_owner_index: 0,
                    subdraw_index: 0,
                    kind: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvIndexSliceKind::SecondListNoToken,
                },
            )
        );
        assert_eq!(second.payload, vec![6, 0]);
    }

    #[test]
    fn upload_plan_rejects_layer_alpha_mask_rt_method8_mdlv_size_mismatch() {
        let resources = vec![SceneResource::LayerAlphaMaskRtMethod8MdlvGeometry {
            geometry: SceneLayerAlphaMaskRtMethod8MdlvGeometry {
                object: SceneObjectId(1530),
                entry_owner_index: 0,
                layout_key: 0x9,
                vertex_stride_bytes: 20,
                vertex_count: 4,
                index_count: 6,
                vertex_payload: vec![1; 79],
                index_payload: vec![2; 12],
                source_records: Vec::new(),
                subdraws: Vec::new(),
            },
        }];

        let err = NativeVulkanSceneGpuUploadPlan::from_resources(&resources)
            .expect_err("raw MDLV vertex bytes must match entry stride/count");

        assert!(matches!(
            err,
            NativeVulkanSceneGpuUploadError::UploadSizeMismatch {
                requirement: NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(
                        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
                            object: SceneObjectId(1530),
                            entry_owner_index: 0,
                        }
                    ),
                    role: NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex,
                    bytes: 80,
                    ..
                },
                actual: 79,
            }
        ));
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
            NativeVulkanSceneGpuUploadPlan::from_resident_resources_for_test(&storage, &resources)
                .unwrap();

        assert_eq!(plan.uploads().len(), 2);
    }

    #[test]
    fn frame_upload_plan_includes_closed_state48_copy_back_utility_geometry() {
        let storage = NativeVulkanSceneResourceStorage::default();
        let frame = copy_back_frame();

        let plan = NativeVulkanSceneGpuUploadPlan::from_scene_frame(&storage, &[], &frame)
            .expect("frame graph derived upload plan");

        assert_eq!(plan.uploads().len(), 1);
        let upload = &plan.uploads()[0];
        assert_eq!(
            upload.requirement,
            NativeVulkanSceneGpuBufferRequirement {
                owner: NativeVulkanSceneGpuBufferOwner::RenderStateUtility(
                    NativeVulkanSceneRenderStateUtilityGeometry::LayerAlphaMaskCopyBackState48,
                ),
                role: NativeVulkanSceneGpuBufferRole::RenderStateFlatTextureVertex,
                bytes: u64::from(FLATTEXTURE_COPY_BACK_VERTEX_COUNT)
                    * u64::from(FLATTEXTURE_COPY_BACK_VERTEX_STRIDE_BYTES),
                usage: NativeVulkanSceneGpuBufferUsage::Vertex,
            }
        );
        assert_eq!(
            upload.payload,
            native_vulkan_scene_layer_alpha_mask_copy_back_fullscreen_triangle_payload(false)
                .bytes
                .to_vec()
        );
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
                clipping_record_bytes: 0,
                clipping_bone_count: 0,
                clipping_bone_bytes: 0,
                clipping_frame_key_count: 0,
                clipping_frame_key_bytes: 0,
                active_source_count: 0,
                active_source_bytes: 0,
            })],
        });

        let err = NativeVulkanSceneGpuUploadPlan::from_resident_resources_for_test(&storage, &[])
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
            clipping: Default::default(),
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

    fn copy_back_frame() -> SceneFramePlan {
        let mut layer_compositor = SceneLayerCompositorPlan::empty();
        layer_compositor.layer_count = 1;
        layer_compositor.command_count = 1;
        layer_compositor.tokenized_layer_count = 1;
        layer_compositor.layers = vec![SceneLayerCompositorLayer {
            object: SceneObjectId(7),
            route: SceneLayerCompositorRoute::ObjectFinalMeshComposite,
            uses_tokenized_subdraw: true,
            has_active_aux_clear_target: false,
            commands: vec![SceneLayerCompositorCommand {
                entry: SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed,
                operation: SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                condition: SceneLayerCompositorCondition::Token2AfterIntermediateMask,
                source: Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
                target: SceneLayerCompositorTarget::FullAlphaMask,
                blend_key: SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
            }],
        }];
        SceneFramePlan {
            residency: SceneResourceResidencyPlan::default(),
            graph: crate::engine::scene_engine::SceneGraph { passes: Vec::new() },
            effect_pass_graph: crate::engine::scene_engine::SceneEffectPassGraphPlan::empty(),
            effect_uniforms: crate::engine::scene_engine::SceneEffectUniformFramePlan::empty(),
            final_compositor: crate::engine::scene_engine::SceneFinalCompositorPlan::empty(),
            layer_compositor,
        }
    }
}
