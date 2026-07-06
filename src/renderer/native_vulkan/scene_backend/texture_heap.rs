//! Retained scene texture descriptor heap modules.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

pub mod bind_command;
pub mod frame_plan;
pub mod store;
pub mod texture_set;
pub mod vk_descriptor;

pub(in crate::renderer::native_vulkan) use bind_command::{
    NativeVulkanSceneTextureHeapDrawBindInfo, NativeVulkanSceneTextureHeapDrawBindPlan,
    native_vulkan_record_scene_texture_heap_draw_bind_command,
};
pub(in crate::renderer::native_vulkan) use frame_plan::NativeVulkanSceneTextureHeapFramePlan;
pub(in crate::renderer::native_vulkan) use store::{
    NativeVulkanSceneTextureHeapStore, NativeVulkanSceneTextureHeapSyncAction,
};
pub(in crate::renderer::native_vulkan) use texture_set::{
    NativeVulkanSceneTextureSetKey, scene_mesh_draw_texture_set_key,
};
