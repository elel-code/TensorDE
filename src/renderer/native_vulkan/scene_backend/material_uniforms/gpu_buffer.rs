//! Retained GPU buffers for WE material uniform records.
//!
//! References:
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/shaders/genericimage4.frag`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaBuffer, NativeVulkanVulkanaliaRecordedBufferUpload,
    native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload,
    native_vulkan_vulkanalia_destroy_buffer,
};

use super::{
    NativeVulkanSceneMaterialUniformKey, NativeVulkanSceneMaterialUniformRecord,
    NativeVulkanSceneMaterialUniformUpload, NativeVulkanSceneMaterialUniformUploadPlan,
    scene_stable_byte_hash,
};
use crate::engine::scene_engine::SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES;
use crate::renderer::native_vulkan::scene_backend::frame_completion::NativeVulkanSceneFrameSubmission;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMaterialUniformGpuBufferBinding {
    pub key: NativeVulkanSceneMaterialUniformKey,
    pub buffer: vk::Buffer,
    pub device_address: vk::DeviceAddress,
    pub record_index: usize,
    pub bytes: u64,
    pub payload_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneMaterialUniformGpuBufferSyncAction {
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

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMaterialUniformGpuBufferStore {
    buffers: BTreeMap<
        NativeVulkanSceneMaterialUniformKey,
        NativeVulkanSceneMaterialUniformGpuBufferSlot,
    >,
    pending_retirements: Vec<NativeVulkanSceneMaterialUniformGpuBufferRetirement>,
    last_actions: Vec<NativeVulkanSceneMaterialUniformGpuBufferSyncAction>,
}

impl NativeVulkanSceneMaterialUniformGpuBufferStore {
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
        frame_submission: NativeVulkanSceneFrameSubmission,
        upload_plan: &NativeVulkanSceneMaterialUniformUploadPlan,
    ) -> Result<&[NativeVulkanSceneMaterialUniformGpuBufferSyncAction], String> {
        let upload_records = material_uniform_upload_records(upload_plan.uploads())?;
        self.last_actions.clear();

        let active_keys = upload_records.keys().cloned().collect::<BTreeSet<_>>();
        let stale_keys = self
            .buffers
            .keys()
            .filter(|key| !active_keys.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in stale_keys {
            if let Some(slot) = self.buffers.remove(&key) {
                self.defer_retirement(frame_submission, slot.buffer);
                self.last_actions.push(
                    NativeVulkanSceneMaterialUniformGpuBufferSyncAction::Release {
                        record: slot.record,
                    },
                );
            }
        }

        for upload in upload_plan.uploads() {
            let new_record = material_uniform_upload_record(upload)?;
            if let Some(old_slot) = self.buffers.get(&upload.key)
                && old_slot.record == new_record
            {
                self.last_actions.push(
                    NativeVulkanSceneMaterialUniformGpuBufferSyncAction::Reuse {
                        record: old_slot.record.clone(),
                    },
                );
                continue;
            }

            let recorded_upload =
                native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload(
                    device,
                    memory_properties,
                    command_buffer,
                    "scene-material-uniform-buffer",
                    new_record.bytes,
                    vk::BufferUsageFlags::UNIFORM_BUFFER
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                    &upload.payload,
                )?;
            let new_buffer = self.finish_recorded_upload(frame_submission, recorded_upload);

            match self.buffers.insert(
                upload.key.clone(),
                NativeVulkanSceneMaterialUniformGpuBufferSlot {
                    record: new_record.clone(),
                    buffer: new_buffer,
                },
            ) {
                Some(old_slot) => {
                    self.defer_retirement(frame_submission, old_slot.buffer);
                    self.last_actions.push(
                        NativeVulkanSceneMaterialUniformGpuBufferSyncAction::Replace {
                            old: old_slot.record,
                            new: new_record,
                        },
                    );
                }
                None => {
                    self.last_actions.push(
                        NativeVulkanSceneMaterialUniformGpuBufferSyncAction::Create {
                            record: new_record,
                        },
                    );
                }
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn material_uniform_buffer(
        &self,
        key: &NativeVulkanSceneMaterialUniformKey,
    ) -> Result<NativeVulkanSceneMaterialUniformGpuBufferBinding, String> {
        let slot = self.buffers.get(key).ok_or_else(|| {
            format!("missing retained scene material uniform GPU buffer for {key:?}")
        })?;
        Ok(material_uniform_buffer_binding(slot))
    }

    pub(in crate::renderer::native_vulkan) fn release_completed_uploads(
        &mut self,
        device: &Device,
        completed_submission: NativeVulkanSceneFrameSubmission,
    ) -> usize {
        let mut retained = Vec::new();
        let mut retired_count = 0usize;
        for retirement in std::mem::take(&mut self.pending_retirements) {
            if completed_submission.covers(retirement.frame_submission) {
                native_vulkan_vulkanalia_destroy_buffer(device, retirement.buffer);
                retired_count = retired_count.saturating_add(1);
            } else {
                retained.push(retirement);
            }
        }
        self.pending_retirements = retained;
        retired_count
    }

    pub(in crate::renderer::native_vulkan) fn destroy_all(&mut self, device: &Device) {
        for (_, slot) in std::mem::take(&mut self.buffers) {
            native_vulkan_vulkanalia_destroy_buffer(device, slot.buffer);
        }
        for retirement in std::mem::take(&mut self.pending_retirements) {
            native_vulkan_vulkanalia_destroy_buffer(device, retirement.buffer);
        }
        self.last_actions.clear();
    }

    pub(in crate::renderer::native_vulkan) fn last_actions(
        &self,
    ) -> &[NativeVulkanSceneMaterialUniformGpuBufferSyncAction] {
        &self.last_actions
    }

    fn finish_recorded_upload(
        &mut self,
        frame_submission: NativeVulkanSceneFrameSubmission,
        recorded_upload: NativeVulkanVulkanaliaRecordedBufferUpload,
    ) -> NativeVulkanVulkanaliaBuffer {
        if let Some(staging) = recorded_upload.staging {
            self.defer_retirement(frame_submission, staging);
        }
        recorded_upload.target
    }

    fn defer_retirement(
        &mut self,
        frame_submission: NativeVulkanSceneFrameSubmission,
        buffer: NativeVulkanVulkanaliaBuffer,
    ) {
        self.pending_retirements
            .push(NativeVulkanSceneMaterialUniformGpuBufferRetirement {
                frame_submission,
                buffer,
            });
    }
}

impl Default for NativeVulkanSceneMaterialUniformGpuBufferStore {
    fn default() -> Self {
        Self::new()
    }
}

struct NativeVulkanSceneMaterialUniformGpuBufferSlot {
    record: NativeVulkanSceneMaterialUniformRecord,
    buffer: NativeVulkanVulkanaliaBuffer,
}

struct NativeVulkanSceneMaterialUniformGpuBufferRetirement {
    frame_submission: NativeVulkanSceneFrameSubmission,
    buffer: NativeVulkanVulkanaliaBuffer,
}

fn material_uniform_buffer_binding(
    slot: &NativeVulkanSceneMaterialUniformGpuBufferSlot,
) -> NativeVulkanSceneMaterialUniformGpuBufferBinding {
    NativeVulkanSceneMaterialUniformGpuBufferBinding {
        key: slot.record.key.clone(),
        buffer: slot.buffer.buffer,
        device_address: slot.buffer.device_address,
        record_index: slot.record.record_index,
        bytes: slot.record.bytes,
        payload_hash: slot.record.payload_hash,
    }
}

fn material_uniform_upload_records(
    uploads: &[NativeVulkanSceneMaterialUniformUpload],
) -> Result<
    BTreeMap<NativeVulkanSceneMaterialUniformKey, NativeVulkanSceneMaterialUniformRecord>,
    String,
> {
    let mut records = BTreeMap::new();
    for upload in uploads {
        let record = material_uniform_upload_record(upload)?;
        if records.insert(upload.key.clone(), record).is_some() {
            return Err(format!(
                "duplicate scene material uniform GPU upload key {:?}",
                upload.key
            ));
        }
    }
    Ok(records)
}

fn material_uniform_upload_record(
    upload: &NativeVulkanSceneMaterialUniformUpload,
) -> Result<NativeVulkanSceneMaterialUniformRecord, String> {
    let payload_bytes = u64::try_from(upload.payload.len()).unwrap_or(u64::MAX);
    if payload_bytes != SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES {
        return Err(format!(
            "scene material uniform GPU upload for {:?} has {} bytes, expected {}",
            upload.key, payload_bytes, SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES
        ));
    }
    Ok(NativeVulkanSceneMaterialUniformRecord {
        key: upload.key.clone(),
        record_index: upload.record_index,
        bytes: payload_bytes,
        payload_hash: scene_stable_byte_hash(&upload.payload),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{SceneObjectId, WeVec4};
    use vulkanalia::vk::Handle;

    #[test]
    fn material_uniform_gpu_record_uses_we_uniform_payload_hash() {
        let upload = upload([1.0, 1.0, 1.0, 1.0]);

        let record = material_uniform_upload_record(&upload).unwrap();

        assert_eq!(record.key.object, SceneObjectId(7));
        assert_eq!(record.record_index, 0);
        assert_eq!(record.bytes, SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES);
        assert_ne!(record.payload_hash, 0);
    }

    #[test]
    fn material_uniform_gpu_record_rejects_wrong_payload_size() {
        let mut upload = upload([1.0, 1.0, 1.0, 1.0]);
        upload.payload.pop();

        let err = material_uniform_upload_record(&upload)
            .expect_err("wrong uniform payload size must fail");

        assert!(err.contains("expected 48"));
    }

    #[test]
    fn material_uniform_buffer_binding_exposes_device_address() {
        let upload = upload([1.0, 1.0, 1.0, 1.0]);
        let slot = NativeVulkanSceneMaterialUniformGpuBufferSlot {
            record: material_uniform_upload_record(&upload).unwrap(),
            buffer: NativeVulkanVulkanaliaBuffer {
                buffer: vk::Buffer::from_raw(17),
                memory: vk::DeviceMemory::from_raw(23),
                device_address: 0x1000,
                snapshot:
                    crate::renderer::native_vulkan::vulkan::NativeVulkanVulkanaliaBufferSnapshot {
                        role: "test-material-uniform",
                        buffer_created: true,
                        memory_bound: true,
                        mapped: false,
                        device_address_nonzero: true,
                        requested_bytes: 48,
                        memory_size: 64,
                        memory_alignment: 16,
                        memory_type_bits: 1,
                        selected_memory_type_index: 0,
                        selected_memory_property_flags: Vec::new(),
                        usage_flags: Vec::new(),
                        host_coherent: false,
                        payload_uploaded: false,
                    },
            },
        };

        let binding = material_uniform_buffer_binding(&slot);

        assert_eq!(binding.buffer.as_raw(), 17);
        assert_eq!(binding.device_address, 0x1000);
        assert_eq!(binding.bytes, 48);
    }

    fn upload(color: [f32; 4]) -> NativeVulkanSceneMaterialUniformUpload {
        let mut payload = Vec::new();
        WeVec4::from_lanes(color).write_le_bytes(&mut payload);
        WeVec4::from_lanes([0.7, 0.0, 0.0, 0.0]).write_le_bytes(&mut payload);
        WeVec4::from_lanes([1.0, 1.0, 1.0, 0.0]).write_le_bytes(&mut payload);
        NativeVulkanSceneMaterialUniformUpload {
            key: NativeVulkanSceneMaterialUniformKey {
                object: SceneObjectId(7),
                shader: "we/genericimage4".to_owned(),
            },
            record_index: 0,
            payload,
        }
    }
}
