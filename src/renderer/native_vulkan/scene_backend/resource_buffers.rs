//! Retained GPU buffer ownership for native Vulkan scene resources.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneGeometryId, ScenePuppetId};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaBuffer, NativeVulkanVulkanaliaRecordedBufferUpload,
    native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload,
    native_vulkan_vulkanalia_destroy_buffer,
};

use super::resource_storage::{
    NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRequirement,
    NativeVulkanSceneGpuBufferRole, NativeVulkanSceneGpuBufferUsage,
};
use super::resource_upload::{NativeVulkanSceneGpuBufferUpload, NativeVulkanSceneGpuUploadPlan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct NativeVulkanSceneGpuBufferKey {
    pub owner: NativeVulkanSceneGpuBufferOwner,
    pub role: NativeVulkanSceneGpuBufferRole,
}

impl From<NativeVulkanSceneGpuBufferRequirement> for NativeVulkanSceneGpuBufferKey {
    fn from(requirement: NativeVulkanSceneGpuBufferRequirement) -> Self {
        Self {
            owner: requirement.owner,
            role: requirement.role,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneGpuBufferRecord {
    pub key: NativeVulkanSceneGpuBufferKey,
    pub requirement: NativeVulkanSceneGpuBufferRequirement,
    pub payload_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneGpuBufferRecordBinding {
    pub key: NativeVulkanSceneGpuBufferKey,
    pub bytes: u64,
    pub payload_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneMeshDrawBufferRecords {
    pub geometry: SceneGeometryId,
    pub vertex: NativeVulkanSceneGpuBufferRecordBinding,
    pub index: NativeVulkanSceneGpuBufferRecordBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanScenePuppetStorageBufferRecords {
    pub puppet: ScenePuppetId,
    pub bones: Option<NativeVulkanSceneGpuBufferRecordBinding>,
    pub skin_vertices: Option<NativeVulkanSceneGpuBufferRecordBinding>,
    pub clip_frames: Option<NativeVulkanSceneGpuBufferRecordBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanSceneGpuBufferBinding {
    pub key: NativeVulkanSceneGpuBufferKey,
    pub buffer: vk::Buffer,
    pub bytes: u64,
    pub payload_hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanSceneMeshDrawBuffers {
    pub geometry: SceneGeometryId,
    pub vertex: NativeVulkanSceneGpuBufferBinding,
    pub index: NativeVulkanSceneGpuBufferBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanScenePuppetStorageBuffers {
    pub puppet: ScenePuppetId,
    pub bones: Option<NativeVulkanSceneGpuBufferBinding>,
    pub skin_vertices: Option<NativeVulkanSceneGpuBufferBinding>,
    pub clip_frames: Option<NativeVulkanSceneGpuBufferBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NativeVulkanSceneGpuBufferSyncAction {
    Create {
        record: NativeVulkanSceneGpuBufferRecord,
    },
    Reuse {
        record: NativeVulkanSceneGpuBufferRecord,
    },
    Replace {
        old: NativeVulkanSceneGpuBufferRecord,
        new: NativeVulkanSceneGpuBufferRecord,
    },
    Release {
        record: NativeVulkanSceneGpuBufferRecord,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeVulkanSceneGpuBufferSyncError {
    DuplicateUploadKey {
        key: NativeVulkanSceneGpuBufferKey,
    },
    UploadSizeMismatch {
        requirement: NativeVulkanSceneGpuBufferRequirement,
        payload_bytes: u64,
    },
}

impl fmt::Display for NativeVulkanSceneGpuBufferSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateUploadKey { key } => {
                write!(f, "duplicate scene GPU upload key {key:?}")
            }
            Self::UploadSizeMismatch {
                requirement,
                payload_bytes,
            } => write!(
                f,
                "scene GPU upload payload size {payload_bytes} does not match {requirement:?}"
            ),
        }
    }
}

impl Error for NativeVulkanSceneGpuBufferSyncError {}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneGpuBufferCatalog {
    records: BTreeMap<NativeVulkanSceneGpuBufferKey, NativeVulkanSceneGpuBufferRecord>,
    last_actions: Vec<NativeVulkanSceneGpuBufferSyncAction>,
}

impl NativeVulkanSceneGpuBufferCatalog {
    pub fn sync_upload_plan(
        &mut self,
        upload_plan: &NativeVulkanSceneGpuUploadPlan,
    ) -> Result<&[NativeVulkanSceneGpuBufferSyncAction], NativeVulkanSceneGpuBufferSyncError> {
        let upload_records = upload_records(upload_plan.uploads())?;
        self.last_actions.clear();

        let active_keys = upload_records.keys().copied().collect::<BTreeSet<_>>();
        let stale_keys = self
            .records
            .keys()
            .copied()
            .filter(|key| !active_keys.contains(key))
            .collect::<Vec<_>>();
        for key in stale_keys {
            if let Some(record) = self.records.remove(&key) {
                self.last_actions
                    .push(NativeVulkanSceneGpuBufferSyncAction::Release { record });
            }
        }

        for (key, new_record) in upload_records {
            match self.records.get(&key).cloned() {
                Some(old_record) if old_record == new_record => {
                    self.last_actions
                        .push(NativeVulkanSceneGpuBufferSyncAction::Reuse { record: old_record });
                }
                Some(old_record) => {
                    self.records.insert(key, new_record.clone());
                    self.last_actions
                        .push(NativeVulkanSceneGpuBufferSyncAction::Replace {
                            old: old_record,
                            new: new_record,
                        });
                }
                None => {
                    self.records.insert(key, new_record.clone());
                    self.last_actions
                        .push(NativeVulkanSceneGpuBufferSyncAction::Create { record: new_record });
                }
            }
        }

        Ok(&self.last_actions)
    }

    pub fn records(
        &self,
    ) -> &BTreeMap<NativeVulkanSceneGpuBufferKey, NativeVulkanSceneGpuBufferRecord> {
        &self.records
    }

    pub fn last_actions(&self) -> &[NativeVulkanSceneGpuBufferSyncAction] {
        &self.last_actions
    }

    pub fn mesh_draw_buffer_records(
        &self,
        geometry: SceneGeometryId,
    ) -> Result<NativeVulkanSceneMeshDrawBufferRecords, String> {
        Ok(NativeVulkanSceneMeshDrawBufferRecords {
            geometry,
            vertex: self.required_record_binding(
                NativeVulkanSceneGpuBufferOwner::MeshGeometry(geometry),
                NativeVulkanSceneGpuBufferRole::MeshVertex,
            )?,
            index: self.required_record_binding(
                NativeVulkanSceneGpuBufferOwner::MeshGeometry(geometry),
                NativeVulkanSceneGpuBufferRole::MeshIndex,
            )?,
        })
    }

    pub fn puppet_storage_buffer_records(
        &self,
        puppet: ScenePuppetId,
    ) -> NativeVulkanScenePuppetStorageBufferRecords {
        NativeVulkanScenePuppetStorageBufferRecords {
            puppet,
            bones: self.record_binding(
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet),
                NativeVulkanSceneGpuBufferRole::PuppetBone,
            ),
            skin_vertices: self.record_binding(
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet),
                NativeVulkanSceneGpuBufferRole::PuppetSkinVertex,
            ),
            clip_frames: self.record_binding(
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet),
                NativeVulkanSceneGpuBufferRole::PuppetClipFrame,
            ),
        }
    }

    fn required_record_binding(
        &self,
        owner: NativeVulkanSceneGpuBufferOwner,
        role: NativeVulkanSceneGpuBufferRole,
    ) -> Result<NativeVulkanSceneGpuBufferRecordBinding, String> {
        self.record_binding(owner, role).ok_or_else(|| {
            format!("missing retained scene GPU buffer record for {owner:?} {role:?}")
        })
    }

    fn record_binding(
        &self,
        owner: NativeVulkanSceneGpuBufferOwner,
        role: NativeVulkanSceneGpuBufferRole,
    ) -> Option<NativeVulkanSceneGpuBufferRecordBinding> {
        self.records
            .get(&NativeVulkanSceneGpuBufferKey { owner, role })
            .map(record_binding)
    }
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneGpuBufferStore {
    buffers: BTreeMap<NativeVulkanSceneGpuBufferKey, NativeVulkanSceneGpuBufferSlot>,
    pending_retirements: Vec<NativeVulkanVulkanaliaBuffer>,
    last_actions: Vec<NativeVulkanSceneGpuBufferSyncAction>,
}

impl NativeVulkanSceneGpuBufferStore {
    pub(in crate::renderer::native_vulkan) fn new() -> Self {
        Self {
            buffers: BTreeMap::new(),
            pending_retirements: Vec::new(),
            last_actions: Vec::new(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn sync_upload_plan_recorded(
        &mut self,
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        command_buffer: vk::CommandBuffer,
        upload_plan: NativeVulkanSceneGpuUploadPlan,
    ) -> Result<&[NativeVulkanSceneGpuBufferSyncAction], String> {
        let uploads = upload_map(upload_plan.into_uploads()).map_err(|err| err.to_string())?;
        self.last_actions.clear();

        let active_keys = uploads.keys().copied().collect::<BTreeSet<_>>();
        let stale_keys = self
            .buffers
            .keys()
            .copied()
            .filter(|key| !active_keys.contains(key))
            .collect::<Vec<_>>();
        for key in stale_keys {
            if let Some(slot) = self.buffers.remove(&key) {
                self.defer_retirement(slot.buffer);
                self.last_actions
                    .push(NativeVulkanSceneGpuBufferSyncAction::Release {
                        record: slot.record,
                    });
            }
        }

        for (key, upload) in uploads {
            let new_record = upload_record(&upload).map_err(|err| err.to_string())?;
            if let Some(old_slot) = self.buffers.get(&key)
                && old_slot.record == new_record
            {
                self.last_actions
                    .push(NativeVulkanSceneGpuBufferSyncAction::Reuse {
                        record: old_slot.record.clone(),
                    });
                continue;
            }

            let recorded_upload =
                native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload(
                    device,
                    memory_properties,
                    command_buffer,
                    scene_gpu_buffer_role_name(new_record.requirement),
                    new_record.requirement.bytes,
                    scene_gpu_buffer_usage_flags(new_record.requirement.usage),
                    &upload.payload,
                )?;
            let new_buffer = self.finish_recorded_upload(recorded_upload);

            match self.buffers.insert(
                key,
                NativeVulkanSceneGpuBufferSlot {
                    record: new_record.clone(),
                    buffer: new_buffer,
                },
            ) {
                Some(old_slot) => {
                    self.defer_retirement(old_slot.buffer);
                    self.last_actions
                        .push(NativeVulkanSceneGpuBufferSyncAction::Replace {
                            old: old_slot.record,
                            new: new_record,
                        });
                }
                None => {
                    self.last_actions
                        .push(NativeVulkanSceneGpuBufferSyncAction::Create { record: new_record });
                }
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn release_completed_uploads(
        &mut self,
        device: &Device,
    ) -> usize {
        let retired_count = self.pending_retirements.len();
        for buffer in std::mem::take(&mut self.pending_retirements) {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        retired_count
    }

    pub(in crate::renderer::native_vulkan) fn destroy_all(&mut self, device: &Device) {
        for (_, slot) in std::mem::take(&mut self.buffers) {
            native_vulkan_vulkanalia_destroy_buffer(device, slot.buffer);
        }
        for buffer in std::mem::take(&mut self.pending_retirements) {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        self.last_actions.clear();
    }

    pub(in crate::renderer::native_vulkan) fn last_actions(
        &self,
    ) -> &[NativeVulkanSceneGpuBufferSyncAction] {
        &self.last_actions
    }

    pub(in crate::renderer::native_vulkan) fn mesh_draw_buffers(
        &self,
        geometry: SceneGeometryId,
    ) -> Result<NativeVulkanSceneMeshDrawBuffers, String> {
        Ok(NativeVulkanSceneMeshDrawBuffers {
            geometry,
            vertex: self.required_buffer_binding(
                NativeVulkanSceneGpuBufferOwner::MeshGeometry(geometry),
                NativeVulkanSceneGpuBufferRole::MeshVertex,
            )?,
            index: self.required_buffer_binding(
                NativeVulkanSceneGpuBufferOwner::MeshGeometry(geometry),
                NativeVulkanSceneGpuBufferRole::MeshIndex,
            )?,
        })
    }

    pub(in crate::renderer::native_vulkan) fn puppet_storage_buffers(
        &self,
        puppet: ScenePuppetId,
    ) -> NativeVulkanScenePuppetStorageBuffers {
        NativeVulkanScenePuppetStorageBuffers {
            puppet,
            bones: self.buffer_binding(
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet),
                NativeVulkanSceneGpuBufferRole::PuppetBone,
            ),
            skin_vertices: self.buffer_binding(
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet),
                NativeVulkanSceneGpuBufferRole::PuppetSkinVertex,
            ),
            clip_frames: self.buffer_binding(
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet),
                NativeVulkanSceneGpuBufferRole::PuppetClipFrame,
            ),
        }
    }

    fn required_buffer_binding(
        &self,
        owner: NativeVulkanSceneGpuBufferOwner,
        role: NativeVulkanSceneGpuBufferRole,
    ) -> Result<NativeVulkanSceneGpuBufferBinding, String> {
        self.buffer_binding(owner, role)
            .ok_or_else(|| format!("missing retained scene GPU buffer for {owner:?} {role:?}"))
    }

    fn buffer_binding(
        &self,
        owner: NativeVulkanSceneGpuBufferOwner,
        role: NativeVulkanSceneGpuBufferRole,
    ) -> Option<NativeVulkanSceneGpuBufferBinding> {
        self.buffers
            .get(&NativeVulkanSceneGpuBufferKey { owner, role })
            .map(|slot| buffer_binding(slot))
    }

    fn finish_recorded_upload(
        &mut self,
        recorded_upload: NativeVulkanVulkanaliaRecordedBufferUpload,
    ) -> NativeVulkanVulkanaliaBuffer {
        if let Some(staging) = recorded_upload.staging {
            self.defer_retirement(staging);
        }
        recorded_upload.target
    }

    fn defer_retirement(&mut self, buffer: NativeVulkanVulkanaliaBuffer) {
        self.pending_retirements.push(buffer);
    }
}

impl Default for NativeVulkanSceneGpuBufferStore {
    fn default() -> Self {
        Self::new()
    }
}

struct NativeVulkanSceneGpuBufferSlot {
    record: NativeVulkanSceneGpuBufferRecord,
    buffer: NativeVulkanVulkanaliaBuffer,
}

fn upload_records(
    uploads: &[NativeVulkanSceneGpuBufferUpload],
) -> Result<
    BTreeMap<NativeVulkanSceneGpuBufferKey, NativeVulkanSceneGpuBufferRecord>,
    NativeVulkanSceneGpuBufferSyncError,
> {
    let mut records = BTreeMap::new();
    for upload in uploads {
        let record = upload_record(upload)?;
        if records.insert(record.key, record).is_some() {
            return Err(NativeVulkanSceneGpuBufferSyncError::DuplicateUploadKey {
                key: NativeVulkanSceneGpuBufferKey::from(upload.requirement),
            });
        }
    }
    Ok(records)
}

fn upload_map(
    uploads: Vec<NativeVulkanSceneGpuBufferUpload>,
) -> Result<
    BTreeMap<NativeVulkanSceneGpuBufferKey, NativeVulkanSceneGpuBufferUpload>,
    NativeVulkanSceneGpuBufferSyncError,
> {
    let mut by_key = BTreeMap::new();
    for upload in uploads {
        let key = NativeVulkanSceneGpuBufferKey::from(upload.requirement);
        if by_key.insert(key, upload).is_some() {
            return Err(NativeVulkanSceneGpuBufferSyncError::DuplicateUploadKey { key });
        }
    }
    Ok(by_key)
}

fn upload_record(
    upload: &NativeVulkanSceneGpuBufferUpload,
) -> Result<NativeVulkanSceneGpuBufferRecord, NativeVulkanSceneGpuBufferSyncError> {
    let payload_bytes = u64::try_from(upload.payload.len()).unwrap_or(u64::MAX);
    if payload_bytes != upload.requirement.bytes {
        return Err(NativeVulkanSceneGpuBufferSyncError::UploadSizeMismatch {
            requirement: upload.requirement,
            payload_bytes,
        });
    }
    Ok(NativeVulkanSceneGpuBufferRecord {
        key: NativeVulkanSceneGpuBufferKey::from(upload.requirement),
        requirement: upload.requirement,
        payload_hash: scene_stable_byte_hash(&upload.payload),
    })
}

fn record_binding(
    record: &NativeVulkanSceneGpuBufferRecord,
) -> NativeVulkanSceneGpuBufferRecordBinding {
    NativeVulkanSceneGpuBufferRecordBinding {
        key: record.key,
        bytes: record.requirement.bytes,
        payload_hash: record.payload_hash,
    }
}

fn buffer_binding(slot: &NativeVulkanSceneGpuBufferSlot) -> NativeVulkanSceneGpuBufferBinding {
    NativeVulkanSceneGpuBufferBinding {
        key: slot.record.key,
        buffer: slot.buffer.buffer,
        bytes: slot.record.requirement.bytes,
        payload_hash: slot.record.payload_hash,
    }
}

fn scene_gpu_buffer_usage_flags(usage: NativeVulkanSceneGpuBufferUsage) -> vk::BufferUsageFlags {
    match usage {
        NativeVulkanSceneGpuBufferUsage::Vertex => vk::BufferUsageFlags::VERTEX_BUFFER,
        NativeVulkanSceneGpuBufferUsage::Index => vk::BufferUsageFlags::INDEX_BUFFER,
        NativeVulkanSceneGpuBufferUsage::Storage => vk::BufferUsageFlags::STORAGE_BUFFER,
    }
}

fn scene_gpu_buffer_role_name(requirement: NativeVulkanSceneGpuBufferRequirement) -> &'static str {
    match requirement.role {
        NativeVulkanSceneGpuBufferRole::MeshVertex => "scene-mesh-vertex-buffer",
        NativeVulkanSceneGpuBufferRole::MeshIndex => "scene-mesh-index-buffer",
        NativeVulkanSceneGpuBufferRole::PuppetBone => "scene-puppet-bone-storage-buffer",
        NativeVulkanSceneGpuBufferRole::PuppetSkinVertex => {
            "scene-puppet-skin-vertex-storage-buffer"
        }
        NativeVulkanSceneGpuBufferRole::PuppetClipFrame => "scene-puppet-clip-frame-storage-buffer",
    }
}

fn scene_stable_byte_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::super::resource_storage::{
        NativeVulkanSceneGpuBufferOwner, NativeVulkanSceneGpuBufferRequirement,
        NativeVulkanSceneGpuBufferRole, NativeVulkanSceneGpuBufferUsage,
    };
    use super::*;
    use crate::engine::scene_engine::{SceneGeometryId, ScenePuppetId};

    #[test]
    fn catalog_creates_then_reuses_unchanged_uploads() {
        let mut catalog = NativeVulkanSceneGpuBufferCatalog::default();
        let plan = upload_plan(vec![upload(
            SceneGeometryId(4),
            NativeVulkanSceneGpuBufferRole::MeshVertex,
            vec![1, 2, 3, 4],
        )]);

        let first = catalog.sync_upload_plan(&plan).unwrap().to_vec();
        let second = catalog.sync_upload_plan(&plan).unwrap().to_vec();

        assert!(matches!(
            first.as_slice(),
            [NativeVulkanSceneGpuBufferSyncAction::Create { .. }]
        ));
        assert!(matches!(
            second.as_slice(),
            [NativeVulkanSceneGpuBufferSyncAction::Reuse { .. }]
        ));
        assert_eq!(catalog.records().len(), 1);
    }

    #[test]
    fn catalog_replaces_changed_payload_and_releases_stale_keys() {
        let mut catalog = NativeVulkanSceneGpuBufferCatalog::default();
        let first_plan = upload_plan(vec![
            upload(
                SceneGeometryId(4),
                NativeVulkanSceneGpuBufferRole::MeshVertex,
                vec![1, 2, 3, 4],
            ),
            upload(
                SceneGeometryId(4),
                NativeVulkanSceneGpuBufferRole::MeshIndex,
                vec![0, 0, 0, 0],
            ),
        ]);
        catalog.sync_upload_plan(&first_plan).unwrap();

        let second_plan = upload_plan(vec![upload(
            SceneGeometryId(4),
            NativeVulkanSceneGpuBufferRole::MeshVertex,
            vec![4, 3, 2, 1],
        )]);
        let actions = catalog.sync_upload_plan(&second_plan).unwrap().to_vec();

        assert!(matches!(
            actions.as_slice(),
            [
                NativeVulkanSceneGpuBufferSyncAction::Release { .. },
                NativeVulkanSceneGpuBufferSyncAction::Replace { .. }
            ]
        ));
        assert_eq!(catalog.records().len(), 1);
    }

    #[test]
    fn catalog_rejects_duplicate_upload_keys() {
        let mut catalog = NativeVulkanSceneGpuBufferCatalog::default();
        let plan = upload_plan(vec![
            upload(
                SceneGeometryId(4),
                NativeVulkanSceneGpuBufferRole::MeshVertex,
                vec![1, 2, 3, 4],
            ),
            upload(
                SceneGeometryId(4),
                NativeVulkanSceneGpuBufferRole::MeshVertex,
                vec![4, 3, 2, 1],
            ),
        ]);

        let err = catalog
            .sync_upload_plan(&plan)
            .expect_err("duplicate upload keys must fail");

        assert!(matches!(
            err,
            NativeVulkanSceneGpuBufferSyncError::DuplicateUploadKey {
                key: NativeVulkanSceneGpuBufferKey {
                    owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(SceneGeometryId(4)),
                    role: NativeVulkanSceneGpuBufferRole::MeshVertex,
                }
            }
        ));
    }

    #[test]
    fn catalog_exposes_mesh_draw_and_puppet_storage_records() {
        let mut catalog = NativeVulkanSceneGpuBufferCatalog::default();
        let plan = upload_plan(vec![
            upload(
                SceneGeometryId(4),
                NativeVulkanSceneGpuBufferRole::MeshVertex,
                vec![1, 2, 3, 4],
            ),
            upload(
                SceneGeometryId(4),
                NativeVulkanSceneGpuBufferRole::MeshIndex,
                vec![0, 0, 0, 0],
            ),
            puppet_upload(
                ScenePuppetId(9),
                NativeVulkanSceneGpuBufferRole::PuppetBone,
                vec![5; 64],
            ),
            puppet_upload(
                ScenePuppetId(9),
                NativeVulkanSceneGpuBufferRole::PuppetClipFrame,
                vec![6; 48],
            ),
        ]);
        catalog.sync_upload_plan(&plan).unwrap();

        let mesh = catalog
            .mesh_draw_buffer_records(SceneGeometryId(4))
            .unwrap();
        assert_eq!(mesh.geometry, SceneGeometryId(4));
        assert_eq!(
            mesh.vertex.key.role,
            NativeVulkanSceneGpuBufferRole::MeshVertex
        );
        assert_eq!(
            mesh.index.key.role,
            NativeVulkanSceneGpuBufferRole::MeshIndex
        );

        let puppet = catalog.puppet_storage_buffer_records(ScenePuppetId(9));
        assert!(puppet.bones.is_some());
        assert!(puppet.skin_vertices.is_none());
        assert!(puppet.clip_frames.is_some());
    }

    #[test]
    fn catalog_requires_complete_mesh_draw_records() {
        let mut catalog = NativeVulkanSceneGpuBufferCatalog::default();
        let plan = upload_plan(vec![upload(
            SceneGeometryId(4),
            NativeVulkanSceneGpuBufferRole::MeshVertex,
            vec![1, 2, 3, 4],
        )]);
        catalog.sync_upload_plan(&plan).unwrap();

        let err = catalog
            .mesh_draw_buffer_records(SceneGeometryId(4))
            .expect_err("mesh draw binding without index buffer must fail");

        assert!(err.contains("MeshIndex"));
    }

    #[test]
    fn catalog_hash_matches_native_vulkan_fnv1a() {
        assert_eq!(
            scene_stable_byte_hash(b"gilder-scene"),
            0xac2b_2346_1210_388f
        );
    }

    fn upload_plan(
        uploads: Vec<NativeVulkanSceneGpuBufferUpload>,
    ) -> NativeVulkanSceneGpuUploadPlan {
        NativeVulkanSceneGpuUploadPlan::from_uploads_for_test(uploads)
    }

    fn upload(
        geometry: SceneGeometryId,
        role: NativeVulkanSceneGpuBufferRole,
        payload: Vec<u8>,
    ) -> NativeVulkanSceneGpuBufferUpload {
        NativeVulkanSceneGpuBufferUpload {
            requirement: NativeVulkanSceneGpuBufferRequirement {
                owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(geometry),
                role,
                bytes: payload.len() as u64,
                usage: match role {
                    NativeVulkanSceneGpuBufferRole::MeshVertex => {
                        NativeVulkanSceneGpuBufferUsage::Vertex
                    }
                    NativeVulkanSceneGpuBufferRole::MeshIndex => {
                        NativeVulkanSceneGpuBufferUsage::Index
                    }
                    NativeVulkanSceneGpuBufferRole::PuppetBone
                    | NativeVulkanSceneGpuBufferRole::PuppetSkinVertex
                    | NativeVulkanSceneGpuBufferRole::PuppetClipFrame => {
                        NativeVulkanSceneGpuBufferUsage::Storage
                    }
                },
            },
            payload,
        }
    }

    fn puppet_upload(
        puppet: ScenePuppetId,
        role: NativeVulkanSceneGpuBufferRole,
        payload: Vec<u8>,
    ) -> NativeVulkanSceneGpuBufferUpload {
        NativeVulkanSceneGpuBufferUpload {
            requirement: NativeVulkanSceneGpuBufferRequirement {
                owner: NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet),
                role,
                bytes: payload.len() as u64,
                usage: NativeVulkanSceneGpuBufferUsage::Storage,
            },
            payload,
        }
    }
}
