//! Retained material uniform upload planning for WE scene shaders.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `reverse-engineered/shaders/genericimage4.frag`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`

mod gpu_buffer;

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use serde::Serialize;

use crate::engine::scene_engine::{
    SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES, SceneGenericImage4MaterialUniformRecord,
    SceneObjectId, SceneShaderUniformFramePlan, WE_VEC4_BYTES, WeVec4,
};

pub(in crate::renderer::native_vulkan) use gpu_buffer::{
    NativeVulkanSceneMaterialUniformGpuBufferBinding,
    NativeVulkanSceneMaterialUniformGpuBufferStore,
    NativeVulkanSceneMaterialUniformGpuBufferSyncAction,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMaterialUniformKey {
    pub object: SceneObjectId,
    pub shader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMaterialUniformRecord {
    pub key: NativeVulkanSceneMaterialUniformKey,
    pub record_index: usize,
    pub bytes: u64,
    pub payload_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMaterialUniformUpload {
    pub key: NativeVulkanSceneMaterialUniformKey,
    pub record_index: usize,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMaterialUniformUploadPlan {
    pub record_count: usize,
    pub total_bytes: u64,
    pub record_bytes: u64,
    #[serde(skip)]
    uploads: Vec<NativeVulkanSceneMaterialUniformUpload>,
    pub command_order: [&'static str; 3],
}

impl NativeVulkanSceneMaterialUniformUploadPlan {
    pub(in crate::renderer::native_vulkan) fn from_shader_uniform_frame_plan(
        plan: &SceneShaderUniformFramePlan,
    ) -> Result<Self, NativeVulkanSceneMaterialUniformError> {
        let mut uploads_by_key = BTreeMap::<
            NativeVulkanSceneMaterialUniformKey,
            NativeVulkanSceneMaterialUniformUpload,
        >::new();
        for record in &plan.genericimage4_material_records {
            let upload = genericimage4_material_upload(record)?;
            match uploads_by_key.get(&upload.key) {
                Some(existing) if existing.payload == upload.payload => {}
                Some(_) => {
                    return Err(NativeVulkanSceneMaterialUniformError::DuplicateUploadKey {
                        key: upload.key,
                    });
                }
                None => {
                    uploads_by_key.insert(upload.key.clone(), upload);
                }
            }
        }
        let uploads = uploads_by_key.into_values().collect::<Vec<_>>();
        Ok(Self {
            record_count: uploads.len(),
            total_bytes: u64::try_from(uploads.len())
                .unwrap_or(u64::MAX)
                .saturating_mul(SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES),
            record_bytes: SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES,
            uploads,
            command_order: [
                "pack_genericimage4_material_uniform_records",
                "diff_retained_material_uniform_records",
                "prepare_material_uniform_gpu_upload",
            ],
        })
    }

    pub(in crate::renderer::native_vulkan) fn uploads(
        &self,
    ) -> &[NativeVulkanSceneMaterialUniformUpload] {
        &self.uploads
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneMaterialUniformSyncAction {
    Create {
        record: NativeVulkanSceneMaterialUniformRecord,
    },
    Reuse {
        record: NativeVulkanSceneMaterialUniformRecord,
    },
    Replace {
        old: NativeVulkanSceneMaterialUniformRecord,
        new: NativeVulkanSceneMaterialUniformRecord,
    },
    Release {
        record: NativeVulkanSceneMaterialUniformRecord,
    },
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMaterialUniformCatalog {
    records: BTreeMap<NativeVulkanSceneMaterialUniformKey, NativeVulkanSceneMaterialUniformRecord>,
    last_actions: Vec<NativeVulkanSceneMaterialUniformSyncAction>,
}

impl NativeVulkanSceneMaterialUniformCatalog {
    pub(in crate::renderer::native_vulkan) fn sync_upload_plan(
        &mut self,
        upload_plan: &NativeVulkanSceneMaterialUniformUploadPlan,
    ) -> Result<&[NativeVulkanSceneMaterialUniformSyncAction], NativeVulkanSceneMaterialUniformError>
    {
        let upload_records = upload_records(upload_plan.uploads())?;
        self.last_actions.clear();

        let active_keys = upload_records.keys().cloned().collect::<BTreeSet<_>>();
        let stale_keys = self
            .records
            .keys()
            .filter(|key| !active_keys.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in stale_keys {
            if let Some(record) = self.records.remove(&key) {
                self.last_actions
                    .push(NativeVulkanSceneMaterialUniformSyncAction::Release { record });
            }
        }

        for (key, new_record) in upload_records {
            match self.records.get(&key).cloned() {
                Some(old_record) if old_record == new_record => {
                    self.last_actions
                        .push(NativeVulkanSceneMaterialUniformSyncAction::Reuse {
                            record: old_record,
                        });
                }
                Some(old_record) => {
                    self.records.insert(key, new_record.clone());
                    self.last_actions
                        .push(NativeVulkanSceneMaterialUniformSyncAction::Replace {
                            old: old_record,
                            new: new_record,
                        });
                }
                None => {
                    self.records.insert(key, new_record.clone());
                    self.last_actions
                        .push(NativeVulkanSceneMaterialUniformSyncAction::Create {
                            record: new_record,
                        });
                }
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn records(
        &self,
    ) -> &BTreeMap<NativeVulkanSceneMaterialUniformKey, NativeVulkanSceneMaterialUniformRecord>
    {
        &self.records
    }

    pub(in crate::renderer::native_vulkan) fn last_actions(
        &self,
    ) -> &[NativeVulkanSceneMaterialUniformSyncAction] {
        &self.last_actions
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneMaterialUniformError {
    NonFiniteFloat {
        object: SceneObjectId,
        shader: String,
        field: &'static str,
        element: usize,
    },
    UploadSizeMismatch {
        key: NativeVulkanSceneMaterialUniformKey,
        expected_bytes: u64,
        actual_bytes: u64,
    },
    DuplicateUploadKey {
        key: NativeVulkanSceneMaterialUniformKey,
    },
}

impl fmt::Display for NativeVulkanSceneMaterialUniformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteFloat {
                object,
                shader,
                field,
                element,
            } => write!(
                f,
                "non-finite scene material uniform float for {object:?} shader '{shader}' {field}[{element}]"
            ),
            Self::UploadSizeMismatch {
                key,
                expected_bytes,
                actual_bytes,
            } => write!(
                f,
                "scene material uniform upload for {key:?} has {actual_bytes} bytes, expected {expected_bytes}"
            ),
            Self::DuplicateUploadKey { key } => {
                write!(f, "duplicate scene material uniform upload key {key:?}")
            }
        }
    }
}

impl Error for NativeVulkanSceneMaterialUniformError {}

fn genericimage4_material_upload(
    record: &SceneGenericImage4MaterialUniformRecord,
) -> Result<NativeVulkanSceneMaterialUniformUpload, NativeVulkanSceneMaterialUniformError> {
    let mut payload = Vec::with_capacity(SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES as usize);
    push_we_vec4(&mut payload, record, "g_Color4", record.color4)?;
    push_we_vec4(
        &mut payload,
        record,
        "g_RoughnessMetallic",
        WeVec4::from_lanes([record.roughness, record.metallic, 0.0, 0.0]),
    )?;
    push_we_vec4(
        &mut payload,
        record,
        "g_SpecularTint",
        WeVec4::from_lanes([
            record.specular_tint[0],
            record.specular_tint[1],
            record.specular_tint[2],
            0.0,
        ]),
    )?;
    let key = NativeVulkanSceneMaterialUniformKey {
        object: record.object,
        shader: record.shader.clone(),
    };
    let actual_bytes = u64::try_from(payload.len()).unwrap_or(u64::MAX);
    if actual_bytes != SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES {
        return Err(NativeVulkanSceneMaterialUniformError::UploadSizeMismatch {
            key,
            expected_bytes: SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES,
            actual_bytes,
        });
    }
    Ok(NativeVulkanSceneMaterialUniformUpload {
        key,
        record_index: record.record_index,
        payload,
    })
}

fn push_we_vec4(
    payload: &mut Vec<u8>,
    record: &SceneGenericImage4MaterialUniformRecord,
    field: &'static str,
    value: WeVec4,
) -> Result<(), NativeVulkanSceneMaterialUniformError> {
    if let Some(element) = value.first_non_finite_lane() {
        return Err(NativeVulkanSceneMaterialUniformError::NonFiniteFloat {
            object: record.object,
            shader: record.shader.clone(),
            field,
            element,
        });
    }
    let before = payload.len();
    value.write_le_bytes(payload);
    let written = payload.len().saturating_sub(before);
    debug_assert_eq!(written, WE_VEC4_BYTES as usize);
    Ok(())
}

#[cfg(test)]
fn we_vec4_bytes(value: WeVec4) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(WE_VEC4_BYTES as usize);
    value.write_le_bytes(&mut bytes);
    bytes
}

fn upload_records(
    uploads: &[NativeVulkanSceneMaterialUniformUpload],
) -> Result<
    BTreeMap<NativeVulkanSceneMaterialUniformKey, NativeVulkanSceneMaterialUniformRecord>,
    NativeVulkanSceneMaterialUniformError,
> {
    let mut records = BTreeMap::new();
    for upload in uploads {
        let actual_bytes = u64::try_from(upload.payload.len()).unwrap_or(u64::MAX);
        if actual_bytes != SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES {
            return Err(NativeVulkanSceneMaterialUniformError::UploadSizeMismatch {
                key: upload.key.clone(),
                expected_bytes: SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES,
                actual_bytes,
            });
        }
        let record = NativeVulkanSceneMaterialUniformRecord {
            key: upload.key.clone(),
            record_index: upload.record_index,
            bytes: actual_bytes,
            payload_hash: scene_stable_byte_hash(&upload.payload),
        };
        if records.insert(upload.key.clone(), record).is_some() {
            return Err(NativeVulkanSceneMaterialUniformError::DuplicateUploadKey {
                key: upload.key.clone(),
            });
        }
    }
    Ok(records)
}

fn scene_stable_byte_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraph, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphResourceRole,
        SceneGraphTarget, SceneMaterialKey, SceneMaterialRenderState, SceneResourceId,
    };

    #[test]
    fn material_uniform_plan_packs_genericimage4_defaults() {
        let frame_plan = SceneShaderUniformFramePlan::from_graph(&graph()).unwrap();

        let upload_plan =
            NativeVulkanSceneMaterialUniformUploadPlan::from_shader_uniform_frame_plan(&frame_plan)
                .unwrap();

        assert_eq!(upload_plan.record_count, 1);
        assert_eq!(upload_plan.total_bytes, 48);
        let upload = &upload_plan.uploads()[0];
        assert_eq!(upload.payload.len(), 48);
        assert_eq!(
            &upload.payload[0..16],
            we_vec4_bytes(WeVec4::from_lanes([1.0, 1.0, 1.0, 1.0])).as_slice()
        );
        assert_eq!(
            &upload.payload[16..32],
            we_vec4_bytes(WeVec4::from_lanes([0.7, 0.0, 0.0, 0.0])).as_slice()
        );
        assert_eq!(
            &upload.payload[32..48],
            we_vec4_bytes(WeVec4::from_lanes([1.0, 1.0, 1.0, 0.0])).as_slice()
        );
    }

    #[test]
    fn material_uniform_catalog_reuses_and_replaces_records() {
        let frame_plan = SceneShaderUniformFramePlan::from_graph(&graph()).unwrap();
        let first_plan =
            NativeVulkanSceneMaterialUniformUploadPlan::from_shader_uniform_frame_plan(&frame_plan)
                .unwrap();
        let mut catalog = NativeVulkanSceneMaterialUniformCatalog::default();

        let first = catalog.sync_upload_plan(&first_plan).unwrap().to_vec();
        let second = catalog.sync_upload_plan(&first_plan).unwrap().to_vec();

        assert!(matches!(
            first.as_slice(),
            [NativeVulkanSceneMaterialUniformSyncAction::Create { .. }]
        ));
        assert!(matches!(
            second.as_slice(),
            [NativeVulkanSceneMaterialUniformSyncAction::Reuse { .. }]
        ));

        let mut changed_frame = frame_plan.clone();
        changed_frame.genericimage4_material_records[0].roughness = 0.25;
        let changed_plan =
            NativeVulkanSceneMaterialUniformUploadPlan::from_shader_uniform_frame_plan(
                &changed_frame,
            )
            .unwrap();
        let third = catalog.sync_upload_plan(&changed_plan).unwrap().to_vec();

        assert!(matches!(
            third.as_slice(),
            [NativeVulkanSceneMaterialUniformSyncAction::Replace { .. }]
        ));
        assert_eq!(catalog.records().len(), 1);
    }

    #[test]
    fn material_uniform_plan_dedupes_identical_object_shader_records() {
        let mut graph = graph();
        let duplicate = graph.passes[0].draws[0].clone();
        graph.passes[0].draws.push(duplicate);
        let frame_plan = SceneShaderUniformFramePlan::from_graph(&graph).unwrap();

        let upload_plan =
            NativeVulkanSceneMaterialUniformUploadPlan::from_shader_uniform_frame_plan(&frame_plan)
                .unwrap();

        assert_eq!(frame_plan.genericimage4_material_record_count, 2);
        assert_eq!(upload_plan.record_count, 1);
        assert_eq!(upload_plan.uploads().len(), 1);
    }

    #[test]
    fn material_uniform_upload_rejects_non_finite_values() {
        let mut frame_plan = SceneShaderUniformFramePlan::from_graph(&graph()).unwrap();
        frame_plan.genericimage4_material_records[0].metallic = f32::NAN;

        let err =
            NativeVulkanSceneMaterialUniformUploadPlan::from_shader_uniform_frame_plan(&frame_plan)
                .expect_err("non-finite material uniform must fail");

        assert!(matches!(
            err,
            NativeVulkanSceneMaterialUniformError::NonFiniteFloat {
                field: "g_RoughnessMetallic",
                ..
            }
        ));
    }

    #[test]
    fn material_uniform_upload_rejects_non_finite_we_vec4_lane() {
        let mut frame_plan = SceneShaderUniformFramePlan::from_graph(&graph()).unwrap();
        frame_plan.genericimage4_material_records[0].color4 =
            WeVec4::from_lanes([1.0, f32::INFINITY, 1.0, 1.0]);

        let err =
            NativeVulkanSceneMaterialUniformUploadPlan::from_shader_uniform_frame_plan(&frame_plan)
                .expect_err("non-finite WE vec4 lane must fail");

        assert!(matches!(
            err,
            NativeVulkanSceneMaterialUniformError::NonFiniteFloat {
                field: "g_Color4",
                element: 1,
                ..
            }
        ));
    }

    fn graph() -> SceneGraph {
        SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws: vec![SceneGraphDraw {
                    object: SceneObjectId(7),
                    pipeline: SceneGraphPipelineClass::Mesh,
                    material: SceneMaterialKey {
                        shader: "we/genericimage4".to_owned(),
                        blend: SceneBlendContract::TranslucentAlpha,
                        render_state: SceneMaterialRenderState::translucent_2d(),
                    },
                    geometry: Some(SceneGeometryId(7)),
                    puppet: None,
                    resources: vec![SceneGraphResourceBinding {
                        slot: 0,
                        role: SceneGraphResourceRole::shader_texture(0),
                        resource: SceneResourceId(3),
                    }],
                    index_count: 6,
                }],
            }],
        }
    }
}
