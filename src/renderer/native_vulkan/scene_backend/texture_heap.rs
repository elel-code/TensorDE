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

pub mod frame_plan;
pub mod store;
pub mod vk_descriptor;

pub(in crate::renderer::native_vulkan) use frame_plan::NativeVulkanSceneTextureHeapFramePlan;
pub(in crate::renderer::native_vulkan) use store::{
    NativeVulkanSceneTextureHeapStore, NativeVulkanSceneTextureHeapSyncAction,
};
