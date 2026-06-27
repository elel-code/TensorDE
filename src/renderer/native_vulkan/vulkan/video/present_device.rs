#![allow(dead_code)]

use std::ffi::CString;

use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
    KhrWaylandSurfaceExtensionInstanceCommands,
};

use crate::renderer::native_vulkan::{NativeVulkanClearColor, NativeVulkanVideoSessionCodec};
use crate::renderer::native_wayland::{
    NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};

use super::features::{
    NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    native_vulkan_vulkanalia_descriptor_heap_device_features,
    native_vulkan_vulkanalia_vulkan12_device_features,
    native_vulkan_vulkanalia_vulkan13_device_features,
    native_vulkan_vulkanalia_vulkan14_device_features,
};
use super::instance::{
    NativeVulkanVulkanaliaInstance,
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::queue_probe::native_vulkan_vulkanalia_video_decode_queue_family_indices;
use super::render_present::{
    NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot,
    NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot,
};
use super::swapchain::{
    NativeVulkanVulkanaliaSwapchainSnapshot, OPTIONAL_INSTANCE_EXTENSIONS,
    REQUIRED_INSTANCE_EXTENSIONS, composite_alpha_label, create_vulkanalia_swapchain_plan,
    create_vulkanalia_wayland_surface, enabled_present_device_extensions, present_mode_label,
    query_vulkanalia_present_feature_selection, queue_flag_labels,
    vulkanalia_surface_capabilities2_enabled, vulkanalia_surface_maintenance1_enabled,
};
use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;
use super::video_device::{
    NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
    native_vulkan_vulkanalia_video_decode_device_extensions,
    native_vulkan_vulkanalia_video_decode_required_device_extensions,
    native_vulkan_vulkanalia_video_device_extension_available,
    native_vulkan_vulkanalia_video_device_feature_selection,
};
use super::video_session::NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot;
use super::video_session_images::NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaVideoPresentDeviceProbeOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub codec: NativeVulkanVideoSessionCodec,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaVideoPresentSessionProbeOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub target_max_fps: Option<u32>,
    pub audio_master_clock: NativeVulkanVulkanaliaVideoPresentAudioMasterClock,
    pub clear_color: NativeVulkanClearColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaVideoPresentAudioMasterClock {
    pub enabled: bool,
    pub start_clock_ns: Option<u64>,
}

impl NativeVulkanVulkanaliaVideoPresentAudioMasterClock {
    pub const DISABLED: Self = Self {
        enabled: false,
        start_clock_ns: None,
    };

    pub fn clock_only(start_clock_ns: Option<u64>) -> Self {
        Self {
            enabled: true,
            start_clock_ns,
        }
    }
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
    pub same_queue_handle: bool,
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
    pub video_session_create_inline_session_parameters: bool,
    pub video_session_create_flags_bits: u32,
    pub memory_binding: NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    pub resource_image: NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot,
    pub picture_format: String,
    pub decode_capability_flags: Vec<&'static str>,
    pub session_max_dpb_slots: u32,
    pub session_max_active_reference_pictures: u32,
    pub resource_queue_family_indices: Vec<u32>,
    pub resource_queue_sharing_model: &'static str,
    pub decoded_image_zero_copy_presentable_candidate: bool,
    pub decoded_image_present_sampler:
        Option<NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot>,
    pub decoded_image_present_sampler_error: Option<String>,
    pub decoded_image_present_pipeline:
        Option<NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot>,
    pub decoded_image_present_pipeline_error: Option<String>,
    pub decoded_image_present_boundary: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoPresentFeatureSnapshot {
    pub core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
    pub synchronization2_enabled: bool,
    pub dynamic_rendering_enabled: bool,
    pub descriptor_heap_enabled: bool,
    pub descriptor_heap_capture_replay_enabled: bool,
    pub descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    pub video_maintenance1_enabled: bool,
    pub video_maintenance2_enabled: bool,
    pub inline_session_parameters_enabled: bool,
    pub present_id2_enabled: bool,
    pub present_wait2_enabled: bool,
    pub swapchain_maintenance1_enabled: bool,
    pub present_mode_fifo_latest_ready_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoPresentQueueSnapshot {
    pub queue_family_index: u32,
    pub queue_index: u32,
    pub queue_count: u32,
    pub queue_flags: Vec<&'static str>,
    pub supports_video_decode: bool,
    pub supports_graphics: bool,
    pub supports_present: bool,
    pub supports_wayland_presentation: bool,
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection
{
    pub(in crate::renderer::native_vulkan::vulkan) physical_device_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) physical_device: vk::PhysicalDevice,
    pub(in crate::renderer::native_vulkan::vulkan) properties: vk::PhysicalDeviceProperties,
    pub(in crate::renderer::native_vulkan::vulkan) device_extensions: Vec<String>,
    pub(in crate::renderer::native_vulkan::vulkan) video_queue_family_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) video_queue_count: u32,
    pub(in crate::renderer::native_vulkan::vulkan) video_queue_flags: vk::QueueFlags,
    pub(in crate::renderer::native_vulkan::vulkan) present_queue_family_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) present_queue_count: u32,
    pub(in crate::renderer::native_vulkan::vulkan) present_queue_flags: vk::QueueFlags,
    pub(in crate::renderer::native_vulkan::vulkan) present_supports_wayland: bool,
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaVideoPresentDeviceContext
{
    pub(in crate::renderer::native_vulkan::vulkan) device: Device,
    pub(in crate::renderer::native_vulkan::vulkan) video_queue: vk::Queue,
    pub(in crate::renderer::native_vulkan::vulkan) present_queue: vk::Queue,
    pub(in crate::renderer::native_vulkan::vulkan) video_queue_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) present_queue_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) enabled_device_extensions: Vec<&'static str>,
    pub(in crate::renderer::native_vulkan::vulkan) video_enabled_device_extensions:
        Vec<&'static str>,
    pub(in crate::renderer::native_vulkan::vulkan) present_enabled_device_extensions:
        Vec<&'static str>,
    pub(in crate::renderer::native_vulkan::vulkan) video_feature_selection:
        NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
    pub(in crate::renderer::native_vulkan::vulkan) present_feature_selection:
        super::swapchain::NativeVulkanVulkanaliaPresentFeatureSelection,
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

    super::video_present_runtime::probe_native_vulkan_vulkanalia_retained_video_present_session(
        options,
    )
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
    let context = create_video_present_device(
        instance,
        &selection,
        codec,
        vulkanalia_surface_maintenance1_enabled(vulkan),
    )?;
    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
        vulkanalia_surface_capabilities2_enabled(vulkan),
        &context.present_feature_selection,
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
    let same_queue_handle =
        same_queue_family && context.video_queue_index == context.present_queue_index;
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
            core_features: context.video_feature_selection.core_features,
            synchronization2_enabled: context.video_feature_selection.synchronization2_enabled
                && context.present_feature_selection.synchronization2_enabled,
            dynamic_rendering_enabled: context.video_feature_selection.dynamic_rendering_enabled,
            descriptor_heap_enabled: context
                .video_feature_selection
                .core_features
                .descriptor_heap,
            descriptor_heap_capture_replay_enabled: context
                .video_feature_selection
                .core_features
                .descriptor_heap_capture_replay,
            descriptor_heap_properties: context.video_feature_selection.descriptor_heap_properties,
            video_maintenance1_enabled: context.video_feature_selection.video_maintenance1_enabled,
            video_maintenance2_enabled: context.video_feature_selection.video_maintenance2_enabled,
            inline_session_parameters_enabled: context
                .video_feature_selection
                .inline_session_parameters_enabled,
            present_id2_enabled: context.present_feature_selection.present_id2_enabled,
            present_wait2_enabled: context.present_feature_selection.present_wait2_enabled,
            swapchain_maintenance1_enabled: context
                .present_feature_selection
                .swapchain_maintenance1_enabled,
            present_mode_fifo_latest_ready_enabled: context
                .present_feature_selection
                .present_mode_fifo_latest_ready_enabled,
        },
        video_queue: queue_snapshot(
            selection.video_queue_family_index,
            context.video_queue_index,
            selection.video_queue_count,
            selection.video_queue_flags,
            true,
            selection.video_queue_family_index == selection.present_queue_family_index,
            selection.video_queue_family_index == selection.present_queue_family_index
                && selection.present_supports_wayland,
        ),
        present_queue: queue_snapshot(
            selection.present_queue_family_index,
            context.present_queue_index,
            selection.present_queue_count,
            selection.present_queue_flags,
            selection
                .present_queue_flags
                .contains(vk::QueueFlags::VIDEO_DECODE_KHR),
            true,
            selection.present_supports_wayland,
        ),
        same_queue_family,
        same_queue_handle,
        queue_family_model: video_present_queue_family_model(same_queue_family, same_queue_handle),
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
            create_flags: super::swapchain::swapchain_create_flag_labels(
                swapchain_plan.create_flags,
            ),
            present_id2_enabled: swapchain_plan.present_id2_enabled,
            present_wait2_enabled: swapchain_plan.present_wait2_enabled,
        },
        present_backend: "vulkanalia-single-device-video-decode-graphics-present",
        decoded_image_present_boundary: "same logical device now owns video-decode and graphics/present queues; next gate records decoded DPB/output image sampling into swapchain instead of clear placeholder",
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    })
}

pub(in crate::renderer::native_vulkan::vulkan) fn select_video_present_physical_device(
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

pub(in crate::renderer::native_vulkan::vulkan) fn create_video_present_device(
    instance: &Instance,
    selection: &NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    codec: NativeVulkanVideoSessionCodec,
    surface_maintenance1_enabled: bool,
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
        surface_maintenance1_enabled,
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

    let one_queue_priorities = [1.0_f32];
    let two_queue_priorities = [1.0_f32, 1.0_f32];
    let same_queue_family =
        selection.video_queue_family_index == selection.present_queue_family_index;
    let (video_queue_index, present_queue_index) =
        video_present_queue_indices(same_queue_family, selection.video_queue_count);
    let queue_family_indices = video_present_queue_family_indices(
        selection.video_queue_family_index,
        selection.present_queue_family_index,
    );
    // FFmpeg's Vulkan hwcontext keeps the queue count for each selected family
    // and locks individual queue indices, not the whole queue family
    // (references/ffmpeg/libavutil/hwcontext_vulkan.c:1580-1665,2005-2035).
    // Request two queues when decode and present share a family and the driver
    // exposes them, so FIFO present cannot serialize decode submits through one
    // host-side VkQueue lock.
    let queue_create_infos = if same_queue_family {
        let priorities = if selection.video_queue_count > 1 {
            &two_queue_priorities[..]
        } else {
            &one_queue_priorities[..]
        };
        vec![
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(selection.video_queue_family_index)
                .queue_priorities(priorities)
                .build(),
        ]
    } else {
        queue_family_indices
            .iter()
            .map(|queue_family_index| {
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(*queue_family_index)
                    .queue_priorities(&one_queue_priorities)
                    .build()
            })
            .collect::<Vec<_>>()
    };

    let mut vulkan12_features =
        native_vulkan_vulkanalia_vulkan12_device_features(video_feature_selection.core_features);
    let mut vulkan13_features =
        native_vulkan_vulkanalia_vulkan13_device_features(video_feature_selection.core_features);
    let mut vulkan14_features =
        native_vulkan_vulkanalia_vulkan14_device_features(video_feature_selection.core_features);
    let mut descriptor_heap_features = native_vulkan_vulkanalia_descriptor_heap_device_features(
        video_feature_selection.core_features,
    );
    let mut video_maintenance1_features = vk::PhysicalDeviceVideoMaintenance1FeaturesKHR::builder()
        .video_maintenance1(true)
        .build();
    let mut video_maintenance2_features = vk::PhysicalDeviceVideoMaintenance2FeaturesKHR::builder()
        .video_maintenance2(true)
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
    let mut present_mode_fifo_latest_ready_features =
        vk::PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR::builder()
            .present_mode_fifo_latest_ready(true)
            .build();

    let mut device_create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_name_ptrs);
    if video_feature_selection
        .core_features
        .enables_vulkan_1_2_features()
    {
        device_create_info = device_create_info.push_next(&mut vulkan12_features);
    }
    if video_feature_selection
        .core_features
        .enables_vulkan_1_3_features()
    {
        device_create_info = device_create_info.push_next(&mut vulkan13_features);
    }
    if video_feature_selection
        .core_features
        .enables_vulkan_1_4_features()
    {
        device_create_info = device_create_info.push_next(&mut vulkan14_features);
    }
    if video_feature_selection
        .core_features
        .enables_descriptor_heap_features()
    {
        device_create_info = device_create_info.push_next(&mut descriptor_heap_features);
    }
    if video_feature_selection.video_maintenance1_enabled {
        device_create_info = device_create_info.push_next(&mut video_maintenance1_features);
    }
    if video_feature_selection.video_maintenance2_enabled {
        device_create_info = device_create_info.push_next(&mut video_maintenance2_features);
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
    if present_feature_selection.present_mode_fifo_latest_ready_enabled {
        device_create_info =
            device_create_info.push_next(&mut present_mode_fifo_latest_ready_features);
    }

    let device =
        unsafe { instance.create_device(selection.physical_device, &device_create_info, None) }
            .map_err(|err| format!("vkCreateDevice(vulkanalia video+present): {err:?}"))?;
    let video_queue =
        unsafe { device.get_device_queue(selection.video_queue_family_index, video_queue_index) };
    let present_queue = unsafe {
        device.get_device_queue(selection.present_queue_family_index, present_queue_index)
    };

    Ok(NativeVulkanVulkanaliaVideoPresentDeviceContext {
        device,
        video_queue,
        present_queue,
        video_queue_index,
        present_queue_index,
        enabled_device_extensions,
        video_enabled_device_extensions,
        present_enabled_device_extensions,
        video_feature_selection,
        present_feature_selection,
    })
}

pub(in crate::renderer::native_vulkan::vulkan) fn device_snapshot_from_selection(
    vulkan: &NativeVulkanVulkanaliaInstance,
    selection: &NativeVulkanVulkanaliaVideoPresentPhysicalDeviceSelection,
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
    codec: NativeVulkanVideoSessionCodec,
    swapchain: NativeVulkanVulkanaliaSwapchainSnapshot,
) -> NativeVulkanVulkanaliaVideoPresentDeviceProbeSnapshot {
    let same_queue_family =
        selection.video_queue_family_index == selection.present_queue_family_index;
    let same_queue_handle =
        same_queue_family && context.video_queue_index == context.present_queue_index;
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
            context.video_queue_index,
            selection.video_queue_count,
            selection.video_queue_flags,
            true,
            same_queue_family,
            same_queue_family && selection.present_supports_wayland,
        ),
        present_queue: queue_snapshot(
            selection.present_queue_family_index,
            context.present_queue_index,
            selection.present_queue_count,
            selection.present_queue_flags,
            selection
                .present_queue_flags
                .contains(vk::QueueFlags::VIDEO_DECODE_KHR),
            true,
            selection.present_supports_wayland,
        ),
        same_queue_family,
        same_queue_handle,
        queue_family_model: video_present_queue_family_model(same_queue_family, same_queue_handle),
        decoded_image_resource_sharing_model: decoded_image_resource_sharing_model(
            same_queue_family,
        ),
        swapchain,
        present_backend: "vulkanalia-single-device-video-decode-graphics-present",
        decoded_image_present_boundary: "same logical device now owns video-decode and graphics/present queues; next gate records decoded DPB/output image sampling into swapchain instead of clear placeholder",
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn feature_snapshot_from_context(
    context: &NativeVulkanVulkanaliaVideoPresentDeviceContext,
) -> NativeVulkanVulkanaliaVideoPresentFeatureSnapshot {
    NativeVulkanVulkanaliaVideoPresentFeatureSnapshot {
        core_features: context.video_feature_selection.core_features,
        synchronization2_enabled: context.video_feature_selection.synchronization2_enabled
            && context.present_feature_selection.synchronization2_enabled,
        dynamic_rendering_enabled: context.video_feature_selection.dynamic_rendering_enabled,
        descriptor_heap_enabled: context
            .video_feature_selection
            .core_features
            .descriptor_heap,
        descriptor_heap_capture_replay_enabled: context
            .video_feature_selection
            .core_features
            .descriptor_heap_capture_replay,
        descriptor_heap_properties: context.video_feature_selection.descriptor_heap_properties,
        video_maintenance1_enabled: context.video_feature_selection.video_maintenance1_enabled,
        video_maintenance2_enabled: context.video_feature_selection.video_maintenance2_enabled,
        inline_session_parameters_enabled: context
            .video_feature_selection
            .inline_session_parameters_enabled,
        present_id2_enabled: context.present_feature_selection.present_id2_enabled,
        present_wait2_enabled: context.present_feature_selection.present_wait2_enabled,
        swapchain_maintenance1_enabled: context
            .present_feature_selection
            .swapchain_maintenance1_enabled,
        present_mode_fifo_latest_ready_enabled: context
            .present_feature_selection
            .present_mode_fifo_latest_ready_enabled,
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn swapchain_plan_snapshot(
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
        create_flags: super::swapchain::swapchain_create_flag_labels(swapchain_plan.create_flags),
        present_id2_enabled: swapchain_plan.present_id2_enabled,
        present_wait2_enabled: swapchain_plan.present_wait2_enabled,
    }
}

fn queue_snapshot(
    queue_family_index: u32,
    queue_index: u32,
    queue_count: u32,
    queue_flags: vk::QueueFlags,
    supports_video_decode: bool,
    supports_present: bool,
    supports_wayland_presentation: bool,
) -> NativeVulkanVulkanaliaVideoPresentQueueSnapshot {
    NativeVulkanVulkanaliaVideoPresentQueueSnapshot {
        queue_family_index,
        queue_index,
        queue_count,
        queue_flags: queue_flag_labels(queue_flags),
        supports_video_decode,
        supports_graphics: queue_flags.contains(vk::QueueFlags::GRAPHICS),
        supports_present,
        supports_wayland_presentation,
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn video_present_queue_family_indices(
    video: u32,
    present: u32,
) -> Vec<u32> {
    if video == present {
        vec![video]
    } else {
        vec![video, present]
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn video_present_queue_indices(
    same_queue_family: bool,
    queue_count: u32,
) -> (u32, u32) {
    if same_queue_family && queue_count > 1 {
        (0, 1)
    } else {
        (0, 0)
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn video_present_queue_family_model(
    same_queue_family: bool,
    same_queue_handle: bool,
) -> &'static str {
    if same_queue_handle {
        "single-video-graphics-present-queue-family-single-queue"
    } else if same_queue_family {
        "single-video-graphics-present-queue-family-split-queue-indices"
    } else {
        "dedicated-video-decode-queue-plus-graphics-present-queue"
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn decoded_image_resource_sharing_model(
    same_queue_family: bool,
) -> &'static str {
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
    fn same_family_queue_indices_split_when_driver_exposes_multiple_queues() {
        assert_eq!(video_present_queue_indices(true, 1), (0, 0));
        assert_eq!(video_present_queue_indices(true, 2), (0, 1));
        assert_eq!(video_present_queue_indices(false, 2), (0, 0));
    }

    #[test]
    fn extension_union_keeps_first_order_and_dedupes() {
        let extensions = dedup_static_extensions([
            "VK_KHR_video_queue",
            "VK_KHR_swapchain",
            "VK_KHR_video_queue",
            "VK_KHR_present_wait2",
        ]);

        assert_eq!(
            extensions,
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_swapchain",
                "VK_KHR_present_wait2"
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
            video_present_queue_family_model(true, true),
            "single-video-graphics-present-queue-family-single-queue"
        );
        assert_eq!(
            video_present_queue_family_model(true, false),
            "single-video-graphics-present-queue-family-split-queue-indices"
        );
    }
}
