//! Vulkanalia scene present session for the new scene engine.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_server_default.h`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::engine::scene_engine::{RenderingServer, SceneEnginePlan, SceneGraphExecutionPlan};
use crate::renderer::native_vulkan::NativeVulkanClearColor;
use crate::renderer::native_vulkan::scene_backend::frame_present_runtime::{
    NativeVulkanScenePresentFrameContext, NativeVulkanScenePresentFrameOutcome,
    NativeVulkanScenePresentFrameSkipReason, native_vulkan_present_scene_mesh_runtime_frame,
};
use crate::renderer::native_vulkan::scene_backend::frame_resources::NativeVulkanSceneFrameResources;
use crate::renderer::native_vulkan::scene_backend::frame_slots::NativeVulkanSceneFrameSlotResources;
use crate::renderer::native_vulkan::scene_backend::renderer_scene_render::NativeVulkanRendererSceneRender;
use crate::renderer::native_vulkan::scene_backend::shader_artifacts::{
    NativeVulkanSceneEffectShaderArtifactCatalog, NativeVulkanSceneShaderArtifactCatalog,
};
use crate::renderer::native_vulkan::scene_backend::target_formats::NativeVulkanSceneGraphTargetFormatPlan;
use crate::renderer::native_wayland::{
    NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};

use super::instance::{
    NativeVulkanVulkanaliaInstance,
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::scene_prepare::{
    NativeVulkanVulkanaliaScenePrepareSnapshot, prepare_scene_resources_and_pipelines,
};
use super::swapchain::{
    NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    NativeVulkanVulkanaliaPresentQueueSnapshot, NativeVulkanVulkanaliaSwapchainSnapshot,
    OPTIONAL_INSTANCE_EXTENSIONS, REQUIRED_INSTANCE_EXTENSIONS, composite_alpha_label,
    create_vulkanalia_present_device, create_vulkanalia_swapchain_plan,
    create_vulkanalia_wayland_surface, present_mode_label, queue_flag_labels,
    select_vulkanalia_present_queue, swapchain_create_flag_labels,
    vulkanalia_surface_capabilities2_enabled, vulkanalia_surface_maintenance1_enabled,
};

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaScenePresentOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub duration: Duration,
    pub clear_color: NativeVulkanClearColor,
    pub shader_artifact_root: PathBuf,
    pub scene: SceneEnginePlan,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaScenePresentSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub loader: String,
    pub requested_api_version: String,
    pub shader_artifact_root: PathBuf,
    pub scene_source: Option<PathBuf>,
    pub scene_snapshot_time_ms: u64,
    pub scene_resource_count: usize,
    pub scene_object_count: usize,
    pub runtime_elapsed_ms: u64,
    pub frames_presented: u64,
    pub frames_skipped: u64,
    pub frames_skipped_frame_slots_pending: u64,
    pub frames_skipped_swapchain_image_pending: u64,
    pub average_present_fps: f64,
    pub selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot,
    pub device_extensions: NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    pub swapchain: NativeVulkanVulkanaliaSwapchainSnapshot,
    pub frame_slot_count: usize,
    pub prepare: NativeVulkanVulkanaliaScenePrepareSnapshot,
    pub command_submit_model: &'static str,
    pub present_backend: &'static str,
    pub references: [&'static str; 7],
}

pub fn run_native_vulkan_vulkanalia_scene_present(
    options: NativeVulkanVulkanaliaScenePresentOptions,
) -> Result<NativeVulkanVulkanaliaScenePresentSnapshot, String> {
    let mut host =
        NativeWaylandHost::connect(options.host.clone()).map_err(|err| err.to_string())?;
    host.wait_until_configured(options.wait_configure_roundtrips)
        .map_err(|err| err.to_string())?;
    let handles = host.surface_handles().map_err(|err| err.to_string())?;

    let mut requested_instance_extensions = REQUIRED_INSTANCE_EXTENSIONS.to_vec();
    requested_instance_extensions.extend_from_slice(OPTIONAL_INSTANCE_EXTENSIONS);
    let vulkan = native_vulkan_vulkanalia_create_instance_with_required_extensions(
        &requested_instance_extensions,
    )?;
    let result = run_vulkanalia_scene_present_inner(&vulkan, handles, options);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn run_vulkanalia_scene_present_inner(
    vulkan: &NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
    options: NativeVulkanVulkanaliaScenePresentOptions,
) -> Result<NativeVulkanVulkanaliaScenePresentSnapshot, String> {
    let missing_required_instance_extensions = REQUIRED_INSTANCE_EXTENSIONS
        .iter()
        .copied()
        .filter(|required| {
            vulkan
                .extension_selection
                .missing_instance_extensions
                .contains(required)
        })
        .collect::<Vec<_>>();
    if !missing_required_instance_extensions.is_empty() {
        return Err(format!(
            "Vulkanalia scene present missing instance extensions: {}",
            missing_required_instance_extensions.join(", ")
        ));
    }

    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result = with_vulkanalia_scene_present(instance, surface, handles, vulkan, options);
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

fn with_vulkanalia_scene_present(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &NativeVulkanVulkanaliaInstance,
    options: NativeVulkanVulkanaliaScenePresentOptions,
) -> Result<NativeVulkanVulkanaliaScenePresentSnapshot, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices(vulkanalia scene present): {err:?}"))?;
    let mut present_queue_family_count = 0usize;
    let selection = select_vulkanalia_present_queue(
        instance,
        surface,
        handles,
        &physical_devices,
        &mut present_queue_family_count,
    )?;
    let present_device = create_vulkanalia_present_device(
        instance,
        &selection,
        vulkanalia_surface_maintenance1_enabled(vulkan),
    )?;
    if !present_device.feature_selection.synchronization2_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(
            "scene present requires synchronization2; legacy submit fallback is forbidden"
                .to_owned(),
        );
    }
    if !present_device.feature_selection.dynamic_rendering_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(
            "scene present requires dynamic rendering; render-pass fallback is forbidden"
                .to_owned(),
        );
    }
    if !present_device
        .feature_selection
        .core_features
        .descriptor_heap
    {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(
            "scene present requires VK_EXT_descriptor_heap; descriptor-set fallback is forbidden"
                .to_owned(),
        );
    }

    let device = &present_device.device;
    let mut swapchain = vk::SwapchainKHR::null();
    let mut frame_slots = None::<NativeVulkanSceneFrameSlotResources>;
    let mut frame_resources = NativeVulkanSceneFrameResources::new();
    let result = (|| -> Result<NativeVulkanVulkanaliaScenePresentSnapshot, String> {
        let swapchain_plan = create_vulkanalia_swapchain_plan(
            instance,
            selection.physical_device,
            surface,
            handles.buffer_size,
            vulkanalia_surface_capabilities2_enabled(vulkan),
            &present_device.feature_selection,
            true,
        )?;
        swapchain = unsafe { device.create_swapchain_khr(&swapchain_plan.create_info, None) }
            .map_err(|err| format!("vkCreateSwapchainKHR(vulkanalia scene present): {err:?}"))?;
        let swapchain_images = unsafe { device.get_swapchain_images_khr(swapchain) }
            .map_err(|err| format!("vkGetSwapchainImagesKHR(vulkanalia scene present): {err:?}"))?;
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };

        frame_slots = Some(NativeVulkanSceneFrameSlotResources::create(
            device,
            &swapchain_images,
            swapchain_plan.format.format,
            selection.queue_family_index,
        )?);
        let frame_slot_count = frame_slots
            .as_ref()
            .map(|slots| slots.frame_slot_count())
            .unwrap_or(0);

        let scene_source = options.scene.source.clone();
        let scene_snapshot_time_ms = options.scene.snapshot_time_ms;
        let scene_resource_count = options.scene.resources.len();
        let scene_object_count = options.scene.objects.len();
        let frame_context = options.scene.frame_context();
        let mut server = RenderingServer::new();
        server.replace_scene(
            options.scene.resources,
            options.scene.objects,
            options.scene.effects,
        );
        let renderer = NativeVulkanRendererSceneRender::new();
        let frame = server.draw(&renderer, frame_context)?;
        let graph_execution = SceneGraphExecutionPlan::from_graph(&frame.graph);
        let target_formats = NativeVulkanSceneGraphTargetFormatPlan::from_execution_plan(
            &graph_execution,
            swapchain_plan.format.format,
        )?;
        let shader_catalog = NativeVulkanSceneShaderArtifactCatalog::from_scene_frame(
            &options.shader_artifact_root,
            &frame,
        )?;
        let effect_shader_catalog =
            NativeVulkanSceneEffectShaderArtifactCatalog::from_effect_pass_graph(
                &options.shader_artifact_root,
                &frame.effect_pass_graph,
            )?;

        let slots = frame_slots
            .as_mut()
            .ok_or_else(|| "scene frame slots were not created".to_owned())?;
        let prepare = prepare_scene_resources_and_pipelines(
            device,
            present_device.queue,
            &memory_properties,
            present_device.feature_selection.descriptor_heap_properties,
            slots,
            &mut frame_resources,
            server.resources(),
            &frame,
            &graph_execution,
            &frame.effect_pass_graph,
            &target_formats,
            swapchain_plan.extent,
            &shader_catalog,
            &effect_shader_catalog,
        )?;

        let started_at = Instant::now();
        let deadline = started_at + options.duration;
        let mut frames_presented = 0u64;
        let mut frames_skipped = 0u64;
        let mut frames_skipped_frame_slots_pending = 0u64;
        let mut frames_skipped_swapchain_image_pending = 0u64;
        let mut frame_index = 0u64;
        while Instant::now() < deadline {
            let outcome = native_vulkan_present_scene_mesh_runtime_frame(
                frame_index,
                slots,
                &mut frame_resources,
                NativeVulkanScenePresentFrameContext {
                    device,
                    queue: present_device.queue,
                    swapchain,
                    swapchain_images: &swapchain_images,
                    swapchain_extent: swapchain_plan.extent,
                    target_formats: &target_formats,
                    clear_color: Some(options.clear_color),
                },
                &frame,
            )?;
            match outcome {
                NativeVulkanScenePresentFrameOutcome::Presented(_) => {
                    frames_presented = frames_presented.saturating_add(1);
                }
                NativeVulkanScenePresentFrameOutcome::Skipped(skip) => {
                    frames_skipped = frames_skipped.saturating_add(1);
                    match skip.reason {
                        NativeVulkanScenePresentFrameSkipReason::FrameSlotsPending => {
                            frames_skipped_frame_slots_pending =
                                frames_skipped_frame_slots_pending.saturating_add(1);
                        }
                        NativeVulkanScenePresentFrameSkipReason::SwapchainImageNotReady => {
                            frames_skipped_swapchain_image_pending =
                                frames_skipped_swapchain_image_pending.saturating_add(1);
                        }
                    }
                }
            }
            frame_index = frame_index.saturating_add(1);
        }
        let elapsed = started_at.elapsed();

        Ok(NativeVulkanVulkanaliaScenePresentSnapshot {
            binding: "vulkanalia",
            route: "scene-present",
            loader: vulkan.loader_name.to_owned(),
            requested_api_version: Version::V1_4_0.to_string(),
            shader_artifact_root: options.shader_artifact_root,
            scene_source,
            scene_snapshot_time_ms,
            scene_resource_count,
            scene_object_count,
            runtime_elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
            frames_presented,
            frames_skipped,
            frames_skipped_frame_slots_pending,
            frames_skipped_swapchain_image_pending,
            average_present_fps: if elapsed.is_zero() {
                0.0
            } else {
                frames_presented as f64 / elapsed.as_secs_f64()
            },
            selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot {
                physical_device_index: selection.physical_device_index,
                physical_device_name: selection.physical_device_name.clone(),
                physical_device_type: selection.physical_device_type.clone(),
                queue_family_index: selection.queue_family_index,
                queue_count: selection.queue_count,
                queue_flags: queue_flag_labels(selection.queue_flags),
                supports_graphics: selection.queue_flags.contains(vk::QueueFlags::GRAPHICS),
                supports_present: true,
                supports_wayland_presentation: selection.supports_wayland_presentation,
            },
            device_extensions: present_device.extension_snapshot.clone(),
            swapchain: NativeVulkanVulkanaliaSwapchainSnapshot {
                created: true,
                format: format!("{:?}", swapchain_plan.format.format),
                color_space: format!("{:?}", swapchain_plan.format.color_space),
                present_mode: present_mode_label(swapchain_plan.present_mode),
                extent: (swapchain_plan.extent.width, swapchain_plan.extent.height),
                extent_selection: swapchain_plan.extent_selection,
                image_count: swapchain_images.len(),
                min_image_count: swapchain_plan.image_count,
                composite_alpha: composite_alpha_label(swapchain_plan.composite_alpha),
                image_usage: vec!["transfer-src", "transfer-dst", "color-attachment"],
                create_flags: swapchain_create_flag_labels(swapchain_plan.create_flags),
                present_id2_enabled: swapchain_plan.present_id2_enabled,
                present_wait2_enabled: swapchain_plan.present_wait2_enabled,
            },
            frame_slot_count,
            prepare,
            command_submit_model: "cold resource prepare submit/wait -> pipeline prepare -> nonblocking acquire/frame-slot present runtime",
            present_backend: "vulkanalia-scene-present-runtime",
            references: [
                "reverse-engineered/docs/scene-format.md",
                "reverse-engineered/docs/material-format.md",
                "reverse-engineered/docs/exe/blend-and-render.md",
                "reverse-engineered/docs/exe/d3d11-context-calls.md",
                "references/godot/servers/rendering/rendering_server_default.h",
                "references/godot/servers/rendering/rendering_device_graph.h",
                "references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp",
            ],
        })
    })();

    let _ = unsafe { device.device_wait_idle() };
    if let Some(slots) = frame_slots.take() {
        slots.destroy_all(device);
    }
    frame_resources.destroy_all(device);
    if swapchain != vk::SwapchainKHR::null() {
        unsafe {
            device.destroy_swapchain_khr(swapchain, None);
        }
    }
    unsafe {
        present_device.device.destroy_device(None);
    }
    result
}
