//! Native Vulkan backend for the new scene engine.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

pub mod draw_command;
pub mod frame_command;
pub mod frame_resources;
pub mod pass_command;
pub mod pipeline;
pub mod pipeline_factory;
pub mod pipeline_warmup;
pub mod render_target;
pub mod renderer_scene_render;
pub mod rendering_device;
pub mod resource_buffers;
pub mod resource_storage;
pub mod resource_upload;
pub mod runtime;

pub use renderer_scene_render::NativeVulkanRendererSceneRender;
pub use rendering_device::NativeVulkanRenderingDevice;
