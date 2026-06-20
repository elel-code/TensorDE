//! Hand-rolled Vulkan renderer spike.
//!
//! This module is intentionally separate from the existing wgpu path. The first
//! step is a concrete backend contract: native Wayland layer-shell ownership,
//! Vulkan surface/swapchain ownership, and direct video texture interop are
//! represented here before any default renderer switch is attempted.

#![allow(unsafe_code)]

use serde::Serialize;
#[cfg(feature = "native-vulkan-gst-video")]
use std::ffi::c_void;
use std::ffi::{CStr, CString};
use std::fmt;
#[cfg(feature = "native-vulkan-gst-video")]
use std::os::raw::c_char;
use std::path::PathBuf;
use std::ptr;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::VideoDecoderPolicy;
use crate::core::{FitMode, Transition};
use crate::renderer::native_wayland::{
    NativeWaylandError, NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};
#[cfg(feature = "native-vulkan-gst-video")]
use crate::renderer::video::{
    actual_decoder_reports, apply_decoder_rank_policy, decoder_policy_status, video_caps_reports,
};
use crate::renderer::{
    SceneLiteDisplayPlan, SceneLiteWallpaperPlan, SlideshowWallpaperPlan, StaticRenderSyncPlan,
    StaticWallpaperPlan, VideoWallpaperPlan,
};
use ash::vk;
#[cfg(feature = "native-vulkan-gst-video")]
use gst::prelude::*;
#[cfg(feature = "native-vulkan-gst-video")]
use gstreamer as gst;
#[cfg(feature = "native-vulkan-gst-video")]
use gstreamer_video as gst_video;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanCapabilities {
    pub built: bool,
    pub experimental: bool,
    pub default_enabled: bool,
    pub reuses_native_wayland_host: bool,
    pub owns_layer_shell_surface_now: bool,
    pub owns_vulkan_instance_now: bool,
    pub owns_vulkan_device_now: bool,
    pub owns_wayland_vulkan_surface_now: bool,
    pub owns_swapchain_now: bool,
    pub renders_frames_now: bool,
    pub consumes_render_sync: bool,
    pub direct_video_memory_status: &'static str,
    pub unsafe_policy: &'static str,
}

pub fn capabilities() -> NativeVulkanCapabilities {
    NativeVulkanCapabilities {
        built: true,
        experimental: true,
        default_enabled: false,
        reuses_native_wayland_host: true,
        owns_layer_shell_surface_now: true,
        owns_vulkan_instance_now: true,
        owns_vulkan_device_now: true,
        owns_wayland_vulkan_surface_now: true,
        owns_swapchain_now: true,
        renders_frames_now: true,
        consumes_render_sync: false,
        direct_video_memory_status: "contract-only: target is importable DMABuf/EGLImage/Vulkan image sampling",
        unsafe_policy: "unsafe is allowed inside audited Vulkan/Wayland/DMABuf FFI boundaries only",
    }
}

#[derive(Debug)]
pub enum NativeVulkanError {
    Wayland(NativeWaylandError),
    Loading(String),
    Vulkan {
        operation: &'static str,
        result: vk::Result,
    },
    MissingDeviceExtension(&'static str),
    MissingPresentQueue,
    MissingSurfaceFormat,
    UnsupportedSwapchainUsage(&'static str),
    InvalidSwapchainExtent,
    StaticImage(String),
    Video(String),
    MissingMemoryType(&'static str),
}

impl fmt::Display for NativeVulkanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(err) => write!(f, "{err}"),
            Self::Loading(err) => write!(f, "load Vulkan entry: {err}"),
            Self::Vulkan { operation, result } => write!(f, "{operation}: {result:?}"),
            Self::MissingDeviceExtension(extension) => {
                write!(f, "selected Vulkan device is missing {extension}")
            }
            Self::MissingPresentQueue => {
                write!(f, "no Vulkan graphics queue can present to Wayland surface")
            }
            Self::MissingSurfaceFormat => write!(f, "Wayland Vulkan surface has no formats"),
            Self::UnsupportedSwapchainUsage(usage) => {
                write!(
                    f,
                    "Wayland Vulkan surface does not support {usage} swapchain usage"
                )
            }
            Self::InvalidSwapchainExtent => write!(f, "invalid Vulkan swapchain extent"),
            Self::StaticImage(err) => write!(f, "static image error: {err}"),
            Self::Video(err) => write!(f, "video error: {err}"),
            Self::MissingMemoryType(label) => write!(f, "missing Vulkan memory type for {label}"),
        }
    }
}

impl std::error::Error for NativeVulkanError {}

impl From<NativeWaylandError> for NativeVulkanError {
    fn from(err: NativeWaylandError) -> Self {
        Self::Wayland(err)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanSurfaceProbeOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
}

impl Default for NativeVulkanSurfaceProbeOptions {
    fn default() -> Self {
        let mut host = NativeWaylandHostOptions::default();
        host.namespace = "gilder-native-vulkan".to_owned();
        Self {
            host,
            wait_configure_roundtrips: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSurfaceProbeSnapshot {
    pub wayland_surface_logical_size: (u32, u32),
    pub wayland_surface_buffer_size: (u32, u32),
    pub dmabuf_main_device: Option<u64>,
    pub physical_device_count: usize,
    pub present_queue_family_count: usize,
    pub selected_physical_device_index: Option<usize>,
    pub selected_physical_device_name: Option<String>,
    pub selected_physical_device_type: Option<&'static str>,
    pub selected_queue_family_index: Option<u32>,
    pub selected_queue_supports_graphics: bool,
    pub surface_capabilities: Option<NativeVulkanSurfaceCapabilitiesSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSurfaceCapabilitiesSnapshot {
    pub min_image_count: u32,
    pub max_image_count: u32,
    pub current_extent: Option<(u32, u32)>,
    pub min_image_extent: (u32, u32),
    pub max_image_extent: (u32, u32),
}

pub struct NativeVulkanSurfaceProbe {
    host: NativeWaylandHost,
    _entry: ash::Entry,
    instance: ash::Instance,
    surface_loader: ash::khr::surface::Instance,
    _wayland_surface_loader: ash::khr::wayland_surface::Instance,
    surface: vk::SurfaceKHR,
    snapshot: NativeVulkanSurfaceProbeSnapshot,
}

impl NativeVulkanSurfaceProbe {
    pub fn connect(options: NativeVulkanSurfaceProbeOptions) -> Result<Self, NativeVulkanError> {
        let mut host = NativeWaylandHost::connect(options.host)?;
        host.wait_until_configured(options.wait_configure_roundtrips)?;
        let handles = host.surface_handles()?;

        let (entry, instance) = create_native_vulkan_instance()?;
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let wayland_surface_loader = ash::khr::wayland_surface::Instance::new(&entry, &instance);
        let surface_create_info = vk::WaylandSurfaceCreateInfoKHR::default()
            .display(handles.display.as_ptr().cast::<vk::wl_display>())
            .surface(handles.surface.as_ptr().cast::<vk::wl_surface>());
        let surface = match unsafe {
            wayland_surface_loader.create_wayland_surface(&surface_create_info, None)
        } {
            Ok(surface) => surface,
            Err(result) => {
                unsafe {
                    instance.destroy_instance(None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateWaylandSurfaceKHR",
                    result,
                });
            }
        };

        let mut probe = Self {
            host,
            _entry: entry,
            instance,
            surface_loader,
            _wayland_surface_loader: wayland_surface_loader,
            surface,
            snapshot: NativeVulkanSurfaceProbeSnapshot::initial(handles),
        };
        probe.snapshot = probe.query_surface_snapshot(handles)?;
        Ok(probe)
    }

    pub fn pump_events(&mut self) -> Result<(), NativeVulkanError> {
        self.host.pump_events().map_err(Into::into)
    }

    pub fn snapshot(&self) -> NativeVulkanSurfaceProbeSnapshot {
        self.snapshot.clone()
    }

    fn query_surface_snapshot(
        &self,
        handles: NativeWaylandSurfaceHandles,
    ) -> Result<NativeVulkanSurfaceProbeSnapshot, NativeVulkanError> {
        let physical_devices =
            unsafe { self.instance.enumerate_physical_devices() }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkEnumeratePhysicalDevices",
                    result,
                }
            })?;
        let mut present_queue_family_count = 0usize;
        let mut selected = None;

        for (physical_device_index, physical_device) in physical_devices.iter().copied().enumerate()
        {
            let properties = unsafe {
                self.instance
                    .get_physical_device_properties(physical_device)
            };
            let queue_families = unsafe {
                self.instance
                    .get_physical_device_queue_family_properties(physical_device)
            };
            for (queue_family_index, queue_family) in queue_families.iter().enumerate() {
                let supports_surface = unsafe {
                    self.surface_loader.get_physical_device_surface_support(
                        physical_device,
                        queue_family_index as u32,
                        self.surface,
                    )
                }
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkGetPhysicalDeviceSurfaceSupportKHR",
                    result,
                })?;
                if !supports_surface {
                    continue;
                }
                present_queue_family_count += 1;

                let supports_graphics = queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
                if selected.is_none() && supports_graphics {
                    selected = Some(NativeVulkanPresentQueueSelection {
                        physical_device,
                        physical_device_index,
                        physical_device_name: native_vulkan_physical_device_name(properties),
                        physical_device_type: native_vulkan_physical_device_type_label(
                            properties.device_type,
                        ),
                        queue_family_index: queue_family_index as u32,
                    });
                }
            }
        }

        let Some(selected) = selected else {
            return Err(NativeVulkanError::MissingPresentQueue);
        };
        let surface_capabilities = unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(selected.physical_device, self.surface)
        }
        .map_err(|result| NativeVulkanError::Vulkan {
            operation: "vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
            result,
        })?;

        Ok(NativeVulkanSurfaceProbeSnapshot {
            wayland_surface_logical_size: handles.logical_size,
            wayland_surface_buffer_size: handles.buffer_size,
            dmabuf_main_device: handles.dmabuf_main_device,
            physical_device_count: physical_devices.len(),
            present_queue_family_count,
            selected_physical_device_index: Some(selected.physical_device_index),
            selected_physical_device_name: Some(selected.physical_device_name),
            selected_physical_device_type: Some(selected.physical_device_type),
            selected_queue_family_index: Some(selected.queue_family_index),
            selected_queue_supports_graphics: true,
            surface_capabilities: Some(surface_capabilities.into()),
        })
    }
}

impl Drop for NativeVulkanSurfaceProbe {
    fn drop(&mut self) {
        unsafe {
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

impl NativeVulkanSurfaceProbeSnapshot {
    fn initial(handles: NativeWaylandSurfaceHandles) -> Self {
        Self {
            wayland_surface_logical_size: handles.logical_size,
            wayland_surface_buffer_size: handles.buffer_size,
            dmabuf_main_device: handles.dmabuf_main_device,
            physical_device_count: 0,
            present_queue_family_count: 0,
            selected_physical_device_index: None,
            selected_physical_device_name: None,
            selected_physical_device_type: None,
            selected_queue_family_index: None,
            selected_queue_supports_graphics: false,
            surface_capabilities: None,
        }
    }
}

impl From<vk::SurfaceCapabilitiesKHR> for NativeVulkanSurfaceCapabilitiesSnapshot {
    fn from(capabilities: vk::SurfaceCapabilitiesKHR) -> Self {
        Self {
            min_image_count: capabilities.min_image_count,
            max_image_count: capabilities.max_image_count,
            current_extent: native_vulkan_extent(capabilities.current_extent),
            min_image_extent: (
                capabilities.min_image_extent.width,
                capabilities.min_image_extent.height,
            ),
            max_image_extent: (
                capabilities.max_image_extent.width,
                capabilities.max_image_extent.height,
            ),
        }
    }
}

struct NativeVulkanPresentQueueSelection {
    physical_device: vk::PhysicalDevice,
    physical_device_index: usize,
    physical_device_name: String,
    physical_device_type: &'static str,
    queue_family_index: u32,
}

pub fn probe_wayland_surface(
    options: NativeVulkanSurfaceProbeOptions,
) -> Result<NativeVulkanSurfaceProbeSnapshot, NativeVulkanError> {
    let mut probe = NativeVulkanSurfaceProbe::connect(options)?;
    probe.pump_events()?;
    Ok(probe.snapshot())
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub clear_color: NativeVulkanClearColor,
    pub target_max_fps: Option<u32>,
}

impl Default for NativeVulkanOptions {
    fn default() -> Self {
        let mut host = NativeWaylandHostOptions::default();
        host.namespace = "gilder-native-vulkan".to_owned();
        Self {
            host,
            wait_configure_roundtrips: 8,
            clear_color: NativeVulkanClearColor::default(),
            target_max_fps: Some(240),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NativeVulkanClearColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for NativeVulkanClearColor {
    fn default() -> Self {
        Self {
            r: 0.02,
            g: 0.04,
            b: 0.07,
            a: 1.0,
        }
    }
}

impl From<NativeVulkanClearColor> for vk::ClearColorValue {
    fn from(color: NativeVulkanClearColor) -> Self {
        Self {
            float32: [color.r, color.g, color.b, color.a],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanRuntimeSnapshot {
    pub runtime_elapsed_ms: u64,
    pub frames_rendered: u64,
    pub average_render_fps: f64,
    pub configured: bool,
    pub wayland_surface_logical_size: (u32, u32),
    pub wayland_surface_buffer_size: (u32, u32),
    pub selected_physical_device_name: String,
    pub selected_physical_device_type: &'static str,
    pub selected_queue_family_index: u32,
    pub swapchain_extent: (u32, u32),
    pub swapchain_image_count: usize,
    pub swapchain_format: String,
    pub present_mode: &'static str,
    pub clear_color: NativeVulkanClearColor,
    pub static_upload_bytes: Option<u64>,
    pub video_runtime: Option<NativeVulkanVideoRuntimeSnapshot>,
    pub render_item: NativeVulkanRenderItem,
    pub last_render_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoRuntimeSnapshot {
    pub source: PathBuf,
    pub poster: Option<PathBuf>,
    pub fit: FitMode,
    pub loop_playback: bool,
    pub muted: bool,
    pub manifest_max_fps: Option<u32>,
    pub target_max_fps: Option<u32>,
    pub decoder_policy: VideoDecoderPolicy,
    pub start_offset_ms: u64,
    pub frontend: &'static str,
    pub frontend_status: &'static str,
    pub handoff_status: &'static str,
    pub texture_import_status: &'static str,
    pub audio_status: &'static str,
    pub gst_state: Option<String>,
    pub eos_messages: u64,
    pub segment_done_messages: u64,
    pub frames_received: u64,
    pub frames_imported: u64,
    pub rendered_placeholder_frames: u64,
    pub poster_upload_bytes: Option<u64>,
    pub last_import_size: Option<(u32, u32)>,
    pub last_import_memory_path: Option<String>,
    pub last_import_error: Option<String>,
    pub last_import_elapsed_us: Option<u64>,
    pub max_import_elapsed_us: Option<u64>,
    pub last_sample_caps: Option<String>,
    pub last_sample_format: Option<String>,
    pub last_sample_size: Option<(u32, u32)>,
    pub last_sample_pts_ms: Option<u64>,
    pub last_sample_duration_ms: Option<u64>,
    pub last_sample_pts_delta_ms: Option<u64>,
    pub last_sample_memory_types: Vec<String>,
    pub actual_decoders: Vec<String>,
    pub decoder_policy_status: Option<String>,
    pub caps_report_count: usize,
    pub caps_memory_features: Vec<String>,
    pub caps_reports: Vec<NativeVulkanVideoCapsSnapshot>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoCapsSnapshot {
    pub element: String,
    pub pad: String,
    pub direction: String,
    pub caps: String,
    pub source: String,
    pub memory_features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanGstVideoFrontendSnapshot {
    gst_state: Option<String>,
    eos_messages: u64,
    segment_done_messages: u64,
    frames_received: u64,
    last_sample_caps: Option<String>,
    last_sample_format: Option<String>,
    last_sample_size: Option<(u32, u32)>,
    last_sample_pts_ms: Option<u64>,
    last_sample_duration_ms: Option<u64>,
    last_sample_pts_delta_ms: Option<u64>,
    last_sample_memory_types: Vec<String>,
    actual_decoders: Vec<String>,
    decoder_policy_status: Option<String>,
    caps_report_count: usize,
    caps_memory_features: Vec<String>,
    caps_reports: Vec<NativeVulkanVideoCapsSnapshot>,
    last_error: Option<String>,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanVideoImportSnapshot {
    texture_import_status: &'static str,
    frames_imported: u64,
    last_import_size: Option<(u32, u32)>,
    last_import_memory_path: Option<String>,
    last_import_error: Option<String>,
    last_import_elapsed_us: Option<u64>,
    max_import_elapsed_us: Option<u64>,
}

#[cfg(not(feature = "native-vulkan-gst-video"))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanVideoImportSnapshot {
    texture_import_status: &'static str,
    frames_imported: u64,
    last_import_size: Option<(u32, u32)>,
    last_import_memory_path: Option<String>,
    last_import_error: Option<String>,
    last_import_elapsed_us: Option<u64>,
    max_import_elapsed_us: Option<u64>,
}

pub struct NativeVulkanSession {
    host: NativeWaylandHost,
    _entry: ash::Entry,
    instance: ash::Instance,
    surface_loader: ash::khr::surface::Instance,
    _wayland_surface_loader: ash::khr::wayland_surface::Instance,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    selected_physical_device_name: String,
    selected_physical_device_type: &'static str,
    queue_family_index: u32,
    device: ash::Device,
    queue: vk::Queue,
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    swapchain_format: vk::Format,
    present_mode: vk::PresentModeKHR,
    swapchain_extent: vk::Extent2D,
    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,
    swapchain_image_layouts: Vec<vk::ImageLayout>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    in_flight: vk::Fence,
    static_upload: Option<NativeVulkanStaticImageUpload>,
    #[cfg(feature = "native-vulkan-gst-video")]
    video_frontend: Option<NativeVulkanGstVideoFrontend>,
    #[cfg(feature = "native-vulkan-gst-video")]
    video_renderer: Option<NativeVulkanVideoRenderer>,
    #[cfg(feature = "native-vulkan-gst-video")]
    video_texture: Option<NativeVulkanCudaVideoTexture>,
    #[cfg(feature = "native-vulkan-gst-video")]
    video_import_status: NativeVulkanVideoImportStatus,
    clear_color: NativeVulkanClearColor,
    render_item: NativeVulkanRenderItem,
    started_at: Instant,
    frames_rendered: u64,
    last_render_error: Option<String>,
}

impl NativeVulkanSession {
    pub fn connect(options: NativeVulkanOptions) -> Result<Self, NativeVulkanError> {
        Self::connect_with_render_item(
            options,
            NativeVulkanRenderItem::Clear {
                output_name: "native-vulkan".to_owned(),
            },
        )
    }

    pub fn connect_with_render_item(
        options: NativeVulkanOptions,
        render_item: NativeVulkanRenderItem,
    ) -> Result<Self, NativeVulkanError> {
        let mut host = NativeWaylandHost::connect(options.host)?;
        host.wait_until_configured(options.wait_configure_roundtrips)?;
        let handles = host.surface_handles()?;

        let (entry, instance) = create_native_vulkan_instance()?;
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let wayland_surface_loader = ash::khr::wayland_surface::Instance::new(&entry, &instance);
        let surface_create_info = vk::WaylandSurfaceCreateInfoKHR::default()
            .display(handles.display.as_ptr().cast::<vk::wl_display>())
            .surface(handles.surface.as_ptr().cast::<vk::wl_surface>());
        let surface = match unsafe {
            wayland_surface_loader.create_wayland_surface(&surface_create_info, None)
        } {
            Ok(surface) => surface,
            Err(result) => {
                unsafe {
                    instance.destroy_instance(None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateWaylandSurfaceKHR",
                    result,
                });
            }
        };

        let selection =
            select_native_vulkan_present_queue(&instance, &surface_loader, surface)?.selection;
        ensure_native_vulkan_device_extension(
            &instance,
            selection.physical_device,
            ash::khr::swapchain::NAME,
        )?;
        #[cfg(feature = "native-vulkan-gst-video")]
        let video_enabled = matches!(&render_item, NativeVulkanRenderItem::Video { .. });
        #[cfg(feature = "native-vulkan-gst-video")]
        if video_enabled {
            ensure_native_vulkan_device_extension(
                &instance,
                selection.physical_device,
                ash::khr::external_memory_fd::NAME,
            )?;
        }
        let priorities = [1.0_f32];
        let queue_create_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(selection.queue_family_index)
            .queue_priorities(&priorities);
        let queue_create_infos = [queue_create_info];
        let mut device_extensions = vec![ash::khr::swapchain::NAME.as_ptr()];
        #[cfg(feature = "native-vulkan-gst-video")]
        if video_enabled {
            device_extensions.push(ash::khr::external_memory_fd::NAME.as_ptr());
        }
        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&device_extensions);
        let device =
            unsafe { instance.create_device(selection.physical_device, &device_create_info, None) }
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkCreateDevice",
                    result,
                })?;
        let queue = unsafe { device.get_device_queue(selection.queue_family_index, 0) };
        let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);
        let swapchain_plan = create_native_vulkan_swapchain_plan(
            &surface_loader,
            selection.physical_device,
            surface,
            handles.logical_size,
            handles.buffer_size,
        )?;
        let swapchain =
            unsafe { swapchain_loader.create_swapchain(&swapchain_plan.create_info, None) }
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkCreateSwapchainKHR",
                    result,
                })?;
        let swapchain_images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkGetSwapchainImagesKHR",
                result,
            })?;
        let swapchain_image_views = create_native_vulkan_swapchain_image_views(
            &device,
            &swapchain_images,
            swapchain_plan.format.format,
        )?;
        let command_pool_create_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(selection.queue_family_index);
        let command_pool = unsafe { device.create_command_pool(&command_pool_create_info, None) }
            .map_err(|result| NativeVulkanError::Vulkan {
            operation: "vkCreateCommandPool",
            result,
        })?;
        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(swapchain_images.len() as u32);
        let command_buffers =
            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info) }.map_err(
                |result| NativeVulkanError::Vulkan {
                    operation: "vkAllocateCommandBuffers",
                    result,
                },
            )?;
        let semaphore_create_info = vk::SemaphoreCreateInfo::default();
        let image_available = unsafe { device.create_semaphore(&semaphore_create_info, None) }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkCreateSemaphore(image_available)",
                result,
            })?;
        let render_finished = unsafe { device.create_semaphore(&semaphore_create_info, None) }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkCreateSemaphore(render_finished)",
                result,
            })?;
        let fence_create_info =
            vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        let in_flight =
            unsafe { device.create_fence(&fence_create_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkCreateFence",
                    result,
                }
            })?;
        let static_upload = match &render_item {
            NativeVulkanRenderItem::StaticImage {
                source,
                fit,
                background,
                ..
            } => Some(NativeVulkanStaticImageUpload::new(
                &instance,
                selection.physical_device,
                &device,
                source,
                *fit,
                background.as_deref(),
                swapchain_plan.format.format,
                swapchain_plan.extent,
            )?),
            NativeVulkanRenderItem::Video {
                poster: Some(poster),
                fit,
                ..
            } => Some(NativeVulkanStaticImageUpload::new(
                &instance,
                selection.physical_device,
                &device,
                poster,
                *fit,
                None,
                swapchain_plan.format.format,
                swapchain_plan.extent,
            )?),
            _ => None,
        };
        #[cfg(feature = "native-vulkan-gst-video")]
        let video_frontend = match &render_item {
            NativeVulkanRenderItem::Video { .. } => {
                Some(NativeVulkanGstVideoFrontend::new(&render_item)?)
            }
            _ => None,
        };
        #[cfg(feature = "native-vulkan-gst-video")]
        let video_renderer = match &render_item {
            NativeVulkanRenderItem::Video { .. } => Some(NativeVulkanVideoRenderer::new(
                &device,
                swapchain_plan.format.format,
                swapchain_plan.extent,
                &swapchain_image_views,
            )?),
            _ => None,
        };

        Ok(Self {
            host,
            _entry: entry,
            instance,
            surface_loader,
            _wayland_surface_loader: wayland_surface_loader,
            surface,
            physical_device: selection.physical_device,
            selected_physical_device_name: selection.physical_device_name,
            selected_physical_device_type: selection.physical_device_type,
            queue_family_index: selection.queue_family_index,
            device,
            queue,
            swapchain_loader,
            swapchain,
            swapchain_format: swapchain_plan.format.format,
            present_mode: swapchain_plan.present_mode,
            swapchain_extent: swapchain_plan.extent,
            swapchain_image_layouts: vec![vk::ImageLayout::UNDEFINED; swapchain_images.len()],
            swapchain_image_views,
            swapchain_images,
            command_pool,
            command_buffers,
            image_available,
            render_finished,
            in_flight,
            static_upload,
            #[cfg(feature = "native-vulkan-gst-video")]
            video_frontend,
            #[cfg(feature = "native-vulkan-gst-video")]
            video_renderer,
            #[cfg(feature = "native-vulkan-gst-video")]
            video_texture: None,
            #[cfg(feature = "native-vulkan-gst-video")]
            video_import_status: NativeVulkanVideoImportStatus::default(),
            clear_color: options.clear_color,
            render_item,
            started_at: Instant::now(),
            frames_rendered: 0,
            last_render_error: None,
        })
    }

    pub fn run_for(
        &mut self,
        duration: Duration,
        target_max_fps: Option<u32>,
    ) -> Result<NativeVulkanRuntimeSnapshot, NativeVulkanError> {
        let deadline = Instant::now() + duration;
        let frame_interval = target_max_fps
            .filter(|fps| *fps > 0)
            .map(|fps| Duration::from_secs_f64(1.0 / fps as f64));
        let mut next_frame = Instant::now();

        while Instant::now() < deadline && !self.host.is_closed() {
            self.host.pump_events()?;
            self.wait_for_in_flight()?;
            self.poll_video_frontend()?;
            match self.render_frame() {
                Ok(()) => {}
                Err(err) => {
                    self.last_render_error = Some(err.to_string());
                    return Err(err);
                }
            }

            if let Some(interval) = frame_interval {
                next_frame += interval;
                let now = Instant::now();
                if next_frame > now {
                    thread::sleep(next_frame - now);
                } else {
                    next_frame = now;
                }
            }
        }

        Ok(self.snapshot())
    }

    pub fn snapshot(&self) -> NativeVulkanRuntimeSnapshot {
        let elapsed = self.started_at.elapsed();
        NativeVulkanRuntimeSnapshot {
            runtime_elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
            frames_rendered: self.frames_rendered,
            average_render_fps: if elapsed.is_zero() {
                0.0
            } else {
                self.frames_rendered as f64 / elapsed.as_secs_f64()
            },
            configured: self.host.snapshot().configured,
            wayland_surface_logical_size: self
                .host
                .logical_size()
                .unwrap_or((self.swapchain_extent.width, self.swapchain_extent.height)),
            wayland_surface_buffer_size: (
                self.swapchain_extent.width,
                self.swapchain_extent.height,
            ),
            selected_physical_device_name: self.selected_physical_device_name.clone(),
            selected_physical_device_type: self.selected_physical_device_type,
            selected_queue_family_index: self.queue_family_index,
            swapchain_extent: (self.swapchain_extent.width, self.swapchain_extent.height),
            swapchain_image_count: self.swapchain_images.len(),
            swapchain_format: format!("{:?}", self.swapchain_format),
            present_mode: native_vulkan_present_mode_label(self.present_mode),
            clear_color: self.clear_color,
            static_upload_bytes: self
                .static_upload
                .as_ref()
                .map(|upload| upload.size_bytes.min(u64::MAX as vk::DeviceSize) as u64),
            video_runtime: native_vulkan_video_runtime_snapshot(
                &self.render_item,
                self.video_frontend_snapshot(),
                self.video_import_snapshot(),
                self.frames_rendered,
                self.static_upload
                    .as_ref()
                    .map(|upload| upload.size_bytes.min(u64::MAX as vk::DeviceSize) as u64),
            ),
            render_item: self.render_item.clone(),
            last_render_error: self.last_render_error.clone(),
        }
    }

    fn render_frame(&mut self) -> Result<(), NativeVulkanError> {
        self.wait_for_in_flight()?;
        let fences = [self.in_flight];
        unsafe {
            self.device
                .reset_fences(&fences)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkResetFences",
                    result,
                })?;
        }

        let (image_index, _) = unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.image_available,
                vk::Fence::null(),
            )
        }
        .map_err(|result| NativeVulkanError::Vulkan {
            operation: "vkAcquireNextImageKHR",
            result,
        })?;
        let image_index = image_index as usize;
        let command_buffer = self.command_buffers[image_index];
        self.record_frame_command(command_buffer, image_index)?;

        let wait_semaphores = [self.image_available];
        let wait_stages = [self.current_render_wait_stage()];
        let command_buffers = [command_buffer];
        let signal_semaphores = [self.render_finished];
        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        let submit_infos = [submit_info];
        unsafe {
            self.device
                .queue_submit(self.queue, &submit_infos, self.in_flight)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkQueueSubmit",
                    result,
                })?;
        }

        let swapchains = [self.swapchain];
        let image_indices = [image_index as u32];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        unsafe {
            self.swapchain_loader
                .queue_present(self.queue, &present_info)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkQueuePresentKHR",
                    result,
                })?;
        }
        self.frames_rendered += 1;
        Ok(())
    }

    fn wait_for_in_flight(&self) -> Result<(), NativeVulkanError> {
        let fences = [self.in_flight];
        unsafe {
            self.device
                .wait_for_fences(&fences, true, u64::MAX)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkWaitForFences",
                    result,
                })
        }
    }

    fn record_frame_command(
        &mut self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
    ) -> Result<(), NativeVulkanError> {
        #[cfg(feature = "native-vulkan-gst-video")]
        if self.video_texture.is_some() && self.video_renderer.is_some() {
            return self.record_video_frame_command(command_buffer, image_index);
        }
        unsafe {
            self.device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkResetCommandBuffer",
                    result,
                })?;
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkBeginCommandBuffer",
                    result,
                })?;

            let image = self.swapchain_images[image_index];
            let old_layout = self.swapchain_image_layouts[image_index];
            let range = native_vulkan_color_subresource_range();
            let to_transfer = vk::ImageMemoryBarrier::default()
                .old_layout(old_layout)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_transfer],
            );

            if let Some(static_upload) = &self.static_upload {
                let copy = static_upload.buffer_image_copy;
                self.device.cmd_copy_buffer_to_image(
                    command_buffer,
                    static_upload.buffer,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[copy],
                );
            } else {
                let clear_color = vk::ClearColorValue::from(self.clear_color);
                self.device.cmd_clear_color_image(
                    command_buffer,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &clear_color,
                    &[range],
                );
            }

            let to_present = vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(range)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::empty());
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_present],
            );

            self.device
                .end_command_buffer(command_buffer)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkEndCommandBuffer",
                    result,
                })?;
            self.swapchain_image_layouts[image_index] = vk::ImageLayout::PRESENT_SRC_KHR;
        }
        Ok(())
    }

    fn current_render_wait_stage(&self) -> vk::PipelineStageFlags {
        #[cfg(feature = "native-vulkan-gst-video")]
        if self.video_texture.is_some() && self.video_renderer.is_some() {
            return vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
        }
        vk::PipelineStageFlags::TRANSFER
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn record_video_frame_command(
        &mut self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
    ) -> Result<(), NativeVulkanError> {
        let texture = self
            .video_texture
            .as_ref()
            .ok_or_else(|| NativeVulkanError::Video("video texture is not ready".to_owned()))?;
        let renderer = self
            .video_renderer
            .as_ref()
            .ok_or_else(|| NativeVulkanError::Video("video renderer is not ready".to_owned()))?;
        let fit = match &self.render_item {
            NativeVulkanRenderItem::Video { fit, .. } => *fit,
            _ => FitMode::Cover,
        };
        renderer.record_frame(
            &self.device,
            command_buffer,
            image_index,
            self.swapchain_images[image_index],
            self.swapchain_image_layouts[image_index],
            texture,
            fit,
        )?;
        self.swapchain_image_layouts[image_index] = vk::ImageLayout::PRESENT_SRC_KHR;
        Ok(())
    }

    fn poll_video_frontend(&mut self) -> Result<(), NativeVulkanError> {
        #[cfg(feature = "native-vulkan-gst-video")]
        if let Some(frontend) = self.video_frontend.as_mut() {
            frontend.poll()?;
            if let Some(sample) = frontend.take_latest_sample() {
                self.import_video_sample(sample);
            }
        }
        Ok(())
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn video_frontend_snapshot(&self) -> Option<NativeVulkanGstVideoFrontendSnapshot> {
        self.video_frontend
            .as_ref()
            .map(NativeVulkanGstVideoFrontend::snapshot)
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn video_import_snapshot(&self) -> Option<NativeVulkanVideoImportSnapshot> {
        matches!(self.render_item, NativeVulkanRenderItem::Video { .. })
            .then(|| self.video_import_status.snapshot())
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn import_video_sample(&mut self, sample: gst::Sample) {
        let started_at = Instant::now();
        let import_result = self.import_video_sample_inner(&sample);
        match import_result {
            Ok(mut report) => {
                report.elapsed_us = native_vulkan_elapsed_us(started_at.elapsed());
                self.video_import_status.record_import(report);
            }
            Err(err) => self.video_import_status.record_error(err.to_string()),
        }
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn import_video_sample_inner(
        &mut self,
        sample: &gst::Sample,
    ) -> Result<NativeVulkanVideoImportReport, NativeVulkanError> {
        let renderer = self.video_renderer.as_mut().ok_or_else(|| {
            NativeVulkanError::Video("native Vulkan video renderer is not initialized".to_owned())
        })?;
        let buffer = sample
            .buffer()
            .ok_or_else(|| NativeVulkanError::Video("appsink sample has no buffer".to_owned()))?;
        let meta = native_vulkan_gst_system_nv12_meta(sample, buffer)?;
        if !native_vulkan_gst_buffer_has_cuda_memory(buffer) {
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan CUDA import expected CUDAMemory, got {}",
                native_vulkan_gst_memory_types(buffer).join("|")
            )));
        }
        let cuda_context = native_vulkan_gst_cuda_context_from_buffer(buffer)?;
        let recreate = self
            .video_texture
            .as_ref()
            .map(|texture| !texture.matches(cuda_context, meta.width, meta.height))
            .unwrap_or(true);
        if recreate {
            if let Some(texture) = self.video_texture.take() {
                texture.destroy(&self.device);
            }
            self.video_texture = Some(NativeVulkanCudaVideoTexture::new(
                &self.instance,
                self.physical_device,
                self.queue,
                self.command_pool,
                &self.device,
                self.queue_family_index,
                cuda_context,
                meta.width,
                meta.height,
            )?);
            renderer.update_descriptors(
                &self.device,
                self.video_texture
                    .as_ref()
                    .expect("video texture must exist after create"),
            );
        }
        let texture = self
            .video_texture
            .as_mut()
            .expect("video texture must exist before copy");
        texture.copy_sample(buffer, &meta)?;
        Ok(NativeVulkanVideoImportReport {
            width: meta.width,
            height: meta.height,
            memory_path: "CUDAMemory->CUDA->Vulkan external image planes".to_owned(),
            elapsed_us: 0,
        })
    }

    #[cfg(not(feature = "native-vulkan-gst-video"))]
    fn video_frontend_snapshot(&self) -> Option<NativeVulkanGstVideoFrontendSnapshot> {
        None
    }

    #[cfg(not(feature = "native-vulkan-gst-video"))]
    fn video_import_snapshot(&self) -> Option<NativeVulkanVideoImportSnapshot> {
        None
    }
}

impl Drop for NativeVulkanSession {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            #[cfg(feature = "native-vulkan-gst-video")]
            if let Some(texture) = self.video_texture.take() {
                texture.destroy(&self.device);
            }
            #[cfg(feature = "native-vulkan-gst-video")]
            if let Some(renderer) = self.video_renderer.take() {
                renderer.destroy(&self.device);
            }
            if let Some(static_upload) = self.static_upload.take() {
                static_upload.destroy(&self.device);
            }
            self.device.destroy_fence(self.in_flight, None);
            self.device.destroy_semaphore(self.render_finished, None);
            self.device.destroy_semaphore(self.image_available, None);
            self.device.destroy_command_pool(self.command_pool, None);
            for view in self.swapchain_image_views.drain(..) {
                self.device.destroy_image_view(view, None);
            }
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

pub fn run_clear(
    options: NativeVulkanOptions,
    duration: Duration,
) -> Result<NativeVulkanRuntimeSnapshot, NativeVulkanError> {
    let target_max_fps = options.target_max_fps;
    let mut session = NativeVulkanSession::connect(options)?;
    session.run_for(duration, target_max_fps)
}

pub fn run_static_image(
    options: NativeVulkanOptions,
    duration: Duration,
    plan: StaticWallpaperPlan,
) -> Result<NativeVulkanRuntimeSnapshot, NativeVulkanError> {
    let target_max_fps = options.target_max_fps;
    let item = native_vulkan_static_item(&plan);
    let mut session = NativeVulkanSession::connect_with_render_item(options, item)?;
    session.run_for(duration, target_max_fps)
}

pub fn run_video(
    options: NativeVulkanOptions,
    duration: Duration,
    plan: VideoWallpaperPlan,
) -> Result<NativeVulkanRuntimeSnapshot, NativeVulkanError> {
    let target_max_fps = options.target_max_fps;
    let item = native_vulkan_video_item(&plan);
    let mut session = NativeVulkanSession::connect_with_render_item(options, item)?;
    session.run_for(duration, target_max_fps)
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanGstVideoFrontend {
    pipeline: gst::Element,
    sink: gst::Element,
    bus: gst::Bus,
    loop_playback: bool,
    decoder_policy: VideoDecoderPolicy,
    eos_messages: u64,
    segment_done_messages: u64,
    frames_received: u64,
    last_sample_caps: Option<String>,
    last_sample_format: Option<String>,
    last_sample_size: Option<(u32, u32)>,
    last_sample_pts_ms: Option<u64>,
    last_sample_duration_ms: Option<u64>,
    last_sample_pts_delta_ms: Option<u64>,
    last_sample_memory_types: Vec<String>,
    latest_sample: Option<gst::Sample>,
    last_error: Option<String>,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanGstVideoFrontend {
    fn new(item: &NativeVulkanRenderItem) -> Result<Self, NativeVulkanError> {
        let NativeVulkanRenderItem::Video {
            source,
            loop_playback,
            decoder_policy,
            start_offset_ms,
            ..
        } = item
        else {
            return Err(NativeVulkanError::Video(
                "GStreamer frontend requires a video render item".to_owned(),
            ));
        };

        gst::init().map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        apply_decoder_rank_policy(*decoder_policy);
        let pipeline = native_vulkan_gst_video_pipeline(source)?;
        let sink = pipeline
            .by_name("gilder-native-vulkan-video-appsink")
            .ok_or_else(|| NativeVulkanError::Video("video appsink not found".to_owned()))?;
        let bus = pipeline
            .bus()
            .ok_or_else(|| NativeVulkanError::Video("video pipeline has no bus".to_owned()))?;
        pipeline
            .set_state(gst::State::Paused)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        let _ = pipeline.state(gst::ClockTime::from_seconds(5));
        if *loop_playback {
            native_vulkan_gst_seek_loop_segment(pipeline.upcast_ref(), *start_offset_ms)?;
        } else if *start_offset_ms > 0 {
            native_vulkan_gst_seek_once(pipeline.upcast_ref(), *start_offset_ms)?;
        }
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

        Ok(Self {
            pipeline: pipeline.upcast::<gst::Element>(),
            sink,
            bus,
            loop_playback: *loop_playback,
            decoder_policy: *decoder_policy,
            eos_messages: 0,
            segment_done_messages: 0,
            frames_received: 0,
            last_sample_caps: None,
            last_sample_format: None,
            last_sample_size: None,
            last_sample_pts_ms: None,
            last_sample_duration_ms: None,
            last_sample_pts_delta_ms: None,
            last_sample_memory_types: Vec::new(),
            latest_sample: None,
            last_error: None,
        })
    }

    fn poll(&mut self) -> Result<(), NativeVulkanError> {
        self.poll_bus()?;
        self.pull_available_samples();
        Ok(())
    }

    fn poll_bus(&mut self) -> Result<(), NativeVulkanError> {
        while let Some(message) = self.bus.pop() {
            match message.view() {
                gst::MessageView::Eos(_) => {
                    self.eos_messages = self.eos_messages.saturating_add(1);
                    if self.loop_playback {
                        native_vulkan_gst_seek_loop_segment(&self.pipeline, 0)?;
                    } else {
                        self.pipeline
                            .set_state(gst::State::Paused)
                            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
                    }
                }
                gst::MessageView::SegmentDone(_) => {
                    self.segment_done_messages = self.segment_done_messages.saturating_add(1);
                    if self.loop_playback {
                        native_vulkan_gst_seek_loop_segment(&self.pipeline, 0)?;
                    }
                }
                gst::MessageView::Error(err) => {
                    let mut message = format!(
                        "{}: {}",
                        err.src()
                            .map(|src| src.path_string())
                            .unwrap_or_else(|| "gstreamer".into()),
                        err.error()
                    );
                    if let Some(debug) = err.debug() {
                        message.push_str(": ");
                        message.push_str(&debug);
                    }
                    self.last_error = Some(message.clone());
                    return Err(NativeVulkanError::Video(message));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn pull_available_samples(&mut self) {
        let sample = self
            .sink
            .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&0u64]);
        let Some(sample) = sample else {
            return;
        };
        self.record_sample(&sample);
        self.latest_sample = Some(sample);
    }

    fn record_sample(&mut self, sample: &gst::Sample) {
        self.frames_received = self.frames_received.saturating_add(1);
        self.last_sample_caps = sample.caps().map(|caps| caps.to_string());
        if let Some(caps) = sample.caps()
            && let Some(structure) = caps.structure(0)
        {
            self.last_sample_format = structure.get::<String>("format").ok();
            let width = structure.get::<i32>("width").ok();
            let height = structure.get::<i32>("height").ok();
            self.last_sample_size = width.zip(height).and_then(|(width, height)| {
                Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
            });
        }
        self.last_sample_memory_types = sample
            .buffer()
            .map(|buffer| {
                let pts_ms = native_vulkan_clock_time_ms(buffer.pts());
                self.last_sample_pts_delta_ms = self
                    .last_sample_pts_ms
                    .zip(pts_ms)
                    .and_then(|(previous, current)| current.checked_sub(previous));
                self.last_sample_pts_ms = pts_ms;
                self.last_sample_duration_ms = native_vulkan_clock_time_ms(buffer.duration());
                native_vulkan_gst_memory_types(buffer)
            })
            .unwrap_or_else(|| {
                self.last_sample_pts_ms = None;
                self.last_sample_duration_ms = None;
                self.last_sample_pts_delta_ms = None;
                Vec::new()
            });
        self.last_error = None;
    }

    fn take_latest_sample(&mut self) -> Option<gst::Sample> {
        self.latest_sample.take()
    }

    fn snapshot(&self) -> NativeVulkanGstVideoFrontendSnapshot {
        let gst_state = Some(
            self.pipeline
                .state(gst::ClockTime::ZERO)
                .1
                .name()
                .to_string(),
        );
        let decoder_reports = actual_decoder_reports(&self.pipeline);
        let actual_decoders = decoder_reports
            .iter()
            .map(|report| report.element.clone())
            .collect::<Vec<_>>();
        let decoder_policy_status = Some(format!(
            "{:?}",
            decoder_policy_status(self.decoder_policy, &decoder_reports)
        ));
        let caps_reports = video_caps_reports(&self.pipeline);
        let mut caps_memory_features = caps_reports
            .iter()
            .flat_map(|report| report.memory_features.iter().cloned())
            .collect::<Vec<_>>();
        caps_memory_features.sort();
        caps_memory_features.dedup();
        let caps_report_count = caps_reports.len();
        let caps_reports = caps_reports
            .into_iter()
            .map(|report| NativeVulkanVideoCapsSnapshot {
                element: report.element,
                pad: report.pad,
                direction: report.direction,
                caps: report.caps,
                source: report.source,
                memory_features: report.memory_features,
            })
            .collect();

        NativeVulkanGstVideoFrontendSnapshot {
            gst_state,
            eos_messages: self.eos_messages,
            segment_done_messages: self.segment_done_messages,
            frames_received: self.frames_received,
            last_sample_caps: self.last_sample_caps.clone(),
            last_sample_format: self.last_sample_format.clone(),
            last_sample_size: self.last_sample_size,
            last_sample_pts_ms: self.last_sample_pts_ms,
            last_sample_duration_ms: self.last_sample_duration_ms,
            last_sample_pts_delta_ms: self.last_sample_pts_delta_ms,
            last_sample_memory_types: self.last_sample_memory_types.clone(),
            actual_decoders,
            decoder_policy_status,
            caps_report_count,
            caps_memory_features,
            caps_reports,
            last_error: self.last_error.clone(),
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanGstVideoFrontend {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_video_pipeline(source: &PathBuf) -> Result<gst::Pipeline, NativeVulkanError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_vulkan_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let decodebin = native_vulkan_gst_element("decodebin")?;
    let queue = native_vulkan_gst_element("queue")?;
    native_vulkan_configure_queue(&queue);
    let sink = native_vulkan_gst_element("appsink")?;
    sink.set_property("name", "gilder-native-vulkan-video-appsink");
    native_vulkan_configure_appsink(&sink);

    pipeline
        .add_many([&filesrc, &decodebin, &queue, &sink])
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    filesrc
        .link(&decodebin)
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    queue
        .link(&sink)
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeVulkanError::Video("queue has no sink pad".to_owned()))?;
    decodebin.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() {
            return;
        }
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let is_video = caps
            .structure(0)
            .map(|structure| structure.name().starts_with("video/"))
            .unwrap_or(false);
        if is_video {
            let _ = pad.link(&queue_sink);
        }
    });

    Ok(pipeline)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_element(name: &str) -> Result<gst::Element, NativeVulkanError> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|err| NativeVulkanError::Video(format!("create {name}: {err}")))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_seek_once(
    pipeline: &gst::Element,
    start_offset_ms: u64,
) -> Result<(), NativeVulkanError> {
    pipeline
        .seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::from_mseconds(start_offset_ms),
        )
        .map_err(|err| NativeVulkanError::Video(err.to_string()))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_seek_loop_segment(
    pipeline: &gst::Element,
    start_offset_ms: u64,
) -> Result<(), NativeVulkanError> {
    pipeline
        .seek(
            1.0,
            gst::SeekFlags::FLUSH | gst::SeekFlags::SEGMENT | gst::SeekFlags::KEY_UNIT,
            gst::SeekType::Set,
            gst::ClockTime::from_mseconds(start_offset_ms),
            gst::SeekType::None,
            gst::ClockTime::NONE,
        )
        .map_err(|err| NativeVulkanError::Video(err.to_string()))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_configure_queue(queue: &gst::Element) {
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", 8u32);
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 0u64);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_configure_appsink(sink: &gst::Element) {
    if sink.find_property("sync").is_some() {
        sink.set_property("sync", true);
    }
    if sink.find_property("async").is_some() {
        sink.set_property("async", false);
    }
    if sink.find_property("emit-signals").is_some() {
        sink.set_property("emit-signals", false);
    }
    if sink.find_property("enable-last-sample").is_some() {
        sink.set_property("enable-last-sample", false);
    }
    if sink.find_property("wait-on-eos").is_some() {
        sink.set_property("wait-on-eos", false);
    }
    if sink.find_property("max-buffers").is_some() {
        sink.set_property("max-buffers", 8u32);
    }
    if sink.find_property("drop").is_some() {
        sink.set_property("drop", false);
    }
    if sink.find_property("qos").is_some() {
        sink.set_property("qos", false);
    }
    if sink.find_property("max-lateness").is_some() {
        sink.set_property("max-lateness", -1i64);
    }
    if sink.find_property("processing-deadline").is_some() {
        sink.set_property("processing-deadline", 0u64);
    }
    if sink.find_property("render-delay").is_some() {
        sink.set_property("render-delay", 0u64);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_clock_time_ms(value: Option<gst::ClockTime>) -> Option<u64> {
    value.map(|value| value.mseconds())
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_elapsed_us(value: Duration) -> u64 {
    value.as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_memory_types(buffer: &gst::BufferRef) -> Vec<String> {
    (0..buffer.n_memory())
        .map(|index| native_vulkan_gst_memory_type(buffer.peek_memory(index)))
        .collect()
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_memory_type(memory: &gst::MemoryRef) -> String {
    for memory_type in ["CUDAMemory", "GLMemory", "DMABuf", "SystemMemory"] {
        if memory.is_type(memory_type) {
            return memory_type.to_owned();
        }
    }
    let Some(memory_type) = memory
        .allocator()
        .map(|allocator| allocator.memory_type().to_string())
    else {
        return "unknown".to_owned();
    };
    let lower = memory_type.to_ascii_lowercase();
    if lower.contains("cuda") {
        "CUDAMemory".to_owned()
    } else if lower.contains("gl") {
        "GLMemory".to_owned()
    } else if lower.contains("dmabuf") || lower.contains("dma-buf") {
        "DMABuf".to_owned()
    } else if lower.contains("system") {
        "SystemMemory".to_owned()
    } else {
        memory_type
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeVulkanVideoImportReport {
    width: u32,
    height: u32,
    memory_path: String,
    elapsed_us: u64,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NativeVulkanVideoImportStatus {
    frames_imported: u64,
    last_import_size: Option<(u32, u32)>,
    last_import_memory_path: Option<String>,
    last_import_error: Option<String>,
    last_import_elapsed_us: Option<u64>,
    max_import_elapsed_us: Option<u64>,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanVideoImportStatus {
    fn record_import(&mut self, report: NativeVulkanVideoImportReport) {
        self.frames_imported = self.frames_imported.saturating_add(1);
        self.last_import_size = Some((report.width, report.height));
        self.last_import_memory_path = Some(report.memory_path);
        self.last_import_error = None;
        self.last_import_elapsed_us = Some(report.elapsed_us);
        self.max_import_elapsed_us = Some(
            self.max_import_elapsed_us
                .map(|current| current.max(report.elapsed_us))
                .unwrap_or(report.elapsed_us),
        );
    }

    fn record_error(&mut self, error: String) {
        self.last_import_error = Some(error);
    }

    fn snapshot(&self) -> NativeVulkanVideoImportSnapshot {
        let texture_import_status = if self.frames_imported > 0 {
            "importing-cuda-vulkan-image-planes"
        } else if self.last_import_error.is_some() {
            "waiting-for-supported-importer"
        } else {
            "waiting-for-importable-sample"
        };
        NativeVulkanVideoImportSnapshot {
            texture_import_status,
            frames_imported: self.frames_imported,
            last_import_size: self.last_import_size,
            last_import_memory_path: self.last_import_memory_path.clone(),
            last_import_error: self.last_import_error.clone(),
            last_import_elapsed_us: self.last_import_elapsed_us,
            max_import_elapsed_us: self.max_import_elapsed_us,
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanVideoRenderer {
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    sampler: vk::Sampler,
    extent: vk::Extent2D,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanVideoRenderer {
    fn new(
        device: &ash::Device,
        swapchain_format: vk::Format,
        extent: vk::Extent2D,
        swapchain_image_views: &[vk::ImageView],
    ) -> Result<Self, NativeVulkanError> {
        let render_pass = native_vulkan_create_video_render_pass(device, swapchain_format)?;
        let framebuffers = native_vulkan_create_video_framebuffers(
            device,
            render_pass,
            extent,
            swapchain_image_views,
        )?;
        let bindings = [
            native_vulkan_video_sampler_binding(0),
            native_vulkan_video_sampler_binding(1),
        ];
        let descriptor_set_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        let descriptor_set_layout =
            unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_info, None) }
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkCreateDescriptorSetLayout(video)",
                    result,
                })?;
        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            descriptor_count: 2,
        }];
        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_sizes);
        let descriptor_pool =
            match unsafe { device.create_descriptor_pool(&descriptor_pool_info, None) } {
                Ok(pool) => pool,
                Err(result) => {
                    unsafe {
                        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                        for framebuffer in framebuffers {
                            device.destroy_framebuffer(framebuffer, None);
                        }
                        device.destroy_render_pass(render_pass, None);
                    }
                    return Err(NativeVulkanError::Vulkan {
                        operation: "vkCreateDescriptorPool(video)",
                        result,
                    });
                }
            };
        let set_layouts = [descriptor_set_layout];
        let descriptor_set_allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&set_layouts);
        let descriptor_set =
            match unsafe { device.allocate_descriptor_sets(&descriptor_set_allocate_info) } {
                Ok(sets) => sets[0],
                Err(result) => {
                    unsafe {
                        device.destroy_descriptor_pool(descriptor_pool, None);
                        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                        for framebuffer in framebuffers {
                            device.destroy_framebuffer(framebuffer, None);
                        }
                        device.destroy_render_pass(render_pass, None);
                    }
                    return Err(NativeVulkanError::Vulkan {
                        operation: "vkAllocateDescriptorSets(video)",
                        result,
                    });
                }
            };
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .max_lod(1.0);
        let sampler = match unsafe { device.create_sampler(&sampler_info, None) } {
            Ok(sampler) => sampler,
            Err(result) => {
                unsafe {
                    device.destroy_descriptor_pool(descriptor_pool, None);
                    device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                    for framebuffer in framebuffers {
                        device.destroy_framebuffer(framebuffer, None);
                    }
                    device.destroy_render_pass(render_pass, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateSampler(video)",
                    result,
                });
            }
        };
        let push_constant_ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(16)];
        let pipeline_set_layouts = [descriptor_set_layout];
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&pipeline_set_layouts)
            .push_constant_ranges(&push_constant_ranges);
        let pipeline_layout =
            match unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) } {
                Ok(layout) => layout,
                Err(result) => {
                    unsafe {
                        device.destroy_sampler(sampler, None);
                        device.destroy_descriptor_pool(descriptor_pool, None);
                        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                        for framebuffer in framebuffers {
                            device.destroy_framebuffer(framebuffer, None);
                        }
                        device.destroy_render_pass(render_pass, None);
                    }
                    return Err(NativeVulkanError::Vulkan {
                        operation: "vkCreatePipelineLayout(video)",
                        result,
                    });
                }
            };
        let pipeline =
            match native_vulkan_create_video_pipeline(device, render_pass, pipeline_layout, extent)
            {
                Ok(pipeline) => pipeline,
                Err(err) => {
                    unsafe {
                        device.destroy_pipeline_layout(pipeline_layout, None);
                        device.destroy_sampler(sampler, None);
                        device.destroy_descriptor_pool(descriptor_pool, None);
                        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                        for framebuffer in framebuffers {
                            device.destroy_framebuffer(framebuffer, None);
                        }
                        device.destroy_render_pass(render_pass, None);
                    }
                    return Err(err);
                }
            };

        Ok(Self {
            render_pass,
            framebuffers,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_set,
            pipeline_layout,
            pipeline,
            sampler,
            extent,
        })
    }

    fn update_descriptors(&mut self, device: &ash::Device, texture: &NativeVulkanCudaVideoTexture) {
        let image_infos = [
            vk::DescriptorImageInfo::default()
                .sampler(self.sampler)
                .image_view(texture.y.view)
                .image_layout(vk::ImageLayout::GENERAL),
            vk::DescriptorImageInfo::default()
                .sampler(self.sampler)
                .image_view(texture.uv.view)
                .image_layout(vk::ImageLayout::GENERAL),
        ];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_infos[0..1]),
            vk::WriteDescriptorSet::default()
                .dst_set(self.descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_infos[1..2]),
        ];
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    fn record_frame(
        &self,
        device: &ash::Device,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
        swapchain_image: vk::Image,
        swapchain_old_layout: vk::ImageLayout,
        texture: &NativeVulkanCudaVideoTexture,
        fit: FitMode,
    ) -> Result<(), NativeVulkanError> {
        unsafe {
            device
                .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkResetCommandBuffer(video)",
                    result,
                })?;
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device
                .begin_command_buffer(command_buffer, &begin_info)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkBeginCommandBuffer(video)",
                    result,
                })?;

            let swapchain_to_attachment = vk::ImageMemoryBarrier::default()
                .old_layout(swapchain_old_layout)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(swapchain_image)
                .subresource_range(native_vulkan_color_subresource_range())
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
            let texture_barriers = texture.shader_read_barriers();
            let barriers = [
                swapchain_to_attachment,
                texture_barriers[0],
                texture_barriers[1],
            ];
            device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::TOP_OF_PIPE | vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barriers,
            );

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            }];
            let render_area = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            };
            let render_pass_begin = vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffers[image_index])
                .render_area(render_area)
                .clear_values(&clear_values);
            device.cmd_begin_render_pass(
                command_buffer,
                &render_pass_begin,
                vk::SubpassContents::INLINE,
            );
            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );
            device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
            );
            let push = native_vulkan_video_fit_push_constants(
                fit,
                (texture.width, texture.height),
                (self.extent.width, self.extent.height),
            );
            let push_bytes = std::slice::from_raw_parts(
                push.as_ptr().cast::<u8>(),
                std::mem::size_of_val(&push),
            );
            device.cmd_push_constants(
                command_buffer,
                self.pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                push_bytes,
            );
            device.cmd_draw(command_buffer, 3, 1, 0, 0);
            device.cmd_end_render_pass(command_buffer);

            device
                .end_command_buffer(command_buffer)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkEndCommandBuffer(video)",
                    result,
                })?;
        }
        Ok(())
    }

    fn destroy(self, device: &ash::Device) {
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_sampler(self.sampler, None);
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            for framebuffer in self.framebuffers {
                device.destroy_framebuffer(framebuffer, None);
            }
            device.destroy_render_pass(self.render_pass, None);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_video_sampler_binding(binding: u32) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(binding)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_video_render_pass(
    device: &ash::Device,
    swapchain_format: vk::Format,
) -> Result<vk::RenderPass, NativeVulkanError> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(swapchain_format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
    let color_attachment_ref = vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    let color_attachment_refs = [color_attachment_ref];
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_attachment_refs);
    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
    let attachments = [color_attachment];
    let subpasses = [subpass];
    let dependencies = [dependency];
    let render_pass_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(&subpasses)
        .dependencies(&dependencies);
    unsafe { device.create_render_pass(&render_pass_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateRenderPass(video)",
            result,
        }
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_video_framebuffers(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    swapchain_image_views: &[vk::ImageView],
) -> Result<Vec<vk::Framebuffer>, NativeVulkanError> {
    let mut framebuffers = Vec::with_capacity(swapchain_image_views.len());
    for view in swapchain_image_views {
        let attachments = [*view];
        let info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(&attachments)
            .width(extent.width)
            .height(extent.height)
            .layers(1);
        let framebuffer = match unsafe { device.create_framebuffer(&info, None) } {
            Ok(framebuffer) => framebuffer,
            Err(result) => {
                for framebuffer in framebuffers {
                    unsafe {
                        device.destroy_framebuffer(framebuffer, None);
                    }
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateFramebuffer(video)",
                    result,
                });
            }
        };
        framebuffers.push(framebuffer);
    }
    Ok(framebuffers)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_video_pipeline(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
    extent: vk::Extent2D,
) -> Result<vk::Pipeline, NativeVulkanError> {
    let vertex_module = native_vulkan_create_shader_module(
        device,
        &NATIVE_VULKAN_VIDEO_VERTEX_SPIRV,
        "video vertex",
    )?;
    let fragment_module = match native_vulkan_create_shader_module(
        device,
        &NATIVE_VULKAN_VIDEO_FRAGMENT_SPIRV,
        "video fragment",
    ) {
        Ok(module) => module,
        Err(err) => {
            unsafe {
                device.destroy_shader_module(vertex_module, None);
            }
            return Err(err);
        }
    };
    let entry = c"main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_module)
            .name(entry),
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    let scissor = vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent,
    };
    let viewports = [viewport];
    let scissors = [scissor];
    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewports(&viewports)
        .scissors(&scissors);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let color_attachment = vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        )
        .blend_enable(false);
    let color_attachments = [color_attachment];
    let color_blend =
        vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_attachments);
    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0);
    let pipelines = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    };
    unsafe {
        device.destroy_shader_module(fragment_module, None);
        device.destroy_shader_module(vertex_module, None);
    }
    pipelines
        .map(|pipelines| pipelines[0])
        .map_err(|(_, result)| NativeVulkanError::Vulkan {
            operation: "vkCreateGraphicsPipelines(video)",
            result,
        })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_create_shader_module(
    device: &ash::Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, NativeVulkanError> {
    let info = vk::ShaderModuleCreateInfo::default().code(code);
    unsafe { device.create_shader_module(&info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: match label {
                "video vertex" => "vkCreateShaderModule(video vertex)",
                "video fragment" => "vkCreateShaderModule(video fragment)",
                _ => "vkCreateShaderModule(video)",
            },
            result,
        }
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_video_fit_push_constants(
    fit: FitMode,
    source_size: (u32, u32),
    surface_size: (u32, u32),
) -> [f32; 4] {
    let (offset, scale) = native_vulkan_video_uv_transform(fit, source_size, surface_size);
    [offset[0], offset[1], scale[0], scale[1]]
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_video_uv_transform(
    fit: FitMode,
    source_size: (u32, u32),
    surface_size: (u32, u32),
) -> ([f32; 2], [f32; 2]) {
    if matches!(fit, FitMode::Stretch | FitMode::Contain | FitMode::Center) {
        return ([0.0, 0.0], [1.0, 1.0]);
    }
    let source_aspect = source_size.0.max(1) as f32 / source_size.1.max(1) as f32;
    let surface_aspect = surface_size.0.max(1) as f32 / surface_size.1.max(1) as f32;
    if source_aspect > surface_aspect {
        let width = (surface_aspect / source_aspect).clamp(0.0, 1.0);
        ([(1.0 - width) * 0.5, 0.0], [width, 1.0])
    } else {
        let height = (source_aspect / surface_aspect).clamp(0.0, 1.0);
        ([0.0, (1.0 - height) * 0.5], [1.0, height])
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanCudaVideoTexture {
    cuda_context: *mut NativeVulkanGstCudaContext,
    width: u32,
    height: u32,
    cuda_stream: NativeVulkanCudaStream,
    y: NativeVulkanCudaVideoPlane,
    uv: NativeVulkanCudaVideoPlane,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanCudaVideoTexture {
    fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        device: &ash::Device,
        queue_family_index: u32,
        cuda_context: *mut NativeVulkanGstCudaContext,
        width: u32,
        height: u32,
    ) -> Result<Self, NativeVulkanError> {
        if width == 0 || height == 0 {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA video frame has zero dimension".to_owned(),
            ));
        }
        if width % 2 != 0 || height % 2 != 0 {
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan CUDA video dimensions must be even, got {width}x{height}"
            )));
        }
        if cuda_context.is_null() {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA video sample has null GstCudaContext".to_owned(),
            ));
        }
        let _guard = NativeVulkanGstCudaContextPushGuard::new(cuda_context)?;
        let cuda_stream = NativeVulkanCudaStream::new()?;
        let y = NativeVulkanCudaVideoPlane::new(
            instance,
            physical_device,
            queue,
            command_pool,
            device,
            queue_family_index,
            width,
            height,
            vk::Format::R8_UNORM,
            1,
            "y",
        )?;
        let uv = match NativeVulkanCudaVideoPlane::new(
            instance,
            physical_device,
            queue,
            command_pool,
            device,
            queue_family_index,
            width / 2,
            height / 2,
            vk::Format::R8G8_UNORM,
            2,
            "uv",
        ) {
            Ok(plane) => plane,
            Err(err) => {
                y.destroy(device);
                return Err(err);
            }
        };
        Ok(Self {
            cuda_context,
            width,
            height,
            cuda_stream,
            y,
            uv,
        })
    }

    fn matches(
        &self,
        cuda_context: *mut NativeVulkanGstCudaContext,
        width: u32,
        height: u32,
    ) -> bool {
        self.cuda_context == cuda_context && self.width == width && self.height == height
    }

    fn copy_sample(
        &mut self,
        buffer: &gst::BufferRef,
        meta: &NativeVulkanGstSystemNv12Meta,
    ) -> Result<(), NativeVulkanError> {
        let _guard = NativeVulkanGstCudaContextPushGuard::new(self.cuda_context)?;
        let y_map = native_vulkan_copy_gst_cuda_plane_to_vulkan_image(
            buffer,
            0,
            meta.y.offset,
            meta.y.stride,
            meta.y.row_bytes,
            meta.y.height,
            self.cuda_context,
            self.cuda_stream.handle,
            &self.y,
            "y",
        )?;
        let uv_map = match native_vulkan_copy_gst_cuda_plane_to_vulkan_image(
            buffer,
            1,
            meta.uv.offset,
            meta.uv.stride,
            meta.uv.row_bytes,
            meta.uv.height,
            self.cuda_context,
            self.cuda_stream.handle,
            &self.uv,
            "uv",
        ) {
            Ok(map) => map,
            Err(err) => {
                let sync_result = native_vulkan_cuda_result(
                    unsafe { CuStreamSynchronize(self.cuda_stream.handle) },
                    "native Vulkan CUDA synchronize after failed uv copy",
                );
                drop(y_map);
                sync_result?;
                return Err(err);
            }
        };
        let sync_result = native_vulkan_cuda_result(
            unsafe { CuStreamSynchronize(self.cuda_stream.handle) },
            "native Vulkan CUDA synchronize copy stream",
        );
        drop(uv_map);
        drop(y_map);
        sync_result?;
        Ok(())
    }

    fn shader_read_barriers(&self) -> [vk::ImageMemoryBarrier<'static>; 2] {
        [self.y.shader_read_barrier(), self.uv.shader_read_barrier()]
    }

    fn destroy(self, device: &ash::Device) {
        let _ = unsafe { CuStreamSynchronize(self.cuda_stream.handle) };
        self.uv.destroy(device);
        self.y.destroy(device);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanCudaVideoPlane {
    cuda_external_memory: NativeVulkanCudaExternalImageMemory,
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    width: u32,
    height: u32,
    channels: u32,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanCudaVideoPlane {
    #[allow(clippy::too_many_arguments)]
    fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        device: &ash::Device,
        queue_family_index: u32,
        width: u32,
        height: u32,
        format: vk::Format,
        channels: u32,
        label: &'static str,
    ) -> Result<Self, NativeVulkanError> {
        let handle_type = vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD;
        let mut external_image_info =
            vk::ExternalMemoryImageCreateInfo::default().handle_types(handle_type);
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(&mut external_image_info);
        let image = unsafe { device.create_image(&image_info, None) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkCreateImage(video CUDA plane)",
                result,
            }
        })?;
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let memory_type_index = native_vulkan_memory_type_index_prefer(
            &memory_properties,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            vk::MemoryPropertyFlags::empty(),
        )
        .ok_or_else(|| {
            unsafe {
                device.destroy_image(image, None);
            }
            NativeVulkanError::MissingMemoryType("video CUDA external image")
        })?;
        let mut export_info = vk::ExportMemoryAllocateInfo::default().handle_types(handle_type);
        let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut dedicated_info)
            .push_next(&mut export_info);
        let memory = match unsafe { device.allocate_memory(&allocate_info, None) } {
            Ok(memory) => memory,
            Err(result) => {
                unsafe {
                    device.destroy_image(image, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(video CUDA plane)",
                    result,
                });
            }
        };
        if let Err(result) = unsafe { device.bind_image_memory(image, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindImageMemory(video CUDA plane)",
                result,
            });
        }
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(native_vulkan_color_subresource_range());
        let view = match unsafe { device.create_image_view(&view_info, None) } {
            Ok(view) => view,
            Err(result) => {
                unsafe {
                    device.free_memory(memory, None);
                    device.destroy_image(image, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateImageView(video CUDA plane)",
                    result,
                });
            }
        };
        let external_memory_fd = ash::khr::external_memory_fd::Device::new(instance, device);
        let fd_info = vk::MemoryGetFdInfoKHR::default()
            .memory(memory)
            .handle_type(handle_type);
        let fd = match unsafe { external_memory_fd.get_memory_fd(&fd_info) } {
            Ok(fd) => fd,
            Err(result) => {
                unsafe {
                    device.destroy_image_view(view, None);
                    device.free_memory(memory, None);
                    device.destroy_image(image, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkGetMemoryFdKHR(video CUDA plane)",
                    result,
                });
            }
        };
        let cuda_external_memory = match NativeVulkanCudaExternalImageMemory::import_opaque_fd(
            fd,
            requirements.size,
            width,
            height,
            channels,
            label,
        ) {
            Ok(memory) => memory,
            Err(err) => {
                unsafe {
                    device.destroy_image_view(view, None);
                    device.free_memory(memory, None);
                    device.destroy_image(image, None);
                }
                return Err(err);
            }
        };
        if let Err(err) = native_vulkan_transition_image_to_general(
            device,
            queue,
            command_pool,
            image,
            queue_family_index,
        ) {
            unsafe {
                device.destroy_image_view(view, None);
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            drop(cuda_external_memory);
            return Err(err);
        }

        Ok(Self {
            cuda_external_memory,
            image,
            memory,
            view,
            width,
            height,
            channels,
        })
    }

    fn shader_read_barrier(&self) -> vk::ImageMemoryBarrier<'static> {
        vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.image)
            .subresource_range(native_vulkan_color_subresource_range())
            .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
    }

    fn destroy(self, device: &ash::Device) {
        drop(self.cuda_external_memory);
        unsafe {
            device.destroy_image_view(self.view, None);
            device.free_memory(self.memory, None);
            device.destroy_image(self.image, None);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_transition_image_to_general(
    device: &ash::Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    image: vk::Image,
    queue_family_index: u32,
) -> Result<(), NativeVulkanError> {
    let allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffer =
        unsafe { device.allocate_command_buffers(&allocate_info) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkAllocateCommandBuffers(video image transition)",
                result,
            }
        })?[0];
    let result = unsafe {
        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkBeginCommandBuffer(video image transition)",
                result,
            })?;
        let barrier = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(native_vulkan_color_subresource_range())
            .src_access_mask(vk::AccessFlags::empty())
            .dst_access_mask(vk::AccessFlags::SHADER_READ);
        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
        device
            .end_command_buffer(command_buffer)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkEndCommandBuffer(video image transition)",
                result,
            })?;
        let command_buffers = [command_buffer];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        device
            .queue_submit(queue, &[submit_info], vk::Fence::null())
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkQueueSubmit(video image transition)",
                result,
            })?;
        device
            .queue_wait_idle(queue)
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkQueueWaitIdle(video image transition)",
                result,
            })
    };
    unsafe {
        device.free_command_buffers(command_pool, &[command_buffer]);
    }
    let _ = queue_family_index;
    result
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_memory_type_index_prefer(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
    preferred: vk::MemoryPropertyFlags,
    required: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let mut fallback = None;
    for (index, memory_type) in memory_properties.memory_types
        [..memory_properties.memory_type_count as usize]
        .iter()
        .enumerate()
    {
        let supported = (memory_type_bits & (1 << index)) != 0;
        if !supported || !memory_type.property_flags.contains(required) {
            continue;
        }
        let index = index as u32;
        if memory_type.property_flags.contains(preferred) {
            return Some(index);
        }
        fallback.get_or_insert(index);
    }
    fallback
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanGstSystemNv12Plane {
    offset: usize,
    stride: u32,
    height: u32,
    row_bytes: u32,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanGstSystemNv12Meta {
    width: u32,
    height: u32,
    y: NativeVulkanGstSystemNv12Plane,
    uv: NativeVulkanGstSystemNv12Plane,
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_system_nv12_meta(
    sample: &gst::Sample,
    buffer: &gst::BufferRef,
) -> Result<NativeVulkanGstSystemNv12Meta, NativeVulkanError> {
    let meta = match native_vulkan_gst_nv12_meta_from_video_meta(sample.caps(), buffer) {
        Ok(meta) => meta,
        Err(meta_err) => native_vulkan_gst_nv12_meta_from_caps(sample)
            .map_err(|caps_err| NativeVulkanError::Video(format!("{meta_err};{caps_err}")))?,
    };
    Ok(meta)
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_nv12_meta_from_video_meta(
    caps: Option<&gst::CapsRef>,
    buffer: &gst::BufferRef,
) -> Result<NativeVulkanGstSystemNv12Meta, String> {
    let meta = buffer
        .meta::<gst_video::VideoMeta>()
        .ok_or_else(|| "appsink buffer has no GstVideoMeta".to_owned())?;
    let caps_format = caps
        .and_then(|caps| caps.structure(0))
        .and_then(|structure| structure.get::<String>("format").ok())
        .unwrap_or_else(|| meta.format().to_str().to_string());
    if meta.format() != gst_video::VideoFormat::Nv12 && caps_format != "NV12" {
        return Err(format!("expected NV12 appsink frame, got {caps_format}"));
    }
    let width = meta.width();
    let height = meta.height();
    if width == 0 || height == 0 {
        return Err("NV12 frame has zero dimension".to_owned());
    }
    if width % 2 != 0 || height % 2 != 0 {
        return Err(format!(
            "NV12 frame dimensions must be even, got {width}x{height}"
        ));
    }
    if meta.offset().len() < 2 || meta.stride().len() < 2 {
        return Err(format!(
            "NV12 frame needs 2 planes, got offsets={} strides={}",
            meta.offset().len(),
            meta.stride().len()
        ));
    }
    let y_stride = native_vulkan_positive_stride("NV12 y", meta.stride()[0])?;
    let uv_stride = native_vulkan_positive_stride("NV12 uv", meta.stride()[1])?;
    if y_stride < width || uv_stride < width {
        return Err(format!(
            "NV12 stride too small: y={y_stride} uv={uv_stride} width={width}"
        ));
    }
    Ok(NativeVulkanGstSystemNv12Meta {
        width,
        height,
        y: NativeVulkanGstSystemNv12Plane {
            offset: meta.offset()[0],
            stride: y_stride,
            height,
            row_bytes: width,
        },
        uv: NativeVulkanGstSystemNv12Plane {
            offset: meta.offset()[1],
            stride: uv_stride,
            height: height / 2,
            row_bytes: width,
        },
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_nv12_meta_from_caps(
    sample: &gst::Sample,
) -> Result<NativeVulkanGstSystemNv12Meta, String> {
    let caps = sample
        .caps()
        .ok_or_else(|| "appsink sample has no caps".to_owned())?;
    let structure = caps
        .structure(0)
        .ok_or_else(|| "appsink caps has no structure".to_owned())?;
    let format = structure
        .get::<String>("format")
        .unwrap_or_else(|_| "unknown".to_owned());
    if format != "NV12" {
        return Err(format!("caps fallback expected NV12, got {format}"));
    }
    let width = structure
        .get::<i32>("width")
        .map_err(|_| "appsink caps missing width".to_owned())
        .and_then(|width| {
            u32::try_from(width)
                .ok()
                .filter(|width| *width > 0)
                .ok_or_else(|| "invalid appsink frame width".to_owned())
        })?;
    let height = structure
        .get::<i32>("height")
        .map_err(|_| "appsink caps missing height".to_owned())
        .and_then(|height| {
            u32::try_from(height)
                .ok()
                .filter(|height| *height > 0)
                .ok_or_else(|| "invalid appsink frame height".to_owned())
        })?;
    if width % 2 != 0 || height % 2 != 0 {
        return Err(format!(
            "NV12 frame dimensions must be even, got {width}x{height}"
        ));
    }
    let y_size = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| "NV12 plane offset overflow".to_owned())?;
    Ok(NativeVulkanGstSystemNv12Meta {
        width,
        height,
        y: NativeVulkanGstSystemNv12Plane {
            offset: 0,
            stride: width,
            height,
            row_bytes: width,
        },
        uv: NativeVulkanGstSystemNv12Plane {
            offset: y_size,
            stride: width,
            height: height / 2,
            row_bytes: width,
        },
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_positive_stride(label: &str, stride: i32) -> Result<u32, String> {
    u32::try_from(stride)
        .ok()
        .filter(|stride| *stride > 0)
        .ok_or_else(|| format!("{label} stride must be positive, got {stride}"))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_buffer_has_cuda_memory(buffer: &gst::BufferRef) -> bool {
    (0..buffer.n_memory()).any(|index| native_vulkan_is_cuda_memory(buffer.peek_memory(index)))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_gst_cuda_context_from_buffer(
    buffer: &gst::BufferRef,
) -> Result<*mut NativeVulkanGstCudaContext, NativeVulkanError> {
    for memory_index in 0..buffer.n_memory() {
        let memory = buffer.peek_memory(memory_index);
        if !native_vulkan_is_cuda_memory(memory) {
            continue;
        }
        let cuda_memory = memory
            .as_ptr()
            .cast_mut()
            .cast::<NativeVulkanGstCudaMemory>();
        let context = unsafe { (*cuda_memory).context };
        if !context.is_null() {
            return Ok(context);
        }
    }
    Err(NativeVulkanError::Video(
        "native Vulkan CUDA buffer has no GstCudaContext".to_owned(),
    ))
}

#[cfg(feature = "native-vulkan-gst-video")]
#[allow(clippy::too_many_arguments)]
fn native_vulkan_copy_gst_cuda_plane_to_vulkan_image(
    buffer: &gst::BufferRef,
    plane_index: usize,
    plane_offset: usize,
    source_stride: u32,
    row_bytes: u32,
    height: u32,
    expected_context: *mut NativeVulkanGstCudaContext,
    stream: NativeVulkanCudaStreamHandle,
    image: &NativeVulkanCudaVideoPlane,
    label: &str,
) -> Result<NativeVulkanCudaMemoryMap, NativeVulkanError> {
    let expected_row_bytes = image.width.checked_mul(image.channels).ok_or_else(|| {
        NativeVulkanError::Video(format!("native Vulkan CUDA {label} row byte overflow"))
    })?;
    if row_bytes != expected_row_bytes || height != image.height {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan CUDA {label} plane shape mismatch: row_bytes={row_bytes} height={height} image={}x{} channels={}",
            image.width, image.height, image.channels
        )));
    }
    let plane_end = plane_offset.checked_add(1).ok_or_else(|| {
        NativeVulkanError::Video(format!("native Vulkan CUDA {label} offset overflow"))
    })?;
    let (memory_range, memory_skip) =
        buffer.find_memory(plane_offset..plane_end).ok_or_else(|| {
            NativeVulkanError::Video(format!("native Vulkan CUDA {label} plane has no memory"))
        })?;
    let memory_index = memory_range.start;
    if memory_index >= buffer.n_memory() {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan CUDA {label} memory index out of range"
        )));
    }
    let memory = buffer.peek_memory(memory_index);
    if !native_vulkan_is_cuda_memory(memory) {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan CUDA {label} plane memory is not CUDAMemory: {}",
            native_vulkan_gst_memory_type(memory)
        )));
    }
    let cuda_memory = memory
        .as_ptr()
        .cast_mut()
        .cast::<NativeVulkanGstCudaMemory>();
    let context = unsafe { (*cuda_memory).context };
    if context != expected_context {
        return Err(NativeVulkanError::Video(format!(
            "native Vulkan CUDA {label} plane context changed"
        )));
    }
    unsafe {
        gst_cuda_memory_sync(cuda_memory);
    }
    let map = NativeVulkanCudaMemoryMap::new(memory).map_err(|err| {
        NativeVulkanError::Video(format!("native Vulkan CUDA {label} map failed: {err}"))
    })?;
    let source = map
        .device_ptr()
        .checked_add(u64::try_from(memory_skip).map_err(|_| {
            NativeVulkanError::Video(format!("native Vulkan CUDA {label} memory skip too large"))
        })?)
        .ok_or_else(|| {
            NativeVulkanError::Video(format!("native Vulkan CUDA {label} source overflow"))
        })?;
    let copy = NativeVulkanCudaMemcpy2D {
        src_x_in_bytes: 0,
        src_y: 0,
        src_memory_type: CUDA_MEMORYTYPE_DEVICE,
        src_host: ptr::null(),
        src_device: source,
        src_array: ptr::null_mut(),
        src_pitch: usize::try_from(source_stride).map_err(|_| {
            NativeVulkanError::Video(format!(
                "native Vulkan CUDA {label} source stride too large"
            ))
        })?,
        dst_x_in_bytes: 0,
        dst_y: 0,
        dst_memory_type: CUDA_MEMORYTYPE_ARRAY,
        dst_host: ptr::null_mut(),
        dst_device: 0,
        dst_array: image.cuda_external_memory.array,
        dst_pitch: 0,
        width_in_bytes: usize::try_from(row_bytes).map_err(|_| {
            NativeVulkanError::Video(format!("native Vulkan CUDA {label} row bytes too large"))
        })?,
        height: usize::try_from(height).map_err(|_| {
            NativeVulkanError::Video(format!("native Vulkan CUDA {label} height too large"))
        })?,
    };
    native_vulkan_cuda_result(
        unsafe { CuMemcpy2DAsync(&copy, stream) },
        &format!("native Vulkan CUDA copy {label} plane {plane_index}"),
    )?;
    Ok(map)
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanCudaMemoryMap {
    memory: *mut gst::ffi::GstMemory,
    info: gst::ffi::GstMapInfo,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanCudaMemoryMap {
    fn new(memory: &gst::MemoryRef) -> Result<Self, String> {
        let memory_ptr = memory.as_ptr().cast_mut();
        let mut info = std::mem::MaybeUninit::<gst::ffi::GstMapInfo>::zeroed();
        let mapped =
            unsafe { gst::ffi::gst_memory_map(memory_ptr, info.as_mut_ptr(), GST_MAP_READ_CUDA) }
                != gst::glib::ffi::GFALSE;
        if !mapped {
            return Err(native_vulkan_gst_memory_type(memory));
        }
        let info = unsafe { info.assume_init() };
        if info.data.is_null() {
            unsafe {
                let mut info = info;
                gst::ffi::gst_memory_unmap(memory_ptr, &mut info);
            }
            return Err("null CUDA map pointer".to_owned());
        }
        Ok(Self {
            memory: memory_ptr,
            info,
        })
    }

    fn device_ptr(&self) -> NativeVulkanCudaDevicePtr {
        self.info.data as usize as NativeVulkanCudaDevicePtr
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanCudaMemoryMap {
    fn drop(&mut self) {
        unsafe {
            gst::ffi::gst_memory_unmap(self.memory, &mut self.info);
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanCudaExternalImageMemory {
    handle: NativeVulkanCudaExternalMemoryHandle,
    _mipmapped_array: NativeVulkanCudaMipmappedArrayHandle,
    array: NativeVulkanCudaArrayHandle,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanCudaExternalImageMemory {
    fn import_opaque_fd(
        fd: i32,
        allocation_size: u64,
        width: u32,
        height: u32,
        channels: u32,
        label: &str,
    ) -> Result<Self, NativeVulkanError> {
        let mut external_memory = ptr::null_mut();
        let desc = NativeVulkanCudaExternalMemoryHandleDesc {
            type_: CUDA_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD,
            handle: NativeVulkanCudaExternalMemoryHandleUnion { fd },
            size: allocation_size,
            flags: 0,
            reserved: [0; 16],
        };
        native_vulkan_cuda_result(
            unsafe { CuImportExternalMemory(&mut external_memory, &desc) },
            &format!("native Vulkan CUDA import {label} Vulkan image external memory"),
        )?;
        if external_memory.is_null() {
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan CUDA imported {label} external memory is null"
            )));
        }
        let mut mipmapped_array = ptr::null_mut();
        let mipmapped_desc = NativeVulkanCudaExternalMemoryMipmappedArrayDesc {
            offset: 0,
            array_desc: NativeVulkanCudaArray3dDesc {
                width: usize::try_from(width).map_err(|_| {
                    NativeVulkanError::Video(format!("native Vulkan CUDA {label} width too large"))
                })?,
                height: usize::try_from(height).map_err(|_| {
                    NativeVulkanError::Video(format!("native Vulkan CUDA {label} height too large"))
                })?,
                depth: 0,
                format: CUDA_ARRAY_FORMAT_UNSIGNED_INT8,
                num_channels: channels,
                flags: 0,
            },
            num_levels: 1,
            reserved: [0; 16],
        };
        if let Err(err) = native_vulkan_cuda_result(
            unsafe {
                CuExternalMemoryGetMappedMipmappedArray(
                    &mut mipmapped_array,
                    external_memory,
                    &mipmapped_desc,
                )
            },
            &format!("native Vulkan CUDA map {label} Vulkan image mipmapped array"),
        ) {
            let _ = unsafe { CuDestroyExternalMemory(external_memory) };
            return Err(err);
        }
        if mipmapped_array.is_null() {
            let _ = unsafe { CuDestroyExternalMemory(external_memory) };
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan CUDA mapped {label} mipmapped array is null"
            )));
        }
        let mut array = ptr::null_mut();
        if let Err(err) = native_vulkan_cuda_result(
            unsafe { cuMipmappedArrayGetLevel(&mut array, mipmapped_array, 0) },
            &format!("native Vulkan CUDA get {label} mipmapped array level 0"),
        ) {
            let _ = unsafe { CuDestroyExternalMemory(external_memory) };
            return Err(err);
        }
        if array.is_null() {
            let _ = unsafe { CuDestroyExternalMemory(external_memory) };
            return Err(NativeVulkanError::Video(format!(
                "native Vulkan CUDA {label} CUDA array level is null"
            )));
        }
        Ok(Self {
            handle: external_memory,
            _mipmapped_array: mipmapped_array,
            array,
        })
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanCudaExternalImageMemory {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            let _ = unsafe { CuDestroyExternalMemory(self.handle) };
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanCudaStream {
    handle: NativeVulkanCudaStreamHandle,
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanCudaStream {
    fn new() -> Result<Self, NativeVulkanError> {
        let mut handle = ptr::null_mut();
        native_vulkan_cuda_result(
            unsafe { CuStreamCreate(&mut handle, CUDA_STREAM_NON_BLOCKING) },
            "native Vulkan CUDA create copy stream",
        )?;
        if handle.is_null() {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA copy stream is null".to_owned(),
            ));
        }
        Ok(Self { handle })
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanCudaStream {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            let _ = unsafe { CuStreamDestroy(self.handle) };
        }
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
struct NativeVulkanGstCudaContextPushGuard;

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanGstCudaContextPushGuard {
    fn new(context: *mut NativeVulkanGstCudaContext) -> Result<Self, NativeVulkanError> {
        if context.is_null() {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA cannot push null GstCudaContext".to_owned(),
            ));
        }
        let pushed = unsafe { gst_cuda_context_push(context) } != gst::glib::ffi::GFALSE;
        if !pushed {
            return Err(NativeVulkanError::Video(
                "native Vulkan CUDA failed to push GstCudaContext".to_owned(),
            ));
        }
        Ok(Self)
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
impl Drop for NativeVulkanGstCudaContextPushGuard {
    fn drop(&mut self) {
        let mut context = ptr::null_mut();
        let _ = unsafe { gst_cuda_context_pop(&mut context) };
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_is_cuda_memory(memory: &gst::MemoryRef) -> bool {
    if memory.is_type("CUDAMemory") || memory.is_type("gst.cuda.memory") {
        return true;
    }
    let is_cuda = unsafe { gst_is_cuda_memory(memory.as_ptr().cast_mut()) };
    is_cuda != gst::glib::ffi::GFALSE
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_cuda_result(result: i32, label: &str) -> Result<(), NativeVulkanError> {
    if result == CUDA_SUCCESS {
        return Ok(());
    }
    Err(NativeVulkanError::Video(format!(
        "{label} failed: {}",
        native_vulkan_cuda_error_label(result)
    )))
}

#[cfg(feature = "native-vulkan-gst-video")]
fn native_vulkan_cuda_error_label(result: i32) -> String {
    let mut name = ptr::null();
    let mut description = ptr::null();
    let name_result = unsafe { CuGetErrorName(result, &mut name) };
    let description_result = unsafe { CuGetErrorString(result, &mut description) };
    let name = if name_result == CUDA_SUCCESS && !name.is_null() {
        unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    } else {
        "unknown".to_owned()
    };
    let description = if description_result == CUDA_SUCCESS && !description.is_null() {
        unsafe { CStr::from_ptr(description) }
            .to_string_lossy()
            .into_owned()
    } else {
        "no description".to_owned()
    };
    format!("{result}:{name}:{description}")
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanGstCudaMemory {
    mem: gst::ffi::GstMemory,
    context: *mut NativeVulkanGstCudaContext,
    info: gst_video::ffi::GstVideoInfo,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanGstCudaContext {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-gst-video")]
const GST_MAP_READ_CUDA: gst::ffi::GstMapFlags =
    gst::ffi::GST_MAP_READ | (gst::ffi::GST_MAP_FLAG_LAST << 1);
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_SUCCESS: i32 = 0;
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_MEMORYTYPE_DEVICE: u32 = 2;
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_MEMORYTYPE_ARRAY: u32 = 3;
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD: u32 = 1;
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_STREAM_NON_BLOCKING: u32 = 1;
#[cfg(feature = "native-vulkan-gst-video")]
const CUDA_ARRAY_FORMAT_UNSIGNED_INT8: u32 = 1;

#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanCudaDevicePtr = u64;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanCudaExternalMemoryHandle = *mut c_void;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanCudaArrayHandle = *mut c_void;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanCudaMipmappedArrayHandle = *mut c_void;
#[cfg(feature = "native-vulkan-gst-video")]
type NativeVulkanCudaStreamHandle = *mut c_void;

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeVulkanCudaExternalMemoryWin32Handle {
    handle: *mut c_void,
    name: *const c_void,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
union NativeVulkanCudaExternalMemoryHandleUnion {
    fd: i32,
    win32: NativeVulkanCudaExternalMemoryWin32Handle,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanCudaExternalMemoryHandleDesc {
    type_: u32,
    handle: NativeVulkanCudaExternalMemoryHandleUnion,
    size: u64,
    flags: u32,
    reserved: [u32; 16],
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanCudaArray3dDesc {
    width: usize,
    height: usize,
    depth: usize,
    format: u32,
    num_channels: u32,
    flags: u32,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanCudaExternalMemoryMipmappedArrayDesc {
    offset: u64,
    array_desc: NativeVulkanCudaArray3dDesc,
    num_levels: u32,
    reserved: [u32; 16],
}

#[cfg(feature = "native-vulkan-gst-video")]
#[repr(C)]
struct NativeVulkanCudaMemcpy2D {
    src_x_in_bytes: usize,
    src_y: usize,
    src_memory_type: u32,
    src_host: *const c_void,
    src_device: NativeVulkanCudaDevicePtr,
    src_array: NativeVulkanCudaArrayHandle,
    src_pitch: usize,
    dst_x_in_bytes: usize,
    dst_y: usize,
    dst_memory_type: u32,
    dst_host: *mut c_void,
    dst_device: NativeVulkanCudaDevicePtr,
    dst_array: NativeVulkanCudaArrayHandle,
    dst_pitch: usize,
    width_in_bytes: usize,
    height: usize,
}

#[cfg(feature = "native-vulkan-gst-video")]
#[link(name = "gstcuda-1.0")]
#[allow(clashing_extern_declarations)]
unsafe extern "C" {
    fn CuGetErrorName(error: i32, p_str: *mut *const c_char) -> i32;
    fn CuGetErrorString(error: i32, p_str: *mut *const c_char) -> i32;
    fn CuMemcpy2DAsync(
        copy: *const NativeVulkanCudaMemcpy2D,
        stream: NativeVulkanCudaStreamHandle,
    ) -> i32;
    fn CuStreamCreate(stream_out: *mut NativeVulkanCudaStreamHandle, flags: u32) -> i32;
    fn CuStreamDestroy(stream: NativeVulkanCudaStreamHandle) -> i32;
    fn CuStreamSynchronize(stream: NativeVulkanCudaStreamHandle) -> i32;
    fn CuImportExternalMemory(
        ext_mem_out: *mut NativeVulkanCudaExternalMemoryHandle,
        mem_handle_desc: *const NativeVulkanCudaExternalMemoryHandleDesc,
    ) -> i32;
    fn CuExternalMemoryGetMappedMipmappedArray(
        mipmap: *mut NativeVulkanCudaMipmappedArrayHandle,
        ext_mem: NativeVulkanCudaExternalMemoryHandle,
        mipmap_desc: *const NativeVulkanCudaExternalMemoryMipmappedArrayDesc,
    ) -> i32;
    fn CuDestroyExternalMemory(ext_mem: NativeVulkanCudaExternalMemoryHandle) -> i32;
    fn gst_cuda_context_push(ctx: *mut NativeVulkanGstCudaContext) -> gst::glib::ffi::gboolean;
    fn gst_cuda_context_pop(cuda_ctx: *mut *mut c_void) -> gst::glib::ffi::gboolean;
    fn gst_is_cuda_memory(mem: *mut gst::ffi::GstMemory) -> gst::glib::ffi::gboolean;
    fn gst_cuda_memory_sync(mem: *mut NativeVulkanGstCudaMemory);
}

#[cfg(feature = "native-vulkan-gst-video")]
#[link(name = "cuda")]
unsafe extern "C" {
    fn cuMipmappedArrayGetLevel(
        level_array: *mut NativeVulkanCudaArrayHandle,
        mipmapped_array: NativeVulkanCudaMipmappedArrayHandle,
        level: u32,
    ) -> i32;
}

#[cfg(feature = "native-vulkan-gst-video")]
const NATIVE_VULKAN_VIDEO_VERTEX_SPIRV: [u32; 440] = [
    119734787, 65536, 851979, 63, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 524303, 0, 4, 1852399981, 0, 33, 37, 48, 196611, 2, 450, 655364, 1197427783,
    1279741775, 1885560645, 1953718128, 1600482425, 1701734764, 1919509599, 1769235301, 25974,
    524292, 1197427783, 1279741775, 1852399429, 1685417059, 1768185701, 1952671090, 6649449,
    262149, 4, 1852399981, 0, 327685, 12, 1769172848, 1852795252, 115, 196613, 21, 7566965, 393221,
    31, 1348430951, 1700164197, 2019914866, 0, 393222, 31, 0, 1348430951, 1953067887, 7237481,
    458758, 31, 1, 1348430951, 1953393007, 1702521171, 0, 458758, 31, 2, 1130327143, 1148217708,
    1635021673, 6644590, 458758, 31, 3, 1130327143, 1147956341, 1635021673, 6644590, 196613, 33, 0,
    393221, 37, 1449094247, 1702130277, 1684949368, 30821, 262149, 48, 1987403638, 0, 196613, 49,
    7629126, 327686, 49, 0, 1936090735, 29797, 327686, 49, 1, 1818321779, 101, 196613, 51, 7629158,
    196679, 31, 2, 327752, 31, 0, 11, 0, 327752, 31, 1, 11, 1, 327752, 31, 2, 11, 3, 327752, 31, 3,
    11, 4, 262215, 37, 11, 42, 262215, 48, 30, 0, 196679, 49, 2, 327752, 49, 0, 35, 0, 327752, 49,
    1, 35, 8, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 2, 262165, 8, 32, 0, 262187, 8,
    9, 3, 262172, 10, 7, 9, 262176, 11, 7, 10, 262187, 6, 13, 3212836864, 262187, 6, 14,
    3225419776, 327724, 7, 15, 13, 14, 262187, 6, 16, 1077936128, 262187, 6, 17, 1065353216,
    327724, 7, 18, 16, 17, 327724, 7, 19, 13, 17, 393260, 10, 20, 15, 18, 19, 262187, 6, 22, 0,
    262187, 6, 23, 1073741824, 327724, 7, 24, 22, 23, 327724, 7, 25, 23, 22, 327724, 7, 26, 22, 22,
    393260, 10, 27, 24, 25, 26, 262167, 28, 6, 4, 262187, 8, 29, 1, 262172, 30, 6, 29, 393246, 31,
    28, 6, 30, 30, 262176, 32, 3, 31, 262203, 32, 33, 3, 262165, 34, 32, 1, 262187, 34, 35, 0,
    262176, 36, 1, 34, 262203, 36, 37, 1, 262176, 39, 7, 7, 262176, 45, 3, 28, 262176, 47, 3, 7,
    262203, 47, 48, 3, 262174, 49, 7, 7, 262176, 50, 9, 49, 262203, 50, 51, 9, 262176, 52, 9, 7,
    262187, 34, 58, 1, 327734, 2, 4, 0, 3, 131320, 5, 262203, 11, 12, 7, 262203, 11, 21, 7, 196670,
    12, 20, 196670, 21, 27, 262205, 34, 38, 37, 327745, 39, 40, 12, 38, 262205, 7, 41, 40, 327761,
    6, 42, 41, 0, 327761, 6, 43, 41, 1, 458832, 28, 44, 42, 43, 22, 17, 327745, 45, 46, 33, 35,
    196670, 46, 44, 327745, 52, 53, 51, 35, 262205, 7, 54, 53, 262205, 34, 55, 37, 327745, 39, 56,
    21, 55, 262205, 7, 57, 56, 327745, 52, 59, 51, 58, 262205, 7, 60, 59, 327813, 7, 61, 57, 60,
    327809, 7, 62, 54, 61, 196670, 48, 62, 65789, 65592,
];

#[cfg(feature = "native-vulkan-gst-video")]
const NATIVE_VULKAN_VIDEO_FRAGMENT_SPIRV: [u32; 554] = [
    119734787, 65536, 851979, 90, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 458767, 4, 4, 1852399981, 0, 16, 82, 196624, 4, 7, 196611, 2, 450, 655364,
    1197427783, 1279741775, 1885560645, 1953718128, 1600482425, 1701734764, 1919509599, 1769235301,
    25974, 524292, 1197427783, 1279741775, 1852399429, 1685417059, 1768185701, 1952671090, 6649449,
    262149, 4, 1852399981, 0, 196613, 8, 121, 327685, 12, 1702125433, 1920300152, 101, 262149, 16,
    1987403638, 0, 196613, 24, 30325, 327685, 25, 1952413301, 1970567269, 25970, 196613, 30, 117,
    196613, 33, 118, 196613, 54, 114, 196613, 62, 103, 196613, 74, 98, 327685, 82, 1601467759,
    1869377379, 114, 262215, 12, 33, 0, 262215, 12, 34, 0, 262215, 16, 30, 0, 262215, 25, 33, 1,
    262215, 25, 34, 0, 262215, 82, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262176, 7, 7, 6,
    589849, 9, 6, 1, 0, 0, 0, 1, 0, 196635, 10, 9, 262176, 11, 0, 10, 262203, 11, 12, 0, 262167,
    14, 6, 2, 262176, 15, 1, 14, 262203, 15, 16, 1, 262167, 18, 6, 4, 262165, 20, 32, 0, 262187,
    20, 21, 0, 262176, 23, 7, 14, 262203, 11, 25, 0, 262187, 20, 34, 1, 262187, 6, 38, 1031831681,
    262187, 6, 40, 1062984668, 262187, 6, 42, 0, 262187, 6, 43, 1065353216, 262187, 6, 47,
    1063313633, 262187, 6, 56, 1070174988, 262187, 6, 58, 1056964608, 262187, 6, 64, 1044368274,
    262187, 6, 69, 1055894222, 262187, 6, 76, 1072530509, 262176, 81, 3, 18, 262203, 81, 82, 3,
    327734, 2, 4, 0, 3, 131320, 5, 262203, 7, 8, 7, 262203, 23, 24, 7, 262203, 7, 30, 7, 262203, 7,
    33, 7, 262203, 7, 54, 7, 262203, 7, 62, 7, 262203, 7, 74, 7, 262205, 10, 13, 12, 262205, 14,
    17, 16, 327767, 18, 19, 13, 17, 327761, 6, 22, 19, 0, 196670, 8, 22, 262205, 10, 26, 25,
    262205, 14, 27, 16, 327767, 18, 28, 26, 27, 458831, 14, 29, 28, 28, 0, 1, 196670, 24, 29,
    327745, 7, 31, 24, 21, 262205, 6, 32, 31, 196670, 30, 32, 327745, 7, 35, 24, 34, 262205, 6, 36,
    35, 196670, 33, 36, 262205, 6, 37, 8, 327811, 6, 39, 37, 38, 327816, 6, 41, 39, 40, 524300, 6,
    44, 1, 43, 41, 42, 43, 196670, 8, 44, 262205, 6, 45, 30, 327811, 6, 46, 45, 38, 327816, 6, 48,
    46, 47, 524300, 6, 49, 1, 43, 48, 42, 43, 196670, 30, 49, 262205, 6, 50, 33, 327811, 6, 51, 50,
    38, 327816, 6, 52, 51, 47, 524300, 6, 53, 1, 43, 52, 42, 43, 196670, 33, 53, 262205, 6, 55, 8,
    262205, 6, 57, 33, 327811, 6, 59, 57, 58, 327813, 6, 60, 56, 59, 327809, 6, 61, 55, 60, 196670,
    54, 61, 262205, 6, 63, 8, 262205, 6, 65, 30, 327811, 6, 66, 65, 58, 327813, 6, 67, 64, 66,
    327811, 6, 68, 63, 67, 262205, 6, 70, 33, 327811, 6, 71, 70, 58, 327813, 6, 72, 69, 71, 327811,
    6, 73, 68, 72, 196670, 62, 73, 262205, 6, 75, 8, 262205, 6, 77, 30, 327811, 6, 78, 77, 58,
    327813, 6, 79, 76, 78, 327809, 6, 80, 75, 79, 196670, 74, 80, 262205, 6, 83, 54, 524300, 6, 84,
    1, 43, 83, 42, 43, 262205, 6, 85, 62, 524300, 6, 86, 1, 43, 85, 42, 43, 262205, 6, 87, 74,
    524300, 6, 88, 1, 43, 87, 42, 43, 458832, 18, 89, 84, 86, 88, 43, 196670, 82, 89, 65789, 65592,
];

struct NativeVulkanStaticImageUpload {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    buffer_image_copy: vk::BufferImageCopy,
    size_bytes: vk::DeviceSize,
}

impl NativeVulkanStaticImageUpload {
    fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        device: &ash::Device,
        source: &PathBuf,
        fit: FitMode,
        background: Option<&str>,
        swapchain_format: vk::Format,
        extent: vk::Extent2D,
    ) -> Result<Self, NativeVulkanError> {
        let pixels = native_vulkan_static_image_pixels(
            source,
            fit,
            background,
            swapchain_format,
            (extent.width, extent.height),
        )?;
        let size_bytes = pixels.len() as vk::DeviceSize;
        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size_bytes)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer =
            unsafe { device.create_buffer(&buffer_create_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkCreateBuffer(static_image)",
                    result,
                }
            })?;
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let memory_type_index = native_vulkan_memory_type_index(
            &memory_properties,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .ok_or(NativeVulkanError::MissingMemoryType(
            "static image staging buffer",
        ))?;
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe { device.allocate_memory(&allocate_info, None) }.map_err(|result| {
            unsafe {
                device.destroy_buffer(buffer, None);
            }
            NativeVulkanError::Vulkan {
                operation: "vkAllocateMemory(static_image)",
                result,
            }
        })?;
        if let Err(err) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_buffer(buffer, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindBufferMemory(static_image)",
                result: err,
            });
        }
        let map = unsafe { device.map_memory(memory, 0, size_bytes, vk::MemoryMapFlags::empty()) }
            .map_err(|result| {
                unsafe {
                    device.free_memory(memory, None);
                    device.destroy_buffer(buffer, None);
                }
                NativeVulkanError::Vulkan {
                    operation: "vkMapMemory(static_image)",
                    result,
                }
            })?;
        unsafe {
            ptr::copy_nonoverlapping(pixels.as_ptr(), map.cast::<u8>(), pixels.len());
            device.unmap_memory(memory);
        }

        Ok(Self {
            buffer,
            memory,
            buffer_image_copy: vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                },
            },
            size_bytes,
        })
    }

    fn destroy(self, device: &ash::Device) {
        unsafe {
            device.free_memory(self.memory, None);
            device.destroy_buffer(self.buffer, None);
        }
    }
}

fn native_vulkan_static_image_pixels(
    source: &PathBuf,
    fit: FitMode,
    background: Option<&str>,
    format: vk::Format,
    target_size: (u32, u32),
) -> Result<Vec<u8>, NativeVulkanError> {
    if target_size.0 == 0 || target_size.1 == 0 {
        return Err(NativeVulkanError::StaticImage(
            "target image size is zero".to_owned(),
        ));
    }
    let image = image::ImageReader::open(source)
        .map_err(|err| NativeVulkanError::StaticImage(format!("open {}: {err}", source.display())))?
        .with_guessed_format()
        .map_err(|err| {
            NativeVulkanError::StaticImage(format!("guess format {}: {err}", source.display()))
        })?
        .decode()
        .map_err(|err| {
            NativeVulkanError::StaticImage(format!("decode {}: {err}", source.display()))
        })?
        .to_rgba8();
    let mut canvas = image::RgbaImage::from_pixel(
        target_size.0,
        target_size.1,
        native_vulkan_parse_background(background),
    );
    native_vulkan_blit_fit(&image, &mut canvas, fit);
    Ok(native_vulkan_encode_swapchain_pixels(&canvas, format))
}

fn native_vulkan_parse_background(background: Option<&str>) -> image::Rgba<u8> {
    let Some(value) = background else {
        return image::Rgba([0, 0, 0, 255]);
    };
    let Some(hex) = value.trim().strip_prefix('#') else {
        return image::Rgba([0, 0, 0, 255]);
    };
    if hex.len() != 6 {
        return image::Rgba([0, 0, 0, 255]);
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    image::Rgba([r, g, b, 255])
}

fn native_vulkan_blit_fit(source: &image::RgbaImage, canvas: &mut image::RgbaImage, fit: FitMode) {
    let source_width = source.width().max(1);
    let source_height = source.height().max(1);
    let target_width = canvas.width().max(1);
    let target_height = canvas.height().max(1);
    match fit {
        FitMode::Stretch => {
            let resized = image::imageops::resize(
                source,
                target_width,
                target_height,
                image::imageops::FilterType::Triangle,
            );
            image::imageops::replace(canvas, &resized, 0, 0);
        }
        FitMode::Center => {
            let x = (target_width as i64 - source_width as i64) / 2;
            let y = (target_height as i64 - source_height as i64) / 2;
            image::imageops::overlay(canvas, source, x, y);
        }
        FitMode::Tile => {
            let mut y = 0;
            while y < target_height {
                let mut x = 0;
                while x < target_width {
                    image::imageops::overlay(canvas, source, x as i64, y as i64);
                    x = x.saturating_add(source_width);
                }
                y = y.saturating_add(source_height);
            }
        }
        FitMode::Contain | FitMode::Cover => {
            let scale_x = target_width as f64 / source_width as f64;
            let scale_y = target_height as f64 / source_height as f64;
            let scale = if fit == FitMode::Cover {
                scale_x.max(scale_y)
            } else {
                scale_x.min(scale_y)
            };
            let scaled_width = ((source_width as f64 * scale).round() as u32).max(1);
            let scaled_height = ((source_height as f64 * scale).round() as u32).max(1);
            let resized = image::imageops::resize(
                source,
                scaled_width,
                scaled_height,
                image::imageops::FilterType::Triangle,
            );
            let x = (target_width as i64 - scaled_width as i64) / 2;
            let y = (target_height as i64 - scaled_height as i64) / 2;
            image::imageops::overlay(canvas, &resized, x, y);
        }
    }
}

fn native_vulkan_encode_swapchain_pixels(image: &image::RgbaImage, format: vk::Format) -> Vec<u8> {
    let mut pixels = image.as_raw().clone();
    if matches!(
        format,
        vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB
    ) {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    pixels
}

fn native_vulkan_memory_type_index(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    memory_type_bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_properties.memory_types[..memory_properties.memory_type_count as usize]
        .iter()
        .enumerate()
        .find_map(|(index, memory_type)| {
            let supported = (memory_type_bits & (1 << index)) != 0;
            (supported && memory_type.property_flags.contains(flags)).then_some(index as u32)
        })
}

struct NativeVulkanPresentQueueQuery {
    selection: NativeVulkanPresentQueueSelection,
    #[allow(dead_code)]
    physical_device_count: usize,
    #[allow(dead_code)]
    present_queue_family_count: usize,
}

fn select_native_vulkan_present_queue(
    instance: &ash::Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<NativeVulkanPresentQueueQuery, NativeVulkanError> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkEnumeratePhysicalDevices",
            result,
        }
    })?;
    let mut present_queue_family_count = 0usize;
    let mut selected = None;

    for (physical_device_index, physical_device) in physical_devices.iter().copied().enumerate() {
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        for (queue_family_index, queue_family) in queue_families.iter().enumerate() {
            let supports_surface = unsafe {
                surface_loader.get_physical_device_surface_support(
                    physical_device,
                    queue_family_index as u32,
                    surface,
                )
            }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkGetPhysicalDeviceSurfaceSupportKHR",
                result,
            })?;
            if !supports_surface {
                continue;
            }
            present_queue_family_count += 1;

            let supports_graphics = queue_family.queue_flags.contains(vk::QueueFlags::GRAPHICS);
            if selected.is_none() && supports_graphics {
                selected = Some(NativeVulkanPresentQueueSelection {
                    physical_device,
                    physical_device_index,
                    physical_device_name: native_vulkan_physical_device_name(properties),
                    physical_device_type: native_vulkan_physical_device_type_label(
                        properties.device_type,
                    ),
                    queue_family_index: queue_family_index as u32,
                });
            }
        }
    }

    let Some(selection) = selected else {
        return Err(NativeVulkanError::MissingPresentQueue);
    };
    Ok(NativeVulkanPresentQueueQuery {
        selection,
        physical_device_count: physical_devices.len(),
        present_queue_family_count,
    })
}

fn ensure_native_vulkan_device_extension(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    extension: &'static CStr,
) -> Result<(), NativeVulkanError> {
    let extensions = unsafe { instance.enumerate_device_extension_properties(physical_device) }
        .map_err(|result| NativeVulkanError::Vulkan {
            operation: "vkEnumerateDeviceExtensionProperties",
            result,
        })?;
    if extensions
        .iter()
        .filter_map(|property| property.extension_name_as_c_str().ok())
        .any(|name| name == extension)
    {
        Ok(())
    } else {
        Err(NativeVulkanError::MissingDeviceExtension(
            ash_extension_name(extension),
        ))
    }
}

struct NativeVulkanSwapchainPlan {
    create_info: vk::SwapchainCreateInfoKHR<'static>,
    format: vk::SurfaceFormatKHR,
    present_mode: vk::PresentModeKHR,
    extent: vk::Extent2D,
}

fn create_native_vulkan_swapchain_plan(
    surface_loader: &ash::khr::surface::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    _logical_size: (u32, u32),
    buffer_size: (u32, u32),
) -> Result<NativeVulkanSwapchainPlan, NativeVulkanError> {
    let capabilities = unsafe {
        surface_loader.get_physical_device_surface_capabilities(physical_device, surface)
    }
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
        result,
    })?;
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::TRANSFER_DST)
    {
        return Err(NativeVulkanError::UnsupportedSwapchainUsage("TRANSFER_DST"));
    }
    if !capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
    {
        return Err(NativeVulkanError::UnsupportedSwapchainUsage(
            "COLOR_ATTACHMENT",
        ));
    }
    let formats =
        unsafe { surface_loader.get_physical_device_surface_formats(physical_device, surface) }
            .map_err(|result| NativeVulkanError::Vulkan {
                operation: "vkGetPhysicalDeviceSurfaceFormatsKHR",
                result,
            })?;
    let format = choose_native_vulkan_surface_format(&formats)?;
    let present_modes = unsafe {
        surface_loader.get_physical_device_surface_present_modes(physical_device, surface)
    }
    .map_err(|result| NativeVulkanError::Vulkan {
        operation: "vkGetPhysicalDeviceSurfacePresentModesKHR",
        result,
    })?;
    let present_mode = choose_native_vulkan_present_mode(&present_modes);
    let extent = choose_native_vulkan_swapchain_extent(&capabilities, buffer_size)?;
    let image_count = native_vulkan_swapchain_image_count(&capabilities);
    let composite_alpha =
        choose_native_vulkan_composite_alpha(capabilities.supported_composite_alpha);
    let create_info = vk::SwapchainCreateInfoKHR::default()
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
        .clipped(true);

    Ok(NativeVulkanSwapchainPlan {
        create_info,
        format,
        present_mode,
        extent,
    })
}

fn create_native_vulkan_swapchain_image_views(
    device: &ash::Device,
    images: &[vk::Image],
    format: vk::Format,
) -> Result<Vec<vk::ImageView>, NativeVulkanError> {
    let mut views = Vec::with_capacity(images.len());
    for image in images {
        let create_info = vk::ImageViewCreateInfo::default()
            .image(*image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(native_vulkan_color_subresource_range());
        let view = match unsafe { device.create_image_view(&create_info, None) } {
            Ok(view) => view,
            Err(result) => {
                for view in views {
                    unsafe {
                        device.destroy_image_view(view, None);
                    }
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateImageView(swapchain)",
                    result,
                });
            }
        };
        views.push(view);
    }
    Ok(views)
}

fn choose_native_vulkan_surface_format(
    formats: &[vk::SurfaceFormatKHR],
) -> Result<vk::SurfaceFormatKHR, NativeVulkanError> {
    if formats.is_empty() {
        return Err(NativeVulkanError::MissingSurfaceFormat);
    }
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
        .ok_or(NativeVulkanError::MissingSurfaceFormat)
}

fn choose_native_vulkan_present_mode(present_modes: &[vk::PresentModeKHR]) -> vk::PresentModeKHR {
    if present_modes.contains(&vk::PresentModeKHR::FIFO) {
        vk::PresentModeKHR::FIFO
    } else {
        present_modes
            .first()
            .copied()
            .unwrap_or(vk::PresentModeKHR::FIFO)
    }
}

fn choose_native_vulkan_swapchain_extent(
    capabilities: &vk::SurfaceCapabilitiesKHR,
    logical_size: (u32, u32),
) -> Result<vk::Extent2D, NativeVulkanError> {
    if let Some((width, height)) = native_vulkan_extent(capabilities.current_extent) {
        return Ok(vk::Extent2D { width, height });
    }
    let width = logical_size.0.clamp(
        capabilities.min_image_extent.width,
        capabilities.max_image_extent.width,
    );
    let height = logical_size.1.clamp(
        capabilities.min_image_extent.height,
        capabilities.max_image_extent.height,
    );
    if width == 0 || height == 0 {
        return Err(NativeVulkanError::InvalidSwapchainExtent);
    }
    Ok(vk::Extent2D { width, height })
}

fn native_vulkan_swapchain_image_count(capabilities: &vk::SurfaceCapabilitiesKHR) -> u32 {
    let preferred = capabilities.min_image_count.saturating_add(1).max(2);
    if capabilities.max_image_count > 0 {
        preferred.min(capabilities.max_image_count)
    } else {
        preferred
    }
}

fn choose_native_vulkan_composite_alpha(
    flags: vk::CompositeAlphaFlagsKHR,
) -> vk::CompositeAlphaFlagsKHR {
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

fn native_vulkan_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
}

fn create_native_vulkan_instance() -> Result<(ash::Entry, ash::Instance), NativeVulkanError> {
    let entry =
        unsafe { ash::Entry::load() }.map_err(|err| NativeVulkanError::Loading(err.to_string()))?;
    let app_name = CString::new("gilder-native-vulkan").expect("static app name has no nul");
    let engine_name = CString::new("gilder").expect("static engine name has no nul");
    let app_info = vk::ApplicationInfo::default()
        .application_name(app_name.as_c_str())
        .application_version(1)
        .engine_name(engine_name.as_c_str())
        .engine_version(1)
        .api_version(vk::API_VERSION_1_3);
    let extension_names = [
        ash::khr::surface::NAME.as_ptr(),
        ash::khr::wayland_surface::NAME.as_ptr(),
    ];
    let create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names);
    let instance = unsafe { entry.create_instance(&create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateInstance",
            result,
        }
    })?;

    Ok((entry, instance))
}

fn native_vulkan_physical_device_name(properties: vk::PhysicalDeviceProperties) -> String {
    unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn native_vulkan_physical_device_type_label(device_type: vk::PhysicalDeviceType) -> &'static str {
    match device_type {
        vk::PhysicalDeviceType::OTHER => "other",
        vk::PhysicalDeviceType::INTEGRATED_GPU => "integrated-gpu",
        vk::PhysicalDeviceType::DISCRETE_GPU => "discrete-gpu",
        vk::PhysicalDeviceType::VIRTUAL_GPU => "virtual-gpu",
        vk::PhysicalDeviceType::CPU => "cpu",
        _ => "unknown",
    }
}

fn native_vulkan_present_mode_label(present_mode: vk::PresentModeKHR) -> &'static str {
    match present_mode {
        vk::PresentModeKHR::IMMEDIATE => "immediate",
        vk::PresentModeKHR::MAILBOX => "mailbox",
        vk::PresentModeKHR::FIFO => "fifo",
        vk::PresentModeKHR::FIFO_RELAXED => "fifo-relaxed",
        _ => "unknown",
    }
}

fn native_vulkan_extent(extent: vk::Extent2D) -> Option<(u32, u32)> {
    if extent.width == u32::MAX || extent.height == u32::MAX {
        None
    } else {
        Some((extent.width, extent.height))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanWallpaperType {
    StaticImage,
    Video,
    Web,
    SceneLite,
    Shader,
    Playlist,
}

pub const WALLPAPER_TYPE_CONTRACT: &[NativeVulkanWallpaperType] = &[
    NativeVulkanWallpaperType::StaticImage,
    NativeVulkanWallpaperType::Video,
    NativeVulkanWallpaperType::Web,
    NativeVulkanWallpaperType::SceneLite,
    NativeVulkanWallpaperType::Shader,
    NativeVulkanWallpaperType::Playlist,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanWallpaperTypeSupport {
    pub wallpaper_type: NativeVulkanWallpaperType,
    pub current_vulkan_item: bool,
    pub current_renderer_status: &'static str,
    pub target_vulkan_path: &'static str,
}

pub fn wallpaper_type_support_matrix() -> Vec<NativeVulkanWallpaperTypeSupport> {
    vec![
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::StaticImage,
            current_vulkan_item: true,
            current_renderer_status: "CPU decode/fit into staging buffer, copied into swapchain image",
            target_vulkan_path: "decode image -> sampled Vulkan image -> fit-aware textured fullscreen pass",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Video,
            current_vulkan_item: true,
            current_renderer_status: "video render item runs through native Vulkan lifecycle; GStreamer appsink feeds CUDA importer on NVIDIA; DMABuf/VAAPI importer still pending",
            target_vulkan_path: "GStreamer decode -> importer-specific CUDAMemory/DMABuf/EGLImage/Vulkan image -> Vulkan YUV sampling",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Web,
            current_vulkan_item: false,
            current_renderer_status: "helper contract only; current render plan may fall back to static image",
            target_vulkan_path: "Web helper -> DMABuf/EGLImage/shared-frame handoff -> Vulkan composite",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::SceneLite,
            current_vulkan_item: true,
            current_renderer_status: "render item mapped; scene draw pass not implemented yet",
            target_vulkan_path: "deterministic scene snapshot -> Vulkan shape/image/text passes",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Shader,
            current_vulkan_item: false,
            current_renderer_status: "shader contract only; current render plan may fall back to static image",
            target_vulkan_path: "fullscreen triangle -> GLSL/WGSL-derived SPIR-V -> time/property uniforms",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Playlist,
            current_vulkan_item: false,
            current_renderer_status: "playlist selection remains in core render sync; selected child maps to Vulkan item",
            target_vulkan_path: "core playlist decision -> selected child item -> same Vulkan runtime path",
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum NativeVulkanRenderItem {
    Clear {
        output_name: String,
    },
    StaticImage {
        output_name: String,
        source: PathBuf,
        fit: FitMode,
        background: Option<String>,
        renderer_status: &'static str,
    },
    Video {
        output_name: String,
        source: PathBuf,
        poster: Option<PathBuf>,
        fit: FitMode,
        loop_playback: bool,
        muted: bool,
        manifest_max_fps: Option<u32>,
        target_max_fps: Option<u32>,
        decoder_policy: VideoDecoderPolicy,
        start_offset_ms: u64,
        renderer_status: &'static str,
    },
    Slideshow {
        output_name: String,
        sources: Vec<PathBuf>,
        interval_ms: u64,
        transition: Transition,
        fit: FitMode,
        target_max_fps: Option<u32>,
        renderer_status: &'static str,
    },
    SceneLite {
        output_name: String,
        fallback: Option<PathBuf>,
        display_image: Option<PathBuf>,
        layer_count: usize,
        target_max_fps: Option<u32>,
        renderer_status: &'static str,
    },
}

impl NativeVulkanRenderItem {
    pub fn wallpaper_type(&self) -> NativeVulkanWallpaperType {
        match self {
            Self::Clear { .. } => NativeVulkanWallpaperType::StaticImage,
            Self::StaticImage { .. } => NativeVulkanWallpaperType::StaticImage,
            Self::Video { .. } => NativeVulkanWallpaperType::Video,
            Self::Slideshow { .. } => NativeVulkanWallpaperType::Playlist,
            Self::SceneLite { .. } => NativeVulkanWallpaperType::SceneLite,
        }
    }
}

pub fn render_items_from_sync_plan(plan: &StaticRenderSyncPlan) -> Vec<NativeVulkanRenderItem> {
    plan.plans
        .iter()
        .map(native_vulkan_static_item)
        .chain(plan.video_plans.iter().map(native_vulkan_video_item))
        .chain(
            plan.slideshow_plans
                .iter()
                .map(native_vulkan_slideshow_item),
        )
        .chain(
            plan.scene_lite_plans
                .iter()
                .map(native_vulkan_scene_lite_item),
        )
        .collect()
}

fn native_vulkan_static_item(plan: &StaticWallpaperPlan) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::StaticImage {
        output_name: plan.output_name.clone(),
        source: plan.source.clone(),
        fit: plan.fit,
        background: plan.background.clone(),
        renderer_status: "cpu-fit-staging-copy",
    }
}

fn native_vulkan_video_item(plan: &VideoWallpaperPlan) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Video {
        output_name: plan.output_name.clone(),
        source: plan.source.clone(),
        poster: plan.poster.clone(),
        fit: plan.fit,
        loop_playback: plan.loop_playback,
        muted: plan.muted,
        manifest_max_fps: plan.manifest_max_fps,
        target_max_fps: plan.target_max_fps,
        decoder_policy: plan.decoder_policy,
        start_offset_ms: plan.start_offset_ms,
        renderer_status: "vulkan-lifecycle-video-placeholder",
    }
}

fn native_vulkan_video_runtime_snapshot(
    item: &NativeVulkanRenderItem,
    frontend: Option<NativeVulkanGstVideoFrontendSnapshot>,
    import: Option<NativeVulkanVideoImportSnapshot>,
    rendered_frames: u64,
    poster_upload_bytes: Option<u64>,
) -> Option<NativeVulkanVideoRuntimeSnapshot> {
    let NativeVulkanRenderItem::Video {
        source,
        poster,
        fit,
        loop_playback,
        muted,
        manifest_max_fps,
        target_max_fps,
        decoder_policy,
        start_offset_ms,
        ..
    } = item
    else {
        return None;
    };

    let frontend_status = match frontend.as_ref() {
        Some(frontend) if frontend.frames_received > 0 => "appsink-receiving-samples",
        Some(_) => "appsink-started-waiting-for-samples",
        None if poster.is_some() => "not-started-poster-placeholder",
        None => "not-started-clear-placeholder",
    };
    let handoff_status = match frontend.as_ref() {
        Some(frontend) if frontend.frames_received > 0 => "appsink-sample-handoff-active",
        Some(_) => "appsink-started-no-sample-yet",
        None => "pending-appsink-dmabuf-or-gpu-memory-handoff",
    };
    let frames_received = frontend
        .as_ref()
        .map(|frontend| frontend.frames_received)
        .unwrap_or(0);
    let frames_imported = import
        .as_ref()
        .map(|import| import.frames_imported)
        .unwrap_or(0);
    let received_placeholder_frames = rendered_frames.saturating_sub(frames_imported);

    Some(NativeVulkanVideoRuntimeSnapshot {
        source: source.clone(),
        poster: poster.clone(),
        fit: *fit,
        loop_playback: *loop_playback,
        muted: *muted,
        manifest_max_fps: *manifest_max_fps,
        target_max_fps: *target_max_fps,
        decoder_policy: *decoder_policy,
        start_offset_ms: *start_offset_ms,
        frontend: if frontend.is_some() {
            "gstreamer-appsink"
        } else {
            "gstreamer-planned"
        },
        frontend_status,
        handoff_status,
        texture_import_status: import
            .as_ref()
            .map(|import| import.texture_import_status)
            .unwrap_or("not-importing-yet"),
        audio_status: if *muted {
            "muted-no-audio-pipeline"
        } else {
            "planned-separate-audio-pipeline"
        },
        gst_state: frontend
            .as_ref()
            .and_then(|frontend| frontend.gst_state.clone()),
        eos_messages: frontend
            .as_ref()
            .map(|frontend| frontend.eos_messages)
            .unwrap_or(0),
        segment_done_messages: frontend
            .as_ref()
            .map(|frontend| frontend.segment_done_messages)
            .unwrap_or(0),
        frames_received,
        frames_imported,
        rendered_placeholder_frames: received_placeholder_frames,
        poster_upload_bytes,
        last_import_size: import.as_ref().and_then(|import| import.last_import_size),
        last_import_memory_path: import
            .as_ref()
            .and_then(|import| import.last_import_memory_path.clone()),
        last_import_error: import
            .as_ref()
            .and_then(|import| import.last_import_error.clone()),
        last_import_elapsed_us: import
            .as_ref()
            .and_then(|import| import.last_import_elapsed_us),
        max_import_elapsed_us: import
            .as_ref()
            .and_then(|import| import.max_import_elapsed_us),
        last_sample_caps: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_caps.clone()),
        last_sample_format: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_format.clone()),
        last_sample_size: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_size),
        last_sample_pts_ms: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_pts_ms),
        last_sample_duration_ms: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_duration_ms),
        last_sample_pts_delta_ms: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_pts_delta_ms),
        last_sample_memory_types: frontend
            .as_ref()
            .map(|frontend| frontend.last_sample_memory_types.clone())
            .unwrap_or_default(),
        actual_decoders: frontend
            .as_ref()
            .map(|frontend| frontend.actual_decoders.clone())
            .unwrap_or_default(),
        decoder_policy_status: frontend
            .as_ref()
            .and_then(|frontend| frontend.decoder_policy_status.clone()),
        caps_report_count: frontend
            .as_ref()
            .map(|frontend| frontend.caps_report_count)
            .unwrap_or(0),
        caps_memory_features: frontend
            .as_ref()
            .map(|frontend| frontend.caps_memory_features.clone())
            .unwrap_or_default(),
        caps_reports: frontend
            .as_ref()
            .map(|frontend| frontend.caps_reports.clone())
            .unwrap_or_default(),
        last_error: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_error.clone()),
    })
}

fn native_vulkan_slideshow_item(plan: &SlideshowWallpaperPlan) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::Slideshow {
        output_name: plan.output_name.clone(),
        sources: plan.sources.clone(),
        interval_ms: plan.interval_ms,
        transition: plan.transition,
        fit: plan.fit,
        target_max_fps: plan.target_max_fps,
        renderer_status: "planned-slideshow-static-texture-sequence",
    }
}

fn native_vulkan_scene_lite_item(plan: &SceneLiteWallpaperPlan) -> NativeVulkanRenderItem {
    NativeVulkanRenderItem::SceneLite {
        output_name: plan.output_name.clone(),
        fallback: plan.fallback.clone(),
        display_image: match &plan.display {
            Some(SceneLiteDisplayPlan::Image { source, .. }) => Some(source.clone()),
            Some(SceneLiteDisplayPlan::Color { .. }) | None => None,
        },
        layer_count: plan.layers.len(),
        target_max_fps: plan.target_max_fps,
        renderer_status: "planned-scene-lite-vulkan-passes",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanBackendContract {
    pub backend_name: &'static str,
    pub default_renderer_candidate: bool,
    pub wallpaper_types: &'static [NativeVulkanWallpaperType],
    pub wallpaper_type_support: Vec<NativeVulkanWallpaperTypeSupport>,
    pub layer_shell_host: &'static str,
    pub render_plan_boundary: &'static str,
    pub lifecycle_boundary: &'static str,
    pub resource_telemetry_boundary: &'static str,
    pub required_instance_extensions: Vec<&'static str>,
    pub required_device_extensions: Vec<&'static str>,
    pub video_interop: NativeVulkanVideoInteropContract,
    pub web_interop: NativeVulkanWebInteropContract,
}

pub fn backend_contract() -> NativeVulkanBackendContract {
    NativeVulkanBackendContract {
        backend_name: "native-vulkan",
        default_renderer_candidate: false,
        wallpaper_types: WALLPAPER_TYPE_CONTRACT,
        wallpaper_type_support: wallpaper_type_support_matrix(),
        layer_shell_host: "reuse NativeWaylandHost raw wl_display/wl_surface first, then move ownership here",
        render_plan_boundary: "consume existing renderer plans; do not introduce Vulkan-only manifest semantics",
        lifecycle_boundary: "pause-dynamic, hidden/fullscreen/session release, resize, and output selection stay backend-neutral",
        resource_telemetry_boundary: "report CPU/RSS/PSS/private_dirty/GPU resource counts through stable renderer telemetry",
        required_instance_extensions: required_instance_extensions(),
        required_device_extensions: required_device_extensions(),
        video_interop: video_interop_contract(),
        web_interop: web_interop_contract(),
    }
}

pub fn required_instance_extensions() -> Vec<&'static str> {
    vec![
        ash_extension_name(ash::khr::surface::NAME),
        ash_extension_name(ash::khr::wayland_surface::NAME),
    ]
}

pub fn required_device_extensions() -> Vec<&'static str> {
    vec![
        ash_extension_name(ash::khr::swapchain::NAME),
        ash_extension_name(ash::khr::external_memory_fd::NAME),
        ash_extension_name(ash::khr::external_semaphore_fd::NAME),
        ash_extension_name(ash::khr::timeline_semaphore::NAME),
        ash_extension_name(ash::ext::external_memory_dma_buf::NAME),
        ash_extension_name(ash::ext::image_drm_format_modifier::NAME),
    ]
}

fn ash_extension_name(name: &'static CStr) -> &'static str {
    name.to_str()
        .expect("Vulkan extension names shipped by ash must be UTF-8")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoInteropContract {
    pub target_memory_flow: &'static str,
    pub current_baseline: &'static str,
    pub target_sampling: &'static str,
    pub avoids_default_rgba_upload: bool,
    pub decoder_policy: &'static str,
    pub audio_strategy: &'static str,
    pub known_blockers: &'static [&'static str],
}

pub fn video_interop_contract() -> NativeVulkanVideoInteropContract {
    NativeVulkanVideoInteropContract {
        target_memory_flow: "decoder GPU memory -> importable DMABuf/EGLImage/Vulkan image -> Vulkan YUV sampling",
        current_baseline: "native-wgpu GStreamer CUDAMemory -> CUDA copy -> external Vulkan image planes -> wgpu present",
        target_sampling: "NV12/P010/YUV planes sampled directly in Vulkan before RGB composition",
        avoids_default_rgba_upload: true,
        decoder_policy: "prefer GStreamer for codec/audio coverage; allow Vulkan Video or libavcodec import paths when they win evidence",
        audio_strategy: "keep audio pipeline separate from the video texture path so decoder choice does not block playback support",
        known_blockers: &[
            "direct gst_cuda_memory_export fd import returned zero Vulkan memory_type_bits on NVIDIA",
            "GLMemory DMABuf export may require libnvrtc on nvcodec systems",
            "default switch requires real Wayland evidence beating the current 4K/240 native-wgpu baseline",
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanWebInteropContract {
    pub helper_boundary: &'static str,
    pub accepted_frame_sources: &'static [&'static str],
    pub blocked_designs: &'static [&'static str],
}

pub fn web_interop_contract() -> NativeVulkanWebInteropContract {
    NativeVulkanWebInteropContract {
        helper_boundary: "WebKitGTK or browser code stays in a helper; native Vulkan receives frames or importable textures",
        accepted_frame_sources: &[
            "DMABuf texture handoff",
            "EGLImage/exportable GL texture handoff",
            "shared-memory frame stream only as a fallback",
        ],
        blocked_designs: &[
            "making GTK/WebKitGTK the native Vulkan renderer host",
            "adding Web-specific daemon or manifest branches",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn reports_vulkan_spike_as_built_but_not_default() {
        let capabilities = capabilities();

        assert!(capabilities.built);
        assert!(capabilities.experimental);
        assert!(!capabilities.default_enabled);
        assert!(capabilities.reuses_native_wayland_host);
        assert!(capabilities.owns_vulkan_instance_now);
        assert!(capabilities.owns_wayland_vulkan_surface_now);
        assert!(capabilities.owns_vulkan_device_now);
        assert!(capabilities.owns_swapchain_now);
        assert!(capabilities.renders_frames_now);
        assert!(!capabilities.consumes_render_sync);
        assert!(capabilities.direct_video_memory_status.contains("DMABuf"));
    }

    #[test]
    fn contract_covers_full_wallpaper_type_matrix() {
        let contract = backend_contract();

        assert_eq!(contract.backend_name, "native-vulkan");
        assert_eq!(
            contract.wallpaper_types,
            &[
                NativeVulkanWallpaperType::StaticImage,
                NativeVulkanWallpaperType::Video,
                NativeVulkanWallpaperType::Web,
                NativeVulkanWallpaperType::SceneLite,
                NativeVulkanWallpaperType::Shader,
                NativeVulkanWallpaperType::Playlist,
            ]
        );
        assert!(contract.video_interop.avoids_default_rgba_upload);
        assert_eq!(contract.wallpaper_type_support.len(), 6);
    }

    #[test]
    fn wallpaper_type_support_marks_current_items_and_future_contracts() {
        let support = wallpaper_type_support_matrix();

        assert_eq!(support.len(), WALLPAPER_TYPE_CONTRACT.len());
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::StaticImage)
                .is_some_and(|entry| entry.current_vulkan_item)
        );
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::Video)
                .is_some_and(|entry| entry.current_vulkan_item)
        );
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::Web)
                .is_some_and(|entry| !entry.current_vulkan_item)
        );
        assert!(
            support
                .iter()
                .find(|entry| entry.wallpaper_type == NativeVulkanWallpaperType::Shader)
                .is_some_and(|entry| !entry.current_vulkan_item)
        );
    }

    #[test]
    fn maps_sync_plan_to_vulkan_items() {
        let sync_plan = StaticRenderSyncPlan {
            plans: vec![StaticWallpaperPlan {
                output_name: "HDMI-A-1".to_owned(),
                source: PathBuf::from("/tmp/static.png"),
                fit: FitMode::Cover,
                background: Some("#000000".to_owned()),
            }],
            video_plans: vec![VideoWallpaperPlan {
                output_name: "HDMI-A-1".to_owned(),
                source: PathBuf::from("/tmp/video.mp4"),
                poster: None,
                fit: FitMode::Contain,
                loop_playback: true,
                muted: true,
                manifest_max_fps: Some(240),
                target_max_fps: Some(240),
                decoder_policy: crate::config::VideoDecoderPolicy::HardwarePreferred,
                start_offset_ms: 0,
            }],
            slideshow_plans: Vec::new(),
            scene_lite_plans: Vec::new(),
            removals: Vec::new(),
            errors: Vec::new(),
            decisions: Vec::new(),
            playlist_clock_dependency: Default::default(),
            cache: Default::default(),
        };

        let items = render_items_from_sync_plan(&sync_plan);

        assert_eq!(items.len(), 2);
        assert!(matches!(
            items[0],
            NativeVulkanRenderItem::StaticImage { .. }
        ));
        assert!(matches!(items[1], NativeVulkanRenderItem::Video { .. }));
        assert_eq!(items[1].wallpaper_type(), NativeVulkanWallpaperType::Video);
        let NativeVulkanRenderItem::Video {
            target_max_fps,
            decoder_policy,
            start_offset_ms,
            renderer_status,
            ..
        } = &items[1]
        else {
            unreachable!("item already matched as video");
        };
        assert_eq!(*target_max_fps, Some(240));
        assert_eq!(
            *decoder_policy,
            crate::config::VideoDecoderPolicy::HardwarePreferred
        );
        assert_eq!(*start_offset_ms, 0);
        assert_eq!(*renderer_status, "vulkan-lifecycle-video-placeholder");
    }

    #[test]
    fn video_runtime_snapshot_reports_pending_gstreamer_handoff() {
        let item = NativeVulkanRenderItem::Video {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/video.mp4"),
            poster: Some(PathBuf::from("/tmp/poster.png")),
            fit: FitMode::Contain,
            loop_playback: true,
            muted: false,
            manifest_max_fps: Some(240),
            target_max_fps: Some(120),
            decoder_policy: crate::config::VideoDecoderPolicy::HardwareRequired,
            start_offset_ms: 1500,
            renderer_status: "vulkan-lifecycle-video-placeholder",
        };

        let snapshot = native_vulkan_video_runtime_snapshot(&item, None, None, 9, Some(1024))
            .expect("video snapshot");

        assert_eq!(snapshot.frontend, "gstreamer-planned");
        assert_eq!(snapshot.frontend_status, "not-started-poster-placeholder");
        assert_eq!(
            snapshot.handoff_status,
            "pending-appsink-dmabuf-or-gpu-memory-handoff"
        );
        assert_eq!(snapshot.audio_status, "planned-separate-audio-pipeline");
        assert_eq!(snapshot.frames_received, 0);
        assert_eq!(snapshot.frames_imported, 0);
        assert_eq!(snapshot.rendered_placeholder_frames, 9);
        assert_eq!(snapshot.poster_upload_bytes, Some(1024));
        assert_eq!(snapshot.texture_import_status, "not-importing-yet");
        assert_eq!(snapshot.last_import_size, None);
        assert_eq!(snapshot.last_import_memory_path, None);
        assert_eq!(snapshot.last_import_error, None);
        assert_eq!(snapshot.last_import_elapsed_us, None);
        assert_eq!(snapshot.max_import_elapsed_us, None);
        assert_eq!(snapshot.start_offset_ms, 1500);
        assert_eq!(snapshot.gst_state, None);
        assert_eq!(snapshot.decoder_policy_status, None);
        assert_eq!(snapshot.caps_report_count, 0);
        assert_eq!(snapshot.segment_done_messages, 0);
    }

    #[test]
    fn video_runtime_snapshot_reports_active_appsink_frontend() {
        let item = NativeVulkanRenderItem::Video {
            output_name: "HDMI-A-1".to_owned(),
            source: PathBuf::from("/tmp/video.mp4"),
            poster: None,
            fit: FitMode::Cover,
            loop_playback: true,
            muted: true,
            manifest_max_fps: None,
            target_max_fps: Some(240),
            decoder_policy: crate::config::VideoDecoderPolicy::HardwarePreferred,
            start_offset_ms: 0,
            renderer_status: "vulkan-lifecycle-video-placeholder",
        };
        let frontend = NativeVulkanGstVideoFrontendSnapshot {
            gst_state: Some("Playing".to_owned()),
            eos_messages: 0,
            segment_done_messages: 1,
            frames_received: 3,
            last_sample_caps: Some("video/x-raw, format=(string)NV12".to_owned()),
            last_sample_format: Some("NV12".to_owned()),
            last_sample_size: Some((3840, 2160)),
            last_sample_pts_ms: Some(8),
            last_sample_duration_ms: Some(4),
            last_sample_pts_delta_ms: Some(4),
            last_sample_memory_types: vec!["CUDAMemory".to_owned()],
            actual_decoders: vec!["nvh264dec".to_owned()],
            decoder_policy_status: Some("Satisfied".to_owned()),
            caps_report_count: 1,
            caps_memory_features: vec!["memory:CUDAMemory".to_owned()],
            caps_reports: vec![NativeVulkanVideoCapsSnapshot {
                element: "appsink0".to_owned(),
                pad: "sink".to_owned(),
                direction: "sink".to_owned(),
                caps: "video/x-raw(memory:CUDAMemory)".to_owned(),
                source: "current".to_owned(),
                memory_features: vec!["memory:CUDAMemory".to_owned()],
            }],
            last_error: None,
        };
        let import = NativeVulkanVideoImportSnapshot {
            texture_import_status: "importing-cuda-vulkan-image-planes",
            frames_imported: 2,
            last_import_size: Some((3840, 2160)),
            last_import_memory_path: Some(
                "CUDAMemory->CUDA->Vulkan external image planes".to_owned(),
            ),
            last_import_error: None,
            last_import_elapsed_us: Some(900),
            max_import_elapsed_us: Some(1200),
        };

        let snapshot =
            native_vulkan_video_runtime_snapshot(&item, Some(frontend), Some(import), 12, None)
                .unwrap();

        assert_eq!(snapshot.frontend, "gstreamer-appsink");
        assert_eq!(snapshot.frontend_status, "appsink-receiving-samples");
        assert_eq!(snapshot.handoff_status, "appsink-sample-handoff-active");
        assert_eq!(snapshot.frames_received, 3);
        assert_eq!(snapshot.frames_imported, 2);
        assert_eq!(snapshot.segment_done_messages, 1);
        assert_eq!(snapshot.rendered_placeholder_frames, 10);
        assert_eq!(
            snapshot.texture_import_status,
            "importing-cuda-vulkan-image-planes"
        );
        assert_eq!(snapshot.last_import_size, Some((3840, 2160)));
        assert_eq!(
            snapshot.last_import_memory_path.as_deref(),
            Some("CUDAMemory->CUDA->Vulkan external image planes")
        );
        assert_eq!(snapshot.last_import_elapsed_us, Some(900));
        assert_eq!(snapshot.max_import_elapsed_us, Some(1200));
        assert_eq!(snapshot.last_sample_format.as_deref(), Some("NV12"));
        assert_eq!(snapshot.last_sample_pts_ms, Some(8));
        assert_eq!(snapshot.last_sample_duration_ms, Some(4));
        assert_eq!(snapshot.last_sample_pts_delta_ms, Some(4));
        assert_eq!(snapshot.last_sample_memory_types, vec!["CUDAMemory"]);
        assert_eq!(snapshot.actual_decoders, vec!["nvh264dec"]);
        assert_eq!(snapshot.decoder_policy_status.as_deref(), Some("Satisfied"));
        assert_eq!(snapshot.caps_memory_features, vec!["memory:CUDAMemory"]);
    }

    #[test]
    fn parses_static_background_hex() {
        assert_eq!(
            native_vulkan_parse_background(Some("#102030")),
            image::Rgba([0x10, 0x20, 0x30, 255])
        );
        assert_eq!(
            native_vulkan_parse_background(Some("bad")),
            image::Rgba([0, 0, 0, 255])
        );
    }

    #[test]
    fn encodes_bgra_swapchain_pixels() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 4]));

        assert_eq!(
            native_vulkan_encode_swapchain_pixels(&image, vk::Format::B8G8R8A8_UNORM),
            vec![3, 2, 1, 4]
        );
        assert_eq!(
            native_vulkan_encode_swapchain_pixels(&image, vk::Format::R8G8B8A8_UNORM),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn contain_fit_preserves_letterbox_background() {
        let source = image::RgbaImage::from_pixel(2, 1, image::Rgba([255, 0, 0, 255]));
        let mut canvas = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));

        native_vulkan_blit_fit(&source, &mut canvas, FitMode::Contain);

        assert_eq!(canvas.get_pixel(0, 0), &image::Rgba([0, 0, 0, 255]));
        assert_eq!(canvas.get_pixel(0, 1), &image::Rgba([255, 0, 0, 255]));
        assert_eq!(canvas.get_pixel(3, 2), &image::Rgba([255, 0, 0, 255]));
        assert_eq!(canvas.get_pixel(0, 3), &image::Rgba([0, 0, 0, 255]));
    }

    #[test]
    fn contract_names_required_vulkan_extensions() {
        let contract = backend_contract();

        assert!(
            contract
                .required_instance_extensions
                .contains(&"VK_KHR_wayland_surface")
        );
        assert!(
            contract
                .required_device_extensions
                .contains(&"VK_KHR_swapchain")
        );
        assert!(
            contract
                .required_device_extensions
                .contains(&"VK_EXT_external_memory_dma_buf")
        );
        assert!(
            contract
                .required_device_extensions
                .contains(&"VK_EXT_image_drm_format_modifier")
        );
    }

    #[test]
    fn unknown_surface_extent_is_none() {
        assert_eq!(
            native_vulkan_extent(vk::Extent2D {
                width: u32::MAX,
                height: u32::MAX,
            }),
            None
        );
        assert_eq!(
            native_vulkan_extent(vk::Extent2D {
                width: 3840,
                height: 2160,
            }),
            Some((3840, 2160))
        );
    }
}
