#![allow(dead_code)]

use std::ffi::{CStr, CString};

use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrGetSurfaceCapabilities2ExtensionInstanceCommands,
    KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
    KhrWaylandSurfaceExtensionInstanceCommands,
};

use crate::renderer::native_wayland::{
    NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};

use super::features::{
    DESCRIPTOR_HEAP_EXTENSION_NAME, NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaVulkan14PropertySnapshot, native_vulkan_vulkanalia_core_feature_snapshot,
    native_vulkan_vulkanalia_descriptor_heap_device_features,
    native_vulkan_vulkanalia_vulkan12_device_features,
    native_vulkan_vulkanalia_vulkan13_device_features,
    native_vulkan_vulkanalia_vulkan14_device_features,
};
use super::instance::{
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};

const GET_SURFACE_CAPABILITIES2_EXTENSION_NAME: &str = "VK_KHR_get_surface_capabilities2";
const SURFACE_MAINTENANCE1_EXTENSION_NAME: &str = "VK_KHR_surface_maintenance1";
pub(in crate::renderer::native_vulkan::vulkan) const REQUIRED_INSTANCE_EXTENSIONS: &[&str] =
    &["VK_KHR_surface", "VK_KHR_wayland_surface"];
pub(in crate::renderer::native_vulkan::vulkan) const OPTIONAL_INSTANCE_EXTENSIONS: &[&str] = &[
    GET_SURFACE_CAPABILITIES2_EXTENSION_NAME,
    SURFACE_MAINTENANCE1_EXTENSION_NAME,
];
const REQUIRED_DEVICE_EXTENSIONS: &[&str] = &["VK_KHR_swapchain"];
const PRESENT_ID2_EXTENSION_NAME: &str = "VK_KHR_present_id2";
const PRESENT_WAIT2_EXTENSION_NAME: &str = "VK_KHR_present_wait2";
const SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME: &str = "VK_KHR_swapchain_maintenance1";
const PRESENT_MODE_FIFO_LATEST_READY_EXTENSION_NAME: &str = "VK_KHR_present_mode_fifo_latest_ready";

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
    pub available_device_extensions: Vec<String>,
    pub enabled_device_extensions: Vec<&'static str>,
    pub required_swapchain: bool,
    pub core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
    pub vulkan_1_4_properties: NativeVulkanVulkanaliaVulkan14PropertySnapshot,
    pub descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    pub synchronization2_enabled: bool,
    pub dynamic_rendering_enabled: bool,
    pub descriptor_heap_available: bool,
    pub descriptor_heap_enabled: bool,
    pub present_id_available: bool,
    pub present_id_enabled: bool,
    pub present_id2_available: bool,
    pub present_id2_enabled: bool,
    pub present_wait_available: bool,
    pub present_wait_enabled: bool,
    pub present_wait2_available: bool,
    pub present_wait2_enabled: bool,
    pub swapchain_maintenance1_available: bool,
    pub swapchain_maintenance1_enabled: bool,
    pub present_mode_fifo_latest_ready_available: bool,
    pub present_mode_fifo_latest_ready_enabled: bool,
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
    pub supports_transfer_dst: bool,
    pub supports_color_attachment: bool,
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
    pub image_count: usize,
    pub min_image_count: u32,
    pub composite_alpha: &'static str,
    pub image_usage: Vec<&'static str>,
    pub create_flags: Vec<&'static str>,
    pub present_id2_enabled: bool,
    pub present_wait2_enabled: bool,
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
    pub(in crate::renderer::native_vulkan::vulkan) present_id_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) present_id2_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) present_wait_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) present_wait2_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) swapchain_maintenance1_enabled: bool,
    pub(in crate::renderer::native_vulkan::vulkan) present_mode_fifo_latest_ready_enabled: bool,
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

    let mut requested_instance_extensions = REQUIRED_INSTANCE_EXTENSIONS.to_vec();
    requested_instance_extensions.extend_from_slice(OPTIONAL_INSTANCE_EXTENSIONS);
    let vulkan = native_vulkan_vulkanalia_create_instance_with_required_extensions(
        &requested_instance_extensions,
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
        vulkanalia_surface_maintenance1_enabled(vulkan),
    )?;
    let extension_snapshot = present_device.extension_snapshot.clone();
    let device = &present_device.device;
    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
        vulkanalia_surface_capabilities2_enabled(vulkan),
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
        requested_api_version: Version::V1_4_0.to_string(),
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
            image_count: swapchain_images.len(),
            min_image_count: swapchain_plan.image_count,
            composite_alpha: composite_alpha_label(swapchain_plan.composite_alpha),
            image_usage: vec!["transfer-dst", "color-attachment"],
            create_flags: swapchain_create_flag_labels(swapchain_plan.create_flags),
            present_id2_enabled: swapchain_plan.present_id2_enabled,
            present_wait2_enabled: swapchain_plan.present_wait2_enabled,
        },
        present_backend: "vulkanalia-wayland-surface-swapchain",
        ffmpeg_reference: "references/ffmpeg/libavutil/vulkan.c",
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

    for (physical_device_index, physical_device) in physical_devices.iter().copied().enumerate() {
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
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
    surface_maintenance1_enabled: bool,
) -> Result<NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot, String> {
    let mut available_device_extensions = selection.device_extensions.clone();
    available_device_extensions.sort();
    let required_swapchain = extension_available(&available_device_extensions, "VK_KHR_swapchain");
    if !required_swapchain {
        return Err("selected Vulkanalia present device missing VK_KHR_swapchain".to_owned());
    }
    let feature_selection = query_vulkanalia_present_feature_selection(
        instance,
        selection.physical_device,
        &available_device_extensions,
        surface_maintenance1_enabled,
    );

    Ok(NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot {
        available_device_extensions,
        enabled_device_extensions: enabled_present_device_extensions(&feature_selection),
        required_swapchain,
        core_features: feature_selection.core_features,
        vulkan_1_4_properties: feature_selection.vulkan_1_4_properties,
        descriptor_heap_properties: feature_selection.descriptor_heap_properties,
        synchronization2_enabled: feature_selection.synchronization2_enabled,
        dynamic_rendering_enabled: feature_selection.dynamic_rendering_enabled,
        descriptor_heap_available: extension_available(
            &selection.device_extensions,
            DESCRIPTOR_HEAP_EXTENSION_NAME,
        ),
        descriptor_heap_enabled: feature_selection.core_features.descriptor_heap,
        present_id_available: false,
        present_id_enabled: false,
        present_id2_available: extension_available(
            &selection.device_extensions,
            PRESENT_ID2_EXTENSION_NAME,
        ),
        present_id2_enabled: feature_selection.present_id2_enabled,
        present_wait_available: false,
        present_wait_enabled: false,
        present_wait2_available: extension_available(
            &selection.device_extensions,
            PRESENT_WAIT2_EXTENSION_NAME,
        ),
        present_wait2_enabled: feature_selection.present_wait2_enabled,
        swapchain_maintenance1_available: extension_available(
            &selection.device_extensions,
            SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME,
        ),
        swapchain_maintenance1_enabled: feature_selection.swapchain_maintenance1_enabled,
        present_mode_fifo_latest_ready_available: extension_available(
            &selection.device_extensions,
            PRESENT_MODE_FIFO_LATEST_READY_EXTENSION_NAME,
        ),
        present_mode_fifo_latest_ready_enabled: feature_selection
            .present_mode_fifo_latest_ready_enabled,
    })
}

pub(in crate::renderer::native_vulkan::vulkan) fn create_vulkanalia_present_device(
    instance: &Instance,
    selection: &NativeVulkanVulkanaliaPresentQueueSelection,
    surface_maintenance1_enabled: bool,
) -> Result<NativeVulkanVulkanaliaPresentDeviceContext, String> {
    let extension_snapshot =
        present_device_extension_snapshot(instance, selection, surface_maintenance1_enabled)?;
    let feature_selection = query_vulkanalia_present_feature_selection(
        instance,
        selection.physical_device,
        &selection.device_extensions,
        surface_maintenance1_enabled,
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
    let mut device_create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_name_ptrs);
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

pub(in crate::renderer::native_vulkan::vulkan) fn query_vulkanalia_present_feature_selection(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    _surface_maintenance1_enabled: bool,
) -> NativeVulkanVulkanaliaPresentFeatureSelection {
    let (mut core_features, vulkan_1_4_properties, descriptor_heap_properties) =
        native_vulkan_vulkanalia_core_feature_snapshot(instance, physical_device);
    if !extension_available(device_extensions, DESCRIPTOR_HEAP_EXTENSION_NAME) {
        core_features.descriptor_heap = false;
        core_features.descriptor_heap_capture_replay = false;
    }
    let synchronization2_enabled = core_features.synchronization2;
    let dynamic_rendering_enabled = core_features.dynamic_rendering;
    let present_id2_enabled = extension_available(device_extensions, PRESENT_ID2_EXTENSION_NAME)
        && query_present_id2_feature(instance, physical_device);
    let present_wait2_enabled = present_id2_enabled
        && extension_available(device_extensions, PRESENT_WAIT2_EXTENSION_NAME)
        && query_present_wait2_feature(instance, physical_device);
    let swapchain_maintenance1_enabled =
        extension_available(device_extensions, SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME)
            && query_swapchain_maintenance1_feature(instance, physical_device);
    let present_mode_fifo_latest_ready_enabled =
        extension_available(
            device_extensions,
            PRESENT_MODE_FIFO_LATEST_READY_EXTENSION_NAME,
        ) && query_present_mode_fifo_latest_ready_feature(instance, physical_device);

    NativeVulkanVulkanaliaPresentFeatureSelection {
        core_features,
        vulkan_1_4_properties,
        descriptor_heap_properties,
        synchronization2_enabled,
        dynamic_rendering_enabled,
        present_id_enabled: false,
        present_id2_enabled,
        present_wait_enabled: false,
        present_wait2_enabled,
        swapchain_maintenance1_enabled,
        present_mode_fifo_latest_ready_enabled,
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn vulkanalia_surface_maintenance1_enabled(
    vulkan: &super::instance::NativeVulkanVulkanaliaInstance,
) -> bool {
    vulkan
        .extension_selection
        .enabled_instance_extensions
        .contains(&SURFACE_MAINTENANCE1_EXTENSION_NAME)
}

pub(in crate::renderer::native_vulkan::vulkan) fn enabled_present_device_extensions(
    feature_selection: &NativeVulkanVulkanaliaPresentFeatureSelection,
) -> Vec<&'static str> {
    let mut extensions = vec!["VK_KHR_swapchain"];
    if feature_selection.present_id2_enabled {
        extensions.push(PRESENT_ID2_EXTENSION_NAME);
    }
    if feature_selection.present_wait2_enabled {
        extensions.push(PRESENT_WAIT2_EXTENSION_NAME);
    }
    if feature_selection.swapchain_maintenance1_enabled {
        extensions.push(SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME);
    }
    if feature_selection.present_mode_fifo_latest_ready_enabled {
        extensions.push(PRESENT_MODE_FIFO_LATEST_READY_EXTENSION_NAME);
    }
    if feature_selection.core_features.descriptor_heap {
        extensions.push(DESCRIPTOR_HEAP_EXTENSION_NAME);
    }
    extensions
}

pub(in crate::renderer::native_vulkan::vulkan) fn vulkanalia_surface_capabilities2_enabled(
    vulkan: &super::instance::NativeVulkanVulkanaliaInstance,
) -> bool {
    vulkan
        .extension_selection
        .enabled_instance_extensions
        .contains(&GET_SURFACE_CAPABILITIES2_EXTENSION_NAME)
}

pub(in crate::renderer::native_vulkan::vulkan) fn create_vulkanalia_swapchain_plan(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    buffer_size: (u32, u32),
    surface_capabilities2_enabled: bool,
    feature_selection: &NativeVulkanVulkanaliaPresentFeatureSelection,
) -> Result<NativeVulkanVulkanaliaSwapchainPlan, String> {
    let capabilities =
        unsafe { instance.get_physical_device_surface_capabilities_khr(physical_device, surface) }
            .map_err(|err| {
                format!("vkGetPhysicalDeviceSurfaceCapabilitiesKHR(vulkanalia): {err:?}")
            })?;
    let present_timing_capabilities = query_surface_present_timing_capabilities(
        instance,
        physical_device,
        surface,
        surface_capabilities2_enabled,
    )?;
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::TRANSFER_DST)
    {
        return Err("Vulkanalia swapchain surface does not support TRANSFER_DST".to_owned());
    }
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
    {
        return Err("Vulkanalia swapchain surface does not support COLOR_ATTACHMENT".to_owned());
    }
    let formats =
        unsafe { instance.get_physical_device_surface_formats_khr(physical_device, surface) }
            .map_err(|err| format!("vkGetPhysicalDeviceSurfaceFormatsKHR(vulkanalia): {err:?}"))?;
    let format = choose_surface_format(&formats)?;
    let present_modes =
        unsafe { instance.get_physical_device_surface_present_modes_khr(physical_device, surface) }
            .map_err(|err| {
                format!("vkGetPhysicalDeviceSurfacePresentModesKHR(vulkanalia): {err:?}")
            })?;
    let present_mode = choose_present_mode(
        &present_modes,
        feature_selection.present_mode_fifo_latest_ready_enabled,
    );
    let extent = choose_swapchain_extent(&capabilities, buffer_size)?;
    let image_count = swapchain_image_count(&capabilities);
    let composite_alpha = choose_composite_alpha(capabilities.supported_composite_alpha);
    let present_id2_enabled =
        feature_selection.present_id2_enabled && present_timing_capabilities.present_id2_supported;
    let present_wait2_enabled = feature_selection.present_wait2_enabled
        && present_id2_enabled
        && present_timing_capabilities.present_wait2_supported;
    let create_flags = swapchain_create_flags(present_id2_enabled, present_wait2_enabled);
    let create_info = vk::SwapchainCreateInfoKHR::builder()
        .flags(create_flags)
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(capabilities.current_transform)
        .composite_alpha(composite_alpha)
        .present_mode(present_mode)
        .clipped(true)
        .build();

    Ok(NativeVulkanVulkanaliaSwapchainPlan {
        create_info,
        format,
        present_mode,
        extent,
        image_count,
        composite_alpha,
        create_flags,
        surface_present_id2_supported: present_timing_capabilities.present_id2_supported,
        surface_present_wait2_supported: present_timing_capabilities.present_wait2_supported,
        present_id2_enabled,
        present_wait2_enabled,
    })
}

fn surface_snapshot_from_plan(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    _swapchain_plan: &NativeVulkanVulkanaliaSwapchainPlan,
) -> Result<NativeVulkanVulkanaliaSurfaceSnapshot, String> {
    let capabilities =
        unsafe { instance.get_physical_device_surface_capabilities_khr(physical_device, surface) }
            .map_err(|err| {
                format!("vkGetPhysicalDeviceSurfaceCapabilitiesKHR(vulkanalia snapshot): {err:?}")
            })?;
    let formats =
        unsafe { instance.get_physical_device_surface_formats_khr(physical_device, surface) }
            .map_err(|err| {
                format!("vkGetPhysicalDeviceSurfaceFormatsKHR(vulkanalia snapshot): {err:?}")
            })?;
    let present_modes =
        unsafe { instance.get_physical_device_surface_present_modes_khr(physical_device, surface) }
            .map_err(|err| {
                format!("vkGetPhysicalDeviceSurfacePresentModesKHR(vulkanalia snapshot): {err:?}")
            })?;

    Ok(NativeVulkanVulkanaliaSurfaceSnapshot {
        capabilities: NativeVulkanVulkanaliaSurfaceCapabilitiesSnapshot {
            min_image_count: capabilities.min_image_count,
            max_image_count: capabilities.max_image_count,
            current_extent: extent_tuple(capabilities.current_extent),
            min_image_extent: (
                capabilities.min_image_extent.width,
                capabilities.min_image_extent.height,
            ),
            max_image_extent: (
                capabilities.max_image_extent.width,
                capabilities.max_image_extent.height,
            ),
            supports_transfer_dst: capabilities
                .supported_usage_flags
                .contains(vk::ImageUsageFlags::TRANSFER_DST),
            supports_color_attachment: capabilities
                .supported_usage_flags
                .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT),
            present_id2_supported: _swapchain_plan.surface_present_id2_supported,
            present_wait2_supported: _swapchain_plan.surface_present_wait2_supported,
        },
        surface_format_count: formats.len(),
        surface_formats: formats
            .into_iter()
            .map(|format| NativeVulkanVulkanaliaSurfaceFormatSnapshot {
                format: format!("{:?}", format.format),
                color_space: format!("{:?}", format.color_space),
            })
            .collect(),
        present_mode_count: present_modes.len(),
        present_modes: present_modes.into_iter().map(present_mode_label).collect(),
    })
}

fn choose_surface_format(formats: &[vk::SurfaceFormatKHR]) -> Result<vk::SurfaceFormatKHR, String> {
    formats
        .iter()
        .copied()
        .find(|format| {
            format.format == vk::Format::B8G8R8A8_UNORM
                && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .or_else(|| {
            formats.iter().copied().find(|format| {
                format.format == vk::Format::B8G8R8A8_SRGB
                    && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
        })
        .or_else(|| formats.first().copied())
        .ok_or_else(|| "Vulkanalia surface reported no surface formats".to_owned())
}

fn choose_present_mode(
    present_modes: &[vk::PresentModeKHR],
    present_mode_fifo_latest_ready_enabled: bool,
) -> vk::PresentModeKHR {
    if present_mode_fifo_latest_ready_enabled
        && present_modes.contains(&vk::PresentModeKHR::FIFO_LATEST_READY)
    {
        return vk::PresentModeKHR::FIFO_LATEST_READY;
    }
    if present_modes.contains(&vk::PresentModeKHR::FIFO_RELAXED) {
        return vk::PresentModeKHR::FIFO_RELAXED;
    }
    vk::PresentModeKHR::FIFO
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SurfacePresentTimingCapabilities {
    present_id2_supported: bool,
    present_wait2_supported: bool,
}

fn query_surface_present_timing_capabilities(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    surface_capabilities2_enabled: bool,
) -> Result<SurfacePresentTimingCapabilities, String> {
    if !surface_capabilities2_enabled {
        return Ok(SurfacePresentTimingCapabilities {
            present_id2_supported: false,
            present_wait2_supported: false,
        });
    }

    let surface_info = vk::PhysicalDeviceSurfaceInfo2KHR::builder()
        .surface(surface)
        .build();
    let mut present_id2 = vk::SurfaceCapabilitiesPresentId2KHR::default();
    let mut present_wait2 = vk::SurfaceCapabilitiesPresentWait2KHR::default();
    let mut capabilities2 = vk::SurfaceCapabilities2KHR::builder()
        .push_next(&mut present_id2)
        .push_next(&mut present_wait2)
        .build();
    unsafe {
        instance.get_physical_device_surface_capabilities2_khr(
            physical_device,
            &surface_info,
            &mut capabilities2,
        )
    }
    .map_err(|err| {
        format!("vkGetPhysicalDeviceSurfaceCapabilities2KHR(vulkanalia present timing): {err:?}")
    })?;

    Ok(SurfacePresentTimingCapabilities {
        present_id2_supported: present_id2.present_id2_supported != 0,
        present_wait2_supported: present_wait2.present_wait2_supported != 0,
    })
}

fn swapchain_create_flags(
    present_id2_enabled: bool,
    present_wait2_enabled: bool,
) -> vk::SwapchainCreateFlagsKHR {
    let mut flags = vk::SwapchainCreateFlagsKHR::empty();
    if present_id2_enabled {
        flags |= vk::SwapchainCreateFlagsKHR::PRESENT_ID_2;
    }
    if present_wait2_enabled {
        flags |= vk::SwapchainCreateFlagsKHR::PRESENT_WAIT_2;
    }
    flags
}

fn choose_swapchain_extent(
    capabilities: &vk::SurfaceCapabilitiesKHR,
    buffer_size: (u32, u32),
) -> Result<vk::Extent2D, String> {
    if let Some((width, height)) = extent_tuple(capabilities.current_extent) {
        return Ok(vk::Extent2D { width, height });
    }
    let width = buffer_size.0.clamp(
        capabilities.min_image_extent.width,
        capabilities.max_image_extent.width,
    );
    let height = buffer_size.1.clamp(
        capabilities.min_image_extent.height,
        capabilities.max_image_extent.height,
    );
    if width == 0 || height == 0 {
        return Err("Vulkanalia swapchain extent resolved to zero".to_owned());
    }
    Ok(vk::Extent2D { width, height })
}

fn swapchain_image_count(capabilities: &vk::SurfaceCapabilitiesKHR) -> u32 {
    let preferred = capabilities.min_image_count.saturating_add(1).max(3);
    if capabilities.max_image_count > 0 {
        preferred.min(capabilities.max_image_count)
    } else {
        preferred
    }
}

fn choose_composite_alpha(flags: vk::CompositeAlphaFlagsKHR) -> vk::CompositeAlphaFlagsKHR {
    [
        vk::CompositeAlphaFlagsKHR::OPAQUE,
        vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::INHERIT,
    ]
    .into_iter()
    .find(|flag| flags.contains(*flag))
    .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE)
}

fn query_present_id2_feature(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let mut feature = vk::PhysicalDevicePresentId2FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.present_id2 != 0
}

fn query_present_wait2_feature(instance: &Instance, physical_device: vk::PhysicalDevice) -> bool {
    let mut feature = vk::PhysicalDevicePresentWait2FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.present_wait2 != 0
}

fn query_swapchain_maintenance1_feature(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> bool {
    let mut feature = vk::PhysicalDeviceSwapchainMaintenance1FeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.swapchain_maintenance1 != 0
}

fn query_present_mode_fifo_latest_ready_feature(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> bool {
    let mut feature = vk::PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
    feature.present_mode_fifo_latest_ready != 0
}

fn extension_available(available: &[String], extension: &str) -> bool {
    available.iter().any(|available| available == extension)
}

fn physical_device_name(properties: vk::PhysicalDeviceProperties) -> String {
    unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

pub(in crate::renderer::native_vulkan::vulkan) fn queue_flag_labels(
    flags: vk::QueueFlags,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::QueueFlags::GRAPHICS) {
        labels.push("graphics");
    }
    if flags.contains(vk::QueueFlags::COMPUTE) {
        labels.push("compute");
    }
    if flags.contains(vk::QueueFlags::TRANSFER) {
        labels.push("transfer");
    }
    if flags.contains(vk::QueueFlags::VIDEO_DECODE_KHR) {
        labels.push("video-decode");
    }
    labels
}

pub(in crate::renderer::native_vulkan::vulkan) fn present_mode_label(
    mode: vk::PresentModeKHR,
) -> &'static str {
    match mode {
        vk::PresentModeKHR::IMMEDIATE => "immediate",
        vk::PresentModeKHR::MAILBOX => "mailbox",
        vk::PresentModeKHR::FIFO => "fifo",
        vk::PresentModeKHR::FIFO_RELAXED => "fifo-relaxed",
        vk::PresentModeKHR::FIFO_LATEST_READY => "fifo-latest-ready",
        vk::PresentModeKHR::SHARED_DEMAND_REFRESH => "shared-demand-refresh",
        vk::PresentModeKHR::SHARED_CONTINUOUS_REFRESH => "shared-continuous-refresh",
        _ => "unknown",
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn composite_alpha_label(
    flags: vk::CompositeAlphaFlagsKHR,
) -> &'static str {
    if flags == vk::CompositeAlphaFlagsKHR::OPAQUE {
        "opaque"
    } else if flags == vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED {
        "pre-multiplied"
    } else if flags == vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED {
        "post-multiplied"
    } else if flags == vk::CompositeAlphaFlagsKHR::INHERIT {
        "inherit"
    } else {
        "unknown"
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn swapchain_create_flag_labels(
    flags: vk::SwapchainCreateFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::SwapchainCreateFlagsKHR::PRESENT_ID_2) {
        labels.push("present-id2");
    }
    if flags.contains(vk::SwapchainCreateFlagsKHR::PRESENT_WAIT_2) {
        labels.push("present-wait2");
    }
    labels
}

fn extent_tuple(extent: vk::Extent2D) -> Option<(u32, u32)> {
    if extent.width == u32::MAX || extent.height == u32::MAX {
        None
    } else {
        Some((extent.width, extent.height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_device_extensions_keep_swapchain_required() {
        let disabled = NativeVulkanVulkanaliaPresentFeatureSelection {
            core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot::default(),
            vulkan_1_4_properties: NativeVulkanVulkanaliaVulkan14PropertySnapshot::default(),
            descriptor_heap_properties:
                NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            synchronization2_enabled: false,
            dynamic_rendering_enabled: false,
            present_id_enabled: false,
            present_id2_enabled: false,
            present_wait_enabled: false,
            present_wait2_enabled: false,
            swapchain_maintenance1_enabled: false,
            present_mode_fifo_latest_ready_enabled: false,
        };
        let enabled = NativeVulkanVulkanaliaPresentFeatureSelection {
            present_id_enabled: true,
            present_wait_enabled: true,
            swapchain_maintenance1_enabled: true,
            ..disabled
        };
        let enabled2 = NativeVulkanVulkanaliaPresentFeatureSelection {
            present_id_enabled: true,
            present_id2_enabled: true,
            present_wait_enabled: true,
            present_wait2_enabled: true,
            swapchain_maintenance1_enabled: true,
            ..disabled
        };
        let descriptor_heap_enabled = NativeVulkanVulkanaliaPresentFeatureSelection {
            core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot {
                descriptor_heap: true,
                ..NativeVulkanVulkanaliaCoreFeatureSnapshot::default()
            },
            ..disabled
        };
        let fifo_latest_ready_enabled = NativeVulkanVulkanaliaPresentFeatureSelection {
            present_mode_fifo_latest_ready_enabled: true,
            ..disabled
        };

        assert_eq!(
            enabled_present_device_extensions(&disabled),
            vec!["VK_KHR_swapchain"]
        );
        assert_eq!(
            enabled_present_device_extensions(&enabled),
            vec!["VK_KHR_swapchain", SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME,]
        );
        assert_eq!(
            enabled_present_device_extensions(&enabled2),
            vec![
                "VK_KHR_swapchain",
                PRESENT_ID2_EXTENSION_NAME,
                PRESENT_WAIT2_EXTENSION_NAME,
                SWAPCHAIN_MAINTENANCE1_EXTENSION_NAME,
            ]
        );
        assert_eq!(
            enabled_present_device_extensions(&descriptor_heap_enabled),
            vec!["VK_KHR_swapchain", DESCRIPTOR_HEAP_EXTENSION_NAME]
        );
        assert_eq!(
            enabled_present_device_extensions(&fifo_latest_ready_enabled),
            vec![
                "VK_KHR_swapchain",
                PRESENT_MODE_FIFO_LATEST_READY_EXTENSION_NAME,
            ]
        );
    }

    #[test]
    fn swapchain_create_flags_report_present_id2_and_wait2() {
        let disabled = swapchain_create_flags(false, false);
        let id2 = swapchain_create_flags(true, false);
        let wait2 = swapchain_create_flags(true, true);

        assert!(disabled.is_empty());
        assert_eq!(swapchain_create_flag_labels(disabled), Vec::<&str>::new());
        assert_eq!(swapchain_create_flag_labels(id2), vec!["present-id2"]);
        assert_eq!(
            swapchain_create_flag_labels(wait2),
            vec!["present-id2", "present-wait2"]
        );
    }

    #[test]
    fn present_mode_prefers_low_blocking_video_swapchain_modes() {
        assert_eq!(
            choose_present_mode(
                &[
                    vk::PresentModeKHR::FIFO,
                    vk::PresentModeKHR::MAILBOX,
                    vk::PresentModeKHR::FIFO_LATEST_READY,
                ],
                true,
            ),
            vk::PresentModeKHR::FIFO_LATEST_READY
        );
        assert_eq!(
            choose_present_mode(
                &[
                    vk::PresentModeKHR::FIFO,
                    vk::PresentModeKHR::MAILBOX,
                    vk::PresentModeKHR::FIFO_LATEST_READY,
                ],
                false,
            ),
            vk::PresentModeKHR::FIFO
        );
        assert_eq!(
            choose_present_mode(&[vk::PresentModeKHR::MAILBOX], true),
            vk::PresentModeKHR::FIFO
        );
        assert_eq!(
            choose_present_mode(
                &[vk::PresentModeKHR::FIFO, vk::PresentModeKHR::FIFO_RELAXED,],
                true,
            ),
            vk::PresentModeKHR::FIFO_RELAXED
        );
        assert_eq!(
            choose_present_mode(&[vk::PresentModeKHR::FIFO], true),
            vk::PresentModeKHR::FIFO
        );
        assert_eq!(
            choose_present_mode(
                &[
                    vk::PresentModeKHR::FIFO,
                    vk::PresentModeKHR::FIFO_LATEST_READY,
                ],
                true,
            ),
            vk::PresentModeKHR::FIFO_LATEST_READY
        );
        assert_eq!(
            choose_present_mode(
                &[
                    vk::PresentModeKHR::FIFO,
                    vk::PresentModeKHR::FIFO_LATEST_READY,
                ],
                false,
            ),
            vk::PresentModeKHR::FIFO
        );
    }

    #[test]
    fn swapchain_image_count_prefers_triple_buffering() {
        let mut capabilities = vk::SurfaceCapabilitiesKHR::default();
        capabilities.min_image_count = 2;
        capabilities.max_image_count = 0;
        assert_eq!(swapchain_image_count(&capabilities), 3);

        capabilities.max_image_count = 2;
        assert_eq!(swapchain_image_count(&capabilities), 2);
    }

    #[test]
    fn unknown_surface_extent_is_none() {
        assert_eq!(
            extent_tuple(vk::Extent2D {
                width: u32::MAX,
                height: 1080,
            }),
            None
        );
        assert_eq!(
            extent_tuple(vk::Extent2D {
                width: 1920,
                height: 1080,
            }),
            Some((1920, 1080))
        );
    }
}
