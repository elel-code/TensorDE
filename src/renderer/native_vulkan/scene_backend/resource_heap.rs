//! Scene draw descriptor heap slice modules.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/renderer_rd/uniform_set_cache_rd.h`
//! - `references/godot/servers/rendering/rendering_device.h`

mod bind_command;
mod frame_plan;
mod store;
pub(in crate::renderer::native_vulkan) mod texture_set;
mod vk_descriptor;

#[cfg(test)]
mod tests;

pub(in crate::renderer::native_vulkan) use bind_command::{
    NativeVulkanSceneResourceHeapDrawBindInfo, NativeVulkanSceneResourceHeapDrawBindPlan,
    native_vulkan_record_scene_resource_heap_draw_bind_command,
};
pub(in crate::renderer::native_vulkan) use frame_plan::NativeVulkanSceneResourceHeapFramePlan;
pub(in crate::renderer::native_vulkan) use store::{
    NativeVulkanSceneResourceHeapStore, NativeVulkanSceneResourceHeapSyncAction,
};
