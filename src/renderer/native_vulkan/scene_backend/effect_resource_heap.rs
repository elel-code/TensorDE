//! Effect pass descriptor heap slice planning.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

mod bind_command;
mod key;
mod resolve;
mod store;
mod vk_descriptor;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::Serialize;
use vulkanalia::vk::{self, Handle};

use crate::engine::scene_engine::{
    SceneEffectUniformFramePlan, SceneGraphTarget, SceneIrisEffectUniformRecord, SceneObjectId,
    SceneResourceId,
};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_plan,
};

use super::effect_descriptors::{
    NativeVulkanSceneEffectTextureDescriptorBinding,
    NativeVulkanSceneEffectTextureDescriptorFramePlan,
};
use super::effect_uniforms::{
    NativeVulkanSceneEffectUniformGpuBufferBinding, NativeVulkanSceneEffectUniformKey,
};
use super::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use super::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;
use super::texture_images::NativeVulkanSceneTextureImageBinding;
pub(in crate::renderer::native_vulkan) use bind_command::{
    NativeVulkanSceneEffectResourceHeapPassBindPlan,
    native_vulkan_record_scene_effect_resource_heap_pass_bind_command,
};
#[cfg(test)]
pub(in crate::renderer::native_vulkan) use key::NativeVulkanSceneEffectTextureSetBinding;
pub(in crate::renderer::native_vulkan) use key::NativeVulkanSceneEffectTextureSetKey;
use key::{
    effect_heap_slice_shader_mappings, effect_texture_descriptors_by_pass, effect_texture_set_key,
};
use resolve::{
    NativeVulkanSceneEffectResolvedSampledImageBinding, resolve_effect_sampled_image_binding,
    validate_effect_texture_binding,
};
pub(in crate::renderer::native_vulkan) use store::{
    NativeVulkanSceneEffectResourceHeapPassBindInfo, NativeVulkanSceneEffectResourceHeapStore,
    NativeVulkanSceneEffectResourceHeapSyncAction,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectResourceHeapFramePlan {
    pub pass_count: usize,
    pub pass_binding_count: usize,
    pub heap_slice_count: usize,
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub descriptor_model: &'static str,
    pub entries: Vec<NativeVulkanSceneEffectResourceHeapEntry>,
    pub pass_bindings: Vec<NativeVulkanSceneEffectResourceHeapPassBinding>,
    pub descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    pub command_order: [&'static str; 5],
    #[serde(skip)]
    pub(super) bindings: Vec<NativeVulkanSceneEffectResourceHeapDescriptorBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectResourceHeapEntry {
    pub heap_slice_index: usize,
    pub descriptor_index: usize,
    pub descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    pub resource_heap_offset: u64,
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub sampler_descriptor_index: Option<usize>,
    pub sampler_heap_offset: Option<u64>,
    pub role: NativeVulkanSceneEffectResourceHeapEntryRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneEffectResourceHeapEntryRole {
    WeEffectUniformPayload {
        uniform: NativeVulkanSceneEffectUniformKey,
        buffer_handle: u64,
        device_address: u64,
        record_index: usize,
        bytes: u64,
        payload_hash: u64,
        shader_mapping: String,
    },
    WeSampledTexture {
        slot: u32,
        source: NativeVulkanSceneTextureDescriptorSource,
        image_handle: u64,
        view_handle: u64,
        sampler_handle: u64,
        format: String,
        width: u32,
        height: u32,
        mip_count: u32,
        shader_mapping: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectResourceHeapPassBinding {
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub heap_slice_index: usize,
    pub effect_uniform: Option<NativeVulkanSceneEffectUniformKey>,
    pub effect_uniform_buffer_handle: Option<u64>,
    pub effect_uniform_device_address: Option<u64>,
    pub effect_uniform_record_index: Option<usize>,
    pub effect_uniform_bytes: Option<u64>,
    pub effect_uniform_payload_hash: Option<u64>,
    pub texture_set: NativeVulkanSceneEffectTextureSetKey,
    pub base_resource_descriptor_index: usize,
    pub base_resource_heap_offset: u64,
    pub base_sampler_descriptor_index: Option<usize>,
    pub base_sampler_heap_offset: Option<u64>,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeVulkanSceneEffectResourceHeapDescriptorBinding {
    UniformBuffer {
        descriptor_index: usize,
        device_address: vk::DeviceAddress,
        bytes: u64,
    },
    SampledImage {
        descriptor_index: usize,
        sampler_descriptor_index: usize,
        source: NativeVulkanSceneTextureDescriptorSource,
        image: vk::Image,
        view: vk::ImageView,
        sampler: vk::Sampler,
        format: vk::Format,
        width: u32,
        height: u32,
        mip_count: u32,
    },
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanSceneEffectHeapSliceLayout {
    heap_slice_index: usize,
    base_resource_descriptor_index: usize,
    base_sampler_descriptor_index: Option<usize>,
    resource_descriptor_count: usize,
    texture_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NativeVulkanSceneEffectHeapSliceKey {
    effect_uniform: Option<NativeVulkanSceneEffectHeapSliceUniformKey>,
    texture_set: NativeVulkanSceneEffectTextureSetKey,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NativeVulkanSceneEffectHeapSliceUniformKey {
    uniform: NativeVulkanSceneEffectUniformKey,
    buffer_handle: u64,
    device_address: u64,
    bytes: u64,
    payload_hash: u64,
}

#[derive(Debug, Clone)]
struct PendingEffectResourceHeapEntry {
    heap_slice_index: usize,
    descriptor_index: usize,
    descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    sampler_descriptor_index: Option<usize>,
    effect_pass_index: usize,
    object: SceneObjectId,
    role: NativeVulkanSceneEffectResourceHeapEntryRole,
}

impl NativeVulkanSceneEffectResourceHeapFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_descriptors(
        descriptors: &NativeVulkanSceneEffectTextureDescriptorFramePlan,
        uniform_plan: &SceneEffectUniformFramePlan,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        effect_uniform_buffer: impl Fn(
            &NativeVulkanSceneEffectUniformKey,
        )
            -> Result<NativeVulkanSceneEffectUniformGpuBufferBinding, String>,
        texture_image_binding: impl Fn(
            SceneResourceId,
        ) -> Result<NativeVulkanSceneTextureImageBinding, String>,
        target_image_binding: impl Fn(
            SceneGraphTarget,
        )
            -> Result<NativeVulkanSceneOffscreenTargetBinding, String>,
    ) -> Result<Self, String> {
        let descriptors_by_pass = effect_texture_descriptors_by_pass(descriptors)?;
        let uniform_records_by_pass = effect_uniform_records_by_pass(uniform_plan)?;
        if uniform_records_by_pass.len() > descriptors.pass_count {
            return Err(format!(
                "scene effect resource heap uniform plan pass count {} exceeds descriptor pass count {}",
                uniform_records_by_pass.len(),
                descriptors.pass_count
            ));
        }
        let mut heap_slice_lookup = BTreeMap::<
            NativeVulkanSceneEffectHeapSliceKey,
            NativeVulkanSceneEffectHeapSliceLayout,
        >::new();
        let mut descriptor_kinds = Vec::new();
        let mut pending_entries = Vec::new();
        let mut descriptor_bindings = Vec::new();
        let mut pass_bindings = Vec::new();
        let mut sampler_descriptor_count = 0usize;

        for (effect_pass_index, pass_descriptors) in descriptors_by_pass.iter().enumerate() {
            let effect_uniform = uniform_records_by_pass
                .get(effect_pass_index)
                .and_then(|record| record.as_ref())
                .map(|record| effect_uniform_binding_from_record(record, &effect_uniform_buffer))
                .transpose()?;
            if pass_descriptors.is_empty() && effect_uniform.is_none() {
                continue;
            }
            let object = effect_heap_pass_object(
                effect_pass_index,
                pass_descriptors,
                effect_uniform.as_ref(),
            )?;
            let texture_set = effect_texture_set_key(pass_descriptors);
            let heap_slice_key = NativeVulkanSceneEffectHeapSliceKey {
                effect_uniform: effect_uniform.as_ref().map(effect_heap_slice_uniform_key),
                texture_set: texture_set.clone(),
            };
            let slice = if let Some(slice) = heap_slice_lookup.get(&heap_slice_key).copied() {
                slice
            } else {
                let heap_slice_index = heap_slice_lookup.len();
                let base_resource_descriptor_index = descriptor_kinds.len();
                let base_sampler_descriptor_index =
                    (!pass_descriptors.is_empty()).then_some(sampler_descriptor_count);
                if let Some(uniform) = effect_uniform.as_ref() {
                    push_effect_uniform_descriptor(
                        &mut descriptor_kinds,
                        &mut pending_entries,
                        &mut descriptor_bindings,
                        heap_slice_index,
                        effect_pass_index,
                        object,
                        uniform,
                    )?;
                }
                for descriptor in pass_descriptors {
                    let sampled_image = resolve_effect_sampled_image_binding(
                        descriptor,
                        &texture_image_binding,
                        &target_image_binding,
                    )?;
                    push_effect_texture_descriptor(
                        &mut descriptor_kinds,
                        &mut pending_entries,
                        &mut descriptor_bindings,
                        heap_slice_index,
                        descriptor,
                        sampled_image,
                        sampler_descriptor_count,
                    )?;
                    sampler_descriptor_count = sampler_descriptor_count.saturating_add(1);
                }
                let slice = NativeVulkanSceneEffectHeapSliceLayout {
                    heap_slice_index,
                    base_resource_descriptor_index,
                    base_sampler_descriptor_index,
                    resource_descriptor_count: descriptor_kinds
                        .len()
                        .saturating_sub(base_resource_descriptor_index),
                    texture_count: pass_descriptors.len(),
                };
                heap_slice_lookup.insert(heap_slice_key, slice);
                slice
            };
            let uniform = effect_uniform.as_ref();
            pass_bindings.push(NativeVulkanSceneEffectResourceHeapPassBinding {
                effect_pass_index,
                object,
                heap_slice_index: slice.heap_slice_index,
                effect_uniform: uniform.map(|binding| binding.key.clone()),
                effect_uniform_buffer_handle: uniform.map(|binding| binding.buffer.as_raw()),
                effect_uniform_device_address: uniform.map(|binding| binding.device_address),
                effect_uniform_record_index: uniform.map(|binding| binding.record_index),
                effect_uniform_bytes: uniform.map(|binding| binding.bytes),
                effect_uniform_payload_hash: uniform.map(|binding| binding.payload_hash),
                shader_mappings: effect_heap_slice_shader_mappings(&texture_set, uniform.is_some()),
                texture_set,
                base_resource_descriptor_index: slice.base_resource_descriptor_index,
                base_resource_heap_offset: 0,
                base_sampler_descriptor_index: slice.base_sampler_descriptor_index,
                base_sampler_heap_offset: None,
                resource_descriptor_count: slice.resource_descriptor_count,
                texture_count: slice.texture_count,
            });
        }

        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: descriptor_kinds,
                sampler_count: sampler_descriptor_count,
                properties: descriptor_heap_properties,
            },
        );
        if !pending_entries.is_empty() && !descriptor_heap_plan.backend_ready {
            return Err(format!(
                "scene effect resource heap requires a ready VK_EXT_descriptor_heap sampled-image resource plan: {:?}",
                descriptor_heap_plan.blocking_reason
            ));
        }
        let entries = finalize_effect_entries(pending_entries, &descriptor_heap_plan)?;
        for binding in &mut pass_bindings {
            binding.base_resource_heap_offset = *descriptor_heap_plan
                .resource_descriptor_offsets
                .get(binding.base_resource_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene effect resource heap pass {} missing base descriptor offset",
                        binding.effect_pass_index
                    )
                })?;
            binding.base_sampler_heap_offset = binding
                .base_sampler_descriptor_index
                .map(|sampler_index| {
                    descriptor_heap_plan
                        .sampler_descriptor_offsets
                        .get(sampler_index)
                        .copied()
                        .ok_or_else(|| {
                            format!(
                                "scene effect resource heap pass {} missing sampler descriptor offset",
                                binding.effect_pass_index
                            )
                        })
                })
                .transpose()?;
        }

        Ok(Self {
            pass_count: descriptors.pass_count,
            pass_binding_count: pass_bindings.len(),
            heap_slice_count: heap_slice_lookup.len(),
            resource_descriptor_count: entries.len(),
            sampler_descriptor_count,
            descriptor_model: "VK_EXT_descriptor_heap",
            entries,
            pass_bindings,
            descriptor_heap_plan,
            command_order: [
                "collect_effect_sampled_texture_descriptors",
                "dedupe_effect_heap_slices",
                "pack_effect_descriptor_heap_slices",
                "bind_effect_heap_slice",
                "record_effect_pass",
            ],
            bindings: descriptor_bindings,
        })
    }
}

fn push_effect_uniform_descriptor(
    descriptor_kinds: &mut Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pending_entries: &mut Vec<PendingEffectResourceHeapEntry>,
    descriptor_bindings: &mut Vec<NativeVulkanSceneEffectResourceHeapDescriptorBinding>,
    heap_slice_index: usize,
    effect_pass_index: usize,
    object: SceneObjectId,
    uniform: &NativeVulkanSceneEffectUniformGpuBufferBinding,
) -> Result<(), String> {
    validate_effect_uniform_binding(effect_pass_index, object, uniform)?;
    let descriptor_index = descriptor_kinds.len();
    descriptor_kinds
        .push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer);
    descriptor_bindings.push(
        NativeVulkanSceneEffectResourceHeapDescriptorBinding::UniformBuffer {
            descriptor_index,
            device_address: uniform.device_address,
            bytes: uniform.bytes,
        },
    );
    pending_entries.push(PendingEffectResourceHeapEntry {
        heap_slice_index,
        descriptor_index,
        descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
        sampler_descriptor_index: None,
        effect_pass_index,
        object,
        role: NativeVulkanSceneEffectResourceHeapEntryRole::WeEffectUniformPayload {
            uniform: uniform.key.clone(),
            buffer_handle: uniform.buffer.as_raw(),
            device_address: uniform.device_address,
            record_index: uniform.record_index,
            bytes: uniform.bytes,
            payload_hash: uniform.payload_hash,
            shader_mapping: "WE effect uniform payload -> effect-heap-slice-offset0".to_owned(),
        },
    });
    Ok(())
}

fn push_effect_texture_descriptor(
    descriptor_kinds: &mut Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pending_entries: &mut Vec<PendingEffectResourceHeapEntry>,
    descriptor_bindings: &mut Vec<NativeVulkanSceneEffectResourceHeapDescriptorBinding>,
    heap_slice_index: usize,
    descriptor: &NativeVulkanSceneEffectTextureDescriptorBinding,
    texture: NativeVulkanSceneEffectResolvedSampledImageBinding,
    sampler_descriptor_index: usize,
) -> Result<(), String> {
    validate_effect_texture_binding(descriptor, texture)?;
    let descriptor_index = descriptor_kinds.len();
    descriptor_kinds.push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage);
    descriptor_bindings.push(
        NativeVulkanSceneEffectResourceHeapDescriptorBinding::SampledImage {
            descriptor_index,
            sampler_descriptor_index,
            source: texture.source,
            image: texture.image,
            view: texture.view,
            sampler: texture.sampler,
            format: texture.format,
            width: texture.width,
            height: texture.height,
            mip_count: texture.mip_count,
        },
    );
    pending_entries.push(PendingEffectResourceHeapEntry {
        heap_slice_index,
        descriptor_index,
        descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
        sampler_descriptor_index: Some(sampler_descriptor_index),
        effect_pass_index: descriptor.effect_pass_index,
        object: descriptor.object,
        role: NativeVulkanSceneEffectResourceHeapEntryRole::WeSampledTexture {
            slot: descriptor.slot,
            source: texture.source,
            image_handle: texture.image.as_raw(),
            view_handle: texture.view.as_raw(),
            sampler_handle: texture.sampler.as_raw(),
            format: format!("{:?}", texture.format),
            width: texture.width,
            height: texture.height,
            mip_count: texture.mip_count,
            shader_mapping: descriptor.shader_mapping.clone(),
        },
    });
    Ok(())
}

fn finalize_effect_entries(
    pending_entries: Vec<PendingEffectResourceHeapEntry>,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
) -> Result<Vec<NativeVulkanSceneEffectResourceHeapEntry>, String> {
    pending_entries
        .into_iter()
        .map(|entry| {
            let resource_heap_offset = *descriptor_heap_plan
                .resource_descriptor_offsets
                .get(entry.descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene effect resource heap descriptor {} missing resource offset",
                        entry.descriptor_index
                    )
                })?;
            let sampler_heap_offset = entry
                .sampler_descriptor_index
                .map(|sampler_index| {
                    descriptor_heap_plan
                        .sampler_descriptor_offsets
                        .get(sampler_index)
                        .copied()
                        .ok_or_else(|| {
                            format!(
                                "scene effect resource heap sampler descriptor {} missing sampler offset",
                                sampler_index
                            )
                        })
                })
                .transpose()?;
            Ok(NativeVulkanSceneEffectResourceHeapEntry {
                heap_slice_index: entry.heap_slice_index,
                descriptor_index: entry.descriptor_index,
                descriptor_kind: entry.descriptor_kind,
                resource_heap_offset,
                sampler_descriptor_index: entry.sampler_descriptor_index,
                sampler_heap_offset,
                effect_pass_index: entry.effect_pass_index,
                object: entry.object,
                role: entry.role,
            })
        })
        .collect()
}

fn effect_uniform_records_by_pass(
    uniform_plan: &SceneEffectUniformFramePlan,
) -> Result<Vec<Option<&SceneIrisEffectUniformRecord>>, String> {
    let mut records = vec![None; uniform_plan.effect_pass_count];
    for record in &uniform_plan.iris_records {
        let slot = records.get_mut(record.effect_pass_index).ok_or_else(|| {
            format!(
                "scene effect uniform record {} pass index {} exceeds effect pass count {}",
                record.record_index, record.effect_pass_index, uniform_plan.effect_pass_count
            )
        })?;
        if slot.replace(record).is_some() {
            return Err(format!(
                "scene effect uniform pass {} has more than one uniform record",
                record.effect_pass_index
            ));
        }
    }
    Ok(records)
}

fn effect_uniform_binding_from_record(
    record: &SceneIrisEffectUniformRecord,
    effect_uniform_buffer: &impl Fn(
        &NativeVulkanSceneEffectUniformKey,
    )
        -> Result<NativeVulkanSceneEffectUniformGpuBufferBinding, String>,
) -> Result<NativeVulkanSceneEffectUniformGpuBufferBinding, String> {
    let key = NativeVulkanSceneEffectUniformKey {
        effect_pass_index: record.effect_pass_index,
        object: record.object,
        shader: record.shader.clone(),
    };
    effect_uniform_buffer(&key)
}

fn effect_heap_pass_object(
    effect_pass_index: usize,
    descriptors: &[&NativeVulkanSceneEffectTextureDescriptorBinding],
    uniform: Option<&NativeVulkanSceneEffectUniformGpuBufferBinding>,
) -> Result<SceneObjectId, String> {
    let texture_object = descriptors.first().map(|descriptor| descriptor.object);
    let uniform_object = uniform.map(|binding| binding.key.object);
    match (texture_object, uniform_object) {
        (Some(texture_object), Some(uniform_object)) if texture_object != uniform_object => {
            Err(format!(
                "scene effect resource heap pass {effect_pass_index} object mismatch between texture {texture_object:?} and uniform {uniform_object:?}"
            ))
        }
        (Some(object), _) | (_, Some(object)) => Ok(object),
        (None, None) => Err(format!(
            "scene effect resource heap pass {effect_pass_index} has neither textures nor uniform"
        )),
    }
}

fn effect_heap_slice_uniform_key(
    binding: &NativeVulkanSceneEffectUniformGpuBufferBinding,
) -> NativeVulkanSceneEffectHeapSliceUniformKey {
    NativeVulkanSceneEffectHeapSliceUniformKey {
        uniform: binding.key.clone(),
        buffer_handle: binding.buffer.as_raw(),
        device_address: binding.device_address,
        bytes: binding.bytes,
        payload_hash: binding.payload_hash,
    }
}

fn validate_effect_uniform_binding(
    effect_pass_index: usize,
    object: SceneObjectId,
    binding: &NativeVulkanSceneEffectUniformGpuBufferBinding,
) -> Result<(), String> {
    if binding.key.effect_pass_index != effect_pass_index {
        return Err(format!(
            "scene effect resource heap uniform resolver returned pass {} for requested pass {}",
            binding.key.effect_pass_index, effect_pass_index
        ));
    }
    if binding.key.object != object {
        return Err(format!(
            "scene effect resource heap uniform resolver returned object {:?} for requested object {:?}",
            binding.key.object, object
        ));
    }
    if binding.buffer.as_raw() == 0 {
        return Err(format!(
            "scene effect resource heap uniform {:?} has null GPU buffer",
            binding.key
        ));
    }
    if binding.device_address == 0 {
        return Err(format!(
            "scene effect resource heap uniform {:?} has zero device address",
            binding.key
        ));
    }
    if binding.bytes == 0 {
        return Err(format!(
            "scene effect resource heap uniform {:?} has zero byte range",
            binding.key
        ));
    }
    Ok(())
}
