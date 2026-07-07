//! Descriptor-heap resource-set planning for WE layer alpha-mask draws.
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
    NativeVulkanSceneLayerAlphaMaskDescriptorPlan,
    NativeVulkanSceneLayerAlphaMaskDescriptorSetPlan,
    NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
    NativeVulkanSceneLayerAlphaMaskDescriptorSource,
};
use super::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use super::texture_images::NativeVulkanSceneTextureImageBinding;
#[cfg(test)]
pub(in crate::renderer::native_vulkan) use key::NativeVulkanSceneLayerAlphaMaskResourceSetBinding;
pub(in crate::renderer::native_vulkan) use key::NativeVulkanSceneLayerAlphaMaskResourceSetKey;
use key::{
    alpha_mask_descriptor_set_key, alpha_mask_resource_set_shader_mappings,
    alpha_mask_slots_by_descriptor_set,
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
    pub descriptor_set_count: usize,
    pub resource_set_count: usize,
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub descriptor_model: &'static str,
    pub entries: Vec<NativeVulkanSceneLayerAlphaMaskResourceHeapEntry>,
    pub descriptor_set_bindings:
        Vec<NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorSetBinding>,
    pub descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    pub command_order: [&'static str; 5],
    #[serde(skip)]
    pub(super) bindings: Vec<NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskResourceHeapEntry {
    pub descriptor_set_index: usize,
    pub resource_set_index: usize,
    pub descriptor_index: usize,
    pub descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    pub resource_heap_offset: u64,
    pub sampler_descriptor_index: usize,
    pub sampler_heap_offset: u64,
    pub object: SceneObjectId,
    pub puppet: ScenePuppetId,
    pub shader: String,
    pub role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
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
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorSetBinding
{
    pub descriptor_set_index: usize,
    pub object: SceneObjectId,
    pub puppet: ScenePuppetId,
    pub shader: String,
    pub role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
    pub resource_set_index: usize,
    pub resource_set: NativeVulkanSceneLayerAlphaMaskResourceSetKey,
    pub base_resource_descriptor_index: usize,
    pub base_resource_heap_offset: u64,
    pub base_sampler_descriptor_index: usize,
    pub base_sampler_heap_offset: u64,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorBinding {
    pub descriptor_index: usize,
    pub sampler_descriptor_index: usize,
    pub source: NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanSceneLayerAlphaMaskResourceSetSlice {
    resource_set_index: usize,
    base_resource_descriptor_index: usize,
    base_sampler_descriptor_index: usize,
    resource_descriptor_count: usize,
    texture_count: usize,
}

#[derive(Debug, Clone)]
struct PendingLayerAlphaMaskResourceHeapEntry {
    descriptor_set_index: usize,
    resource_set_index: usize,
    descriptor_index: usize,
    sampler_descriptor_index: usize,
    object: SceneObjectId,
    puppet: ScenePuppetId,
    shader: String,
    role: NativeVulkanSceneLayerAlphaMaskDescriptorSetRole,
    slot: u32,
    shader_mapping: String,
    sampled_image: NativeVulkanSceneLayerAlphaMaskResolvedSampledImageBinding,
}

impl NativeVulkanSceneLayerAlphaMaskResourceHeapFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_descriptors(
        descriptors: &NativeVulkanSceneLayerAlphaMaskDescriptorPlan,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        texture_image_binding: impl Fn(
            SceneResourceId,
        ) -> Result<NativeVulkanSceneTextureImageBinding, String>,
        target_image_binding: impl Fn(
            SceneGraphTarget,
        )
            -> Result<NativeVulkanSceneOffscreenTargetBinding, String>,
    ) -> Result<Self, String> {
        let slots_by_set = alpha_mask_slots_by_descriptor_set(descriptors)?;
        let mut resource_set_to_slice = BTreeMap::<
            NativeVulkanSceneLayerAlphaMaskResourceSetKey,
            NativeVulkanSceneLayerAlphaMaskResourceSetSlice,
        >::new();
        let mut descriptor_kinds = Vec::new();
        let mut pending_entries = Vec::new();
        let mut descriptor_bindings = Vec::new();
        let mut descriptor_set_bindings = Vec::new();
        let mut sampler_descriptor_count = 0usize;

        for (descriptor_set_index, descriptor_set) in descriptors.entries.iter().enumerate() {
            let resource_set = alpha_mask_descriptor_set_key(descriptor_set)?;
            let slice = if let Some(slice) = resource_set_to_slice.get(&resource_set).copied() {
                slice
            } else {
                let resource_set_index = resource_set_to_slice.len();
                let base_resource_descriptor_index = descriptor_kinds.len();
                let base_sampler_descriptor_index = sampler_descriptor_count;
                for slot in &slots_by_set[descriptor_set_index] {
                    let sampled_image = resolve_alpha_mask_sampled_image_binding(
                        slot.source,
                        &texture_image_binding,
                        &target_image_binding,
                    )?;
                    push_layer_alpha_mask_texture_descriptor(
                        &mut descriptor_kinds,
                        &mut pending_entries,
                        &mut descriptor_bindings,
                        resource_set_index,
                        descriptor_set_index,
                        descriptor_set,
                        slot.slot,
                        &slot.shader_mapping,
                        sampled_image,
                        sampler_descriptor_count,
                    );
                    sampler_descriptor_count = sampler_descriptor_count.saturating_add(1);
                }
                let slice = NativeVulkanSceneLayerAlphaMaskResourceSetSlice {
                    resource_set_index,
                    base_resource_descriptor_index,
                    base_sampler_descriptor_index,
                    resource_descriptor_count: descriptor_kinds
                        .len()
                        .saturating_sub(base_resource_descriptor_index),
                    texture_count: descriptor_set.slots.len(),
                };
                resource_set_to_slice.insert(resource_set.clone(), slice);
                slice
            };
            descriptor_set_bindings.push(
                NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorSetBinding {
                    descriptor_set_index,
                    object: descriptor_set.object,
                    puppet: descriptor_set.puppet,
                    shader: descriptor_set.shader.to_owned(),
                    role: descriptor_set.role,
                    resource_set_index: slice.resource_set_index,
                    shader_mappings: alpha_mask_resource_set_shader_mappings(&resource_set),
                    resource_set,
                    base_resource_descriptor_index: slice.base_resource_descriptor_index,
                    base_resource_heap_offset: 0,
                    base_sampler_descriptor_index: slice.base_sampler_descriptor_index,
                    base_sampler_heap_offset: 0,
                    resource_descriptor_count: slice.resource_descriptor_count,
                    texture_count: slice.texture_count,
                },
            );
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
                "scene layer alpha-mask resource heap requires a ready VK_EXT_descriptor_heap sampled-image resource plan: {:?}",
                descriptor_heap_plan.blocking_reason
            ));
        }

        let entries = finalize_layer_alpha_mask_entries(pending_entries, &descriptor_heap_plan)?;
        for binding in &mut descriptor_set_bindings {
            binding.base_resource_heap_offset = *descriptor_heap_plan
                .resource_descriptor_offsets
                .get(binding.base_resource_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask descriptor set {} missing base resource descriptor offset",
                        binding.descriptor_set_index
                    )
                })?;
            binding.base_sampler_heap_offset = *descriptor_heap_plan
                .sampler_descriptor_offsets
                .get(binding.base_sampler_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask descriptor set {} missing base sampler descriptor offset",
                        binding.descriptor_set_index
                    )
                })?;
        }

        Ok(Self {
            descriptor_set_count: descriptors.descriptor_set_count,
            resource_set_count: resource_set_to_slice.len(),
            resource_descriptor_count: entries.len(),
            sampler_descriptor_count,
            descriptor_model: "VK_EXT_descriptor_heap",
            entries,
            descriptor_set_bindings,
            descriptor_heap_plan,
            command_order: [
                "collect_alpha_mask_sampled_texture_descriptors",
                "dedupe_alpha_mask_resource_sets",
                "pack_alpha_mask_descriptor_heap_slices",
                "bind_alpha_mask_resource_set_slice",
                "record_tokenized_alpha_mask_draws",
            ],
            bindings: descriptor_bindings,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn push_layer_alpha_mask_texture_descriptor(
    descriptor_kinds: &mut Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pending_entries: &mut Vec<PendingLayerAlphaMaskResourceHeapEntry>,
    descriptor_bindings: &mut Vec<NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorBinding>,
    resource_set_index: usize,
    descriptor_set_index: usize,
    descriptor_set: &NativeVulkanSceneLayerAlphaMaskDescriptorSetPlan,
    slot: u32,
    shader_mapping: &str,
    texture: NativeVulkanSceneLayerAlphaMaskResolvedSampledImageBinding,
    sampler_descriptor_index: usize,
) {
    let descriptor_index = descriptor_kinds.len();
    descriptor_kinds.push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage);
    descriptor_bindings.push(
        NativeVulkanSceneLayerAlphaMaskResourceHeapDescriptorBinding {
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
        descriptor_set_index,
        resource_set_index,
        descriptor_index,
        sampler_descriptor_index,
        object: descriptor_set.object,
        puppet: descriptor_set.puppet,
        shader: descriptor_set.shader.to_owned(),
        role: descriptor_set.role,
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
                descriptor_set_index: entry.descriptor_set_index,
                resource_set_index: entry.resource_set_index,
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
