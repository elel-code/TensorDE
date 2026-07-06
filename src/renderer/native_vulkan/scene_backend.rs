//! Native Vulkan backend for the new scene engine.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

pub mod renderer_scene_render;
pub mod rendering_device;

pub use renderer_scene_render::NativeVulkanRendererSceneRender;
pub use rendering_device::NativeVulkanRenderingDevice;
