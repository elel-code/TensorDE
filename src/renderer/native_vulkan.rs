//! Hand-rolled Vulkan renderer spike.
//!
//! This module is intentionally separate from the existing wgpu path. The first
//! step is a concrete backend contract: native Wayland layer-shell ownership,
//! Vulkan surface/swapchain ownership, and direct video texture interop are
//! represented here before any default renderer switch is attempted.

#![allow(unsafe_code)]

use serde::Serialize;
use std::ffi::{CStr, CString};
use std::fmt;
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
    pub frames_received: u64,
    pub frames_imported: u64,
    pub rendered_placeholder_frames: u64,
    pub poster_upload_bytes: Option<u64>,
    pub last_sample_caps: Option<String>,
    pub last_sample_format: Option<String>,
    pub last_sample_size: Option<(u32, u32)>,
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
    frames_received: u64,
    last_sample_caps: Option<String>,
    last_sample_format: Option<String>,
    last_sample_size: Option<(u32, u32)>,
    last_sample_memory_types: Vec<String>,
    actual_decoders: Vec<String>,
    decoder_policy_status: Option<String>,
    caps_report_count: usize,
    caps_memory_features: Vec<String>,
    caps_reports: Vec<NativeVulkanVideoCapsSnapshot>,
    last_error: Option<String>,
}

pub struct NativeVulkanSession {
    host: NativeWaylandHost,
    _entry: ash::Entry,
    instance: ash::Instance,
    surface_loader: ash::khr::surface::Instance,
    _wayland_surface_loader: ash::khr::wayland_surface::Instance,
    surface: vk::SurfaceKHR,
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
    swapchain_image_layouts: Vec<vk::ImageLayout>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    in_flight: vk::Fence,
    static_upload: Option<NativeVulkanStaticImageUpload>,
    #[cfg(feature = "native-vulkan-gst-video")]
    video_frontend: Option<NativeVulkanGstVideoFrontend>,
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
        let priorities = [1.0_f32];
        let queue_create_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(selection.queue_family_index)
            .queue_priorities(&priorities);
        let queue_create_infos = [queue_create_info];
        let device_extensions = [ash::khr::swapchain::NAME.as_ptr()];
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

        Ok(Self {
            host,
            _entry: entry,
            instance,
            surface_loader,
            _wayland_surface_loader: wayland_surface_loader,
            surface,
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
            swapchain_images,
            command_pool,
            command_buffers,
            image_available,
            render_finished,
            in_flight,
            static_upload,
            #[cfg(feature = "native-vulkan-gst-video")]
            video_frontend,
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
        let fences = [self.in_flight];
        unsafe {
            self.device
                .wait_for_fences(&fences, true, u64::MAX)
                .map_err(|result| NativeVulkanError::Vulkan {
                    operation: "vkWaitForFences",
                    result,
                })?;
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
        let wait_stages = [vk::PipelineStageFlags::TRANSFER];
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

    fn record_frame_command(
        &mut self,
        command_buffer: vk::CommandBuffer,
        image_index: usize,
    ) -> Result<(), NativeVulkanError> {
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

    fn poll_video_frontend(&mut self) -> Result<(), NativeVulkanError> {
        #[cfg(feature = "native-vulkan-gst-video")]
        if let Some(frontend) = self.video_frontend.as_mut() {
            frontend.poll()?;
        }
        Ok(())
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    fn video_frontend_snapshot(&self) -> Option<NativeVulkanGstVideoFrontendSnapshot> {
        self.video_frontend
            .as_ref()
            .map(NativeVulkanGstVideoFrontend::snapshot)
    }

    #[cfg(not(feature = "native-vulkan-gst-video"))]
    fn video_frontend_snapshot(&self) -> Option<NativeVulkanGstVideoFrontendSnapshot> {
        None
    }
}

impl Drop for NativeVulkanSession {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            if let Some(static_upload) = self.static_upload.take() {
                static_upload.destroy(&self.device);
            }
            self.device.destroy_fence(self.in_flight, None);
            self.device.destroy_semaphore(self.render_finished, None);
            self.device.destroy_semaphore(self.image_available, None);
            self.device.destroy_command_pool(self.command_pool, None);
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
    frames_received: u64,
    last_sample_caps: Option<String>,
    last_sample_format: Option<String>,
    last_sample_size: Option<(u32, u32)>,
    last_sample_memory_types: Vec<String>,
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
            .set_state(gst::State::Playing)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        if *start_offset_ms > 0 {
            pipeline
                .seek_simple(
                    gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                    gst::ClockTime::from_mseconds(*start_offset_ms),
                )
                .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        }

        Ok(Self {
            pipeline: pipeline.upcast::<gst::Element>(),
            sink,
            bus,
            loop_playback: *loop_playback,
            decoder_policy: *decoder_policy,
            eos_messages: 0,
            frames_received: 0,
            last_sample_caps: None,
            last_sample_format: None,
            last_sample_size: None,
            last_sample_memory_types: Vec::new(),
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
                        self.pipeline
                            .seek_simple(gst::SeekFlags::FLUSH, gst::ClockTime::ZERO)
                            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
                        self.pipeline
                            .set_state(gst::State::Playing)
                            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
                    } else {
                        self.pipeline
                            .set_state(gst::State::Paused)
                            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
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
        for _ in 0..4 {
            let sample = self
                .sink
                .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&0u64]);
            let Some(sample) = sample else {
                break;
            };
            self.record_sample(&sample);
        }
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
            .map(native_vulkan_gst_memory_types)
            .unwrap_or_default();
        self.last_error = None;
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
            frames_received: self.frames_received,
            last_sample_caps: self.last_sample_caps.clone(),
            last_sample_format: self.last_sample_format.clone(),
            last_sample_size: self.last_sample_size,
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
fn native_vulkan_configure_queue(queue: &gst::Element) {
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", 4u32);
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 25_000_000u64);
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
        sink.set_property("max-buffers", 2u32);
    }
    if sink.find_property("drop").is_some() {
        sink.set_property("drop", true);
    }
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
    logical_size: (u32, u32),
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
    let extent = choose_native_vulkan_swapchain_extent(&capabilities, logical_size)?;
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
        .image_usage(vk::ImageUsageFlags::TRANSFER_DST)
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
            current_renderer_status: "video render item runs through native Vulkan lifecycle; optional GStreamer appsink frontend records GPU-memory sample handoff; texture import still experimental",
            target_vulkan_path: "GStreamer decode -> importable DMABuf/EGLImage/Vulkan image -> YUV sampling",
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
    let received_placeholder_frames = rendered_frames.saturating_sub(frames_received);

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
        texture_import_status: "not-importing-yet",
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
        frames_received,
        frames_imported: 0,
        rendered_placeholder_frames: received_placeholder_frames,
        poster_upload_bytes,
        last_sample_caps: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_caps.clone()),
        last_sample_format: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_format.clone()),
        last_sample_size: frontend
            .as_ref()
            .and_then(|frontend| frontend.last_sample_size),
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

        let snapshot = native_vulkan_video_runtime_snapshot(&item, None, 9, Some(1024))
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
        assert_eq!(snapshot.start_offset_ms, 1500);
        assert_eq!(snapshot.gst_state, None);
        assert_eq!(snapshot.decoder_policy_status, None);
        assert_eq!(snapshot.caps_report_count, 0);
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
            frames_received: 3,
            last_sample_caps: Some("video/x-raw, format=(string)NV12".to_owned()),
            last_sample_format: Some("NV12".to_owned()),
            last_sample_size: Some((3840, 2160)),
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

        let snapshot =
            native_vulkan_video_runtime_snapshot(&item, Some(frontend), 12, None).unwrap();

        assert_eq!(snapshot.frontend, "gstreamer-appsink");
        assert_eq!(snapshot.frontend_status, "appsink-receiving-samples");
        assert_eq!(snapshot.handoff_status, "appsink-sample-handoff-active");
        assert_eq!(snapshot.frames_received, 3);
        assert_eq!(snapshot.rendered_placeholder_frames, 9);
        assert_eq!(snapshot.last_sample_format.as_deref(), Some("NV12"));
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
