#![allow(dead_code)]

use std::ffi::CString;

use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
    KhrWaylandSurfaceExtensionInstanceCommands,
};

use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;
use crate::renderer::native_wayland::{
    NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};

use super::instance::{
    NativeVulkanVulkanaliaInstance,
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::queue_probe::native_vulkan_vulkanalia_video_decode_queue_family_indices;
use super::swapchain::{
    NativeVulkanVulkanaliaSwapchainSnapshot, OPTIONAL_INSTANCE_EXTENSIONS,
    REQUIRED_INSTANCE_EXTENSIONS, composite_alpha_label, create_vulkanalia_swapchain_plan,
    create_vulkanalia_wayland_surface, enabled_present_device_extensions, present_mode_label,
    query_vulkanalia_present_feature_selection, queue_flag_labels,
};
use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;
use super::video_device::{
    NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
    native_vulkan_vulkanalia_video_decode_device_extensions,
    native_vulkan_vulkanalia_video_decode_required_device_extensions,
    native_vulkan_vulkanalia_video_device_extension_available,
    native_vulkan_vulkanalia_video_device_feature_selection,
};
use super::video_profile_labels::video_decode_capability_flag_labels;
use super::video_session::{
    NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    native_vulkan_vulkanalia_bind_video_session_memory_resources,
    native_vulkan_vulkanalia_create_video_session, native_vulkan_vulkanalia_destroy_video_session,
    native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources,
};
use super::video_session_capabilities::{
    native_vulkan_vulkanalia_video_session_effective_picture_format,
    native_vulkan_vulkanalia_video_session_extent_supported,
    native_vulkan_vulkanalia_video_session_max_active_reference_pictures,
    native_vulkan_vulkanalia_video_session_max_dpb_slots,
    with_native_vulkan_vulkanalia_video_session_capabilities,
};
use super::video_session_images::{
    NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot,
    native_vulkan_vulkanalia_create_video_session_resource_image,
    native_vulkan_vulkanalia_destroy_video_session_resource_image,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaVideoPresentDeviceProbeOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub codec: NativeVulkanVideoSessionCodec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaVideoPresentSessionProbeOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub loader: String,
    pub requested_api_version: String,
    pub codec: NativeVulkanVideoSessionCodec,
    pub physical_device_index: usize,
    pub physical_device_name: String,
    pub physical_device_type: String,
    pub api_version: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub driver_version: u32,
    pub single_logical_device_created: bool,
    pub enabled_device_extensions: Vec<&'static str>,
    pub video_enabled_device_extensions: Vec<&'static str>,
    pub present_enabled_device_extensions: Vec<&'static str>,
    pub feature_selection: NativeVulkanVulkanaliaVideoPresentFeatureSnapshot,
    pub video_queue: NativeVulkanVulkanaliaVideoPresentQueueSnapshot,
    pub present_queue: NativeVulkanVulkanaliaVideoPresentQueueSnapshot,
    pub same_queue_family: bool,
    pub queue_family_model: &'static str,
    pub decoded_image_resource_sharing_model: &'static str,
    pub swapchain: NativeVulkanVulkanaliaSwapchainSnapshot,
    pub present_backend: &'static str,
    pub decoded_image_present_boundary: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub codec: NativeVulkanVideoSessionCodec,
    pub requested_extent: (u32, u32),
    pub device: NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot,
    pub video_session_created: bool,
    pub memory_binding: NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    pub resource_image: NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot,
    pub picture_format: String,
    pub decode_capability_flags: Vec<&'static str>,
    pub session_max_dpb_slots: u32,
    pub session_max_active_reference_pictures: u32,
    pub resource_queue_family_indices: Vec<u32>,
    pub resource_queue_sharing_model: &'static str,
    pub decoded_image_zero_copy_presentable_candidate: bool,
    pub decoded_image_present_boundary: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoPresentFeatureSnapshot {
    pub synchronization2_enabled: bool,
    pub video_maintenance1_enabled: bool,
    pub video_maintenance2_enabled: bool,
    pub inline_session_parameters_enabled: bool,
    pub present_id_enabled: bool,
    pub present_wait_enabled: bool,
    pub present_id2_enabled: bool,
    pub present_wait2_enabled: bool,
    pub swapchain_maintenance1_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoPresentQueueSnapshot {
    pub queue_family_index: u32,
    pub queue_count: u32,
    pub queue_flags: Vec<&'static str>,
    pub supports_video_decode: bool,
    pub supports_graphics: bool,
    pub supports_present: bool,
    pub supports_wayland_presentation: bool,
}

struct NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection {
    physical_device_index: usize,
    physical_device: vk::PhysicalDevice,
    properties: vk::PhysicalDeviceProperties,
    device_extensions: Vec<String>,
    video_queue_family_index: u32,
    video_queue_count: u32,
    video_queue_flags: vk::QueueFlags,
    present_queue_family_index: u32,
    present_queue_count: u32,
    present_queue_flags: vk::QueueFlags,
    present_supports_wayland: bool,
}

struct NativeVulkanVulkanaliaVideoPresentDeviceContext {
    device: Device,
    video_queue: vk::Queue,
    present_queue: vk::Queue,
    enabled_device_extensions: Vec<&'static str>,
    video_enabled_device_extensions: Vec<&'static str>,
    present_enabled_device_extensions: Vec<&'static str>,
    video_feature_selection: NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
    present_feature_selection: super::swapchain::NativeVulkanVulkanaliaPresentFeatureSelection,
}

struct VideoPresentSessionResourceSnapshots {
    memory_binding: NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    resource_image: NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot,
    resource_queue_family_indices: Vec<u32>,
}

pub fn probe_native_vulkan_vulkanalia_video_present_device(
    options: NativeVulkanVulkanaliaVideoPresentDeviceProbeOptions,
) -> Result<NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot, String> {
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
    let result = probe_video_present_device_inner(&vulkan, handles, options.codec);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

pub fn probe_native_vulkan_vulkanalia_video_present_session(
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot, String> {
    if options.width == 0 || options.height == 0 {
        return Err("Vulkanalia video present session probe requires non-zero extent".to_owned());
    }

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
    let result = probe_video_present_session_inner(&vulkan, handles, options);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn probe_video_present_device_inner(
    vulkan: &NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
    codec: NativeVulkanVideoSessionCodec,
) -> Result<NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot, String> {
    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result = with_video_present_device(instance, surface, handles, vulkan, codec);
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

fn probe_video_present_session_inner(
    vulkan: &NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot, String> {
    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result = with_video_present_session(instance, surface, handles, vulkan, options);
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

fn with_video_present_device(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &NativeVulkanVulkanaliaInstance,
    codec: NativeVulkanVideoSessionCodec,
) -> Result<NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices(vulkanalia video present): {err:?}"))?;
    let selection =
        select_video_present_physical_device(instance, surface, handles, &physical_devices, codec)?;
    let context = create_video_present_device(instance, &selection, codec)?;
    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            unsafe {
                context.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let device = &context.device;
    let swapchain = match unsafe { device.create_swapchain_khr(&swapchain_plan.create_info, None) }
    {
        Ok(swapchain) => swapchain,
        Err(err) => {
            unsafe {
                context.device.destroy_device(None);
            }
            return Err(format!(
                "vkCreateSwapchainKHR(vulkanalia video present): {err:?}"
            ));
        }
    };
    let swapchain_images = match unsafe { device.get_swapchain_images_khr(swapchain) } {
        Ok(images) => images,
        Err(err) => {
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                context.device.destroy_device(None);
            }
            return Err(format!(
                "vkGetSwapchainImagesKHR(vulkanalia video present): {err:?}"
            ));
        }
    };
    let _ = unsafe { device.device_wait_idle() };
    unsafe {
        device.destroy_swapchain_khr(swapchain, None);
        context.device.destroy_device(None);
    }

    let same_queue_family =
        selection.video_queue_family_index == selection.present_queue_family_index;
    Ok(NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot {
        binding: "vulkanalia",
        route: "video-present-device",
        loader: vulkan.loader_name.to_owned(),
        requested_api_version: Version::V1_4_0.to_string(),
        codec,
        physical_device_index: selection.physical_device_index,
        physical_device_name: selection
            .properties
            .device_name
            .to_string_lossy()
            .into_owned(),
        physical_device_type: format!("{:?}", selection.properties.device_type),
        api_version: Version::from(selection.properties.api_version).to_string(),
        vendor_id: selection.properties.vendor_id,
        device_id: selection.properties.device_id,
        driver_version: selection.properties.driver_version,
        single_logical_device_created: true,
        enabled_device_extensions: context.enabled_device_extensions,
        video_enabled_device_extensions: context.video_enabled_device_extensions,
        present_enabled_device_extensions: context.present_enabled_device_extensions,
        feature_selection: NativeVulkanVulkanaliaVideoPresentFeatureSnapshot {
            synchronization2_enabled: context.video_feature_selection.synchronization2_enabled
                && context.present_feature_selection.synchronization2_enabled,
            video_maintenance1_enabled: context.video_feature_selection.video_maintenance1_enabled,
            video_maintenance2_enabled: context.video_feature_selection.video_maintenance2_enabled,
            inline_session_parameters_enabled: context
                .video_feature_selection
                .inline_session_parameters_enabled,
            present_id_enabled: context.present_feature_selection.present_id_enabled,
            present_wait_enabled: context.present_feature_selection.present_wait_enabled,
            present_id2_enabled: context.present_feature_selection.present_id2_enabled,
            present_wait2_enabled: context.present_feature_selection.present_wait2_enabled,
            swapchain_maintenance1_enabled: context
                .present_feature_selection
                .swapchain_maintenance1_enabled,
        },
        video_queue: queue_snapshot(
            selection.video_queue_family_index,
            selection.video_queue_count,
            selection.video_queue_flags,
            true,
            selection.video_queue_family_index == selection.present_queue_family_index,
            selection.video_queue_family_index == selection.present_queue_family_index
                && selection.present_supports_wayland,
        ),
        present_queue: queue_snapshot(
            selection.present_queue_family_index,
            selection.present_queue_count,
            selection.present_queue_flags,
            selection
                .present_queue_flags
                .contains(vk::QueueFlags::VIDEO_DECODE_KHR),
            true,
            selection.present_supports_wayland,
        ),
        same_queue_family,
        queue_family_model: video_present_queue_family_model(same_queue_family),
        decoded_image_resource_sharing_model: decoded_image_resource_sharing_model(
            same_queue_family,
        ),
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
        },
        present_backend: "vulkanalia-single-device-video-decode-graphics-present",
        decoded_image_present_boundary: "same logical device now owns video-decode and graphics/present queues; next gate records decoded DPB/output image sampling into swapchain instead of clear placeholder",
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    })
}

fn with_video_present_session(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &NativeVulkanVulkanaliaInstance,
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|err| {
        format!("vkEnumeratePhysicalDevices(vulkanalia video present session): {err:?}")
    })?;
    let selection = select_video_present_physical_device(
        instance,
        surface,
        handles,
        &physical_devices,
        options.codec,
    )?;
    let context = create_video_present_device(instance, &selection, options.codec)?;
    let result = create_video_present_session_resources(
        instance, surface, handles, vulkan, &selection, &context, options,
    );
    unsafe {
        context.device.destroy_device(None);
    }
    result
}

fn create_video_present_session_resources(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &NativeVulkanVulkanaliaInstance,
    selection: &NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    options: NativeVulkanVulkanaliaVideoPresentSessionProbeOptions,
) -> Result<NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot, String> {
    let swapchain_plan = create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
    )?;
    let swapchain = unsafe {
        context
            .device
            .create_swapchain_khr(&swapchain_plan.create_info, None)
    }
    .map_err(|err| format!("vkCreateSwapchainKHR(vulkanalia video present session): {err:?}"))?;
    let swapchain_images = match unsafe { context.device.get_swapchain_images_khr(swapchain) } {
        Ok(images) => images,
        Err(err) => {
            unsafe {
                context.device.destroy_swapchain_khr(swapchain, None);
            }
            return Err(format!(
                "vkGetSwapchainImagesKHR(vulkanalia video present session): {err:?}"
            ));
        }
    };

    let result = with_native_vulkan_vulkanalia_video_session_capabilities(
        instance,
        selection.physical_device,
        options.codec,
        None,
        None,
        |profile_info, queried| {
            let requested_extent = vk::Extent2D {
                width: options.width,
                height: options.height,
            };
            if !native_vulkan_vulkanalia_video_session_extent_supported(
                requested_extent,
                queried.capabilities,
            ) {
                return Err(format!(
                    "requested Vulkanalia video present session extent {}x{} is outside driver capabilities",
                    requested_extent.width, requested_extent.height
                ));
            }
            let session_max_dpb_slots = native_vulkan_vulkanalia_video_session_max_dpb_slots(
                queried.capabilities.max_dpb_slots,
            );
            let session_max_active_reference_pictures =
                native_vulkan_vulkanalia_video_session_max_active_reference_pictures(
                    queried.capabilities.max_active_reference_pictures,
                    session_max_dpb_slots,
                );
            let picture_format = native_vulkan_vulkanalia_video_session_effective_picture_format(
                options.codec,
                None,
            );
            let create_info = vk::VideoSessionCreateInfoKHR::builder()
                .queue_family_index(selection.video_queue_family_index)
                .video_profile(profile_info)
                .picture_format(picture_format)
                .reference_picture_format(picture_format)
                .max_coded_extent(requested_extent)
                .max_dpb_slots(session_max_dpb_slots)
                .max_active_reference_pictures(session_max_active_reference_pictures)
                .std_header_version(&queried.capabilities.std_header_version)
                .build();
            let session =
                native_vulkan_vulkanalia_create_video_session(&context.device, &create_info)?;
            let mut memory_resources = None;
            let session_result = (|| -> Result<VideoPresentSessionResourceSnapshots, String> {
                let memory_properties = unsafe {
                    instance.get_physical_device_memory_properties(selection.physical_device)
                };
                let resources = native_vulkan_vulkanalia_bind_video_session_memory_resources(
                    &context.device,
                    &memory_properties,
                    session,
                )?;
                let memory_binding = resources.snapshot.clone();
                memory_resources = Some(resources);
                let resource_queue_family_indices = video_present_queue_family_indices(
                    selection.video_queue_family_index,
                    selection.present_queue_family_index,
                );
                let resource_image = native_vulkan_vulkanalia_create_video_session_resource_image(
                    instance,
                    &context.device,
                    &memory_properties,
                    selection.physical_device,
                    profile_info,
                    requested_extent,
                    session_max_dpb_slots.max(1),
                    picture_format,
                    queried.decode_capability_flags,
                    &resource_queue_family_indices,
                )?;
                let resource_image_snapshot =
                    NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot {
                        image_created: true,
                        memory_bound: true,
                        image_view_created: resource_image.view != vk::ImageView::default(),
                        layer_view_count: resource_image.layer_views.len(),
                        resource_image: resource_image.snapshot.clone(),
                    };
                native_vulkan_vulkanalia_destroy_video_session_resource_image(
                    &context.device,
                    resource_image,
                );
                Ok(VideoPresentSessionResourceSnapshots {
                    memory_binding,
                    resource_image: resource_image_snapshot,
                    resource_queue_family_indices,
                })
            })();
            if let Some(resources) = memory_resources.take() {
                native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(
                    &context.device,
                    resources,
                );
            }
            native_vulkan_vulkanalia_destroy_video_session(&context.device, session);
            let resource_snapshots = session_result?;
            let same_queue_family =
                selection.video_queue_family_index == selection.present_queue_family_index;
            Ok(NativeVulkanVulkanaliaVideoPresentSessionProbeSnapshot {
                binding: "vulkanalia",
                route: "video-present-session-resource",
                codec: options.codec,
                requested_extent: (requested_extent.width, requested_extent.height),
                device: device_snapshot_from_selection(
                    vulkan,
                    selection,
                    context,
                    options.codec,
                    swapchain_plan_snapshot(&swapchain_plan, swapchain_images.len()),
                ),
                video_session_created: true,
                memory_binding: resource_snapshots.memory_binding,
                resource_image: resource_snapshots.resource_image,
                picture_format: format!("{picture_format:?}"),
                decode_capability_flags: video_decode_capability_flag_labels(
                    queried.decode_capability_flags,
                ),
                session_max_dpb_slots,
                session_max_active_reference_pictures,
                resource_queue_family_indices: resource_snapshots.resource_queue_family_indices,
                resource_queue_sharing_model: decoded_image_resource_sharing_model(
                    same_queue_family,
                ),
                decoded_image_zero_copy_presentable_candidate: true,
                decoded_image_present_boundary: "same Vulkanalia device owns video session memory, coincident sampled DPB/output image and Wayland swapchain; next gate records decode into this retained image and samples it in the graphics present pass",
                ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
            })
        },
    );

    let _ = unsafe { context.device.device_wait_idle() };
    unsafe {
        context.device.destroy_swapchain_khr(swapchain, None);
    }
    result
}

fn select_video_present_physical_device(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    physical_devices: &[vk::PhysicalDevice],
    codec: NativeVulkanVideoSessionCodec,
) -> Result<NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection, String> {
    let required_video_extensions =
        native_vulkan_vulkanalia_video_decode_required_device_extensions(codec);
    let mut rejected = Vec::new();

    for (physical_device_index, physical_device) in physical_devices.iter().copied().enumerate() {
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let physical_device_name = properties.device_name.to_string_lossy().into_owned();
        let device_extensions =
            unsafe { instance.enumerate_device_extension_properties(physical_device, None) }
                .map_err(|err| {
                    format!(
                        "vkEnumerateDeviceExtensionProperties(vulkanalia video present): {err:?}"
                    )
                })?
                .into_iter()
                .map(|property| property.extension_name.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
        let missing_extensions = required_video_extensions
            .iter()
            .copied()
            .chain(["VK_KHR_swapchain"])
            .filter(|required| {
                !native_vulkan_vulkanalia_video_device_extension_available(
                    &device_extensions,
                    required,
                )
            })
            .collect::<Vec<_>>();
        if !missing_extensions.is_empty() {
            rejected.push(format!(
                "{physical_device_name} missing {}",
                missing_extensions.join(", ")
            ));
            continue;
        }

        let video_queue_family_indices =
            native_vulkan_vulkanalia_video_decode_queue_family_indices(instance, physical_device);
        if video_queue_family_indices.is_empty() {
            rejected.push(format!(
                "{physical_device_name} has no VIDEO_DECODE_KHR queue family"
            ));
            continue;
        };
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let Some(present) = select_graphics_present_queue(
            instance,
            surface,
            handles,
            physical_device,
            &queue_families,
        )?
        else {
            rejected.push(format!(
                "{physical_device_name} has no GRAPHICS queue that can present to this Wayland surface"
            ));
            continue;
        };
        let video_queue_family_index =
            if video_queue_family_indices.contains(&present.queue_family_index) {
                present.queue_family_index
            } else {
                video_queue_family_indices[0]
            };
        let Some(video_queue_family) = queue_families.get(video_queue_family_index as usize) else {
            rejected.push(format!(
                "{physical_device_name} selected invalid video queue family {video_queue_family_index}"
            ));
            continue;
        };

        return Ok(NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection {
            physical_device_index,
            physical_device,
            properties,
            device_extensions,
            video_queue_family_index,
            video_queue_count: video_queue_family.queue_count,
            video_queue_flags: video_queue_family.queue_flags,
            present_queue_family_index: present.queue_family_index,
            present_queue_count: present.queue_count,
            present_queue_flags: present.queue_flags,
            present_supports_wayland: present.supports_wayland_presentation,
        });
    }

    Err(if rejected.is_empty() {
        "no Vulkanalia physical device can create a single video+present device".to_owned()
    } else {
        format!(
            "no Vulkanalia physical device can create a single video+present device: {}",
            rejected.join("; ")
        )
    })
}

#[derive(Debug, Clone, Copy)]
struct GraphicsPresentQueueCandidate {
    queue_family_index: u32,
    queue_count: u32,
    queue_flags: vk::QueueFlags,
    supports_wayland_presentation: bool,
}

fn select_graphics_present_queue(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    physical_device: vk::PhysicalDevice,
    queue_families: &[vk::QueueFamilyProperties],
) -> Result<Option<GraphicsPresentQueueCandidate>, String> {
    let mut fallback = None;
    for (queue_family_index, queue_family) in queue_families.iter().enumerate() {
        if !queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            continue;
        }
        let supports_present = unsafe {
            instance.get_physical_device_surface_support_khr(
                physical_device,
                queue_family_index as u32,
                surface,
            )
        }
        .map_err(|err| {
            format!("vkGetPhysicalDeviceSurfaceSupportKHR(vulkanalia video present): {err:?}")
        })?;
        if !supports_present {
            continue;
        }
        let supports_wayland_presentation = unsafe {
            instance.get_physical_device_wayland_presentation_support_khr(
                physical_device,
                queue_family_index as u32,
                handles.display.as_ptr().cast::<vk::wl_display>(),
            ) == vk::TRUE
        };
        let candidate = GraphicsPresentQueueCandidate {
            queue_family_index: queue_family_index as u32,
            queue_count: queue_family.queue_count,
            queue_flags: queue_family.queue_flags,
            supports_wayland_presentation,
        };
        if supports_wayland_presentation {
            return Ok(Some(candidate));
        }
        fallback.get_or_insert(candidate);
    }
    Ok(fallback)
}

fn create_video_present_device(
    instance: &Instance,
    selection: &NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    codec: NativeVulkanVideoSessionCodec,
) -> Result<NativeVulkanVulkanaliaVideoPresentDeviceContext, String> {
    let video_feature_selection = native_vulkan_vulkanalia_video_device_feature_selection(
        instance,
        selection.physical_device,
        &selection.device_extensions,
    );
    if !video_feature_selection.synchronization2_enabled {
        return Err(
            "single Vulkanalia video+present device requires synchronization2 for QueueSubmit2"
                .to_owned(),
        );
    }
    let present_feature_selection = query_vulkanalia_present_feature_selection(
        instance,
        selection.physical_device,
        &selection.device_extensions,
    );
    if !present_feature_selection.synchronization2_enabled {
        return Err(
            "single Vulkanalia video+present device requires present synchronization2".to_owned(),
        );
    }

    let video_enabled_device_extensions =
        native_vulkan_vulkanalia_video_decode_device_extensions(codec, video_feature_selection);
    let present_enabled_device_extensions =
        enabled_present_device_extensions(&present_feature_selection);
    let enabled_device_extensions = dedup_static_extensions(
        video_enabled_device_extensions
            .iter()
            .copied()
            .chain(present_enabled_device_extensions.iter().copied()),
    );
    let extension_names = enabled_device_extensions
        .iter()
        .map(|extension| CString::new(*extension).expect("static extension name has no nul"))
        .collect::<Vec<_>>();
    let extension_name_ptrs = extension_names
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();

    let priorities = [1.0_f32];
    let queue_family_indices = video_present_queue_family_indices(
        selection.video_queue_family_index,
        selection.present_queue_family_index,
    );
    let queue_create_infos = queue_family_indices
        .iter()
        .map(|queue_family_index| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(*queue_family_index)
                .queue_priorities(&priorities)
                .build()
        })
        .collect::<Vec<_>>();

    let mut synchronization2_features = vk::PhysicalDeviceSynchronization2Features::builder()
        .synchronization2(true)
        .build();
    let mut video_maintenance1_features = vk::PhysicalDeviceVideoMaintenance1FeaturesKHR::builder()
        .video_maintenance1(true)
        .build();
    let mut video_maintenance2_features = vk::PhysicalDeviceVideoMaintenance2FeaturesKHR::builder()
        .video_maintenance2(true)
        .build();
    let mut present_id_features = vk::PhysicalDevicePresentIdFeaturesKHR::builder()
        .present_id(true)
        .build();
    let mut present_wait_features = vk::PhysicalDevicePresentWaitFeaturesKHR::builder()
        .present_wait(true)
        .build();
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

    let mut device_create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_name_ptrs)
        .push_next(&mut synchronization2_features);
    if video_feature_selection.video_maintenance1_enabled {
        device_create_info = device_create_info.push_next(&mut video_maintenance1_features);
    }
    if video_feature_selection.video_maintenance2_enabled {
        device_create_info = device_create_info.push_next(&mut video_maintenance2_features);
    }
    if present_feature_selection.present_id_enabled {
        device_create_info = device_create_info.push_next(&mut present_id_features);
    }
    if present_feature_selection.present_wait_enabled {
        device_create_info = device_create_info.push_next(&mut present_wait_features);
    }
    if present_feature_selection.present_id2_enabled {
        device_create_info = device_create_info.push_next(&mut present_id2_features);
    }
    if present_feature_selection.present_wait2_enabled {
        device_create_info = device_create_info.push_next(&mut present_wait2_features);
    }
    if present_feature_selection.swapchain_maintenance1_enabled {
        device_create_info = device_create_info.push_next(&mut swapchain_maintenance1_features);
    }

    let device =
        unsafe { instance.create_device(selection.physical_device, &device_create_info, None) }
            .map_err(|err| format!("vkCreateDevice(vulkanalia video+present): {err:?}"))?;
    let video_queue = unsafe { device.get_device_queue(selection.video_queue_family_index, 0) };
    let present_queue = unsafe { device.get_device_queue(selection.present_queue_family_index, 0) };

    Ok(NativeVulkanVulkanaliaVideoPresentDeviceContext {
        device,
        video_queue,
        present_queue,
        enabled_device_extensions,
        video_enabled_device_extensions,
        present_enabled_device_extensions,
        video_feature_selection,
        present_feature_selection,
    })
}

fn device_snapshot_from_selection(
    vulkan: &NativeVulkanVulkanaliaInstance,
    selection: &NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    codec: NativeVulkanVideoSessionCodec,
    swapchain: NativeVulkanVulkanaliaSwapchainSnapshot,
) -> NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot {
    let same_queue_family =
        selection.video_queue_family_index == selection.present_queue_family_index;
    NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot {
        binding: "vulkanalia",
        route: "video-present-device",
        loader: vulkan.loader_name.to_owned(),
        requested_api_version: Version::V1_4_0.to_string(),
        codec,
        physical_device_index: selection.physical_device_index,
        physical_device_name: selection
            .properties
            .device_name
            .to_string_lossy()
            .into_owned(),
        physical_device_type: format!("{:?}", selection.properties.device_type),
        api_version: Version::from(selection.properties.api_version).to_string(),
        vendor_id: selection.properties.vendor_id,
        device_id: selection.properties.device_id,
        driver_version: selection.properties.driver_version,
        single_logical_device_created: true,
        enabled_device_extensions: context.enabled_device_extensions.clone(),
        video_enabled_device_extensions: context.video_enabled_device_extensions.clone(),
        present_enabled_device_extensions: context.present_enabled_device_extensions.clone(),
        feature_selection: feature_snapshot_from_context(context),
        video_queue: queue_snapshot(
            selection.video_queue_family_index,
            selection.video_queue_count,
            selection.video_queue_flags,
            true,
            same_queue_family,
            same_queue_family && selection.present_supports_wayland,
        ),
        present_queue: queue_snapshot(
            selection.present_queue_family_index,
            selection.present_queue_count,
            selection.present_queue_flags,
            selection
                .present_queue_flags
                .contains(vk::QueueFlags::VIDEO_DECODE_KHR),
            true,
            selection.present_supports_wayland,
        ),
        same_queue_family,
        queue_family_model: video_present_queue_family_model(same_queue_family),
        decoded_image_resource_sharing_model: decoded_image_resource_sharing_model(
            same_queue_family,
        ),
        swapchain,
        present_backend: "vulkanalia-single-device-video-decode-graphics-present",
        decoded_image_present_boundary: "same logical device now owns video-decode and graphics/present queues; next gate records decoded DPB/output image sampling into swapchain instead of clear placeholder",
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    }
}

fn feature_snapshot_from_context(
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
) -> NativeVulkanVulkanaliaVideoPresentFeatureSnapshot {
    NativeVulkanVulkanaliaVideoPresentFeatureSnapshot {
        synchronization2_enabled: context.video_feature_selection.synchronization2_enabled
            && context.present_feature_selection.synchronization2_enabled,
        video_maintenance1_enabled: context.video_feature_selection.video_maintenance1_enabled,
        video_maintenance2_enabled: context.video_feature_selection.video_maintenance2_enabled,
        inline_session_parameters_enabled: context
            .video_feature_selection
            .inline_session_parameters_enabled,
        present_id_enabled: context.present_feature_selection.present_id_enabled,
        present_wait_enabled: context.present_feature_selection.present_wait_enabled,
        present_id2_enabled: context.present_feature_selection.present_id2_enabled,
        present_wait2_enabled: context.present_feature_selection.present_wait2_enabled,
        swapchain_maintenance1_enabled: context
            .present_feature_selection
            .swapchain_maintenance1_enabled,
    }
}

fn swapchain_plan_snapshot(
    swapchain_plan: &super::swapchain::NativeVulkanVulkanaliaSwapchainPlan,
    image_count: usize,
) -> NativeVulkanVulkanaliaSwapchainSnapshot {
    NativeVulkanVulkanaliaSwapchainSnapshot {
        created: true,
        format: format!("{:?}", swapchain_plan.format.format),
        color_space: format!("{:?}", swapchain_plan.format.color_space),
        present_mode: present_mode_label(swapchain_plan.present_mode),
        extent: (swapchain_plan.extent.width, swapchain_plan.extent.height),
        image_count,
        min_image_count: swapchain_plan.image_count,
        composite_alpha: composite_alpha_label(swapchain_plan.composite_alpha),
        image_usage: vec!["transfer-dst", "color-attachment"],
    }
}

fn queue_snapshot(
    queue_family_index: u32,
    queue_count: u32,
    queue_flags: vk::QueueFlags,
    supports_video_decode: bool,
    supports_present: bool,
    supports_wayland_presentation: bool,
) -> NativeVulkanVulkanaliaVideoPresentQueueSnapshot {
    NativeVulkanVulkanaliaVideoPresentQueueSnapshot {
        queue_family_index,
        queue_count,
        queue_flags: queue_flag_labels(queue_flags),
        supports_video_decode,
        supports_graphics: queue_flags.contains(vk::QueueFlags::GRAPHICS),
        supports_present,
        supports_wayland_presentation,
    }
}

fn video_present_queue_family_indices(video: u32, present: u32) -> Vec<u32> {
    if video == present {
        vec![video]
    } else {
        vec![video, present]
    }
}

fn video_present_queue_family_model(same_queue_family: bool) -> &'static str {
    if same_queue_family {
        "single-video-graphics-present-queue-family"
    } else {
        "dedicated-video-decode-queue-plus-graphics-present-queue"
    }
}

fn decoded_image_resource_sharing_model(same_queue_family: bool) -> &'static str {
    if same_queue_family {
        "exclusive-image-ownership-on-single-queue-family"
    } else {
        "concurrent-image-sharing-or-explicit-ownership-transfer-between-video-and-present-queue-families"
    }
}

fn dedup_static_extensions(
    extensions: impl IntoIterator<Item = &'static str>,
) -> Vec<&'static str> {
    let mut deduped = Vec::new();
    for extension in extensions {
        if !deduped.contains(&extension) {
            deduped.push(extension);
        }
    }
    deduped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_family_indices_are_deduped_for_single_family_device() {
        assert_eq!(video_present_queue_family_indices(3, 3), vec![3]);
        assert_eq!(video_present_queue_family_indices(3, 0), vec![3, 0]);
    }

    #[test]
    fn extension_union_keeps_first_order_and_dedupes() {
        let extensions = dedup_static_extensions([
            "VK_KHR_video_queue",
            "VK_KHR_swapchain",
            "VK_KHR_video_queue",
            "VK_KHR_present_wait",
        ]);

        assert_eq!(
            extensions,
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_swapchain",
                "VK_KHR_present_wait"
            ]
        );
    }

    #[test]
    fn resource_sharing_model_names_the_real_boundary() {
        assert_eq!(
            decoded_image_resource_sharing_model(false),
            "concurrent-image-sharing-or-explicit-ownership-transfer-between-video-and-present-queue-families"
        );
        assert_eq!(
            video_present_queue_family_model(true),
            "single-video-graphics-present-queue-family"
        );
    }
}
