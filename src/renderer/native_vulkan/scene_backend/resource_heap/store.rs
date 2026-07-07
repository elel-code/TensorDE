//! Retained Vulkan descriptor heap store for scene draw resource sets.
//!
//! References:
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::renderer::native_vulkan::vulkan::{
    VulkanaliaDescriptorHeapResourceResources,
    native_vulkan_vulkanalia_create_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor,
    native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor,
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources,
};

use super::bind_command::NativeVulkanSceneResourceHeapDrawBindInfo;
use super::frame_plan::{
    NativeVulkanSceneResourceHeapEntry, NativeVulkanSceneResourceHeapFramePlan,
};

use super::vk_descriptor::write_scene_resource_heap_descriptors;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneResourceHeapSyncAction {
    Create {
        entries: Vec<NativeVulkanSceneResourceHeapEntry>,
    },
    Reuse {
        entries: Vec<NativeVulkanSceneResourceHeapEntry>,
    },
    Replace {
        old_entries: Vec<NativeVulkanSceneResourceHeapEntry>,
        new_entries: Vec<NativeVulkanSceneResourceHeapEntry>,
    },
    Release {
        entries: Vec<NativeVulkanSceneResourceHeapEntry>,
    },
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneResourceHeapStore {
    resources: Option<VulkanaliaDescriptorHeapResourceResources>,
    current: Option<NativeVulkanSceneResourceHeapFramePlan>,
    last_actions: Vec<NativeVulkanSceneResourceHeapSyncAction>,
}

impl NativeVulkanSceneResourceHeapStore {
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
        frame_plan: NativeVulkanSceneResourceHeapFramePlan,
    ) -> Result<&[NativeVulkanSceneResourceHeapSyncAction], String> {
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
                    .push(NativeVulkanSceneResourceHeapSyncAction::Release {
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
                .push(NativeVulkanSceneResourceHeapSyncAction::Reuse {
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
            write_scene_resource_heap_descriptors(device, &mut next_resources, &frame_plan)
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
                    .push(NativeVulkanSceneResourceHeapSyncAction::Replace {
                        old_entries: old_plan.entries,
                        new_entries,
                    });
            }
            None => {
                self.last_actions
                    .push(NativeVulkanSceneResourceHeapSyncAction::Create {
                        entries: new_entries,
                    });
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn current_frame_plan(
        &self,
    ) -> Option<&NativeVulkanSceneResourceHeapFramePlan> {
        self.current.as_ref()
    }

    pub(in crate::renderer::native_vulkan) fn draw_bind_info_for_draw_index(
        &self,
        draw_index: usize,
    ) -> Result<NativeVulkanSceneResourceHeapDrawBindInfo, String> {
        let current = self
            .current
            .as_ref()
            .ok_or_else(|| "scene draw resource heap has no frame plan".to_owned())?;
        let binding = current
            .draw_bindings
            .iter()
            .find(|binding| binding.draw_index == draw_index)
            .ok_or_else(|| {
                format!("scene draw resource heap has no binding for draw {draw_index}")
            })?;
        let resources = self
            .resources
            .as_ref()
            .ok_or_else(|| "scene draw resource heap is not resident".to_owned())?;
        Ok(NativeVulkanSceneResourceHeapDrawBindInfo {
            draw_index,
            object: binding.object,
            resource_set_index: binding.resource_set_index,
            texture_set: binding.texture_set.clone(),
            base_resource_descriptor_index: binding.base_resource_descriptor_index,
            resource_descriptor_count: binding.resource_descriptor_count,
            texture_count: binding.texture_count,
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
    ) -> &[NativeVulkanSceneResourceHeapSyncAction] {
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

impl Default for NativeVulkanSceneResourceHeapStore {
    fn default() -> Self {
        Self::new()
    }
}
