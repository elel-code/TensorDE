//! Retained Vulkan descriptor heap store for WE auxiliary material heap slices.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{SceneGraphTarget, SceneObjectId};
use crate::renderer::native_vulkan::vulkan::{
    VulkanaliaDescriptorHeapResourceResources,
    native_vulkan_vulkanalia_create_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor,
    native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor,
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources,
};

use super::vk_descriptor::write_scene_layer_aux_material_resource_heap_descriptors;
use super::{
    NativeVulkanSceneLayerAuxMaterialResourceHeapClearBinding,
    NativeVulkanSceneLayerAuxMaterialResourceHeapEntry,
    NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAuxMaterialResourceHeapSyncAction
{
    Create {
        entries: Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapEntry>,
    },
    Reuse {
        entries: Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapEntry>,
    },
    Replace {
        old_entries: Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapEntry>,
        new_entries: Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapEntry>,
    },
    Release {
        entries: Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapEntry>,
    },
}

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo
{
    pub clear_bind_index: usize,
    pub command_index: usize,
    pub block_index: usize,
    pub object: SceneObjectId,
    pub material: &'static str,
    pub shader: &'static str,
    pub source: &'static str,
    pub source_target: SceneGraphTarget,
    pub target: SceneGraphTarget,
    pub texture_slot: u32,
    pub heap_slice_index: usize,
    pub base_resource_descriptor_index: usize,
    pub base_sampler_descriptor_index: usize,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
    pub resource_bind: vk::BindHeapInfoEXT,
    pub sampler_bind: vk::BindHeapInfoEXT,
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAuxMaterialResourceHeapStore {
    resources: Option<VulkanaliaDescriptorHeapResourceResources>,
    current: Option<NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan>,
    last_actions: Vec<NativeVulkanSceneLayerAuxMaterialResourceHeapSyncAction>,
}

impl NativeVulkanSceneLayerAuxMaterialResourceHeapStore {
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
        frame_plan: NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan,
    ) -> Result<&[NativeVulkanSceneLayerAuxMaterialResourceHeapSyncAction], String> {
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
                self.last_actions.push(
                    NativeVulkanSceneLayerAuxMaterialResourceHeapSyncAction::Release {
                        entries: old_plan.entries,
                    },
                );
            }
            return Ok(&self.last_actions);
        }

        if self.resources.is_some()
            && let Some(current) = &self.current
            && current.entries == frame_plan.entries
            && current.clear_bindings == frame_plan.clear_bindings
            && current.descriptor_heap_plan == frame_plan.descriptor_heap_plan
        {
            self.last_actions.push(
                NativeVulkanSceneLayerAuxMaterialResourceHeapSyncAction::Reuse {
                    entries: current.entries.clone(),
                },
            );
            return Ok(&self.last_actions);
        }

        let mut next_resources =
            native_vulkan_vulkanalia_create_descriptor_heap_resource_resources(
                device,
                memory_properties,
                &frame_plan.descriptor_heap_plan,
            )?;
        if let Err(err) = write_scene_layer_aux_material_resource_heap_descriptors(
            device,
            &mut next_resources,
            &frame_plan,
        ) {
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
                self.last_actions.push(
                    NativeVulkanSceneLayerAuxMaterialResourceHeapSyncAction::Replace {
                        old_entries: old_plan.entries,
                        new_entries,
                    },
                );
            }
            None => {
                self.last_actions.push(
                    NativeVulkanSceneLayerAuxMaterialResourceHeapSyncAction::Create {
                        entries: new_entries,
                    },
                );
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn current_frame_plan(
        &self,
    ) -> Option<&NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan> {
        self.current.as_ref()
    }

    pub(in crate::renderer::native_vulkan) fn bind_info_for_clear_bind(
        &self,
        clear_bind_index: usize,
    ) -> Result<NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo, String> {
        let current = self
            .current
            .as_ref()
            .ok_or_else(|| "scene aux material resource heap has no frame plan".to_owned())?;
        let binding = current
            .clear_bindings
            .iter()
            .find(|binding| binding.clear_bind_index == clear_bind_index)
            .ok_or_else(|| {
                format!(
                    "scene aux material resource heap has no binding for clear bind {clear_bind_index}"
                )
            })?;
        self.bind_info_from_binding(binding)
    }

    pub(in crate::renderer::native_vulkan) fn bind_info_for_command(
        &self,
        command_index: usize,
    ) -> Result<NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo, String> {
        let current = self
            .current
            .as_ref()
            .ok_or_else(|| "scene aux material resource heap has no frame plan".to_owned())?;
        let binding = current
            .clear_bindings
            .iter()
            .find(|binding| binding.command_index == command_index)
            .ok_or_else(|| {
                format!(
                    "scene aux material resource heap has no binding for command {command_index}"
                )
            })?;
        self.bind_info_from_binding(binding)
    }

    fn bind_info_from_binding(
        &self,
        binding: &NativeVulkanSceneLayerAuxMaterialResourceHeapClearBinding,
    ) -> Result<NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo, String> {
        let resources = self
            .resources
            .as_ref()
            .ok_or_else(|| "scene aux material resource heap is not resident".to_owned())?;
        Ok(NativeVulkanSceneLayerAuxMaterialResourceHeapBindInfo {
            clear_bind_index: binding.clear_bind_index,
            command_index: binding.command_index,
            block_index: binding.block_index,
            object: binding.object,
            material: binding.material,
            shader: binding.shader,
            source: binding.source,
            source_target: binding.source_target,
            target: binding.target,
            texture_slot: binding.texture_slot,
            heap_slice_index: binding.heap_slice_index,
            base_resource_descriptor_index: binding.base_resource_descriptor_index,
            base_sampler_descriptor_index: binding.base_sampler_descriptor_index,
            resource_descriptor_count: binding.resource_descriptor_count,
            texture_count: binding.texture_count,
            shader_mappings: binding.shader_mappings.clone(),
            resource_bind:
                native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor(
                    resources,
                    binding.base_resource_descriptor_index,
                )?,
            sampler_bind:
                native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor(
                    resources,
                    binding.base_sampler_descriptor_index,
                )?,
        })
    }

    pub(in crate::renderer::native_vulkan) fn last_actions(
        &self,
    ) -> &[NativeVulkanSceneLayerAuxMaterialResourceHeapSyncAction] {
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

impl Default for NativeVulkanSceneLayerAuxMaterialResourceHeapStore {
    fn default() -> Self {
        Self::new()
    }
}
