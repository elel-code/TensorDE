//! Draw resource-set descriptor heap planning for WE scene draws.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/shaders/genericimage4.frag`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use std::collections::BTreeMap;

use serde::Serialize;
use vulkanalia::vk::Handle;

use crate::engine::scene_engine::{SceneGraph, SceneObjectId, SceneResourceId};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_plan,
};

use super::super::material_uniforms::{
    NativeVulkanSceneMaterialUniformGpuBufferBinding, NativeVulkanSceneMaterialUniformKey,
};
use super::super::texture_descriptors::{
    NativeVulkanSceneTextureDescriptorBinding, NativeVulkanSceneTextureDescriptorFramePlan,
};
use super::super::texture_heap::texture_set::{
    NativeVulkanSceneTextureSetKey, scene_mesh_draw_texture_set_key,
};

const WE_GENERICIMAGE4_PS_MATERIAL_CONSTANT_BUFFER_SLOT: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneResourceHeapFramePlan {
    pub draw_count: usize,
    pub draw_binding_count: usize,
    pub resource_set_count: usize,
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub descriptor_model: &'static str,
    pub entries: Vec<NativeVulkanSceneResourceHeapEntry>,
    pub draw_bindings: Vec<NativeVulkanSceneResourceHeapDrawBinding>,
    pub descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneResourceHeapEntry {
    pub resource_set_index: usize,
    pub descriptor_index: usize,
    pub descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    pub resource_heap_offset: u64,
    pub sampler_descriptor_index: Option<usize>,
    pub sampler_heap_offset: Option<u64>,
    pub draw_index: usize,
    pub object: SceneObjectId,
    pub role: NativeVulkanSceneResourceHeapEntryRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneResourceHeapEntryRole {
    WePsMaterialConstantsSlot3 {
        material: NativeVulkanSceneMaterialUniformKey,
        buffer_handle: u64,
        device_address: u64,
        record_index: usize,
        bytes: u64,
        payload_hash: u64,
        shader_mapping: &'static str,
    },
    WeSampledTexture {
        resource: SceneResourceId,
        slot: u32,
        shader_mapping: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneResourceHeapDrawBinding {
    pub draw_index: usize,
    pub object: SceneObjectId,
    pub resource_set_index: usize,
    pub material: NativeVulkanSceneMaterialUniformKey,
    pub material_buffer_handle: u64,
    pub material_device_address: u64,
    pub material_record_index: usize,
    pub material_bytes: u64,
    pub material_payload_hash: u64,
    pub texture_set: NativeVulkanSceneTextureSetKey,
    pub base_resource_descriptor_index: usize,
    pub base_resource_heap_offset: u64,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NativeVulkanSceneResourceSetKey {
    material: NativeVulkanSceneMaterialUniformKey,
    material_buffer_handle: u64,
    material_device_address: u64,
    material_bytes: u64,
    material_payload_hash: u64,
    texture_set: NativeVulkanSceneTextureSetKey,
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanSceneResourceSetSlice {
    resource_set_index: usize,
    base_resource_descriptor_index: usize,
    resource_descriptor_count: usize,
}

#[derive(Debug, Clone)]
struct PendingResourceHeapEntry {
    resource_set_index: usize,
    descriptor_index: usize,
    descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    sampler_descriptor_index: Option<usize>,
    draw_index: usize,
    object: SceneObjectId,
    role: NativeVulkanSceneResourceHeapEntryRole,
}

impl NativeVulkanSceneResourceHeapFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_graph(
        graph: &SceneGraph,
        texture_descriptors: &NativeVulkanSceneTextureDescriptorFramePlan,
        descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        material_uniform_buffer: impl Fn(
            &NativeVulkanSceneMaterialUniformKey,
        ) -> Result<
            NativeVulkanSceneMaterialUniformGpuBufferBinding,
            String,
        >,
    ) -> Result<Self, String> {
        let textures_by_draw = texture_descriptors_by_draw(texture_descriptors)?;
        let mut resource_set_to_slice =
            BTreeMap::<NativeVulkanSceneResourceSetKey, NativeVulkanSceneResourceSetSlice>::new();
        let mut descriptor_kinds = Vec::new();
        let mut pending_entries = Vec::new();
        let mut draw_bindings = Vec::with_capacity(texture_descriptors.draw_count);
        let mut draw_index = 0usize;
        let mut sampler_descriptor_count = 0usize;

        for pass in &graph.passes {
            for draw in &pass.draws {
                let material_key = NativeVulkanSceneMaterialUniformKey {
                    object: draw.object,
                    shader: draw.material.shader.clone(),
                };
                let material = material_uniform_buffer(&material_key)?;
                validate_material_binding(&material_key, &material)?;
                let texture_set = scene_mesh_draw_texture_set_key(draw)?;
                let resource_set_key = NativeVulkanSceneResourceSetKey {
                    material: material_key.clone(),
                    material_buffer_handle: material.buffer.as_raw(),
                    material_device_address: material.device_address,
                    material_bytes: material.bytes,
                    material_payload_hash: material.payload_hash,
                    texture_set: texture_set.clone(),
                };
                let slice = if let Some(slice) =
                    resource_set_to_slice.get(&resource_set_key).copied()
                {
                    slice
                } else {
                    let resource_set_index = resource_set_to_slice.len();
                    let base_resource_descriptor_index = descriptor_kinds.len();
                    push_material_descriptor(
                        &mut descriptor_kinds,
                        &mut pending_entries,
                        resource_set_index,
                        draw_index,
                        draw.object,
                        &material,
                    );
                    for descriptor in textures_by_draw
                            .get(draw_index)
                            .ok_or_else(|| {
                                format!(
                                    "scene resource heap draw index {} exceeds texture descriptor draw count {}",
                                    draw_index, texture_descriptors.draw_count
                                )
                            })?
                            .iter()
                        {
                            push_texture_descriptor(
                                &mut descriptor_kinds,
                                &mut pending_entries,
                                resource_set_index,
                                draw_index,
                                *descriptor,
                                sampler_descriptor_count,
                            );
                            sampler_descriptor_count = sampler_descriptor_count.saturating_add(1);
                        }
                    let slice = NativeVulkanSceneResourceSetSlice {
                        resource_set_index,
                        base_resource_descriptor_index,
                        resource_descriptor_count: descriptor_kinds
                            .len()
                            .saturating_sub(base_resource_descriptor_index),
                    };
                    resource_set_to_slice.insert(resource_set_key.clone(), slice);
                    slice
                };
                draw_bindings.push(NativeVulkanSceneResourceHeapDrawBinding {
                    draw_index,
                    object: draw.object,
                    resource_set_index: slice.resource_set_index,
                    material: material_key,
                    material_buffer_handle: material.buffer.as_raw(),
                    material_device_address: material.device_address,
                    material_record_index: material.record_index,
                    material_bytes: material.bytes,
                    material_payload_hash: material.payload_hash,
                    texture_set,
                    base_resource_descriptor_index: slice.base_resource_descriptor_index,
                    base_resource_heap_offset: 0,
                    resource_descriptor_count: slice.resource_descriptor_count,
                    texture_count: draw.resources.len(),
                    shader_mappings: draw_resource_set_shader_mappings(),
                });
                draw_index = draw_index.saturating_add(1);
            }
        }

        if draw_index != texture_descriptors.draw_count {
            return Err(format!(
                "scene resource heap draw count {} does not match texture descriptor draw count {}",
                draw_index, texture_descriptors.draw_count
            ));
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
                "scene draw resource heap requires a ready VK_EXT_descriptor_heap mixed resource plan: {:?}",
                descriptor_heap_plan.blocking_reason
            ));
        }
        let entries = finalize_entries(pending_entries, &descriptor_heap_plan)?;
        for binding in &mut draw_bindings {
            binding.base_resource_heap_offset = *descriptor_heap_plan
                .resource_descriptor_offsets
                .get(binding.base_resource_descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene resource heap draw binding {} missing base descriptor offset",
                        binding.draw_index
                    )
                })?;
        }

        Ok(Self {
            draw_count: draw_index,
            draw_binding_count: draw_bindings.len(),
            resource_set_count: resource_set_to_slice.len(),
            resource_descriptor_count: entries.len(),
            sampler_descriptor_count,
            descriptor_model: "VK_EXT_descriptor_heap",
            entries,
            draw_bindings,
            descriptor_heap_plan,
            command_order: [
                "collect_we_material_constant_buffers",
                "collect_we_texture_descriptors",
                "dedupe_draw_resource_sets",
                "pack_mixed_descriptor_heap_slices",
                "bind_draw_resource_set_slice",
            ],
        })
    }
}

fn push_material_descriptor(
    descriptor_kinds: &mut Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pending_entries: &mut Vec<PendingResourceHeapEntry>,
    resource_set_index: usize,
    draw_index: usize,
    object: SceneObjectId,
    material: &NativeVulkanSceneMaterialUniformGpuBufferBinding,
) {
    let descriptor_index = descriptor_kinds.len();
    descriptor_kinds
        .push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer);
    pending_entries.push(PendingResourceHeapEntry {
        resource_set_index,
        descriptor_index,
        descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
        sampler_descriptor_index: None,
        draw_index,
        object,
        role: NativeVulkanSceneResourceHeapEntryRole::WePsMaterialConstantsSlot3 {
            material: material.key.clone(),
            buffer_handle: material.buffer.as_raw(),
            device_address: material.device_address,
            record_index: material.record_index,
            bytes: material.bytes,
            payload_hash: material.payload_hash,
            shader_mapping: "WE PSSetConstantBuffers(slot=3)",
        },
    });
}

fn push_texture_descriptor(
    descriptor_kinds: &mut Vec<NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind>,
    pending_entries: &mut Vec<PendingResourceHeapEntry>,
    resource_set_index: usize,
    draw_index: usize,
    descriptor: &NativeVulkanSceneTextureDescriptorBinding,
    sampler_descriptor_index: usize,
) {
    let descriptor_index = descriptor_kinds.len();
    descriptor_kinds.push(NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage);
    pending_entries.push(PendingResourceHeapEntry {
        resource_set_index,
        descriptor_index,
        descriptor_kind: NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
        sampler_descriptor_index: Some(sampler_descriptor_index),
        draw_index,
        object: descriptor.object,
        role: NativeVulkanSceneResourceHeapEntryRole::WeSampledTexture {
            resource: descriptor.resource,
            slot: descriptor.slot,
            shader_mapping: descriptor.shader_mapping.clone(),
        },
    });
}

fn finalize_entries(
    pending_entries: Vec<PendingResourceHeapEntry>,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
) -> Result<Vec<NativeVulkanSceneResourceHeapEntry>, String> {
    pending_entries
        .into_iter()
        .map(|entry| {
            let resource_heap_offset = *descriptor_heap_plan
                .resource_descriptor_offsets
                .get(entry.descriptor_index)
                .ok_or_else(|| {
                    format!(
                        "scene resource heap descriptor {} missing resource offset",
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
                                "scene resource heap sampler descriptor {} missing sampler offset",
                                sampler_index
                            )
                        })
                })
                .transpose()?;
            Ok(NativeVulkanSceneResourceHeapEntry {
                resource_set_index: entry.resource_set_index,
                descriptor_index: entry.descriptor_index,
                descriptor_kind: entry.descriptor_kind,
                resource_heap_offset,
                sampler_descriptor_index: entry.sampler_descriptor_index,
                sampler_heap_offset,
                draw_index: entry.draw_index,
                object: entry.object,
                role: entry.role,
            })
        })
        .collect()
}

fn texture_descriptors_by_draw(
    descriptors: &NativeVulkanSceneTextureDescriptorFramePlan,
) -> Result<Vec<Vec<&NativeVulkanSceneTextureDescriptorBinding>>, String> {
    let mut by_draw = vec![Vec::new(); descriptors.draw_count];
    for descriptor in &descriptors.bindings {
        let draw = by_draw.get_mut(descriptor.draw_index).ok_or_else(|| {
            format!(
                "scene resource heap texture descriptor draw index {} exceeds draw count {}",
                descriptor.draw_index, descriptors.draw_count
            )
        })?;
        draw.push(descriptor);
    }
    for draw in &mut by_draw {
        draw.sort_by_key(|descriptor| descriptor.slot);
    }
    Ok(by_draw)
}

fn validate_material_binding(
    requested: &NativeVulkanSceneMaterialUniformKey,
    binding: &NativeVulkanSceneMaterialUniformGpuBufferBinding,
) -> Result<(), String> {
    if &binding.key != requested {
        return Err(format!(
            "scene resource heap material uniform resolver returned {:?} for requested {:?}",
            binding.key, requested
        ));
    }
    if binding.buffer.as_raw() == 0 {
        return Err(format!(
            "scene resource heap material uniform {:?} has null GPU buffer",
            requested
        ));
    }
    if binding.device_address == 0 {
        return Err(format!(
            "scene resource heap material uniform {:?} has zero device address",
            requested
        ));
    }
    if binding.bytes == 0 {
        return Err(format!(
            "scene resource heap material uniform {:?} has zero byte range",
            requested
        ));
    }
    Ok(())
}

fn draw_resource_set_shader_mappings() -> Vec<String> {
    vec![format!(
        "WE PSSetConstantBuffers(slot={WE_GENERICIMAGE4_PS_MATERIAL_CONSTANT_BUFFER_SLOT}) -> draw-resource-set-offset0"
    )]
}
