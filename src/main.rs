mod windowing;

#[path = "main/native_vulkan_app.rs"]
mod native_vulkan_app;
#[path = "main/vulkan_frame.rs"]
mod vulkan_frame;
#[path = "main/vulkan_rect.rs"]
mod vulkan_rect;
#[path = "main/vulkan_rect_spirv.rs"]
mod vulkan_rect_spirv;
#[path = "main/vulkan_state.rs"]
mod vulkan_state;
#[path = "main/vulkan_text.rs"]
mod vulkan_text;
#[path = "main/vulkan_text_spirv.rs"]
mod vulkan_text_spirv;

include!("main/crate_prelude.rs");

mod app_actions;
mod shell;

include!("main/startup_settings.rs");
include!("main/app_runtime.rs");
include!("main/input_and_scene_state.rs");
include!("main/scene_and_icon_cache.rs");
include!("main/thumbnail_source_jobs.rs");
include!("main/thumbnail_jobs.rs");
include!("main/folder_preview_runtime.rs");
include!("main/folder_preview_layout.rs");
include!("main/gpu_svg_renderer.rs");
include!("main/gpu_icon_source_renderer.rs");
include!("main/icon_renderer_and_text_stats.rs");
include!("main/text_cache_and_builder.rs");
include!("main/text_renderer_and_icon_theme.rs");
include!("main/icon_theme_resolver.rs");
include!("main/geometry_tasks_places.rs");
include!("main/places_filters_text_metrics.rs");
