//! Effect pass descriptor-heap resource-set planning.
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

mod key;
mod resolve;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde::Serialize;
use vulkanalia::vk::{self, Handle};

use crate::engine::scene_engine::{SceneGraphTarget, SceneObjectId, SceneResourceId};
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
use super::offscreen_targets::NativeVulkanSceneOffscreenTargetBinding;
use super::texture_descriptors::NativeVulkanSceneTextureDescriptorSource;
use super::texture_images::NativeVulkanSceneTextureImageBinding;
#[allow(unused_imports)]
pub(in crate::renderer::native_vulkan) use key::NativeVulkanSceneEffectTextureSetBinding;
pub(in crate::renderer::native_vulkan) use key::NativeVulkanSceneEffectTextureSetKey;
use key::{
    effect_resource_set_shader_mappings, effect_texture_descriptors_by_pass, effect_texture_set_key,
};
use resolve::{
    NativeVulkanSceneEffectResolvedSampledImageBinding, resolve_effect_sampled_image_binding,
    validate_effect_texture_binding,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectResourceHeapFramePlan {
    pub pass_count: usize,
    pub pass_binding_count: usize,
    pub resource_set_count: usize,
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
    pub resource_set_index: usize,
    pub descriptor_index: usize,
    pub descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    pub resource_heap_offset: u64,
    pub sampler_descriptor_index: usize,
    pub sampler_heap_offset: u64,
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub slot: u32,
    pub source: NativeVulkanSceneTextureDescriptorSource,
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
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectResourceHeapPassBinding {
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub resource_set_index: usize,
    pub texture_set: NativeVulkanSceneEffectTextureSetKey,
    pub base_resource_descriptor_index: usize,
    pub base_resource_heap_offset: u64,
    pub base_sampler_descriptor_index: usize,
    pub base_sampler_heap_offset: u64,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanSceneEffectResourceHeapDescriptorBinding {
    pub descriptor_index: usize,
    pub sampler_descriptor_index: usize,
    pub source: NativeVulkanSceneTextureDescriptorSource,
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanSceneEffectResourceSetSlice {
    resource_set_index: usize,
    base_resource_descriptor_index: usize,
    base_sampler_descriptor_index: usize,
    resource_descriptor_count: usize,
    texture_count: usize,
}

#[derive(Debug, Clone)]
struct PendingEffectResourceHeapEntry {
    resource_set_index: usize,
    descriptor_index: usize,
    sampler_descriptor_index: usize,
    effect_pass_index: usize,
    object: SceneObjectId,
    slot: u32,
    shader_mapping: String,
    sampled_image: NativeVulkanSceneEffectResolvedSampledImageBinding,
}

impl NativeVulkanSceneEffectResourceHeapFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_descriptors(
        descriptors: &NativeVulkanSceneEffectTextureDescriptorFramePlan,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        texture_image_binding: impl Fn(
            SceneResourceId,
        ) -> Result<NativeVulkanSceneTextureImageBinding, String>,
        target_image_binding: impl Fn(
            SceneGraphTarget,
        )
            -> Result<NativeVulkanSceneOffscreenTargetBinding, String>,
    ) -> Result<Self, String> {
        let descriptors_by_pass = effect_texture_descriptors_by_pass(descriptors)?;
        let mut resource_set_to_slice = BTreeMap::<
            NativeVulkanSceneEffectTextureSetKey,
            NativeVulkanSceneEffectResourceSetSlice,
        >::new();
        let mut descriptor_kinds = Vec::new();
        let mut pending_entries = Vec::new();
        let mut descriptor_bindings = Vec::new();
        let mut pass_bindings = Vec::new();
        let mut sampler_descriptor_count = 0usize;

        for (effect_pass_index, pass_descriptors) in descriptors_by_pass.iter().enumerate() {
            if pass_descriptors.is_empty() {
                continue;
            }
            let object = pass_descriptors[0].object;
            let texture_set = effect_texture_set_key(pass_descriptors);
            let slice = if let Some(slice) = resource_set_to_slice.get(&texture_set).copied() {
                slice
            } else {
                let resource_set_index = resource_set_to_slice.len();
                let base_resource_descriptor_index = descriptor_kinds.len();
                let base_sampler_descriptor_index = sampler_descriptor_count;
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
                        resource_set_index,
                        descriptor,
                        sampled_image,
                        sampler_descriptor_count,
                    )?;
                    sampler_descriptor_count = sampler_descriptor_count.saturating_add(1);
                }
                let slice = NativeVulkanSceneEffectResourceSetSlice {
                    resource_set_index,
                    base_resource_descriptor_index,
                    base_sampler_descriptor_index,
                    resource_descriptor_count: descriptor_kinds
                        .len()
                        .saturating_sub(base_resource_descriptor_index),
                    texture_count: pass_descriptors.len(),
                };
                resource_set_to_slice.insert(texture_set.clone(), slice);
                slice
            };
            pass_bindings.push(NativeVulkanSceneEffectResourceHeapPassBinding {
                effect_pass_index,
                object,
                resource_set_index: slice.resource_set_index,
                shader_mappings: effect_resource_set_shader_mappings(&texture_set),
                texture_set,
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
            binding.base_sampler_heap_offset = *descriptor_heap_plan
                .sampler_descriptor_offsets
                .get(binding.base_sampler_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene effect resource heap pass {} missing sampler descriptor offset",
                        binding.effect_pass_index
                    )
                })?;
        }

        Ok(Self {
            pass_count: descriptors.pass_count,
            pass_binding_count: pass_bindings.len(),
            resource_set_count: resource_set_to_slice.len(),
            resource_descriptor_count: entries.len(),
            sampler_descriptor_count,
            descriptor_model: "VK_EXT_descriptor_heap",
            entries,
            pass_bindings,
            descriptor_heap_plan,
            command_order: [
                "collect_effect_sampled_texture_descriptors",
                "dedupe_effect_resource_sets",
                "pack_effect_descriptor_heap_slices",
                "bind_effect_resource_set_slice",
                "record_effect_pass",
            ],
            bindings: descriptor_bindings,
        })
    }
}

fn push_effect_texture_descriptor(
    descriptor_kinds: &mut Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pending_entries: &mut Vec<PendingEffectResourceHeapEntry>,
    descriptor_bindings: &mut Vec<NativeVulkanSceneEffectResourceHeapDescriptorBinding>,
    resource_set_index: usize,
    descriptor: &NativeVulkanSceneEffectTextureDescriptorBinding,
    texture: NativeVulkanSceneEffectResolvedSampledImageBinding,
    sampler_descriptor_index: usize,
) -> Result<(), String> {
    validate_effect_texture_binding(descriptor, texture)?;
    let descriptor_index = descriptor_kinds.len();
    descriptor_kinds.push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage);
    descriptor_bindings.push(NativeVulkanSceneEffectResourceHeapDescriptorBinding {
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
    });
    pending_entries.push(PendingEffectResourceHeapEntry {
        resource_set_index,
        descriptor_index,
        sampler_descriptor_index,
        effect_pass_index: descriptor.effect_pass_index,
        object: descriptor.object,
        slot: descriptor.slot,
        shader_mapping: descriptor.shader_mapping.clone(),
        sampled_image: texture,
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
            let sampler_heap_offset = *descriptor_heap_plan
                .sampler_descriptor_offsets
                .get(entry.sampler_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene effect resource heap sampler descriptor {} missing sampler offset",
                        entry.sampler_descriptor_index
                    )
                })?;
            Ok(NativeVulkanSceneEffectResourceHeapEntry {
                resource_set_index: entry.resource_set_index,
                descriptor_index: entry.descriptor_index,
                descriptor_kind:
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                resource_heap_offset,
                sampler_descriptor_index: entry.sampler_descriptor_index,
                sampler_heap_offset,
                effect_pass_index: entry.effect_pass_index,
                object: entry.object,
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
