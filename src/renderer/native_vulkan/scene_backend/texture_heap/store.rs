//! Retained Vulkan descriptor heap store for scene textures.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::SceneResourceId;
use crate::renderer::native_vulkan::vulkan::{
    VulkanaliaDescriptorHeapImageSamplerResources,
    native_vulkan_vulkanalia_create_descriptor_heap_image_sampler_resources,
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info_for_image,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info_for_image,
    native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources,
};

use super::frame_plan::{NativeVulkanSceneTextureHeapEntry, NativeVulkanSceneTextureHeapFramePlan};
use super::vk_descriptor::write_scene_texture_heap_descriptors;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneTextureHeapSyncAction {
    Create {
        entries: Vec<NativeVulkanSceneTextureHeapEntry>,
    },
    Reuse {
        entries: Vec<NativeVulkanSceneTextureHeapEntry>,
    },
    Replace {
        old_entries: Vec<NativeVulkanSceneTextureHeapEntry>,
        new_entries: Vec<NativeVulkanSceneTextureHeapEntry>,
    },
    Release {
        entries: Vec<NativeVulkanSceneTextureHeapEntry>,
    },
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureHeapStore {
    resources: Option<VulkanaliaDescriptorHeapImageSamplerResources>,
    current: Option<NativeVulkanSceneTextureHeapFramePlan>,
    last_actions: Vec<NativeVulkanSceneTextureHeapSyncAction>,
}

impl NativeVulkanSceneTextureHeapStore {
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
        frame_plan: NativeVulkanSceneTextureHeapFramePlan,
    ) -> Result<&[NativeVulkanSceneTextureHeapSyncAction], String> {
        self.last_actions.clear();

        if frame_plan.entries.is_empty() {
            if let Some(resources) = self.resources.take() {
                native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                    device, resources,
                );
            }
            if let Some(old_plan) = self.current.take() {
                self.last_actions
                    .push(NativeVulkanSceneTextureHeapSyncAction::Release {
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
                .push(NativeVulkanSceneTextureHeapSyncAction::Reuse {
                    entries: current.entries.clone(),
                });
            return Ok(&self.last_actions);
        }

        let mut next_resources =
            native_vulkan_vulkanalia_create_descriptor_heap_image_sampler_resources(
                device,
                memory_properties,
                &frame_plan.descriptor_heap_plan,
            )?;
        if let Err(err) =
            write_scene_texture_heap_descriptors(device, &mut next_resources, &frame_plan)
        {
            native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                device,
                next_resources,
            );
            return Err(err);
        }

        if let Some(old_resources) = self.resources.replace(next_resources) {
            native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                device,
                old_resources,
            );
        }
        let new_entries = frame_plan.entries.clone();
        match self.current.replace(frame_plan) {
            Some(old_plan) => {
                self.last_actions
                    .push(NativeVulkanSceneTextureHeapSyncAction::Replace {
                        old_entries: old_plan.entries,
                        new_entries,
                    });
            }
            None => {
                self.last_actions
                    .push(NativeVulkanSceneTextureHeapSyncAction::Create {
                        entries: new_entries,
                    });
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn current_frame_plan(
        &self,
    ) -> Option<&NativeVulkanSceneTextureHeapFramePlan> {
        self.current.as_ref()
    }

    pub(in crate::renderer::native_vulkan) fn resource_bind_info(
        &self,
    ) -> Result<vk::BindHeapInfoEXT, String> {
        let resources = self
            .resources
            .as_ref()
            .ok_or_else(|| "scene texture descriptor heap is not resident".to_owned())?;
        Ok(native_vulkan_vulkanalia_descriptor_heap_resource_bind_info(
            resources,
        ))
    }

    pub(in crate::renderer::native_vulkan) fn sampler_bind_info(
        &self,
    ) -> Result<vk::BindHeapInfoEXT, String> {
        let resources = self
            .resources
            .as_ref()
            .ok_or_else(|| "scene texture sampler descriptor heap is not resident".to_owned())?;
        Ok(native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info(
            resources,
        ))
    }

    pub(in crate::renderer::native_vulkan) fn resource_bind_info_for_texture(
        &self,
        resource: SceneResourceId,
    ) -> Result<vk::BindHeapInfoEXT, String> {
        let resources = self
            .resources
            .as_ref()
            .ok_or_else(|| "scene texture descriptor heap is not resident".to_owned())?;
        let heap_index = self
            .current
            .as_ref()
            .ok_or_else(|| "scene texture descriptor heap has no frame plan".to_owned())?
            .heap_index(resource)?;
        native_vulkan_vulkanalia_descriptor_heap_resource_bind_info_for_image(resources, heap_index)
    }

    pub(in crate::renderer::native_vulkan) fn sampler_bind_info_for_texture(
        &self,
        resource: SceneResourceId,
    ) -> Result<vk::BindHeapInfoEXT, String> {
        let resources = self
            .resources
            .as_ref()
            .ok_or_else(|| "scene texture sampler descriptor heap is not resident".to_owned())?;
        let heap_index = self
            .current
            .as_ref()
            .ok_or_else(|| "scene texture descriptor heap has no frame plan".to_owned())?
            .heap_index(resource)?;
        native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info_for_image(resources, heap_index)
    }

    pub(in crate::renderer::native_vulkan) fn shader_mapping_for_texture(
        &self,
        resource: SceneResourceId,
    ) -> Result<vk::DescriptorSetAndBindingMappingEXT, String> {
        let current = self
            .current
            .as_ref()
            .ok_or_else(|| "scene texture descriptor heap has no frame plan".to_owned())?;
        let heap_index = current.heap_index(resource)?;
        native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
            &current.descriptor_heap_plan,
            0,
            heap_index,
        )
    }

    pub(in crate::renderer::native_vulkan) fn last_actions(
        &self,
    ) -> &[NativeVulkanSceneTextureHeapSyncAction] {
        &self.last_actions
    }

    pub(in crate::renderer::native_vulkan) fn destroy_all(&mut self, device: &Device) {
        if let Some(resources) = self.resources.take() {
            native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                device, resources,
            );
        }
        self.current = None;
        self.last_actions.clear();
    }
}

impl Default for NativeVulkanSceneTextureHeapStore {
    fn default() -> Self {
        Self::new()
    }
}
