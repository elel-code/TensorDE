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
use std::thread;
use std::time::{Duration, Instant};

use crate::core::{FitMode, Transition};
use crate::renderer::native_wayland::{
    NativeWaylandError, NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};
use crate::renderer::{
    SceneLiteDisplayPlan, SceneLiteWallpaperPlan, SlideshowWallpaperPlan, StaticRenderSyncPlan,
    StaticWallpaperPlan, VideoWallpaperPlan,
};
use ash::vk;

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
    pub render_item: NativeVulkanRenderItem,
    pub last_render_error: Option<String>,
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
        self.record_clear_command(command_buffer, image_index)?;

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

    fn record_clear_command(
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

            let clear_color = vk::ClearColorValue::from(self.clear_color);
            self.device.cmd_clear_color_image(
                command_buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &clear_color,
                &[range],
            );

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
}

impl Drop for NativeVulkanSession {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
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
            current_renderer_status: "render item mapped; clear pass placeholder until image upload lands",
            target_vulkan_path: "decode image -> Vulkan image -> fit-aware textured fullscreen pass",
        },
        NativeVulkanWallpaperTypeSupport {
            wallpaper_type: NativeVulkanWallpaperType::Video,
            current_vulkan_item: true,
            current_renderer_status: "render item mapped; video texture import still experimental",
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
        target_max_fps: Option<u32>,
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
        renderer_status: "planned-static-texture-upload",
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
        target_max_fps: plan.target_max_fps,
        renderer_status: "planned-video-dmabuf-yuv-sampling",
    }
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
