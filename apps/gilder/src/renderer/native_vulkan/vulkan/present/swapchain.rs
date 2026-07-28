#![allow(dead_code)]

use std::ffi::{CStr, CString};

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrGetSurfaceCapabilities2ExtensionInstanceCommands,
    KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
    KhrWaylandSurfaceExtensionInstanceCommands,
};

use crate::renderer::native_wayland::{
    NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};

use super::super::device_selection::ranked_physical_devices;
use super::features::{
    DESCRIPTOR_HEAP_EXTENSION_NAME, NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaVulkan14PropertySnapshot, native_vulkan_vulkanalia_core_feature_snapshot,
    native_vulkan_vulkanalia_descriptor_heap_device_features,
    native_vulkan_vulkanalia_vulkan10_device_features,
    native_vulkan_vulkanalia_vulkan12_device_features,
    native_vulkan_vulkanalia_vulkan13_device_features,
    native_vulkan_vulkanalia_vulkan14_device_features,
};
use super::instance::{
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::super::core::roadmap_2026::{
    GILDER_ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS, ROADMAP_2026_API_VERSION,
};

pub(in crate::renderer::native_vulkan::vulkan) const REQUIRED_INSTANCE_EXTENSIONS: &[&str] =
    GILDER_ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS;
const REQUIRED_DEVICE_EXTENSIONS: &[&str] = &["VK_KHR_swapchain"];
const PRESENT_ID2_EXTENSION_NAME: &str = "VK_KHR_present_id2";
const PRESENT_WAIT2_EXTENSION_NAME: &str = "VK_KHR_present_wait2";
const SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME: &str = "VK_KHR_swapchain_maintenance1";
const PRESENT_MODE_FIFO_LATEST_READY_EXTENSION_NAME: &str = "VK_KHR_present_mode_fifo_latest_ready";
const BLEND_OPERATION_ADVANCED_EXTENSION_NAME: &str = "VK_EXT_blend_operation_advanced";
const MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED_EXTENSION_NAME: &str =
    "VK_EXT_multisampled_render_to_single_sampled";
const MAINTENANCE7_EXTENSION_NAME: &str = "VK_KHR_maintenance7";
const MAINTENANCE8_EXTENSION_NAME: &str = "VK_KHR_maintenance8";
const MAINTENANCE9_EXTENSION_NAME: &str = "VK_KHR_maintenance9";
const MAINTENANCE10_EXTENSION_NAME: &str = "VK_KHR_maintenance10";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaSurfaceSwapchainProbeOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
}

impl Default for NativeVulkanVulkanaliaSurfaceSwapchainProbeOptions {
    fn default() -> Self {
        let mut host = NativeWaylandHostOptions::default();
        host.namespace = "gilder-vulkanalia-swapchain".to_owned();
        Self {
            host,
            wait_configure_roundtrips: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub loader: String,
    pub entry_version: String,
    pub requested_api_version: String,
    pub enabled_instance_extensions: Vec<&'static str>,
    pub missing_instance_extensions: Vec<&'static str>,
    pub physical_device_count: usize,
    pub present_queue_family_count: usize,
    pub wayland_surface_logical_size: (u32, u32),
    pub wayland_surface_buffer_size: (u32, u32),
    pub selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot,
    pub device_extensions: NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    pub surface: NativeVulkanVulkanaliaSurfaceSnapshot,
    pub swapchain: NativeVulkanVulkanaliaSwapchainSnapshot,
    pub present_backend: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaPresentQueueSnapshot {
    pub physical_device_index: usize,
    pub physical_device_name: String,
    pub physical_device_type: String,
    pub queue_family_index: u32,
    pub queue_count: u32,
    pub queue_flags: Vec<&'static str>,
    pub supports_graphics: bool,
    pub supports_present: bool,
    pub supports_wayland_presentation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot {
    pub available_device_extension_count: usize,
    pub enabled_device_extensions: Vec<&'static str>,
    pub required_swapchain: bool,
    pub core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
    pub vulkan_1_4_properties: NativeVulkanVulkanaliaVulkan14PropertySnapshot,
    pub descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    pub synchronization2_enabled: bool,
    pub dynamic_rendering_enabled: bool,
    pub descriptor_heap_available: bool,
    pub descriptor_heap_enabled: bool,
    pub blend_operation_advanced_available: bool,
    pub blend_operation_advanced_enabled: bool,
    pub blend_operation_advanced_coherent_operations: bool,
    pub multisampled_render_to_single_sampled_available: bool,
    pub multisampled_render_to_single_sampled_enabled: bool,
    pub present_id2_available: bool,
    pub present_id2_enabled: bool,
    pub present_wait2_available: bool,
    pub present_wait2_enabled: bool,
    pub swapchain_maintenance1_available: bool,
    pub swapchain_maintenance1_enabled: bool,
    pub present_mode_fifo_latest_ready_available: bool,
    pub present_mode_fifo_latest_ready_enabled: bool,
    pub maintenance7_available: bool,
    pub maintenance7_enabled: bool,
    pub maintenance8_available: bool,
    pub maintenance8_enabled: bool,
    pub maintenance9_available: bool,
    pub maintenance9_enabled: bool,
    pub maintenance10_available: bool,
    pub maintenance10_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSurfaceSnapshot {
    pub capabilities: NativeVulkanVulkanaliaSurfaceCapabilitiesSnapshot,
    pub surface_format_count: usize,
    pub surface_formats: Vec<NativeVulkanVulkanaliaSurfaceFormatSnapshot>,
    pub present_mode_count: usize,
    pub present_modes: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSurfaceCapabilitiesSnapshot {
    pub min_image_count: u32,
    pub max_image_count: u32,
    pub current_extent: Option<(u32, u32)>,
    pub min_image_extent: (u32, u32),
    pub max_image_extent: (u32, u32),
    pub supports_transfer_src: bool,
    pub supports_transfer_dst: bool,
    pub supports_color_attachment: bool,
    pub supports_sampled: bool,
    pub present_id2_supported: bool,
    pub present_wait2_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSurfaceFormatSnapshot {
    pub format: String,
    pub color_space: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSwapchainSnapshot {
    pub created: bool,
    pub format: String,
    pub color_space: String,
    pub present_mode: &'static str,
    pub extent: (u32, u32),
    pub extent_selection: NativeVulkanVulkanaliaSwapchainExtentSelectionSnapshot,
    pub image_count: usize,
    pub min_image_count: u32,
    pub composite_alpha: &'static str,
    pub image_usage: Vec<&'static str>,
    pub create_flags: Vec<&'static str>,
    pub present_id2_enabled: bool,
    pub present_wait2_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSwapchainExtentSelectionSnapshot {
    pub source: &'static str,
    pub requested_wayland_buffer_size: (u32, u32),
    pub surface_current_extent: Option<(u32, u32)>,
    pub surface_min_image_extent: (u32, u32),
    pub surface_max_image_extent: (u32, u32),
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaPresentQueueSelection {
    pub(in crate::renderer::native_vulkan::vulkan) physical_device_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) physical_device: vk::PhysicalDevice,
    pub(in crate::renderer::native_vulkan::vulkan) physical_device_name: String,
    pub(in crate::renderer::native_vulkan::vulkan) physical_device_type: String,
    pub(in crate::renderer::native_vulkan::vulkan) queue_family_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) queue_count: u32,
    pub(in crate::renderer::native_vulkan::vulkan) queue_flags: vk::QueueFlags,
    pub(in crate::renderer::native_vulkan::vulkan) supports_wayland_presentation: bool,
    pub(in crate::renderer::native_vulkan::vulkan) device_extensions: Vec<String>,
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaSwapchainPlan {
    pub(in crate::renderer::native_vulkan::vulkan) create_info: vk::SwapchainCreateInfoKHR,
    pub(in crate::renderer::native_vulkan::vulkan) format: vk::SurfaceFormatKHR,
    pub(in crate::renderer::native_vulkan::vulkan) present_mode: vk::PresentModeKHR,
    pub(in crate::renderer::native_vulkan::vulkan) extent: vk::Extent2D,
    pub(in crate::renderer::native_vulkan::vulkan) extent_selection:
        NativeVulkanVulkanaliaSwapchainExtentSelectionSnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) image_count: u32,
    pub(in crate::renderer::native_vulkan::vulkan) composite_alpha: vk::CompositeAlphaFlagsKHR,
    pub(in crate::renderer::native_vulkan::vulkan) create_flags: vk::SwapchainCreateFlagsKHR,
    pub(in crate::renderer::native_vulkan::vulkan) surface_present_id2_supported: bool,
    pub(in crate::renderer::native_vulkan::vulkan) surface_present_wait2_supported: bool,
    pub(in crate::renderer::native_vulkan::vulkan) present_id2_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) present_wait2_enabled: bool,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaPresentFeatureSelection
{
    pub(in crate::renderer::native_vulkan::vulkan) core_features:
        NativeVulkanVulkanaliaCoreFeatureSnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) vulkan_1_4_properties:
        NativeVulkanVulkanaliaVulkan14PropertySnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) descriptor_heap_properties:
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) synchronization2_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) dynamic_rendering_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) present_id2_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) present_wait2_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) swapchain_maintenance1_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) present_mode_fifo_latest_ready_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) blend_operation_advanced_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) blend_operation_advanced_coherent_operations:
        bool,
    pub(in crate::renderer::native_vulkan::vulkan) multisampled_render_to_single_sampled_enabled:
        bool,
    pub(in crate::renderer::native_vulkan::vulkan) scene_color_4x_msaa_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) maintenance7_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) maintenance8_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) maintenance9_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) maintenance10_enabled: bool,
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaPresentDeviceContext {
    pub(in crate::renderer::native_vulkan::vulkan) device: Device,
    pub(in crate::renderer::native_vulkan::vulkan) queue: vk::Queue,
    pub(in crate::renderer::native_vulkan::vulkan) extension_snapshot:
        NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    pub(in crate::renderer::native_vulkan::vulkan) feature_selection:
        NativeVulkanVulkanaliaPresentFeatureSelection,
}

pub fn probe_native_vulkan_vulkanalia_surface_swapchain(
    options: NativeVulkanVulkanaliaSurfaceSwapchainProbeOptions,
) -> Result<NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot, String> {
    let mut host = NativeWaylandHost::connect(options.host).map_err(|err| err.to_string())?;
    host.wait_until_configured(options.wait_configure_roundtrips)
        .map_err(|err| err.to_string())?;
    let handles = host.surface_handles().map_err(|err| err.to_string())?;

    let vulkan = native_vulkan_vulkanalia_create_instance_with_required_extensions(
        REQUIRED_INSTANCE_EXTENSIONS,
    )?;
    let result = probe_vulkanalia_surface_swapchain_inner(&vulkan, handles);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn probe_vulkanalia_surface_swapchain_inner(
    vulkan: &super::instance::NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
) -> Result<NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot, String> {
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
            "Vulkanalia Wayland swapchain probe missing instance extensions: {}",
            missing_required_instance_extensions.join(", ")
        ));
    }

    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result = with_vulkanalia_surface_swapchain(instance, surface, handles, vulkan);
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

fn with_vulkanalia_surface_swapchain(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &super::instance::NativeVulkanVulkanaliaInstance,
) -> Result<NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices(vulkanalia present): {err:?}"))?;
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
    )?;
    let extension_snapshot = present_device.extension_snapshot.clone();
    let device = &present_device.device;
    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
        &present_device.feature_selection,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            unsafe {
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let surface_snapshot = surface_snapshot_from_plan(
        instance,
        selection.physical_device,
        surface,
        &swapchain_plan,
    )?;
    let swapchain = match unsafe { device.create_swapchain_khr(&swapchain_plan.create_info, None) }
    {
        Ok(swapchain) => swapchain,
        Err(err) => {
            unsafe {
                present_device.device.destroy_device(None);
            }
            return Err(format!("vkCreateSwapchainKHR(vulkanalia): {err:?}"));
        }
    };
    let swapchain_images = match unsafe { device.get_swapchain_images_khr(swapchain) } {
        Ok(images) => images,
        Err(err) => {
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!("vkGetSwapchainImagesKHR(vulkanalia): {err:?}"));
        }
    };
    let _ = unsafe { device.device_wait_idle() };
    unsafe {
        device.destroy_swapchain_khr(swapchain, None);
        present_device.device.destroy_device(None);
    }

    Ok(NativeVulkanVulkanaliaSurfaceSwapchainProbeSnapshot {
        binding: "vulkanalia",
        route: "wayland-surface-swapchain",
        loader: vulkan.loader_name.to_owned(),
        entry_version: vulkan.entry_version.to_string(),
        requested_api_version: ROADMAP_2026_API_VERSION.to_string(),
        enabled_instance_extensions: vulkan
            .extension_selection
            .enabled_instance_extensions
            .clone(),
        missing_instance_extensions: vulkan
            .extension_selection
            .missing_instance_extensions
            .clone(),
        physical_device_count: physical_devices.len(),
        present_queue_family_count,
        wayland_surface_logical_size: handles.logical_size,
        wayland_surface_buffer_size: handles.buffer_size,
        selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot {
            physical_device_index: selection.physical_device_index,
            physical_device_name: selection.physical_device_name,
            physical_device_type: selection.physical_device_type,
            queue_family_index: selection.queue_family_index,
            queue_count: selection.queue_count,
            queue_flags: queue_flag_labels(selection.queue_flags),
            supports_graphics: selection.queue_flags.contains(vk::QueueFlags::GRAPHICS),
            supports_present: true,
            supports_wayland_presentation: selection.supports_wayland_presentation,
        },
        device_extensions: extension_snapshot,
        surface: surface_snapshot,
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
            image_usage: vec![
                "transfer-src",
                "transfer-dst",
                "color-attachment",
                "sampled",
            ],
            create_flags: swapchain_create_flag_labels(swapchain_plan.create_flags),
            present_id2_enabled: swapchain_plan.present_id2_enabled,
            present_wait2_enabled: swapchain_plan.present_wait2_enabled,
        },
        present_backend: "vulkanalia-wayland-surface-swapchain",
        ffmpeg_reference: "references/gilder/ffmpeg/libavutil/vulkan.c",
    })
}

pub(in crate::renderer::native_vulkan::vulkan) fn create_vulkanalia_wayland_surface(
    instance: &Instance,
    handles: NativeWaylandSurfaceHandles,
) -> Result<vk::SurfaceKHR, String> {
    let create_info = vk::WaylandSurfaceCreateInfoKHR::builder()
        .display(handles.display.as_ptr().cast::<vk::wl_display>())
        .surface(handles.surface.as_ptr().cast::<vk::wl_surface>());
    unsafe { instance.create_wayland_surface_khr(&create_info, None) }
        .map_err(|err| format!("vkCreateWaylandSurfaceKHR(vulkanalia): {err:?}"))
}

pub(in crate::renderer::native_vulkan::vulkan) fn select_vulkanalia_present_queue(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    physical_devices: &[vk::PhysicalDevice],
    present_queue_family_count: &mut usize,
) -> Result<NativeVulkanVulkanaliaPresentQueueSelection, String> {
    let mut rejected = Vec::new();
    let mut fallback = None;

    for ranked in ranked_physical_devices(instance, physical_devices)? {
        let physical_device_index = ranked.original_index;
        let physical_device = ranked.physical_device;
        let properties = ranked.properties;
        let physical_device_name = physical_device_name(properties);
        let device_extensions =
            unsafe { instance.enumerate_device_extension_properties(physical_device, None) }
                .map_err(|err| {
                    format!("vkEnumerateDeviceExtensionProperties(vulkanalia present): {err:?}")
                })?
                .into_iter()
                .map(|property| property.extension_name.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
        if !extension_available(&device_extensions, REQUIRED_DEVICE_EXTENSIONS[0]) {
            rejected.push(format!("{physical_device_name} missing VK_KHR_swapchain"));
            continue;
        }

        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        for (queue_family_index, queue_family) in queue_families.iter().enumerate() {
            let supports_present = unsafe {
                instance.get_physical_device_surface_support_khr(
                    physical_device,
                    queue_family_index as u32,
                    surface,
                )
            }
            .map_err(|err| format!("vkGetPhysicalDeviceSurfaceSupportKHR(vulkanalia): {err:?}"))?;
            if !supports_present {
                continue;
            }
            *present_queue_family_count += 1;
            let supports_wayland_presentation = unsafe {
                instance.get_physical_device_wayland_presentation_support_khr(
                    physical_device,
                    queue_family_index as u32,
                    handles.display.as_ptr().cast::<vk::wl_display>(),
                ) == vk::TRUE
            };
            let candidate = NativeVulkanVulkanaliaPresentQueueSelection {
                physical_device_index,
                physical_device,
                physical_device_name: physical_device_name.clone(),
                physical_device_type: format!("{:?}", properties.device_type),
                queue_family_index: queue_family_index as u32,
                queue_count: queue_family.queue_count,
                queue_flags: queue_family.queue_flags,
                supports_wayland_presentation,
                device_extensions: device_extensions.clone(),
            };
            if queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                return Ok(candidate);
            }
            fallback.get_or_insert(candidate);
        }
        if fallback.is_none() {
            rejected.push(format!(
                "{physical_device_name} has no surface-present queue"
            ));
        }
    }

    fallback.ok_or_else(|| {
        if rejected.is_empty() {
            "no Vulkanalia physical device can present to the Wayland surface".to_owned()
        } else {
            format!(
                "no Vulkanalia physical device can present to the Wayland surface: {}",
                rejected.join("; ")
            )
        }
    })
}

fn present_device_extension_snapshot(
    instance: &Instance,
    selection: &NativeVulkanVulkanaliaPresentQueueSelection,
) -> Result<NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot, String> {
    let available_device_extensions = selection.device_extensions.as_slice();
    let required_swapchain = extension_available(available_device_extensions, "VK_KHR_swapchain");
    if !required_swapchain {
        return Err("selected Vulkanalia present device missing VK_KHR_swapchain".to_owned());
    }
    let feature_selection = query_vulkanalia_present_feature_selection(
        instance,
        selection.physical_device,
        available_device_extensions,
    );

    Ok(NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot {
        available_device_extension_count: available_device_extensions.len(),
        enabled_device_extensions: enabled_present_device_extensions(&feature_selection),
        required_swapchain,
        core_features: feature_selection.core_features,
        vulkan_1_4_properties: feature_selection.vulkan_1_4_properties,
        descriptor_heap_properties: feature_selection.descriptor_heap_properties,
        synchronization2_enabled: feature_selection.synchronization2_enabled,
        dynamic_rendering_enabled: feature_selection.dynamic_rendering_enabled,
        descriptor_heap_available: extension_available(
            available_device_extensions,
            DESCRIPTOR_HEAP_EXTENSION_NAME,
        ),
        descriptor_heap_enabled: feature_selection.core_features.descriptor_heap,
        blend_operation_advanced_available: extension_available(
            available_device_extensions,
            BLEND_OPERATION_ADVANCED_EXTENSION_NAME,
        ),
        blend_operation_advanced_enabled: feature_selection.blend_operation_advanced_enabled,
        blend_operation_advanced_coherent_operations: feature_selection
            .blend_operation_advanced_coherent_operations,
        multisampled_render_to_single_sampled_available: extension_available(
            available_device_extensions,
            MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED_EXTENSION_NAME,
        ),
        multisampled_render_to_single_sampled_enabled: feature_selection
            .multisampled_render_to_single_sampled_enabled,
        present_id2_available: extension_available(
            available_device_extensions,
            PRESENT_ID2_EXTENSION_NAME,
        ),
        present_id2_enabled: feature_selection.present_id2_enabled,
        present_wait2_available: extension_available(
            available_device_extensions,
            PRESENT_WAIT2_EXTENSION_NAME,
        ),
        present_wait2_enabled: feature_selection.present_wait2_enabled,
        swapchain_maintenance1_available: extension_available(
            available_device_extensions,
            SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME,
        ),
        swapchain_maintenance1_enabled: feature_selection.swapchain_maintenance1_enabled,
        present_mode_fifo_latest_ready_available: extension_available(
            available_device_extensions,
            PRESENT_MODE_FIFO_LATEST_READY_EXTENSION_NAME,
        ),
        present_mode_fifo_latest_ready_enabled: feature_selection
            .present_mode_fifo_latest_ready_enabled,
        maintenance7_available: extension_available(
            available_device_extensions,
            MAINTENANCE7_EXTENSION_NAME,
        ),
        maintenance7_enabled: feature_selection.maintenance7_enabled,
        maintenance8_available: extension_available(
            available_device_extensions,
            MAINTENANCE8_EXTENSION_NAME,
        ),
        maintenance8_enabled: feature_selection.maintenance8_enabled,
        maintenance9_available: extension_available(
            available_device_extensions,
            MAINTENANCE9_EXTENSION_NAME,
        ),
        maintenance9_enabled: feature_selection.maintenance9_enabled,
        maintenance10_available: extension_available(
            available_device_extensions,
            MAINTENANCE10_EXTENSION_NAME,
        ),
        maintenance10_enabled: feature_selection.maintenance10_enabled,
    })
}

pub(in crate::renderer::native_vulkan::vulkan) fn create_vulkanalia_present_device(
    instance: &Instance,
    selection: &NativeVulkanVulkanaliaPresentQueueSelection,
) -> Result<NativeVulkanVulkanaliaPresentDeviceContext, String> {
    let extension_snapshot = present_device_extension_snapshot(instance, selection)?;
    let feature_selection = query_vulkanalia_present_feature_selection(
        instance,
        selection.physical_device,
        &selection.device_extensions,
    );
    let enabled_device_extensions = enabled_present_device_extensions(&feature_selection);
    let priorities = [1.0_f32];
    let queue_create_info = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(selection.queue_family_index)
        .queue_priorities(&priorities)
        .build();
    let queue_create_infos = [queue_create_info];
    let extension_names = enabled_device_extensions
        .iter()
        .map(|extension| CString::new(*extension).expect("static extension has no nul"))
        .collect::<Vec<_>>();
    let extension_name_ptrs = extension_names
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();

    let core10_features =
        native_vulkan_vulkanalia_vulkan10_device_features(feature_selection.core_features);
    let mut vulkan12_features =
        native_vulkan_vulkanalia_vulkan12_device_features(feature_selection.core_features);
    let mut vulkan13_features =
        native_vulkan_vulkanalia_vulkan13_device_features(feature_selection.core_features);
    let mut vulkan14_features =
        native_vulkan_vulkanalia_vulkan14_device_features(feature_selection.core_features);
    let mut descriptor_heap_features =
        native_vulkan_vulkanalia_descriptor_heap_device_features(feature_selection.core_features);
    let mut present_id2_features = vk::PhysicalDevicePresentId2FeaturesKHR::builder()
        .present_id2(true)
        .build();
    let mut present_wait2_features = vk::PhysicalDevicePresentWait2FeaturesKHR::builder()
        .present_wait2(true)
        .build();
    let mut swapchain_maintenance1_features =
        vk::PhysicalDeviceSwapchainMaintenance1FeaturesKHR::builder()
            .swapchain_maintenance1(true)
            .build();
    let mut present_mode_fifo_latest_ready_features =
        vk::PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR::builder()
            .present_mode_fifo_latest_ready(true)
            .build();
    let mut blend_operation_advanced_features =
        vk::PhysicalDeviceBlendOperationAdvancedFeaturesEXT::builder()
            .advanced_blend_coherent_operations(
                feature_selection.blend_operation_advanced_coherent_operations,
            )
            .build();
    let mut multisampled_render_to_single_sampled_features =
        vk::PhysicalDeviceMultisampledRenderToSingleSampledFeaturesEXT::builder()
            .multisampled_render_to_single_sampled(true)
            .build();
    let mut maintenance7_features = vk::PhysicalDeviceMaintenance7FeaturesKHR::builder()
        .maintenance7(true)
        .build();
    let mut maintenance8_features = vk::PhysicalDeviceMaintenance8FeaturesKHR::builder()
        .maintenance8(true)
        .build();
    let mut maintenance9_features = vk::PhysicalDeviceMaintenance9FeaturesKHR::builder()
        .maintenance9(true)
        .build();
    let mut maintenance10_features = vk::PhysicalDeviceMaintenance10FeaturesKHR::builder()
        .maintenance10(true)
        .build();
    let mut device_create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_name_ptrs);
    if feature_selection
        .core_features
        .enables_vulkan_1_0_features()
    {
        device_create_info = device_create_info.enabled_features(&core10_features);
    }
    if feature_selection
        .core_features
        .enables_vulkan_1_2_features()
    {
        device_create_info = device_create_info.push_next(&mut vulkan12_features);
    }
    if feature_selection
        .core_features
        .enables_vulkan_1_3_features()
    {
        device_create_info = device_create_info.push_next(&mut vulkan13_features);
    }
    if feature_selection
        .core_features
        .enables_vulkan_1_4_features()
    {
        device_create_info = device_create_info.push_next(&mut vulkan14_features);
    }
    if feature_selection
        .core_features
        .enables_descriptor_heap_features()
    {
        device_create_info = device_create_info.push_next(&mut descriptor_heap_features);
    }
    if feature_selection.present_id2_enabled {
        device_create_info = device_create_info.push_next(&mut present_id2_features);
    }
    if feature_selection.present_wait2_enabled {
        device_create_info = device_create_info.push_next(&mut present_wait2_features);
    }
    if feature_selection.swapchain_maintenance1_enabled {
        device_create_info = device_create_info.push_next(&mut swapchain_maintenance1_features);
    }
    if feature_selection.present_mode_fifo_latest_ready_enabled {
        device_create_info =
            device_create_info.push_next(&mut present_mode_fifo_latest_ready_features);
    }
    if feature_selection.blend_operation_advanced_enabled {
        device_create_info = device_create_info.push_next(&mut blend_operation_advanced_features);
    }
    if feature_selection.multisampled_render_to_single_sampled_enabled {
        device_create_info =
            device_create_info.push_next(&mut multisampled_render_to_single_sampled_features);
    }
    if feature_selection.maintenance7_enabled {
        device_create_info = device_create_info.push_next(&mut maintenance7_features);
    }
    if feature_selection.maintenance8_enabled {
        device_create_info = device_create_info.push_next(&mut maintenance8_features);
    }
    if feature_selection.maintenance9_enabled {
        device_create_info = device_create_info.push_next(&mut maintenance9_features);
    }
    if feature_selection.maintenance10_enabled {
        device_create_info = device_create_info.push_next(&mut maintenance10_features);
    }

    let device =
        unsafe { instance.create_device(selection.physical_device, &device_create_info, None) }
            .map_err(|err| format!("vkCreateDevice(vulkanalia present/swapchain): {err:?}"))?;
    let queue = unsafe { device.get_device_queue(selection.queue_family_index, 0) };

    Ok(NativeVulkanVulkanaliaPresentDeviceContext {
        device,
        queue,
        extension_snapshot,
        feature_selection,
    })
}

include!("swapchain/capabilities_and_plan.rs");
