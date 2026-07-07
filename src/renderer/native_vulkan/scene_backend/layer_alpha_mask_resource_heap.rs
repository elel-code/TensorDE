//! Descriptor heap slice planning for WE layer alpha-mask draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

pub(in crate::renderer::native_vulkan) mod bind_command;
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
    SceneGraphTarget, SceneObjectId, ScenePuppetId, SceneResourceId,
};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_plan,
};

use super::layer_alpha_mask_executor::{
    NativeVulkanSceneLayerAlphaMaskDescriptorPlan, NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    NativeVulkanSceneLayerAlphaMaskTextureBindPlan, NativeVulkanSceneLayerAlphaMaskTextureBindRole,
};
use super::material_uniforms::{
    NativeVulkanSceneMaterialUniformGpuBufferBinding, NativeVulkanSceneMaterialUniformKey,
};
use super::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use super::texture_images::NativeVulkanSceneTextureImageBinding;
pub(in crate::renderer::native_vulkan) use bind_command::NativeVulkanSceneLayerAlphaMaskResourceHeapBindPlan;
#[cfg(test)]
pub(in crate::renderer::native_vulkan) use key::NativeVulkanSceneLayerAlphaMaskHeapSliceBinding;
pub(in crate::renderer::native_vulkan) use key::NativeVulkanSceneLayerAlphaMaskHeapSliceKey;
use key::{
    alpha_mask_heap_slice_shader_mappings, alpha_mask_slots_by_texture_bind,
    alpha_mask_texture_bind_heap_slice,
};
use resolve::{
    NativeVulkanSceneLayerAlphaMaskResolvedSampledImageBinding,
    resolve_alpha_mask_sampled_image_binding,
};
pub(in crate::renderer::native_vulkan) use store::{
    NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
    NativeVulkanSceneLayerAlphaMaskResourceHeapStore,
    NativeVulkanSceneLayerAlphaMaskResourceHeapSyncAction,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskResourceHeapFramePlan {
    pub heap_bind_count: usize,
    pub heap_slice_count: usize,
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub material_uniform_count: usize,
    pub descriptor_model: &'static str,
    pub material_uniforms: Vec<NativeVulkanSceneLayerAlphaMaskMaterialUniformHeapEntry>,
    pub entries: Vec<NativeVulkanSceneLayerAlphaMaskResourceHeapEntry>,
    pub heap_bindings: Vec<NativeVulkanSceneLayerAlphaMaskResourceHeapBindSlice>,
    pub descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    pub command_order: [&'static str; 5],
    #[serde(skip)]
    pub(super) bindings: Vec<NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskResourceHeapEntry {
    pub heap_bind_index: usize,
    pub heap_slice_index: usize,
    pub descriptor_index: usize,
    pub descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    pub resource_heap_offset: u64,
    pub sampler_descriptor_index: usize,
    pub sampler_heap_offset: u64,
    pub object: SceneObjectId,
    pub puppet: ScenePuppetId,
    pub shader: String,
    pub role: NativeVulkanSceneLayerAlphaMaskTextureBindRole,
    pub slot: u32,
    pub source: NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    pub image_handle: u64,
    pub view_handle: u64,
    pub sampler_handle: u64,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub shader_mapping: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskMaterialUniformHeapEntry
{
    pub heap_bind_index: usize,
    pub heap_slice_index: usize,
    pub descriptor_index: usize,
    pub descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    pub resource_heap_offset: u64,
    pub object: SceneObjectId,
    pub puppet: ScenePuppetId,
    pub shader: String,
    pub role: NativeVulkanSceneLayerAlphaMaskTextureBindRole,
    pub material: NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding,
    pub shader_mapping: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding
{
    pub key: NativeVulkanSceneMaterialUniformKey,
    pub buffer_handle: u64,
    pub device_address: u64,
    pub record_index: usize,
    pub bytes: u64,
    pub payload_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskResourceHeapBindSlice {
    pub heap_bind_index: usize,
    pub object: SceneObjectId,
    pub puppet: ScenePuppetId,
    pub shader: String,
    pub role: NativeVulkanSceneLayerAlphaMaskTextureBindRole,
    pub heap_slice_index: usize,
    pub heap_slice: NativeVulkanSceneLayerAlphaMaskHeapSliceKey,
    pub material: Option<NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding>,
    pub base_resource_descriptor_index: usize,
    pub base_resource_heap_offset: u64,
    pub base_sampler_descriptor_index: usize,
    pub base_sampler_heap_offset: u64,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorBinding {
    UniformBuffer {
        descriptor_index: usize,
        device_address: vk::DeviceAddress,
        bytes: u64,
    },
    SampledImage {
        descriptor_index: usize,
        sampler_descriptor_index: usize,
        source: NativeVulkanSceneLayerAlphaMaskDescriptorSource,
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
struct NativeVulkanSceneLayerAlphaMaskHeapSliceLayout {
    heap_slice_index: usize,
    base_resource_descriptor_index: usize,
    base_sampler_descriptor_index: usize,
    resource_descriptor_count: usize,
    texture_count: usize,
}

#[derive(Debug, Clone)]
struct PendingLayerAlphaMaskResourceHeapEntry {
    heap_bind_index: usize,
    heap_slice_index: usize,
    descriptor_index: usize,
    sampler_descriptor_index: usize,
    object: SceneObjectId,
    puppet: ScenePuppetId,
    shader: String,
    role: NativeVulkanSceneLayerAlphaMaskTextureBindRole,
    slot: u32,
    shader_mapping: String,
    sampled_image: NativeVulkanSceneLayerAlphaMaskResolvedSampledImageBinding,
}

#[derive(Debug, Clone)]
struct PendingLayerAlphaMaskMaterialUniformHeapEntry {
    heap_bind_index: usize,
    heap_slice_index: usize,
    descriptor_index: usize,
    object: SceneObjectId,
    puppet: ScenePuppetId,
    shader: String,
    role: NativeVulkanSceneLayerAlphaMaskTextureBindRole,
    material: NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LayerAlphaMaskHeapSliceDedupeKey {
    heap_slice: NativeVulkanSceneLayerAlphaMaskHeapSliceKey,
    material: Option<NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding>,
}

impl NativeVulkanSceneLayerAlphaMaskResourceHeapFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_descriptors(
        descriptors: &NativeVulkanSceneLayerAlphaMaskDescriptorPlan,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        material_uniform_buffer: impl Fn(
            &NativeVulkanSceneMaterialUniformKey,
        ) -> Result<
            NativeVulkanSceneMaterialUniformGpuBufferBinding,
            String,
        >,
        texture_image_binding: impl Fn(
            SceneResourceId,
        ) -> Result<NativeVulkanSceneTextureImageBinding, String>,
        target_image_binding: impl Fn(
            SceneGraphTarget,
        )
            -> Result<NativeVulkanSceneOffscreenTargetBinding, String>,
    ) -> Result<Self, String> {
        let slots_by_heap_bind = alpha_mask_slots_by_texture_bind(descriptors)?;
        let mut heap_slice_lookup = BTreeMap::<
            LayerAlphaMaskHeapSliceDedupeKey,
            NativeVulkanSceneLayerAlphaMaskHeapSliceLayout,
        >::new();
        let mut descriptor_kinds = Vec::new();
        let mut pending_material_uniforms = Vec::new();
        let mut pending_entries = Vec::new();
        let mut descriptor_bindings = Vec::new();
        let mut heap_bindings = Vec::new();
        let mut sampler_descriptor_count = 0usize;

        for (heap_bind_index, texture_bind) in descriptors.entries.iter().enumerate() {
            let heap_slice = alpha_mask_texture_bind_heap_slice(texture_bind)?;
            let material = layer_alpha_mask_generated_material_binding(
                texture_bind,
                &material_uniform_buffer,
            )?;
            let dedupe_key = LayerAlphaMaskHeapSliceDedupeKey {
                heap_slice: heap_slice.clone(),
                material: material.clone(),
            };
            let slice = if let Some(slice) = heap_slice_lookup.get(&dedupe_key).copied() {
                slice
            } else {
                let heap_slice_index = heap_slice_lookup.len();
                let base_resource_descriptor_index = descriptor_kinds.len();
                let base_sampler_descriptor_index = sampler_descriptor_count;
                if let Some(material) = &material {
                    push_layer_alpha_mask_material_descriptor(
                        &mut descriptor_kinds,
                        &mut pending_material_uniforms,
                        &mut descriptor_bindings,
                        heap_slice_index,
                        heap_bind_index,
                        texture_bind,
                        material.clone(),
                    );
                }
                for slot in &slots_by_heap_bind[heap_bind_index] {
                    let sampled_image = resolve_alpha_mask_sampled_image_binding(
                        slot.source,
                        &texture_image_binding,
                        &target_image_binding,
                    )?;
                    push_layer_alpha_mask_texture_descriptor(
                        &mut descriptor_kinds,
                        &mut pending_entries,
                        &mut descriptor_bindings,
                        heap_slice_index,
                        heap_bind_index,
                        texture_bind,
                        slot.slot,
                        &slot.shader_mapping,
                        sampled_image,
                        sampler_descriptor_count,
                    );
                    sampler_descriptor_count = sampler_descriptor_count.saturating_add(1);
                }
                let slice = NativeVulkanSceneLayerAlphaMaskHeapSliceLayout {
                    heap_slice_index,
                    base_resource_descriptor_index,
                    base_sampler_descriptor_index,
                    resource_descriptor_count: descriptor_kinds
                        .len()
                        .saturating_sub(base_resource_descriptor_index),
                    texture_count: texture_bind.slots.len(),
                };
                heap_slice_lookup.insert(dedupe_key, slice);
                slice
            };
            heap_bindings.push(NativeVulkanSceneLayerAlphaMaskResourceHeapBindSlice {
                heap_bind_index,
                object: texture_bind.object,
                puppet: texture_bind.puppet,
                shader: texture_bind.shader.to_owned(),
                role: texture_bind.role,
                heap_slice_index: slice.heap_slice_index,
                shader_mappings: alpha_mask_heap_slice_shader_mappings(
                    &heap_slice,
                    material.is_some(),
                ),
                heap_slice,
                material,
                base_resource_descriptor_index: slice.base_resource_descriptor_index,
                base_resource_heap_offset: 0,
                base_sampler_descriptor_index: slice.base_sampler_descriptor_index,
                base_sampler_heap_offset: 0,
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
        if !descriptor_heap_plan.resource_descriptor_offsets.is_empty()
            && !descriptor_heap_plan.backend_ready
        {
            return Err(format!(
                "scene layer alpha-mask resource heap requires a ready VK_EXT_descriptor_heap mixed resource plan: {:?}",
                descriptor_heap_plan.blocking_reason
            ));
        }

        let material_uniforms = finalize_layer_alpha_mask_material_uniform_entries(
            pending_material_uniforms,
            &descriptor_heap_plan,
        )?;
        let entries = finalize_layer_alpha_mask_entries(pending_entries, &descriptor_heap_plan)?;
        for binding in &mut heap_bindings {
            binding.base_resource_heap_offset = *descriptor_heap_plan
                .resource_descriptor_offsets
                .get(binding.base_resource_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask heap bind {} missing base resource descriptor offset",
                        binding.heap_bind_index
                    )
                })?;
            binding.base_sampler_heap_offset = *descriptor_heap_plan
                .sampler_descriptor_offsets
                .get(binding.base_sampler_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask heap bind {} missing base sampler descriptor offset",
                        binding.heap_bind_index
                    )
                })?;
        }

        Ok(Self {
            heap_bind_count: descriptors.heap_bind_count,
            heap_slice_count: heap_slice_lookup.len(),
            resource_descriptor_count: descriptor_heap_plan.resource_descriptor_offsets.len(),
            sampler_descriptor_count,
            material_uniform_count: material_uniforms.len(),
            descriptor_model: "VK_EXT_descriptor_heap",
            material_uniforms,
            entries,
            heap_bindings,
            descriptor_heap_plan,
            command_order: [
                "collect_alpha_mask_sampled_texture_descriptors",
                "dedupe_alpha_mask_heap_slices",
                "pack_alpha_mask_descriptor_heap_slices",
                "bind_alpha_mask_heap_slice",
                "record_tokenized_alpha_mask_draws",
            ],
            bindings: descriptor_bindings,
        })
    }
}

fn layer_alpha_mask_generated_material_binding(
    texture_bind: &NativeVulkanSceneLayerAlphaMaskTextureBindPlan,
    material_uniform_buffer: &impl Fn(
        &NativeVulkanSceneMaterialUniformKey,
    ) -> Result<
        NativeVulkanSceneMaterialUniformGpuBufferBinding,
        String,
    >,
) -> Result<Option<NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding>, String> {
    if texture_bind.role != NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget
    {
        return Ok(None);
    }
    let material_key = NativeVulkanSceneMaterialUniformKey {
        object: texture_bind.object,
        shader: texture_bind.shader.to_owned(),
    };
    let material = material_uniform_buffer(&material_key)?;
    validate_alpha_mask_generated_material_binding(&material_key, &material)?;
    Ok(Some(
        NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding {
            key: material.key,
            buffer_handle: material.buffer.as_raw(),
            device_address: material.device_address,
            record_index: material.record_index,
            bytes: material.bytes,
            payload_hash: material.payload_hash,
        },
    ))
}

fn validate_alpha_mask_generated_material_binding(
    requested: &NativeVulkanSceneMaterialUniformKey,
    binding: &NativeVulkanSceneMaterialUniformGpuBufferBinding,
) -> Result<(), String> {
    if &binding.key != requested {
        return Err(format!(
            "scene layer alpha-mask generated material uniform resolver returned {:?} for requested {:?}",
            binding.key, requested
        ));
    }
    if binding.buffer.as_raw() == 0 {
        return Err(format!(
            "scene layer alpha-mask generated material uniform {:?} has null GPU buffer",
            requested
        ));
    }
    if binding.device_address == 0 {
        return Err(format!(
            "scene layer alpha-mask generated material uniform {:?} has zero device address",
            requested
        ));
    }
    if binding.bytes == 0 {
        return Err(format!(
            "scene layer alpha-mask generated material uniform {:?} has zero byte range",
            requested
        ));
    }
    Ok(())
}

fn push_layer_alpha_mask_material_descriptor(
    descriptor_kinds: &mut Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pending_material_uniforms: &mut Vec<PendingLayerAlphaMaskMaterialUniformHeapEntry>,
    descriptor_bindings: &mut Vec<NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorBinding>,
    heap_slice_index: usize,
    heap_bind_index: usize,
    texture_bind: &NativeVulkanSceneLayerAlphaMaskTextureBindPlan,
    material: NativeVulkanSceneLayerAlphaMaskMaterialUniformBinding,
) {
    let descriptor_index = descriptor_kinds.len();
    descriptor_kinds
        .push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer);
    descriptor_bindings.push(
        NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorBinding::UniformBuffer {
            descriptor_index,
            device_address: material.device_address,
            bytes: material.bytes,
        },
    );
    pending_material_uniforms.push(PendingLayerAlphaMaskMaterialUniformHeapEntry {
        heap_bind_index,
        heap_slice_index,
        descriptor_index,
        object: texture_bind.object,
        puppet: texture_bind.puppet,
        shader: texture_bind.shader.to_owned(),
        role: texture_bind.role,
        material,
    });
}

#[allow(clippy::too_many_arguments)]
fn push_layer_alpha_mask_texture_descriptor(
    descriptor_kinds: &mut Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pending_entries: &mut Vec<PendingLayerAlphaMaskResourceHeapEntry>,
    descriptor_bindings: &mut Vec<NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorBinding>,
    heap_slice_index: usize,
    heap_bind_index: usize,
    texture_bind: &NativeVulkanSceneLayerAlphaMaskTextureBindPlan,
    slot: u32,
    shader_mapping: &str,
    texture: NativeVulkanSceneLayerAlphaMaskResolvedSampledImageBinding,
    sampler_descriptor_index: usize,
) {
    let descriptor_index = descriptor_kinds.len();
    descriptor_kinds.push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage);
    descriptor_bindings.push(
        NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorBinding::SampledImage {
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
    pending_entries.push(PendingLayerAlphaMaskResourceHeapEntry {
        heap_bind_index,
        heap_slice_index,
        descriptor_index,
        sampler_descriptor_index,
        object: texture_bind.object,
        puppet: texture_bind.puppet,
        shader: texture_bind.shader.to_owned(),
        role: texture_bind.role,
        slot,
        shader_mapping: shader_mapping.to_owned(),
        sampled_image: texture,
    });
}

fn finalize_layer_alpha_mask_entries(
    pending_entries: Vec<PendingLayerAlphaMaskResourceHeapEntry>,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
) -> Result<Vec<NativeVulkanSceneLayerAlphaMaskResourceHeapEntry>, String> {
    pending_entries
        .into_iter()
        .map(|entry| {
            let resource_heap_offset = *descriptor_heap_plan
                .resource_descriptor_offsets
                .get(entry.descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask resource heap descriptor {} missing resource offset",
                        entry.descriptor_index
                    )
                })?;
            let sampler_heap_offset = *descriptor_heap_plan
                .sampler_descriptor_offsets
                .get(entry.sampler_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask resource heap sampler descriptor {} missing sampler offset",
                        entry.sampler_descriptor_index
                    )
                })?;
            Ok(NativeVulkanSceneLayerAlphaMaskResourceHeapEntry {
                heap_bind_index: entry.heap_bind_index,
                heap_slice_index: entry.heap_slice_index,
                descriptor_index: entry.descriptor_index,
                descriptor_kind:
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                resource_heap_offset,
                sampler_descriptor_index: entry.sampler_descriptor_index,
                sampler_heap_offset,
                object: entry.object,
                puppet: entry.puppet,
                shader: entry.shader,
                role: entry.role,
                slot: entry.slot,
                source: entry.sampled_image.source,
                image_handle: entry.sampled_image.image.as_raw(),
                view_handle: entry.sampled_image.view.as_raw(),
                sampler_handle: entry.sampled_image.sampler.as_raw(),
                format: format!("{:?}", entry.sampled_image.format),
                width: entry.sampled_image.width,
                height: entry.sampled_image.height,
                mip_count: entry.sampled_image.mip_count,
                shader_mapping: entry.shader_mapping,
            })
        })
        .collect()
}

fn finalize_layer_alpha_mask_material_uniform_entries(
    pending_entries: Vec<PendingLayerAlphaMaskMaterialUniformHeapEntry>,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
) -> Result<Vec<NativeVulkanSceneLayerAlphaMaskMaterialUniformHeapEntry>, String> {
    pending_entries
        .into_iter()
        .map(|entry| {
            let resource_heap_offset = *descriptor_heap_plan
                .resource_descriptor_offsets
                .get(entry.descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask material uniform descriptor {} missing resource offset",
                        entry.descriptor_index
                    )
                })?;
            Ok(NativeVulkanSceneLayerAlphaMaskMaterialUniformHeapEntry {
                heap_bind_index: entry.heap_bind_index,
                heap_slice_index: entry.heap_slice_index,
                descriptor_index: entry.descriptor_index,
                descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                resource_heap_offset,
                object: entry.object,
                puppet: entry.puppet,
                shader: entry.shader,
                role: entry.role,
                material: entry.material,
                shader_mapping: "WE PSSetConstantBuffers(slot=3) -> alpha-mask-heap-slice-offset0",
            })
        })
        .collect()
}
