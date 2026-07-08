//! Retained Vulkan descriptor heap store for scene effect heap slices.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::SceneObjectId;
use crate::renderer::native_vulkan::vulkan::{
    VulkanaliaDescriptorHeapResourceResources,
    native_vulkan_vulkanalia_create_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor,
    native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor,
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources,
};

use super::super::effect_uniforms::NativeVulkanSceneEffectUniformKey;
use super::key::NativeVulkanSceneEffectTextureSetKey;
use super::vk_descriptor::write_scene_effect_resource_heap_descriptors;
use super::{
    NativeVulkanSceneEffectResourceHeapEntry, NativeVulkanSceneEffectResourceHeapFramePlan,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneEffectResourceHeapSyncAction {
    Create {
        entries: Vec<NativeVulkanSceneEffectResourceHeapEntry>,
    },
    Reuse {
        entries: Vec<NativeVulkanSceneEffectResourceHeapEntry>,
    },
    Replace {
        old_entries: Vec<NativeVulkanSceneEffectResourceHeapEntry>,
        new_entries: Vec<NativeVulkanSceneEffectResourceHeapEntry>,
    },
    Release {
        entries: Vec<NativeVulkanSceneEffectResourceHeapEntry>,
    },
}

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectResourceHeapPassBindInfo {
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub heap_slice_index: usize,
    pub effect_uniform_buffer_count: usize,
    pub effect_uniforms: Vec<NativeVulkanSceneEffectUniformKey>,
    pub effect_uniform_buffer_handles: Vec<u64>,
    pub effect_uniform_device_addresses: Vec<u64>,
    pub effect_uniform_record_indices: Vec<usize>,
    pub effect_uniform_bytes: Vec<u64>,
    pub effect_uniform_payload_hashes: Vec<u64>,
    pub texture_set: NativeVulkanSceneEffectTextureSetKey,
    pub base_resource_descriptor_index: usize,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
    pub resource_bind: vk::BindHeapInfoEXT,
    pub sampler_bind: Option<vk::BindHeapInfoEXT>,
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectResourceHeapStore {
    resources: Option<VulkanaliaDescriptorHeapResourceResources>,
    current: Option<NativeVulkanSceneEffectResourceHeapFramePlan>,
    last_actions: Vec<NativeVulkanSceneEffectResourceHeapSyncAction>,
}

impl NativeVulkanSceneEffectResourceHeapStore {
    pub(in crate::renderer::native_vulkan) fn new() -> Self {
        Self {
            resources: None,
            current: None,
            last_actions: Vec::new(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn sync_frame_plan(
        &mut self,
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        frame_plan: NativeVulkanSceneEffectResourceHeapFramePlan,
    ) -> Result<&[NativeVulkanSceneEffectResourceHeapSyncAction], String> {
        self.last_actions.clear();

        if frame_plan.entries.is_empty() {
            if let Some(resources) = self.resources.take() {
                native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
                    device, resources,
                );
            }
            if let Some(old_plan) = self.current.replace(frame_plan)
                && !old_plan.entries.is_empty()
            {
                self.last_actions
                    .push(NativeVulkanSceneEffectResourceHeapSyncAction::Release {
                        entries: old_plan.entries,
                    });
            }
            return Ok(&self.last_actions);
        }

        if self.resources.is_some()
            && let Some(current) = &self.current
            && current.entries == frame_plan.entries
            && current.descriptor_heap_plan == frame_plan.descriptor_heap_plan
        {
            self.last_actions
                .push(NativeVulkanSceneEffectResourceHeapSyncAction::Reuse {
                    entries: current.entries.clone(),
                });
            return Ok(&self.last_actions);
        }

        let mut next_resources =
            native_vulkan_vulkanalia_create_descriptor_heap_resource_resources(
                device,
                memory_properties,
                &frame_plan.descriptor_heap_plan,
            )?;
        if let Err(err) =
            write_scene_effect_resource_heap_descriptors(device, &mut next_resources, &frame_plan)
        {
            native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
                device,
                next_resources,
            );
            return Err(err);
        }

        if let Some(old_resources) = self.resources.replace(next_resources) {
            native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
                device,
                old_resources,
            );
        }
        let new_entries = frame_plan.entries.clone();
        match self.current.replace(frame_plan) {
            Some(old_plan) => {
                self.last_actions
                    .push(NativeVulkanSceneEffectResourceHeapSyncAction::Replace {
                        old_entries: old_plan.entries,
                        new_entries,
                    });
            }
            None => {
                self.last_actions
                    .push(NativeVulkanSceneEffectResourceHeapSyncAction::Create {
                        entries: new_entries,
                    });
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn current_frame_plan(
        &self,
    ) -> Option<&NativeVulkanSceneEffectResourceHeapFramePlan> {
        self.current.as_ref()
    }

    pub(in crate::renderer::native_vulkan) fn pass_bind_info_for_effect_pass(
        &self,
        effect_pass_index: usize,
    ) -> Result<NativeVulkanSceneEffectResourceHeapPassBindInfo, String> {
        let current = self
            .current
            .as_ref()
            .ok_or_else(|| "scene effect resource heap has no frame plan".to_owned())?;
        let binding = current
            .pass_bindings
            .iter()
            .find(|binding| binding.effect_pass_index == effect_pass_index)
            .ok_or_else(|| {
                format!("scene effect resource heap has no binding for pass {effect_pass_index}")
            })?;
        let resources = self
            .resources
            .as_ref()
            .ok_or_else(|| "scene effect resource heap is not resident".to_owned())?;
        Ok(NativeVulkanSceneEffectResourceHeapPassBindInfo {
            effect_pass_index,
            object: binding.object,
            heap_slice_index: binding.heap_slice_index,
            effect_uniform_buffer_count: binding.effect_uniform_buffer_count,
            effect_uniforms: binding.effect_uniforms.clone(),
            effect_uniform_buffer_handles: binding.effect_uniform_buffer_handles.clone(),
            effect_uniform_device_addresses: binding.effect_uniform_device_addresses.clone(),
            effect_uniform_record_indices: binding.effect_uniform_record_indices.clone(),
            effect_uniform_bytes: binding.effect_uniform_bytes.clone(),
            effect_uniform_payload_hashes: binding.effect_uniform_payload_hashes.clone(),
            texture_set: binding.texture_set.clone(),
            base_resource_descriptor_index: binding.base_resource_descriptor_index,
            resource_descriptor_count: binding.resource_descriptor_count,
            texture_count: binding.texture_count,
            shader_mappings: binding.shader_mappings.clone(),
            resource_bind:
                native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor(
                    resources,
                    binding.base_resource_descriptor_index,
                )?,
            sampler_bind: binding
                .base_sampler_descriptor_index
                .map(|sampler_index| {
                    native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor(
                        resources,
                        sampler_index,
                    )
                })
                .transpose()?,
        })
    }

    pub(in crate::renderer::native_vulkan) fn last_actions(
        &self,
    ) -> &[NativeVulkanSceneEffectResourceHeapSyncAction] {
        &self.last_actions
    }

    pub(in crate::renderer::native_vulkan) fn destroy_all(&mut self, device: &Device) {
        if let Some(resources) = self.resources.take() {
            native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(device, resources);
        }
        self.current = None;
        self.last_actions.clear();
    }
}

impl Default for NativeVulkanSceneEffectResourceHeapStore {
    fn default() -> Self {
        Self::new()
    }
}
