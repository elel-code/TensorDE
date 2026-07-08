//! Retained GPU buffers for WE effect uniform payloads.
//!
//! References:
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/shaders/effects/iris.vert`
//! - `reverse-engineered/shaders/effects/iris.frag`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/storage/`

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SCENE_GPU_IRIS_EFFECT_UNIFORM_BYTES, SceneEffectUniformFramePlan, SceneIrisEffectUniformRecord,
    SceneObjectId, WE_VEC4_BYTES, WeVec4,
};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaBuffer, NativeVulkanVulkanaliaRecordedBufferUpload,
    native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload,
    native_vulkan_vulkanalia_destroy_buffer,
};

use super::effect_descriptors::{
    NativeVulkanSceneEffectTextureDescriptorBinding,
    NativeVulkanSceneEffectTextureDescriptorFramePlan,
};
use super::frame_completion::NativeVulkanSceneFrameSubmission;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectUniformKey {
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub shader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectUniformRecord {
    pub key: NativeVulkanSceneEffectUniformKey,
    pub record_index: usize,
    pub bytes: u64,
    pub payload_hash: u64,
    pub payload_layout: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectUniformUpload {
    pub key: NativeVulkanSceneEffectUniformKey,
    pub record_index: usize,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectUniformUploadPlan {
    pub record_count: usize,
    pub iris_record_count: usize,
    pub total_bytes: u64,
    pub record_bytes: u64,
    pub payload_layout: &'static str,
    #[serde(skip)]
    uploads: Vec<NativeVulkanSceneEffectUniformUpload>,
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectUniformGpuBufferBinding {
    pub key: NativeVulkanSceneEffectUniformKey,
    pub buffer: vk::Buffer,
    pub device_address: vk::DeviceAddress,
    pub record_index: usize,
    pub bytes: u64,
    pub payload_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneEffectUniformGpuBufferSyncAction {
    Create {
        record: NativeVulkanSceneEffectUniformRecord,
    },
    Reuse {
        record: NativeVulkanSceneEffectUniformRecord,
    },
    Replace {
        old: NativeVulkanSceneEffectUniformRecord,
        new: NativeVulkanSceneEffectUniformRecord,
    },
    Release {
        record: NativeVulkanSceneEffectUniformRecord,
    },
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectUniformGpuBufferStore {
    buffers:
        BTreeMap<NativeVulkanSceneEffectUniformKey, NativeVulkanSceneEffectUniformGpuBufferSlot>,
    pending_retirements: Vec<NativeVulkanSceneEffectUniformGpuBufferRetirement>,
    last_actions: Vec<NativeVulkanSceneEffectUniformGpuBufferSyncAction>,
}

impl NativeVulkanSceneEffectUniformUploadPlan {
    pub(in crate::renderer::native_vulkan) fn from_effect_uniform_frame_plan(
        plan: &SceneEffectUniformFramePlan,
        texture_descriptors: &NativeVulkanSceneEffectTextureDescriptorFramePlan,
    ) -> Result<Self, String> {
        let mut uploads = Vec::new();
        for record in &plan.iris_records {
            uploads.push(iris_effect_uniform_upload(record, texture_descriptors)?);
        }
        Ok(Self {
            record_count: uploads.len(),
            iris_record_count: plan.iris_record_count,
            total_bytes: u64::try_from(uploads.len())
                .unwrap_or(u64::MAX)
                .saturating_mul(SCENE_GPU_IRIS_EFFECT_UNIFORM_BYTES),
            record_bytes: SCENE_GPU_IRIS_EFFECT_UNIFORM_BYTES,
            payload_layout: "iris-effect-uniform-v0-four-vec4",
            uploads,
            command_order: [
                "read_engine_effect_uniform_frame_plan",
                "resolve_effect_texture_resolution_uniforms",
                "pack_iris_effect_uniform_payloads",
                "diff_retained_effect_uniform_records",
                "prepare_effect_uniform_gpu_upload",
            ],
        })
    }

    pub(in crate::renderer::native_vulkan) fn uploads(
        &self,
    ) -> &[NativeVulkanSceneEffectUniformUpload] {
        &self.uploads
    }
}

impl NativeVulkanSceneEffectUniformGpuBufferStore {
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
        upload_plan: &NativeVulkanSceneEffectUniformUploadPlan,
    ) -> Result<&[NativeVulkanSceneEffectUniformGpuBufferSyncAction], String> {
        let upload_records = effect_uniform_upload_records(upload_plan.uploads())?;
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
                    NativeVulkanSceneEffectUniformGpuBufferSyncAction::Release {
                        record: slot.record,
                    },
                );
            }
        }

        for upload in upload_plan.uploads() {
            let new_record = effect_uniform_upload_record(upload)?;
            if let Some(old_slot) = self.buffers.get(&upload.key)
                && old_slot.record == new_record
            {
                self.last_actions
                    .push(NativeVulkanSceneEffectUniformGpuBufferSyncAction::Reuse {
                        record: old_slot.record.clone(),
                    });
                continue;
            }

            let recorded_upload =
                native_vulkan_vulkanalia_create_device_local_buffer_with_recorded_staging_upload(
                    device,
                    memory_properties,
                    command_buffer,
                    "scene-effect-uniform-buffer",
                    new_record.bytes,
                    vk::BufferUsageFlags::UNIFORM_BUFFER
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                    &upload.payload,
                )?;
            let new_buffer = self.finish_recorded_upload(frame_submission, recorded_upload);

            match self.buffers.insert(
                upload.key.clone(),
                NativeVulkanSceneEffectUniformGpuBufferSlot {
                    record: new_record.clone(),
                    buffer: new_buffer,
                },
            ) {
                Some(old_slot) => {
                    self.defer_retirement(frame_submission, old_slot.buffer);
                    self.last_actions.push(
                        NativeVulkanSceneEffectUniformGpuBufferSyncAction::Replace {
                            old: old_slot.record,
                            new: new_record,
                        },
                    );
                }
                None => {
                    self.last_actions.push(
                        NativeVulkanSceneEffectUniformGpuBufferSyncAction::Create {
                            record: new_record,
                        },
                    );
                }
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn effect_uniform_buffer(
        &self,
        key: &NativeVulkanSceneEffectUniformKey,
    ) -> Result<NativeVulkanSceneEffectUniformGpuBufferBinding, String> {
        let slot = self.buffers.get(key).ok_or_else(|| {
            format!("missing retained scene effect uniform GPU buffer for {key:?}")
        })?;
        Ok(effect_uniform_buffer_binding(slot))
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
    ) -> &[NativeVulkanSceneEffectUniformGpuBufferSyncAction] {
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
            .push(NativeVulkanSceneEffectUniformGpuBufferRetirement {
                frame_submission,
                buffer,
            });
    }
}

impl Default for NativeVulkanSceneEffectUniformGpuBufferStore {
    fn default() -> Self {
        Self::new()
    }
}

struct NativeVulkanSceneEffectUniformGpuBufferSlot {
    record: NativeVulkanSceneEffectUniformRecord,
    buffer: NativeVulkanVulkanaliaBuffer,
}

struct NativeVulkanSceneEffectUniformGpuBufferRetirement {
    frame_submission: NativeVulkanSceneFrameSubmission,
    buffer: NativeVulkanVulkanaliaBuffer,
}

fn iris_effect_uniform_upload(
    record: &SceneIrisEffectUniformRecord,
    texture_descriptors: &NativeVulkanSceneEffectTextureDescriptorFramePlan,
) -> Result<NativeVulkanSceneEffectUniformUpload, String> {
    let texture1_resolution = if record.texture_resolution_slots.contains(&1) {
        let descriptor = effect_pass_texture_descriptor(texture_descriptors, record, 1)?;
        texture_resolution_vec4(descriptor)
    } else {
        WeVec4::ZERO
    };
    let mut payload = Vec::with_capacity(SCENE_GPU_IRIS_EFFECT_UNIFORM_BYTES as usize);
    push_we_vec4(
        &mut payload,
        record,
        "g_Time/g_Speed/g_Rough/g_NoiseAmount",
        WeVec4::from_lanes([
            record.time_seconds,
            record.speed,
            record.rough,
            record.noise_amount,
        ]),
    )?;
    push_we_vec4(
        &mut payload,
        record,
        "g_Scale/g_PhaseOffset/MASK",
        WeVec4::from_lanes([
            record.scale[0],
            record.scale[1],
            record.phase_offset,
            record.mask_combo as f32,
        ]),
    )?;
    push_we_vec4(
        &mut payload,
        record,
        "g_Texture1Resolution",
        texture1_resolution,
    )?;
    push_we_vec4(
        &mut payload,
        record,
        "g_EyeColor/BACKGROUND",
        WeVec4::from_lanes([
            record.eye_color[0],
            record.eye_color[1],
            record.eye_color[2],
            record.background_combo as f32,
        ]),
    )?;
    let key = NativeVulkanSceneEffectUniformKey {
        effect_pass_index: record.effect_pass_index,
        object: record.object,
        shader: record.shader.clone(),
    };
    let actual_bytes = u64::try_from(payload.len()).unwrap_or(u64::MAX);
    if actual_bytes != SCENE_GPU_IRIS_EFFECT_UNIFORM_BYTES {
        return Err(format!(
            "scene iris effect uniform upload for {key:?} has {actual_bytes} bytes, expected {SCENE_GPU_IRIS_EFFECT_UNIFORM_BYTES}"
        ));
    }
    Ok(NativeVulkanSceneEffectUniformUpload {
        key,
        record_index: record.record_index,
        payload,
    })
}

fn effect_pass_texture_descriptor<'a>(
    texture_descriptors: &'a NativeVulkanSceneEffectTextureDescriptorFramePlan,
    record: &SceneIrisEffectUniformRecord,
    slot: u32,
) -> Result<&'a NativeVulkanSceneEffectTextureDescriptorBinding, String> {
    texture_descriptors
        .bindings
        .iter()
        .find(|binding| {
            binding.effect_pass_index == record.effect_pass_index
                && binding.object == record.object
                && binding.slot == slot
        })
        .ok_or_else(|| {
            format!(
                "scene iris effect uniform for pass {} object {:?} requires g_Texture{slot} resolution but no descriptor was resolved",
                record.effect_pass_index, record.object
            )
        })
}

fn texture_resolution_vec4(descriptor: &NativeVulkanSceneEffectTextureDescriptorBinding) -> WeVec4 {
    let width = descriptor.width as f32;
    let height = descriptor.height as f32;
    WeVec4::from_lanes([width, height, 1.0 / width, 1.0 / height])
}

fn push_we_vec4(
    payload: &mut Vec<u8>,
    record: &SceneIrisEffectUniformRecord,
    field: &'static str,
    value: WeVec4,
) -> Result<(), String> {
    if let Some(element) = value.first_non_finite_lane() {
        return Err(format!(
            "non-finite scene iris effect uniform float for object {:?} pass {} {field}[{element}]",
            record.object, record.effect_pass_index
        ));
    }
    let before = payload.len();
    value.write_le_bytes(payload);
    let written = payload.len().saturating_sub(before);
    debug_assert_eq!(written, WE_VEC4_BYTES as usize);
    Ok(())
}

fn effect_uniform_upload_records(
    uploads: &[NativeVulkanSceneEffectUniformUpload],
) -> Result<BTreeMap<NativeVulkanSceneEffectUniformKey, NativeVulkanSceneEffectUniformRecord>, String>
{
    let mut records = BTreeMap::new();
    for upload in uploads {
        let record = effect_uniform_upload_record(upload)?;
        if records.insert(upload.key.clone(), record).is_some() {
            return Err(format!(
                "duplicate scene effect uniform upload key {:?}",
                upload.key
            ));
        }
    }
    Ok(records)
}

fn effect_uniform_upload_record(
    upload: &NativeVulkanSceneEffectUniformUpload,
) -> Result<NativeVulkanSceneEffectUniformRecord, String> {
    let actual_bytes = u64::try_from(upload.payload.len()).unwrap_or(u64::MAX);
    if actual_bytes != SCENE_GPU_IRIS_EFFECT_UNIFORM_BYTES {
        return Err(format!(
            "scene effect uniform upload for {:?} has {actual_bytes} bytes, expected {SCENE_GPU_IRIS_EFFECT_UNIFORM_BYTES}",
            upload.key
        ));
    }
    Ok(NativeVulkanSceneEffectUniformRecord {
        key: upload.key.clone(),
        record_index: upload.record_index,
        bytes: actual_bytes,
        payload_hash: scene_stable_byte_hash(&upload.payload),
        payload_layout: "iris-effect-uniform-v0-four-vec4",
    })
}

fn effect_uniform_buffer_binding(
    slot: &NativeVulkanSceneEffectUniformGpuBufferSlot,
) -> NativeVulkanSceneEffectUniformGpuBufferBinding {
    NativeVulkanSceneEffectUniformGpuBufferBinding {
        key: slot.record.key.clone(),
        buffer: slot.buffer.buffer,
        device_address: slot.buffer.device_address,
        record_index: slot.record.record_index,
        bytes: slot.record.bytes,
        payload_hash: slot.record.payload_hash,
    }
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
        SceneEffectUniformFramePlan, SceneIrisEffectUniformRecord, SceneObjectId,
    };
    use crate::renderer::native_vulkan::scene_backend::texture_descriptors::{
        NativeVulkanSceneTextureDescriptorFormat, NativeVulkanSceneTextureDescriptorSource,
        NativeVulkanSceneTextureDescriptorVkFormat,
    };

    #[test]
    fn effect_uniform_upload_plan_packs_iris_payload_with_mask_resolution() {
        let plan = NativeVulkanSceneEffectUniformUploadPlan::from_effect_uniform_frame_plan(
            &SceneEffectUniformFramePlan {
                effect_pass_count: 1,
                iris_record_count: 1,
                iris_records: vec![iris_record()],
                command_order: [
                    "scan_effect_material_pass_uniform_contracts",
                    "lower_iris_material_constants",
                    "resolve_iris_combo_uniform_requirements",
                    "emit_effect_uniform_frame_plan",
                ],
            },
            &texture_descriptors(),
        )
        .expect("effect uniform upload plan");

        assert_eq!(plan.record_count, 1);
        assert_eq!(plan.total_bytes, 64);
        let upload = &plan.uploads()[0];
        assert_eq!(upload.payload.len(), 64);
        assert_eq!(
            &upload.payload[0..16],
            we_vec4_bytes(WeVec4::from_lanes([1.25, 1.5, 0.25, 0.75])).as_slice()
        );
        assert_eq!(
            &upload.payload[16..32],
            we_vec4_bytes(WeVec4::from_lanes([2.0, 3.0, -0.2, 1.0])).as_slice()
        );
        assert_eq!(
            &upload.payload[32..48],
            we_vec4_bytes(WeVec4::from_lanes([256.0, 128.0, 1.0 / 256.0, 1.0 / 128.0])).as_slice()
        );
        assert_eq!(
            &upload.payload[48..64],
            we_vec4_bytes(WeVec4::from_lanes([0.1, 0.2, 0.3, 1.0])).as_slice()
        );
    }

    #[test]
    fn effect_uniform_upload_plan_requires_mask_texture_resolution_descriptor() {
        let err = NativeVulkanSceneEffectUniformUploadPlan::from_effect_uniform_frame_plan(
            &SceneEffectUniformFramePlan {
                effect_pass_count: 1,
                iris_record_count: 1,
                iris_records: vec![iris_record()],
                command_order: [
                    "scan_effect_material_pass_uniform_contracts",
                    "lower_iris_material_constants",
                    "resolve_iris_combo_uniform_requirements",
                    "emit_effect_uniform_frame_plan",
                ],
            },
            &NativeVulkanSceneEffectTextureDescriptorFramePlan {
                pass_count: 1,
                binding_count: 0,
                bindings: Vec::new(),
                descriptor_model: "VK_EXT_descriptor_heap",
                command_order: [
                    "resolve_effect_source_texture_descriptors",
                    "resolve_effect_named_fbo_texture_descriptors",
                    "resolve_effect_previous_scene_texture_descriptors",
                    "bind_descriptor_heap_texture_mapping",
                ],
            },
        )
        .expect_err("missing mask resolution descriptor must fail");

        assert!(err.contains("requires g_Texture1 resolution"));
    }

    fn iris_record() -> SceneIrisEffectUniformRecord {
        SceneIrisEffectUniformRecord {
            record_index: 0,
            effect_pass_index: 2,
            object: SceneObjectId(7),
            pass_index: 9,
            shader: "effects/iris".to_owned(),
            time_seconds: 1.25,
            texture_slot_mask: 0b11,
            texture_resolution_slots: vec![1],
            scale: [2.0, 3.0],
            speed: 1.5,
            rough: 0.25,
            noise_amount: 0.75,
            phase_offset: -0.2,
            eye_color: [0.1, 0.2, 0.3],
            mask_combo: 1,
            background_combo: 1,
        }
    }

    fn texture_descriptors() -> NativeVulkanSceneEffectTextureDescriptorFramePlan {
        NativeVulkanSceneEffectTextureDescriptorFramePlan {
            pass_count: 1,
            binding_count: 1,
            bindings: vec![NativeVulkanSceneEffectTextureDescriptorBinding {
                effect_pass_index: 2,
                object: SceneObjectId(7),
                slot: 1,
                role: crate::engine::scene_engine::SceneGraphResourceRole::shader_texture(1),
                source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(
                    crate::engine::scene_engine::SceneResourceId(4),
                ),
                width: 256,
                height: 128,
                format: NativeVulkanSceneTextureDescriptorFormat::VkFormat(
                    NativeVulkanSceneTextureDescriptorVkFormat::R8G8B8A8Unorm,
                ),
                mip_count: 1,
                payload_bytes: Some(131_072),
                shader_mapping: "we.texture_slot1.g_Texture1".to_owned(),
            }],
            descriptor_model: "VK_EXT_descriptor_heap",
            command_order: [
                "resolve_effect_source_texture_descriptors",
                "resolve_effect_named_fbo_texture_descriptors",
                "resolve_effect_previous_scene_texture_descriptors",
                "bind_descriptor_heap_texture_mapping",
            ],
        }
    }

    fn we_vec4_bytes(value: WeVec4) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(WE_VEC4_BYTES as usize);
        value.write_le_bytes(&mut bytes);
        bytes
    }
}
