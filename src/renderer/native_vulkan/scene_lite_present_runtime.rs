use std::time::Duration;

use crate::renderer::SceneLiteWallpaperPlan;

use super::render_item::native_vulkan_scene_lite_item;
use super::scene_lite_runtime::native_vulkan_scene_lite_runtime_snapshot;
use super::{
    NativeVulkanError, NativeVulkanOptions,
    NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions,
    NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot,
    NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions,
    NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot,
    run_native_vulkan_vulkanalia_scene_lite_sampled_image_present,
    run_native_vulkan_vulkanalia_scene_lite_solid_quad_present,
};

pub fn run_scene_lite(
    mut options: NativeVulkanOptions,
    duration: Duration,
    plan: SceneLiteWallpaperPlan,
) -> Result<NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot, NativeVulkanError> {
    if options.host.output_name.is_none() {
        options.host.output_name = Some(plan.output_name.clone());
    }
    let target_max_fps = options.target_max_fps.or(plan.target_max_fps);
    let render_item = native_vulkan_scene_lite_item(&plan);
    let runtime = native_vulkan_scene_lite_runtime_snapshot(&render_item).ok_or_else(|| {
        NativeVulkanError::SceneLite("scene-lite runtime snapshot is unavailable".to_owned())
    })?;
    let geometry = runtime
        .vulkanalia_solid_quad_geometry_input()
        .ok_or_else(|| {
            NativeVulkanError::SceneLite(format!(
                "scene-lite draw plan is not yet solid-quad recordable: {}",
                runtime.draw_pass_backend_status
            ))
        })?;

    run_native_vulkan_vulkanalia_scene_lite_solid_quad_present(
        NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions {
            host: options.host,
            wait_configure_roundtrips: options.wait_configure_roundtrips,
            duration,
            target_max_fps,
            quad_color: options.clear_color,
            geometry: Some(geometry),
        },
    )
    .map_err(NativeVulkanError::SceneLite)
}

pub fn run_scene_lite_sampled_image(
    mut options: NativeVulkanOptions,
    duration: Duration,
    plan: SceneLiteWallpaperPlan,
) -> Result<NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot, NativeVulkanError> {
    if options.host.output_name.is_none() {
        options.host.output_name = Some(plan.output_name.clone());
    }
    let target_max_fps = options.target_max_fps.or(plan.target_max_fps);
    let render_item = native_vulkan_scene_lite_item(&plan);
    let runtime = native_vulkan_scene_lite_runtime_snapshot(&render_item).ok_or_else(|| {
        NativeVulkanError::SceneLite("scene-lite runtime snapshot is unavailable".to_owned())
    })?;
    let (source, geometry) = runtime
        .vulkanalia_sampled_image_geometry_input()
        .ok_or_else(|| {
            NativeVulkanError::SceneLite(format!(
                "scene-lite draw plan is not sampled-image recordable: {}",
                runtime.draw_pass_backend_status
            ))
        })?;
    let solid_geometry = runtime.vulkanalia_mixed_solid_quad_geometry_input();

    run_native_vulkan_vulkanalia_scene_lite_sampled_image_present(
        NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions {
            host: options.host,
            wait_configure_roundtrips: options.wait_configure_roundtrips,
            duration,
            target_max_fps,
            source,
            clear_color: options.clear_color,
            fit: None,
            solid_geometry,
            geometry: Some(geometry),
        },
    )
    .map_err(NativeVulkanError::SceneLite)
}
