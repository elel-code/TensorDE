//! Native Wayland layer-shell renderer backed by wgpu/Vulkan.
//!
//! Wayland owns only the layer-shell surface. Buffer allocation, swapchain
//! lifetime, and presentation are delegated to wgpu's Vulkan surface path
//! instead of manually creating and attaching linux-dmabuf buffers per frame.

use super::native_wayland::{
    NativeWaylandError, NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandLayer,
    NativeWaylandOutputSnapshot,
};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use serde::Serialize;
use std::{
    fmt,
    str::FromStr,
    time::{Duration, Instant},
};

#[cfg(any(feature = "native-wgpu-gst-dmabuf", feature = "native-wgpu-gpu-video"))]
use std::sync::Arc;

#[cfg(feature = "native-wgpu-gst-dmabuf")]
use std::{
    ffi::{CString, c_void},
    os::{
        fd::{BorrowedFd, FromRawFd, IntoRawFd, OwnedFd},
        raw::c_char,
    },
    ptr,
    ptr::NonNull,
};

#[cfg(feature = "native-wgpu-gpu-video")]
use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek, SeekFrom},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
};

#[cfg(feature = "video-renderer")]
use gst::prelude::*;
#[cfg(feature = "video-renderer")]
use gstreamer as gst;
#[cfg(feature = "video-renderer")]
use gstreamer_video as gst_video;

#[derive(Debug, Clone, PartialEq)]
pub struct NativeWgpuOptions {
    pub namespace: String,
    pub layer: NativeWaylandLayer,
    pub output_name: Option<String>,
    pub initial_color: NativeWgpuColor,
    pub render_mode: NativeWgpuRenderMode,
}

impl Default for NativeWgpuOptions {
    fn default() -> Self {
        Self {
            namespace: "gilder-wallpaper-native-wgpu".to_owned(),
            layer: NativeWaylandLayer::Bottom,
            output_name: None,
            initial_color: NativeWgpuColor::default(),
            render_mode: NativeWgpuRenderMode::Solid,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NativeWgpuColor {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

impl Default for NativeWgpuColor {
    fn default() -> Self {
        Self {
            red: 0.03,
            green: 0.04,
            blue: 0.06,
            alpha: 1.0,
        }
    }
}

impl NativeWgpuColor {
    fn as_wgpu(self) -> wgpu::Color {
        wgpu::Color {
            r: self.red,
            g: self.green,
            b: self.blue,
            a: self.alpha,
        }
    }

    fn blend(self, other: Self, amount: f64) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        Self {
            red: blend_channel(self.red, other.red, amount),
            green: blend_channel(self.green, other.green, amount),
            blue: blend_channel(self.blue, other.blue, amount),
            alpha: blend_channel(self.alpha, other.alpha, amount),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeWgpuRenderMode {
    Solid,
    Pulse,
}

impl NativeWgpuRenderMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Pulse => "pulse",
        }
    }
}

impl FromStr for NativeWgpuRenderMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "solid" => Ok(Self::Solid),
            "pulse" => Ok(Self::Pulse),
            other => Err(format!("unsupported native wgpu render mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWgpuCapabilities {
    pub built: bool,
    pub backend_policy: &'static str,
    pub layer_shell: bool,
    pub raw_wayland_handles: bool,
    pub wgpu_surface_swapchain: bool,
    pub manual_linux_dmabuf_attach: bool,
    pub intended_video_path: &'static str,
    pub unsafe_policy: &'static str,
}

pub fn capabilities() -> NativeWgpuCapabilities {
    NativeWgpuCapabilities {
        built: true,
        backend_policy: "wgpu Backends::VULKAN",
        layer_shell: true,
        raw_wayland_handles: true,
        wgpu_surface_swapchain: true,
        manual_linux_dmabuf_attach: false,
        intended_video_path: "decode to GPU/external image, composite through Vulkan, present through wgpu surface",
        unsafe_policy: "unsafe covers raw Wayland handles plus Vulkan/CUDA/GStreamer FFI for direct GPU video",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWgpuRuntimeSnapshot {
    pub runtime_elapsed_ms: u64,
    pub configured: bool,
    pub layer: NativeWaylandLayer,
    pub render_mode: NativeWgpuRenderMode,
    pub requested_output_name: Option<String>,
    pub selected_output: Option<NativeWaylandOutputSnapshot>,
    pub known_outputs: Vec<NativeWaylandOutputSnapshot>,
    pub surface_logical_size: Option<(u32, u32)>,
    pub surface_config_size: Option<(u32, u32)>,
    pub surface_format: Option<String>,
    pub present_mode: Option<String>,
    pub render_calls: u64,
    pub frames_rendered: u64,
    pub frames_skipped: u64,
    pub average_render_fps: f64,
    pub render_duration_us_avg: Option<u64>,
    pub render_duration_us_max: Option<u64>,
    pub last_render_duration_us: Option<u64>,
    pub surface_suboptimal_frames: u64,
    pub surface_lost_skips: u64,
    pub surface_outdated_skips: u64,
    pub surface_timeout_skips: u64,
    pub surface_occluded_skips: u64,
    pub surface_validation_skips: u64,
    pub last_render_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeWgpuError {
    Wayland(String),
    Timeout(String),
    Wgpu(String),
    Video(String),
}

impl fmt::Display for NativeWgpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(err) => write!(f, "wayland error: {err}"),
            Self::Timeout(err) => write!(f, "timeout: {err}"),
            Self::Wgpu(err) => write!(f, "wgpu error: {err}"),
            Self::Video(err) => write!(f, "video error: {err}"),
        }
    }
}

impl std::error::Error for NativeWgpuError {}

impl From<NativeWaylandError> for NativeWgpuError {
    fn from(value: NativeWaylandError) -> Self {
        match value {
            NativeWaylandError::Timeout(err) => Self::Timeout(err),
            other => Self::Wayland(other.to_string()),
        }
    }
}

pub struct NativeWgpuSession {
    renderer: NativeWgpuSurfaceRenderer,
    host: NativeWaylandHost,
    layer: NativeWaylandLayer,
    requested_output_name: Option<String>,
    started: Instant,
}

impl NativeWgpuSession {
    pub fn connect(options: NativeWgpuOptions) -> Result<Self, NativeWgpuError> {
        let mut host = NativeWaylandHost::connect(NativeWaylandHostOptions {
            namespace: options.namespace,
            layer: options.layer,
            output_name: options.output_name.clone(),
            opaque_region: true,
            input_passthrough: true,
            attach_parent_mapping_buffer: false,
        })?;
        host.wait_until_configured(8)?;
        let handles = host.surface_handles()?;
        let raw_display_handle =
            RawDisplayHandle::Wayland(WaylandDisplayHandle::new(handles.display));
        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(handles.surface));

        let renderer = pollster::block_on(NativeWgpuSurfaceRenderer::new(
            raw_display_handle,
            raw_window_handle,
            handles.logical_size,
            options.initial_color,
            options.render_mode,
        ))?;

        Ok(Self {
            renderer,
            host,
            layer: options.layer,
            requested_output_name: options.output_name,
            started: Instant::now(),
        })
    }

    pub fn tick(&mut self) -> Result<(), NativeWgpuError> {
        self.host.pump_events()?;
        if let Some(size) = self.host.logical_size() {
            self.renderer.resize(size);
        }
        self.renderer.render()?;
        Ok(())
    }

    pub fn run_for(
        &mut self,
        duration: Duration,
        target_fps: Option<u32>,
    ) -> Result<(), NativeWgpuError> {
        let started = Instant::now();
        let frame_interval = target_fps
            .filter(|fps| *fps > 0)
            .map(|fps| Duration::from_secs_f64(1.0 / f64::from(fps)));
        let mut next_frame_deadline = started;
        while started.elapsed() < duration && !self.host.is_closed() {
            self.tick()?;
            if let Some(interval) = frame_interval {
                next_frame_deadline = next_frame_deadline
                    .checked_add(interval)
                    .unwrap_or_else(Instant::now);
                let now = Instant::now();
                if let Some(remaining) = next_frame_deadline.checked_duration_since(now) {
                    std::thread::sleep(remaining);
                } else if now.duration_since(next_frame_deadline) > interval {
                    next_frame_deadline = now;
                }
            }
        }
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.host.is_closed()
    }

    pub fn snapshot(&self) -> NativeWgpuRuntimeSnapshot {
        let surface = self.host.snapshot();
        let elapsed_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        NativeWgpuRuntimeSnapshot {
            runtime_elapsed_ms: elapsed_ms,
            configured: surface.configured,
            layer: self.layer,
            render_mode: self.renderer.render_mode,
            requested_output_name: self.requested_output_name.clone(),
            selected_output: surface.selected_output,
            known_outputs: surface.known_outputs,
            surface_logical_size: surface.logical_size,
            surface_config_size: Some((self.renderer.config.width, self.renderer.config.height)),
            surface_format: Some(format!("{:?}", self.renderer.config.format)),
            present_mode: Some(format!("{:?}", self.renderer.config.present_mode)),
            render_calls: self.renderer.render_calls,
            frames_rendered: self.renderer.frames_rendered,
            frames_skipped: self.renderer.frames_skipped,
            average_render_fps: average_fps(self.renderer.frames_rendered, self.started.elapsed()),
            render_duration_us_avg: self.renderer.render_duration_us_avg(),
            render_duration_us_max: self.renderer.render_duration_us_max(),
            last_render_duration_us: self.renderer.last_render_duration_us,
            surface_suboptimal_frames: self.renderer.surface_suboptimal_frames,
            surface_lost_skips: self.renderer.surface_lost_skips,
            surface_outdated_skips: self.renderer.surface_outdated_skips,
            surface_timeout_skips: self.renderer.surface_timeout_skips,
            surface_occluded_skips: self.renderer.surface_occluded_skips,
            surface_validation_skips: self.renderer.surface_validation_skips,
            last_render_error: self.renderer.last_render_error.clone(),
        }
    }
}

#[cfg(feature = "native-wgpu-gpu-video")]
#[derive(Debug, Clone, PartialEq)]
pub struct NativeWgpuGpuVideoOptions {
    pub wayland: NativeWgpuOptions,
    pub source: std::path::PathBuf,
    pub fit: crate::core::FitMode,
    pub loop_playback: bool,
    pub input_mode: NativeWgpuGpuVideoInputMode,
}

#[cfg(feature = "native-wgpu-gpu-video")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeWgpuGpuVideoInputMode {
    AnnexBFile,
    GstH264ByteStream,
}

#[cfg(feature = "native-wgpu-gpu-video")]
impl NativeWgpuGpuVideoInputMode {
    fn backend_label(self) -> &'static str {
        match self {
            Self::AnnexBFile => "gpu-video",
            Self::GstH264ByteStream => "gst-gpu-video",
        }
    }
}

#[cfg(feature = "native-wgpu-gpu-video")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWgpuGpuVideoSessionSnapshot {
    pub renderer: NativeWgpuRuntimeSnapshot,
    pub video: NativeWgpuGpuVideoPlayerSnapshot,
}

#[cfg(feature = "native-wgpu-gpu-video")]
pub struct NativeWgpuGpuVideoSession {
    renderer: NativeWgpuSurfaceRenderer,
    player: NativeWgpuGpuVideoPlayer,
    host: NativeWaylandHost,
    layer: NativeWaylandLayer,
    requested_output_name: Option<String>,
    started: Instant,
    _vulkan_instance: Arc<gpu_video::VulkanInstance>,
    _vulkan_device: Arc<gpu_video::VulkanDevice>,
    fit: crate::core::FitMode,
}

#[cfg(feature = "native-wgpu-gpu-video")]
impl NativeWgpuGpuVideoSession {
    #[allow(unsafe_code)]
    pub fn connect(options: NativeWgpuGpuVideoOptions) -> Result<Self, NativeWgpuError> {
        let mut host = NativeWaylandHost::connect(NativeWaylandHostOptions {
            namespace: options.wayland.namespace,
            layer: options.wayland.layer,
            output_name: options.wayland.output_name.clone(),
            opaque_region: true,
            input_passthrough: true,
            attach_parent_mapping_buffer: false,
        })?;
        host.wait_until_configured(8)?;
        let handles = host.surface_handles()?;
        let raw_display_handle =
            RawDisplayHandle::Wayland(WaylandDisplayHandle::new(handles.display));
        let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(handles.surface));

        let vulkan_instance = gpu_video::VulkanInstance::new()
            .map_err(|err| NativeWgpuError::Wgpu(err.to_string()))?;
        let wgpu_instance = vulkan_instance.wgpu_instance();

        // SAFETY: both temporary and retained surfaces use raw Wayland handles
        // owned by NativeWaylandHost. The retained surface is stored in renderer,
        // which is dropped before host. The temporary surface is only used to
        // filter for a Vulkan Video adapter that can present to this wl_surface.
        let compatibility_surface = unsafe {
            wgpu_instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display_handle),
                raw_window_handle,
            })
        }
        .map_err(|err| NativeWgpuError::Wgpu(err.to_string()))?;
        let vulkan_adapter = vulkan_instance
            .create_adapter(&gpu_video::parameters::VulkanAdapterDescriptor {
                supports_decoding: true,
                supports_encoding: false,
                compatible_surface: Some(&compatibility_surface),
            })
            .map_err(|err| NativeWgpuError::Wgpu(err.to_string()))?;
        drop(compatibility_surface);

        let vulkan_device = vulkan_adapter
            .create_device(&gpu_video::parameters::VulkanDeviceDescriptor {
                wgpu_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .map_err(|err| NativeWgpuError::Wgpu(err.to_string()))?;
        let adapter = vulkan_device.wgpu_adapter();
        let device = vulkan_device.wgpu_device();
        let queue = vulkan_device.wgpu_queue();

        // SAFETY: see the compatibility surface above. This is the surface used
        // by wgpu for swapchain presentation for the lifetime of the renderer.
        let surface: wgpu::Surface<'static> = unsafe {
            wgpu_instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display_handle),
                raw_window_handle,
            })
        }
        .map_err(|err| NativeWgpuError::Wgpu(err.to_string()))?;

        let renderer = NativeWgpuSurfaceRenderer::from_wgpu_surface(
            surface,
            adapter,
            device,
            queue,
            handles.logical_size,
            options.wayland.initial_color,
            options.wayland.render_mode,
        )?;
        let player = NativeWgpuGpuVideoPlayer::new(
            &options.source,
            options.loop_playback,
            Arc::clone(&vulkan_device),
            options.input_mode,
        )?;

        Ok(Self {
            renderer,
            player,
            host,
            layer: options.wayland.layer,
            requested_output_name: options.wayland.output_name,
            started: Instant::now(),
            _vulkan_instance: vulkan_instance,
            _vulkan_device: vulkan_device,
            fit: options.fit,
        })
    }

    pub fn tick(&mut self) -> Result<(), NativeWgpuError> {
        self.host.pump_events()?;
        if let Some(size) = self.host.logical_size() {
            self.renderer.resize(size);
        }
        if let Some(frame) = self.player.take_next_frame()? {
            match self.renderer.present_gpu_video_frame(frame, self.fit) {
                Ok(report) => self.player.record_present(report),
                Err(err) => {
                    self.player.record_error(err.to_string());
                    return Err(err);
                }
            }
        }
        self.renderer.render()?;
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.host.is_closed()
    }

    pub fn snapshot(&self) -> NativeWgpuGpuVideoSessionSnapshot {
        let surface = self.host.snapshot();
        let elapsed_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        NativeWgpuGpuVideoSessionSnapshot {
            renderer: NativeWgpuRuntimeSnapshot {
                runtime_elapsed_ms: elapsed_ms,
                configured: surface.configured,
                layer: self.layer,
                render_mode: self.renderer.render_mode,
                requested_output_name: self.requested_output_name.clone(),
                selected_output: surface.selected_output,
                known_outputs: surface.known_outputs,
                surface_logical_size: surface.logical_size,
                surface_config_size: Some((
                    self.renderer.config.width,
                    self.renderer.config.height,
                )),
                surface_format: Some(format!("{:?}", self.renderer.config.format)),
                present_mode: Some(format!("{:?}", self.renderer.config.present_mode)),
                render_calls: self.renderer.render_calls,
                frames_rendered: self.renderer.frames_rendered,
                frames_skipped: self.renderer.frames_skipped,
                average_render_fps: average_fps(
                    self.renderer.frames_rendered,
                    self.started.elapsed(),
                ),
                render_duration_us_avg: self.renderer.render_duration_us_avg(),
                render_duration_us_max: self.renderer.render_duration_us_max(),
                last_render_duration_us: self.renderer.last_render_duration_us,
                surface_suboptimal_frames: self.renderer.surface_suboptimal_frames,
                surface_lost_skips: self.renderer.surface_lost_skips,
                surface_outdated_skips: self.renderer.surface_outdated_skips,
                surface_timeout_skips: self.renderer.surface_timeout_skips,
                surface_occluded_skips: self.renderer.surface_occluded_skips,
                surface_validation_skips: self.renderer.surface_validation_skips,
                last_render_error: self.renderer.last_render_error.clone(),
            },
            video: self.player.snapshot(),
        }
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq)]
pub struct NativeWgpuVideoOptions {
    pub wayland: NativeWgpuOptions,
    pub source: std::path::PathBuf,
    pub fit: crate::core::FitMode,
    pub loop_playback: bool,
    pub target_max_fps: Option<u32>,
    pub decoder_policy: crate::config::VideoDecoderPolicy,
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWgpuVideoSessionSnapshot {
    pub renderer: NativeWgpuRuntimeSnapshot,
    pub video: NativeWgpuVideoPlayerSnapshot,
}

#[cfg(feature = "video-renderer")]
pub struct NativeWgpuVideoSession {
    session: NativeWgpuSession,
    player: NativeWgpuVideoPlayer,
    fit: crate::core::FitMode,
}

#[cfg(feature = "video-renderer")]
impl NativeWgpuVideoSession {
    pub fn connect(options: NativeWgpuVideoOptions) -> Result<Self, NativeWgpuError> {
        let mut session = NativeWgpuSession::connect(options.wayland)?;
        let mut player = NativeWgpuVideoPlayer::new(
            &options.source,
            options.loop_playback,
            options.target_max_fps,
            options.decoder_policy,
        )?;
        player.play()?;
        if let Some(sample) = player.pull_latest_sample() {
            let upload = session.renderer.upload_video_sample(&sample, options.fit)?;
            player.record_upload(upload);
        }
        Ok(Self {
            session,
            player,
            fit: options.fit,
        })
    }

    pub fn tick(&mut self) -> Result<(), NativeWgpuError> {
        self.session.host.pump_events()?;
        if let Some(size) = self.session.host.logical_size() {
            self.session.renderer.resize(size);
        }
        self.player.poll_bus()?;
        if let Some(sample) = self.player.pull_latest_sample() {
            match self.session.renderer.upload_video_sample(&sample, self.fit) {
                Ok(upload) => self.player.record_upload(upload),
                Err(err) => {
                    self.player.record_upload_error(err.to_string());
                    return Err(err);
                }
            }
        }
        self.session.renderer.render()?;
        Ok(())
    }

    pub fn run_for(
        &mut self,
        duration: Duration,
        target_fps: Option<u32>,
    ) -> Result<(), NativeWgpuError> {
        let started = Instant::now();
        let frame_interval = target_fps
            .filter(|fps| *fps > 0)
            .map(|fps| Duration::from_secs_f64(1.0 / f64::from(fps)));
        while started.elapsed() < duration && !self.session.is_closed() {
            let frame_started = Instant::now();
            self.tick()?;
            if let Some(interval) = frame_interval
                && let Some(remaining) = interval.checked_sub(frame_started.elapsed())
            {
                std::thread::sleep(remaining);
            }
        }
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.session.is_closed()
    }

    pub fn snapshot(&self) -> NativeWgpuVideoSessionSnapshot {
        NativeWgpuVideoSessionSnapshot {
            renderer: self.session.snapshot(),
            video: self.player.snapshot(),
        }
    }

    pub fn shutdown(&mut self) {
        self.player.shutdown();
    }
}

#[cfg(feature = "video-renderer")]
impl Drop for NativeWgpuVideoSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[derive(Debug, Clone, PartialEq)]
pub struct NativeWgpuGstDmabufOptions {
    pub wayland: NativeWgpuOptions,
    pub source: std::path::PathBuf,
    pub fit: crate::core::FitMode,
    pub loop_playback: bool,
    pub target_max_fps: Option<u32>,
    pub decoder_policy: crate::config::VideoDecoderPolicy,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWgpuGstDmabufSessionSnapshot {
    pub renderer: NativeWgpuRuntimeSnapshot,
    pub video: NativeWgpuGstDmabufPlayerSnapshot,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
pub struct NativeWgpuGstDmabufSession {
    session: NativeWgpuSession,
    player: NativeWgpuGstDmabufPlayer,
    fit: crate::core::FitMode,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl NativeWgpuGstDmabufSession {
    pub fn connect(options: NativeWgpuGstDmabufOptions) -> Result<Self, NativeWgpuError> {
        let mut session = NativeWgpuSession::connect(options.wayland)?;
        let mut player = NativeWgpuGstDmabufPlayer::new(
            &options.source,
            options.loop_playback,
            options.target_max_fps,
            options.decoder_policy,
        )?;
        player.play()?;
        player.try_present_latest_frame(&mut session.renderer, options.fit)?;
        Ok(Self {
            session,
            player,
            fit: options.fit,
        })
    }

    pub fn tick(&mut self) -> Result<(), NativeWgpuError> {
        self.session.host.pump_events()?;
        if let Some(size) = self.session.host.logical_size() {
            self.session.renderer.resize(size);
        }
        self.player.poll_bus()?;
        self.player
            .try_present_latest_frame(&mut self.session.renderer, self.fit)?;
        self.session.renderer.render()?;
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.session.is_closed()
    }

    pub fn snapshot(&self) -> NativeWgpuGstDmabufSessionSnapshot {
        NativeWgpuGstDmabufSessionSnapshot {
            renderer: self.session.snapshot(),
            video: self.player.snapshot(),
        }
    }

    pub fn shutdown(&mut self) {
        self.player.shutdown();
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl Drop for NativeWgpuGstDmabufSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuGstDmabufPlayer {
    pipeline: gst::Element,
    sink: gst::Element,
    bus: gst::Bus,
    loop_playback: bool,
    pulled_samples: u64,
    exported_frames: u64,
    export_failures: u64,
    import_attempts: u64,
    imported_frames: u64,
    import_failures: u64,
    eos_messages: u64,
    last_frame_size: Option<(u32, u32)>,
    last_frame_format: Option<String>,
    last_memory_types: Vec<String>,
    last_export_source: Option<String>,
    last_drm_fourcc: Option<u32>,
    last_drm_modifier: Option<u64>,
    last_plane_offsets: Vec<u32>,
    last_plane_strides: Vec<u32>,
    last_fd_count: Option<usize>,
    last_error: Option<String>,
    pipeline_kind: NativeWgpuGstDmabufPipelineKind,
    _cuda_context: Option<Arc<NativeWgpuCudaContextHandle>>,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWgpuGstDmabufPlayerSnapshot {
    pub backend: &'static str,
    pub pipeline_kind: &'static str,
    pub gst_state: String,
    pub pulled_samples: u64,
    pub exported_frames: u64,
    pub export_failures: u64,
    pub import_attempts: u64,
    pub imported_frames: u64,
    pub import_failures: u64,
    pub eos_messages: u64,
    pub last_frame_size: Option<(u32, u32)>,
    pub last_frame_format: Option<String>,
    pub last_memory_types: Vec<String>,
    pub last_export_source: Option<String>,
    pub last_drm_fourcc: Option<u32>,
    pub last_drm_modifier: Option<u64>,
    pub last_plane_offsets: Vec<u32>,
    pub last_plane_strides: Vec<u32>,
    pub last_fd_count: Option<usize>,
    pub last_error: Option<String>,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl NativeWgpuGstDmabufPlayer {
    fn new(
        source: &std::path::Path,
        loop_playback: bool,
        target_max_fps: Option<u32>,
        decoder_policy: crate::config::VideoDecoderPolicy,
    ) -> Result<Self, NativeWgpuError> {
        gst::init().map_err(|err| NativeWgpuError::Video(err.to_string()))?;
        crate::renderer::video::apply_decoder_rank_policy(decoder_policy);
        let pipeline_kind = native_wgpu_gst_dmabuf_pipeline_kind();
        native_wgpu_check_gst_dmabuf_pipeline_requirements(pipeline_kind)?;
        let cuda_context = if pipeline_kind == NativeWgpuGstDmabufPipelineKind::CudaMmap {
            Some(Arc::new(NativeWgpuCudaContextHandle::new(0)?))
        } else {
            None
        };
        let sink = match pipeline_kind {
            NativeWgpuGstDmabufPipelineKind::CudaMmap => native_wgpu_gst_dmabuf_appsink(
                target_max_fps,
                Arc::clone(cuda_context.as_ref().expect("cuda context must exist")),
            )?,
            NativeWgpuGstDmabufPipelineKind::GlDmabuf => {
                native_wgpu_gst_dmabuf_gl_appsink(target_max_fps)?
            }
            NativeWgpuGstDmabufPipelineKind::DecoderGlMemoryExport => {
                native_wgpu_gst_gl_memory_appsink(target_max_fps)?
            }
            NativeWgpuGstDmabufPipelineKind::CudaDirect => {
                native_wgpu_gst_cuda_memory_appsink(target_max_fps)?
            }
            NativeWgpuGstDmabufPipelineKind::CudaNv12Upload => {
                native_wgpu_gst_nv12_upload_appsink(target_max_fps)?
            }
        };
        let pipeline = match pipeline_kind {
            NativeWgpuGstDmabufPipelineKind::CudaMmap => {
                native_wgpu_gst_dmabuf_pipeline(source, &sink)?
            }
            NativeWgpuGstDmabufPipelineKind::GlDmabuf => {
                native_wgpu_gst_gl_dmabuf_pipeline(source, &sink)?
            }
            NativeWgpuGstDmabufPipelineKind::DecoderGlMemoryExport => {
                native_wgpu_gst_decoder_glmemory_pipeline(source, &sink)?
            }
            NativeWgpuGstDmabufPipelineKind::CudaDirect => {
                native_wgpu_gst_cuda_direct_pipeline(source, &sink)?
            }
            NativeWgpuGstDmabufPipelineKind::CudaNv12Upload => {
                native_wgpu_gst_cuda_nv12_upload_pipeline(source, &sink)?
            }
        };
        if let Some(cuda_context) = cuda_context.as_ref() {
            pipeline.set_context(&cuda_context.gst_context()?);
        }
        crate::renderer::video::configure_video_pipeline_low_memory(&pipeline);
        let bus = pipeline
            .bus()
            .ok_or_else(|| NativeWgpuError::Video("gst-dmabuf pipeline has no bus".to_owned()))?;
        Ok(Self {
            pipeline,
            sink,
            bus,
            loop_playback,
            pulled_samples: 0,
            exported_frames: 0,
            export_failures: 0,
            import_attempts: 0,
            imported_frames: 0,
            import_failures: 0,
            eos_messages: 0,
            last_frame_size: None,
            last_frame_format: None,
            last_memory_types: Vec::new(),
            last_export_source: None,
            last_drm_fourcc: None,
            last_drm_modifier: None,
            last_plane_offsets: Vec::new(),
            last_plane_strides: Vec::new(),
            last_fd_count: None,
            last_error: None,
            pipeline_kind,
            _cuda_context: cuda_context,
        })
    }

    fn play(&mut self) -> Result<(), NativeWgpuError> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
        Ok(())
    }

    fn shutdown(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        let _ = self.pipeline.state(gst::ClockTime::from_mseconds(500));
    }

    fn poll_bus(&mut self) -> Result<(), NativeWgpuError> {
        while let Some(message) = self.bus.pop() {
            match message.view() {
                gst::MessageView::Eos(_) => {
                    self.eos_messages = self.eos_messages.saturating_add(1);
                    if self.loop_playback {
                        self.pipeline
                            .seek_simple(
                                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                                gst::ClockTime::ZERO,
                            )
                            .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
                        self.play()?;
                        return Ok(());
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
                    return Err(NativeWgpuError::Video(message));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn try_present_latest_frame(
        &mut self,
        renderer: &mut NativeWgpuSurfaceRenderer,
        fit: crate::core::FitMode,
    ) -> Result<(), NativeWgpuError> {
        match self.pipeline_kind {
            NativeWgpuGstDmabufPipelineKind::CudaMmap
            | NativeWgpuGstDmabufPipelineKind::GlDmabuf
            | NativeWgpuGstDmabufPipelineKind::DecoderGlMemoryExport => {
                if let Some(frame) = self.pull_latest_dmabuf_frame()? {
                    self.record_export(&frame);
                    match renderer.present_gst_dmabuf_frame(frame, fit) {
                        Ok(report) => self.record_import(report),
                        Err(err) => {
                            self.record_import_error(err.to_string());
                            return Err(err);
                        }
                    }
                }
            }
            NativeWgpuGstDmabufPipelineKind::CudaDirect => {
                if let Some(sample) = self.pull_next_sample() {
                    match renderer.present_gst_cuda_direct_sample(&sample, fit) {
                        Ok(report) => {
                            self.record_cuda_direct_upload(&sample, report);
                        }
                        Err(err) => {
                            self.record_import_error(err.to_string());
                            return Err(err);
                        }
                    }
                }
            }
            NativeWgpuGstDmabufPipelineKind::CudaNv12Upload => {
                if let Some(sample) = self.pull_next_sample() {
                    match renderer.present_gst_system_nv12_sample(&sample, fit) {
                        Ok(report) => {
                            self.record_system_nv12_upload(&sample, report);
                        }
                        Err(err) => {
                            self.record_import_error(err.to_string());
                            return Err(err);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn pull_next_sample(&mut self) -> Option<gst::Sample> {
        self.pull_next_sample_with_timeout(1_000_000)
    }

    fn pull_next_sample_with_timeout(&mut self, timeout_ns: u64) -> Option<gst::Sample> {
        let sample = self
            .sink
            .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&timeout_ns]);
        if sample.is_some() {
            self.pulled_samples = self.pulled_samples.saturating_add(1);
        }
        sample
    }

    fn pull_latest_dmabuf_frame(
        &mut self,
    ) -> Result<Option<NativeWgpuGstDmabufFrame>, NativeWgpuError> {
        let Some(sample) = self.pull_next_sample() else {
            return Ok(None);
        };
        match native_wgpu_gst_dmabuf_frame_from_sample(&sample) {
            Ok(frame) => Ok(Some(frame)),
            Err(err) => {
                self.export_failures = self.export_failures.saturating_add(1);
                if let Some(buffer) = sample.buffer() {
                    self.last_memory_types = native_wgpu_gst_memory_types(buffer);
                    if let Ok(meta) = native_wgpu_gst_dmabuf_meta(sample.caps(), buffer) {
                        self.last_frame_size = Some((meta.width, meta.height));
                        self.last_frame_format = Some(meta.caps_format);
                        self.last_drm_fourcc = meta.drm_fourcc;
                        self.last_drm_modifier = meta.drm_modifier;
                    }
                }
                self.last_error = Some(err);
                Ok(None)
            }
        }
    }

    fn record_export(&mut self, frame: &NativeWgpuGstDmabufFrame) {
        self.exported_frames = self.exported_frames.saturating_add(1);
        self.last_frame_size = Some((frame.width, frame.height));
        self.last_frame_format = Some(frame.caps_format.clone());
        self.last_memory_types = frame.memory_types.clone();
        self.last_export_source = Some(frame.export_source.to_owned());
        self.last_drm_fourcc = Some(frame.format);
        self.last_drm_modifier = frame.modifier;
        self.last_plane_offsets = frame.planes.iter().map(|plane| plane.offset).collect();
        self.last_plane_strides = frame.planes.iter().map(|plane| plane.stride).collect();
        self.last_fd_count = Some(frame.fds.len());
        self.last_error = None;
    }

    fn record_system_nv12_upload(
        &mut self,
        sample: &gst::Sample,
        report: NativeWgpuGstDmabufImportReport,
    ) {
        self.exported_frames = self.exported_frames.saturating_add(1);
        self.import_attempts = self.import_attempts.saturating_add(1);
        self.imported_frames = report.imported_frames;
        if let Some(buffer) = sample.buffer() {
            self.last_memory_types = native_wgpu_gst_memory_types(buffer);
            if let Ok(meta) = native_wgpu_gst_dmabuf_meta(sample.caps(), buffer) {
                self.last_frame_size = Some((meta.width, meta.height));
                self.last_frame_format = Some(meta.caps_format);
                self.last_drm_fourcc = meta.drm_fourcc;
                self.last_drm_modifier = meta.drm_modifier;
                self.last_plane_offsets = meta
                    .offsets
                    .iter()
                    .take(2)
                    .filter_map(|offset| u32::try_from(*offset).ok())
                    .collect();
                self.last_plane_strides = meta
                    .strides
                    .iter()
                    .take(2)
                    .filter_map(|stride| u32::try_from(*stride).ok())
                    .collect();
            }
        }
        self.last_export_source = Some("system-nv12-upload".to_owned());
        self.last_fd_count = Some(0);
        self.last_error = None;
    }

    fn record_cuda_direct_upload(
        &mut self,
        sample: &gst::Sample,
        report: NativeWgpuGstDmabufImportReport,
    ) {
        self.exported_frames = self.exported_frames.saturating_add(1);
        self.import_attempts = self.import_attempts.saturating_add(1);
        self.imported_frames = report.imported_frames;
        if let Some(buffer) = sample.buffer() {
            self.last_memory_types = native_wgpu_gst_memory_types(buffer);
            if let Ok(meta) = native_wgpu_gst_dmabuf_meta(sample.caps(), buffer) {
                self.last_frame_size = Some((meta.width, meta.height));
                self.last_frame_format = Some(meta.caps_format);
                self.last_drm_fourcc = meta.drm_fourcc;
                self.last_drm_modifier = meta.drm_modifier;
                self.last_plane_offsets = meta
                    .offsets
                    .iter()
                    .take(2)
                    .filter_map(|offset| u32::try_from(*offset).ok())
                    .collect();
                self.last_plane_strides = meta
                    .strides
                    .iter()
                    .take(2)
                    .filter_map(|stride| u32::try_from(*stride).ok())
                    .collect();
            }
        }
        self.last_export_source = Some("cuda-direct-vulkan-staging".to_owned());
        self.last_fd_count = Some(1);
        self.last_error = None;
    }

    fn record_import(&mut self, report: NativeWgpuGstDmabufImportReport) {
        self.import_attempts = self.import_attempts.saturating_add(1);
        self.imported_frames = report.imported_frames;
        self.last_error = None;
    }

    fn record_import_error(&mut self, error: String) {
        self.import_attempts = self.import_attempts.saturating_add(1);
        self.import_failures = self.import_failures.saturating_add(1);
        self.last_error = Some(error);
    }

    fn snapshot(&self) -> NativeWgpuGstDmabufPlayerSnapshot {
        let state = self
            .pipeline
            .state(gst::ClockTime::ZERO)
            .1
            .name()
            .to_string();
        NativeWgpuGstDmabufPlayerSnapshot {
            backend: "gst-dmabuf",
            pipeline_kind: self.pipeline_kind.as_str(),
            gst_state: state,
            pulled_samples: self.pulled_samples,
            exported_frames: self.exported_frames,
            export_failures: self.export_failures,
            import_attempts: self.import_attempts,
            imported_frames: self.imported_frames,
            import_failures: self.import_failures,
            eos_messages: self.eos_messages,
            last_frame_size: self.last_frame_size,
            last_frame_format: self.last_frame_format.clone(),
            last_memory_types: self.last_memory_types.clone(),
            last_export_source: self.last_export_source.clone(),
            last_drm_fourcc: self.last_drm_fourcc,
            last_drm_modifier: self.last_drm_modifier,
            last_plane_offsets: self.last_plane_offsets.clone(),
            last_plane_strides: self.last_plane_strides.clone(),
            last_fd_count: self.last_fd_count,
            last_error: self.last_error.clone(),
        }
    }
}

#[cfg(feature = "video-renderer")]
struct NativeWgpuVideoPlayer {
    pipeline: gst::Element,
    sink: gst::Element,
    bus: gst::Bus,
    loop_playback: bool,
    pulled_samples: u64,
    uploaded_frames: u64,
    eos_messages: u64,
    last_frame_size: Option<(u32, u32)>,
    last_frame_format: Option<String>,
    last_source_stride: Option<u32>,
    last_upload_stride: Option<u32>,
    last_error: Option<String>,
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWgpuVideoPlayerSnapshot {
    pub gst_state: String,
    pub pulled_samples: u64,
    pub uploaded_frames: u64,
    pub eos_messages: u64,
    pub last_frame_size: Option<(u32, u32)>,
    pub last_frame_format: Option<String>,
    pub last_source_stride: Option<u32>,
    pub last_upload_stride: Option<u32>,
    pub last_error: Option<String>,
}

#[cfg(feature = "video-renderer")]
impl NativeWgpuVideoPlayer {
    fn new(
        source: &std::path::Path,
        loop_playback: bool,
        target_max_fps: Option<u32>,
        decoder_policy: crate::config::VideoDecoderPolicy,
    ) -> Result<Self, NativeWgpuError> {
        gst::init().map_err(|err| NativeWgpuError::Video(err.to_string()))?;
        crate::renderer::video::apply_decoder_rank_policy(decoder_policy);
        let sink = native_wgpu_appsink(target_max_fps)?;
        let pipeline = native_wgpu_video_pipeline(source, &sink)?;
        crate::renderer::video::configure_video_pipeline_low_memory(&pipeline);
        let bus = pipeline
            .bus()
            .ok_or_else(|| NativeWgpuError::Video("video pipeline has no bus".to_owned()))?;
        Ok(Self {
            pipeline,
            sink,
            bus,
            loop_playback,
            pulled_samples: 0,
            uploaded_frames: 0,
            eos_messages: 0,
            last_frame_size: None,
            last_frame_format: None,
            last_source_stride: None,
            last_upload_stride: None,
            last_error: None,
        })
    }

    fn play(&mut self) -> Result<(), NativeWgpuError> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
        Ok(())
    }

    fn shutdown(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        let _ = self.pipeline.state(gst::ClockTime::from_mseconds(500));
    }

    fn poll_bus(&mut self) -> Result<(), NativeWgpuError> {
        while let Some(message) = self.bus.pop() {
            match message.view() {
                gst::MessageView::Eos(_) => {
                    self.eos_messages = self.eos_messages.saturating_add(1);
                    if self.loop_playback {
                        self.pipeline
                            .seek_simple(gst::SeekFlags::FLUSH, gst::ClockTime::ZERO)
                            .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
                        self.play()?;
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
                    return Err(NativeWgpuError::Video(message));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn pull_latest_sample(&mut self) -> Option<gst::Sample> {
        let mut latest_sample = None;
        let mut pulled_samples = 0u64;
        let mut timeout_ns = 1_000_000u64;
        loop {
            let sample = self
                .sink
                .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&timeout_ns]);
            let Some(sample) = sample else {
                break;
            };
            timeout_ns = 0;
            pulled_samples = pulled_samples.saturating_add(1);
            latest_sample = Some(sample);
        }
        self.pulled_samples = self.pulled_samples.saturating_add(pulled_samples);
        latest_sample
    }

    fn record_upload(&mut self, upload: NativeWgpuVideoUploadReport) {
        self.uploaded_frames = upload.uploaded_frames;
        self.last_frame_size = Some((upload.width, upload.height));
        self.last_frame_format = Some(upload.format);
        self.last_source_stride = Some(upload.source_stride);
        self.last_upload_stride = Some(upload.upload_stride);
        self.last_error = None;
    }

    fn record_upload_error(&mut self, error: String) {
        self.last_error = Some(error);
    }

    fn snapshot(&self) -> NativeWgpuVideoPlayerSnapshot {
        let state = self
            .pipeline
            .state(gst::ClockTime::ZERO)
            .1
            .name()
            .to_string();
        NativeWgpuVideoPlayerSnapshot {
            gst_state: state,
            pulled_samples: self.pulled_samples,
            uploaded_frames: self.uploaded_frames,
            eos_messages: self.eos_messages,
            last_frame_size: self.last_frame_size,
            last_frame_format: self.last_frame_format.clone(),
            last_source_stride: self.last_source_stride,
            last_upload_stride: self.last_upload_stride,
            last_error: self.last_error.clone(),
        }
    }
}

#[cfg(feature = "video-renderer")]
fn native_wgpu_appsink(target_max_fps: Option<u32>) -> Result<gst::Element, NativeWgpuError> {
    let sink = gst::ElementFactory::make("appsink")
        .property("sync", true)
        .property("async", false)
        .property("emit-signals", false)
        .property("enable-last-sample", false)
        .property("wait-on-eos", false)
        .property("max-buffers", 2u32)
        .property_from_str("leaky-type", "downstream")
        .build()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let _ = crate::renderer::video::configure_video_sink_low_memory(&sink, target_max_fps);
    if sink.find_property("qos").is_some() {
        sink.set_property("qos", false);
    }
    if sink.find_property("max-lateness").is_some() {
        sink.set_property("max-lateness", -1i64);
    }
    Ok(sink)
}

#[cfg(feature = "video-renderer")]
fn native_wgpu_video_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
) -> Result<gst::Element, NativeWgpuError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_wgpu_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let decodebin = native_wgpu_gst_element("decodebin")?;
    let queue = native_wgpu_gst_element("queue")?;
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", 2u32);
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 25_000_000u64);
    }
    if queue.find_property("leaky").is_some() {
        queue.set_property_from_str("leaky", "downstream");
    }
    let videoconvert = native_wgpu_gst_element("videoconvert")?;
    let capsfilter = native_wgpu_gst_element("capsfilter")?;
    let caps = "video/x-raw,format=RGBA"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    capsfilter.set_property("caps", &caps);

    pipeline
        .add_many([
            &filesrc,
            &decodebin,
            &queue,
            &videoconvert,
            &capsfilter,
            sink,
        ])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    filesrc
        .link(&decodebin)
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    gst::Element::link_many([&queue, &videoconvert, &capsfilter, sink])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeWgpuError::Video("queue has no sink pad".to_owned()))?;
    decodebin.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() || !native_wgpu_pad_is_video(pad) {
            return;
        }
        let _ = pad.link(&queue_sink);
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_check_gst_dmabuf_pipeline_requirements(
    pipeline_kind: NativeWgpuGstDmabufPipelineKind,
) -> Result<(), NativeWgpuError> {
    if matches!(
        pipeline_kind,
        NativeWgpuGstDmabufPipelineKind::GlDmabuf
            | NativeWgpuGstDmabufPipelineKind::DecoderGlMemoryExport
    ) && !native_wgpu_system_library_available(b"libnvrtc.so\0")
    {
        return Err(NativeWgpuError::Video(
            "GStreamer nvcodec GLMemory interop requires libnvrtc.so; this optional path is unavailable without an NVRTC runtime package".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_system_library_available(name: &[u8]) -> bool {
    if name.last().copied() != Some(0) {
        return false;
    }
    let handle = unsafe { libc::dlopen(name.as_ptr().cast(), libc::RTLD_LAZY) };
    if handle.is_null() {
        return false;
    }
    unsafe {
        libc::dlclose(handle);
    }
    true
}

#[cfg(feature = "video-renderer")]
fn native_wgpu_gst_element(name: &str) -> Result<gst::Element, NativeWgpuError> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))
}

#[cfg(feature = "video-renderer")]
fn native_wgpu_pad_is_video(pad: &gst::Pad) -> bool {
    if pad.name().starts_with("video") {
        return true;
    }
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    caps.structure(0)
        .map(|structure| structure.name().starts_with("video/"))
        .unwrap_or(false)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeWgpuGstDmabufPipelineKind {
    CudaMmap,
    GlDmabuf,
    DecoderGlMemoryExport,
    CudaDirect,
    CudaNv12Upload,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl NativeWgpuGstDmabufPipelineKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::CudaMmap => "cuda-mmap",
            Self::GlDmabuf => "gl-dmabuf",
            Self::DecoderGlMemoryExport => "decoder-glmemory",
            Self::CudaDirect => "cuda-direct",
            Self::CudaNv12Upload => "cuda-nv12-upload",
        }
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_dmabuf_pipeline_kind() -> NativeWgpuGstDmabufPipelineKind {
    let value = std::env::var("GILDER_WGPU_GST_DMABUF_PIPELINE").unwrap_or_default();
    match value.trim() {
        "" | "cuda-direct" | "direct" | "nvdec-direct" | "cudamemory-direct" => {
            NativeWgpuGstDmabufPipelineKind::CudaDirect
        }
        "cuda-mmap" | "mmap" | "legacy-mmap" => NativeWgpuGstDmabufPipelineKind::CudaMmap,
        "gl" | "gl-dmabuf" | "gldownload" => NativeWgpuGstDmabufPipelineKind::GlDmabuf,
        "decoder-glmemory" | "decoder-gl-memory" | "nvdec-glmemory" | "nvdec-gl-memory"
        | "direct-glmemory" | "direct-gl-memory" => {
            NativeWgpuGstDmabufPipelineKind::DecoderGlMemoryExport
        }
        "nv12-upload" | "cuda-nv12-upload" | "cudadownload" => {
            NativeWgpuGstDmabufPipelineKind::CudaNv12Upload
        }
        _ => NativeWgpuGstDmabufPipelineKind::CudaDirect,
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_dmabuf_queue_frames() -> u32 {
    std::env::var("GILDER_WGPU_GST_DMABUF_QUEUE_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .map(|value| value.clamp(2, 16))
        .unwrap_or(2)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_dmabuf_appsink(
    target_max_fps: Option<u32>,
    cuda_context: Arc<NativeWgpuCudaContextHandle>,
) -> Result<gst::Element, NativeWgpuError> {
    let caps = "video/x-raw(memory:CUDAMemory),format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let sink = gst::ElementFactory::make("appsink")
        .property("sync", true)
        .property("async", false)
        .property("emit-signals", false)
        .property("enable-last-sample", false)
        .property("wait-on-eos", false)
        .property("max-buffers", native_wgpu_gst_dmabuf_queue_frames())
        .property("caps", &caps)
        .build()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let _ = crate::renderer::video::configure_video_sink_low_memory(&sink, target_max_fps);
    if sink.find_property("qos").is_some() {
        sink.set_property("qos", false);
    }
    if sink.find_property("max-lateness").is_some() {
        sink.set_property("max-lateness", -1i64);
    }
    install_native_wgpu_cuda_mmap_allocation_probe(&sink, cuda_context)?;
    Ok(sink)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_cuda_memory_appsink(
    target_max_fps: Option<u32>,
) -> Result<gst::Element, NativeWgpuError> {
    let caps = "video/x-raw(memory:CUDAMemory),format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let sink = gst::ElementFactory::make("appsink")
        .property("sync", true)
        .property("async", false)
        .property("emit-signals", false)
        .property("enable-last-sample", false)
        .property("wait-on-eos", false)
        .property("max-buffers", native_wgpu_gst_dmabuf_queue_frames())
        .property("caps", &caps)
        .build()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let _ = crate::renderer::video::configure_video_sink_low_memory(&sink, target_max_fps);
    if sink.find_property("qos").is_some() {
        sink.set_property("qos", false);
    }
    if sink.find_property("max-lateness").is_some() {
        sink.set_property("max-lateness", -1i64);
    }
    Ok(sink)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_dmabuf_gl_appsink(
    target_max_fps: Option<u32>,
) -> Result<gst::Element, NativeWgpuError> {
    let caps = "video/x-raw(memory:DMABuf),format=DMA_DRM"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let sink = gst::ElementFactory::make("appsink")
        .property("sync", true)
        .property("async", false)
        .property("emit-signals", false)
        .property("enable-last-sample", false)
        .property("wait-on-eos", false)
        .property("max-buffers", native_wgpu_gst_dmabuf_queue_frames())
        .property("caps", &caps)
        .build()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let _ = crate::renderer::video::configure_video_sink_low_memory(&sink, target_max_fps);
    if sink.find_property("qos").is_some() {
        sink.set_property("qos", false);
    }
    if sink.find_property("max-lateness").is_some() {
        sink.set_property("max-lateness", -1i64);
    }
    Ok(sink)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_gl_memory_appsink(
    target_max_fps: Option<u32>,
) -> Result<gst::Element, NativeWgpuError> {
    let caps = "video/x-raw(memory:GLMemory),format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let sink = gst::ElementFactory::make("appsink")
        .property("sync", true)
        .property("async", false)
        .property("emit-signals", false)
        .property("enable-last-sample", false)
        .property("wait-on-eos", false)
        .property("max-buffers", native_wgpu_gst_dmabuf_queue_frames())
        .property("caps", &caps)
        .build()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let _ = crate::renderer::video::configure_video_sink_low_memory(&sink, target_max_fps);
    if sink.find_property("qos").is_some() {
        sink.set_property("qos", false);
    }
    if sink.find_property("max-lateness").is_some() {
        sink.set_property("max-lateness", -1i64);
    }
    Ok(sink)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_nv12_upload_appsink(
    target_max_fps: Option<u32>,
) -> Result<gst::Element, NativeWgpuError> {
    let caps = "video/x-raw,format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let sink = gst::ElementFactory::make("appsink")
        .property("sync", true)
        .property("async", false)
        .property("emit-signals", false)
        .property("enable-last-sample", false)
        .property("wait-on-eos", false)
        .property("max-buffers", native_wgpu_gst_dmabuf_queue_frames())
        .property("caps", &caps)
        .build()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    let _ = crate::renderer::video::configure_video_sink_low_memory(&sink, target_max_fps);
    if sink.find_property("qos").is_some() {
        sink.set_property("qos", false);
    }
    if sink.find_property("max-lateness").is_some() {
        sink.set_property("max-lateness", -1i64);
    }
    Ok(sink)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_dmabuf_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
) -> Result<gst::Element, NativeWgpuError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_wgpu_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let decodebin = native_wgpu_gst_element("decodebin")?;
    let queue = native_wgpu_gst_element("queue")?;
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", native_wgpu_gst_dmabuf_queue_frames());
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 25_000_000u64);
    }
    let capsfilter = native_wgpu_gst_element("capsfilter")?;
    let caps = "video/x-raw(memory:CUDAMemory),format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    capsfilter.set_property("caps", &caps);

    pipeline
        .add_many([&filesrc, &decodebin, &queue, &capsfilter, sink])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    filesrc
        .link(&decodebin)
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    gst::Element::link_many([&queue, &capsfilter, sink])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeWgpuError::Video("queue has no sink pad".to_owned()))?;
    decodebin.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() || !native_wgpu_pad_is_video(pad) {
            return;
        }
        let _ = pad.link(&queue_sink);
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_gl_dmabuf_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
) -> Result<gst::Element, NativeWgpuError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_wgpu_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_wgpu_gst_element("qtdemux")?;
    let queue = native_wgpu_gst_element("queue")?;
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", native_wgpu_gst_dmabuf_queue_frames());
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 25_000_000u64);
    }
    let h264parse = native_wgpu_gst_element("h264parse")?;
    if h264parse.find_property("config-interval").is_some() {
        h264parse.set_property("config-interval", -1i32);
    }
    let decoder = native_wgpu_gst_element("nvh264dec")?;
    if decoder.find_property("qos").is_some() {
        decoder.set_property("qos", false);
    }
    if decoder.find_property("num-output-surfaces").is_some() {
        decoder.set_property("num-output-surfaces", 4u32);
    }
    let gl_nv12_capsfilter = native_wgpu_gst_element("capsfilter")?;
    let gl_nv12_caps = "video/x-raw(memory:GLMemory),format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    gl_nv12_capsfilter.set_property("caps", &gl_nv12_caps);
    let gldownload = native_wgpu_gst_element("gldownload")?;
    if gldownload.find_property("qos").is_some() {
        gldownload.set_property("qos", false);
    }
    let dmabuf_capsfilter = native_wgpu_gst_element("capsfilter")?;
    let dmabuf_caps = "video/x-raw(memory:DMABuf),format=DMA_DRM"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    dmabuf_capsfilter.set_property("caps", &dmabuf_caps);

    pipeline
        .add_many([
            &filesrc,
            &demux,
            &queue,
            &h264parse,
            &decoder,
            &gl_nv12_capsfilter,
            &gldownload,
            &dmabuf_capsfilter,
            sink,
        ])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    gst::Element::link_many([
        &queue,
        &h264parse,
        &decoder,
        &gl_nv12_capsfilter,
        &gldownload,
        &dmabuf_capsfilter,
        sink,
    ])
    .map_err(|err| NativeWgpuError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeWgpuError::Video("queue has no sink pad".to_owned()))?;
    demux.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() || !native_wgpu_pad_is_video(pad) {
            return;
        }
        let _ = pad.link(&queue_sink);
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_decoder_glmemory_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
) -> Result<gst::Element, NativeWgpuError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_wgpu_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_wgpu_gst_element("qtdemux")?;
    let queue = native_wgpu_gst_element("queue")?;
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", native_wgpu_gst_dmabuf_queue_frames());
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 25_000_000u64);
    }
    let h264parse = native_wgpu_gst_element("h264parse")?;
    if h264parse.find_property("config-interval").is_some() {
        h264parse.set_property("config-interval", -1i32);
    }
    let decoder = native_wgpu_gst_element("nvh264dec")?;
    if decoder.find_property("qos").is_some() {
        decoder.set_property("qos", false);
    }
    if decoder.find_property("num-output-surfaces").is_some() {
        decoder.set_property("num-output-surfaces", 4u32);
    }
    let gl_nv12_capsfilter = native_wgpu_gst_element("capsfilter")?;
    let gl_nv12_caps = "video/x-raw(memory:GLMemory),format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    gl_nv12_capsfilter.set_property("caps", &gl_nv12_caps);

    pipeline
        .add_many([
            &filesrc,
            &demux,
            &queue,
            &h264parse,
            &decoder,
            &gl_nv12_capsfilter,
            sink,
        ])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    gst::Element::link_many([&queue, &h264parse, &decoder, &gl_nv12_capsfilter, sink])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeWgpuError::Video("queue has no sink pad".to_owned()))?;
    demux.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() || !native_wgpu_pad_is_video(pad) {
            return;
        }
        let _ = pad.link(&queue_sink);
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_cuda_direct_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
) -> Result<gst::Element, NativeWgpuError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_wgpu_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_wgpu_gst_element("qtdemux")?;
    let queue = native_wgpu_gst_element("queue")?;
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", native_wgpu_gst_dmabuf_queue_frames());
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 25_000_000u64);
    }
    let h264parse = native_wgpu_gst_element("h264parse")?;
    if h264parse.find_property("config-interval").is_some() {
        h264parse.set_property("config-interval", -1i32);
    }
    let decoder = native_wgpu_gst_element("nvh264dec")?;
    if decoder.find_property("qos").is_some() {
        decoder.set_property("qos", false);
    }
    if decoder.find_property("num-output-surfaces").is_some() {
        decoder.set_property("num-output-surfaces", 4u32);
    }
    let cuda_capsfilter = native_wgpu_gst_element("capsfilter")?;
    let cuda_caps = "video/x-raw(memory:CUDAMemory),format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    cuda_capsfilter.set_property("caps", &cuda_caps);

    pipeline
        .add_many([
            &filesrc,
            &demux,
            &queue,
            &h264parse,
            &decoder,
            &cuda_capsfilter,
            sink,
        ])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    gst::Element::link_many([&queue, &h264parse, &decoder, &cuda_capsfilter, sink])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeWgpuError::Video("queue has no sink pad".to_owned()))?;
    demux.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() || !native_wgpu_pad_is_video(pad) {
            return;
        }
        let _ = pad.link(&queue_sink);
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_cuda_nv12_upload_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
) -> Result<gst::Element, NativeWgpuError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_wgpu_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_wgpu_gst_element("qtdemux")?;
    let queue = native_wgpu_gst_element("queue")?;
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", native_wgpu_gst_dmabuf_queue_frames());
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 25_000_000u64);
    }
    let h264parse = native_wgpu_gst_element("h264parse")?;
    if h264parse.find_property("config-interval").is_some() {
        h264parse.set_property("config-interval", -1i32);
    }
    let decoder = native_wgpu_gst_element("nvh264dec")?;
    if decoder.find_property("qos").is_some() {
        decoder.set_property("qos", false);
    }
    if decoder.find_property("num-output-surfaces").is_some() {
        decoder.set_property("num-output-surfaces", 4u32);
    }
    let cudadownload = native_wgpu_gst_element("cudadownload")?;
    let capsfilter = native_wgpu_gst_element("capsfilter")?;
    let caps = "video/x-raw,format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    capsfilter.set_property("caps", &caps);

    pipeline
        .add_many([
            &filesrc,
            &demux,
            &queue,
            &h264parse,
            &decoder,
            &cudadownload,
            &capsfilter,
            sink,
        ])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    gst::Element::link_many([
        &queue,
        &h264parse,
        &decoder,
        &cudadownload,
        &capsfilter,
        sink,
    ])
    .map_err(|err| NativeWgpuError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeWgpuError::Video("queue has no sink pad".to_owned()))?;
    demux.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() || !native_wgpu_pad_is_video(pad) {
            return;
        }
        let _ = pad.link(&queue_sink);
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuCudaContextHandle {
    ptr: NonNull<NativeWgpuGstCudaContext>,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl NativeWgpuCudaContextHandle {
    fn new(device_id: u32) -> Result<Self, NativeWgpuError> {
        let loaded = unsafe { gst_cuda_load_library() } != gst::glib::ffi::GFALSE;
        if !loaded {
            return Err(NativeWgpuError::Video(
                "failed to load CUDA library for gst-dmabuf shared CUDA context".to_owned(),
            ));
        }
        let ptr = unsafe { gst_cuda_context_new(device_id) };
        let ptr = NonNull::new(ptr).ok_or_else(|| {
            NativeWgpuError::Video(format!(
                "failed to create gst-dmabuf CUDA context for device {device_id}"
            ))
        })?;
        Ok(Self { ptr })
    }

    fn as_ptr(&self) -> *mut NativeWgpuGstCudaContext {
        self.ptr.as_ptr()
    }

    fn gst_context(&self) -> Result<gst::Context, NativeWgpuError> {
        let context = unsafe { gst_context_new_cuda_context(self.as_ptr()) };
        if context.is_null() {
            return Err(NativeWgpuError::Video(
                "failed to create GstContext for gst-dmabuf CUDA context".to_owned(),
            ));
        }
        Ok(unsafe { gst::glib::translate::from_glib_full(context) })
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl Drop for NativeWgpuCudaContextHandle {
    fn drop(&mut self) {
        unsafe {
            gst::ffi::gst_object_unref(self.ptr.as_ptr().cast::<gst::ffi::GstObject>());
        }
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
unsafe impl Send for NativeWgpuCudaContextHandle {}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
unsafe impl Sync for NativeWgpuCudaContextHandle {}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn install_native_wgpu_cuda_mmap_allocation_probe(
    sink: &gst::Element,
    cuda_context: Arc<NativeWgpuCudaContextHandle>,
) -> Result<(), NativeWgpuError> {
    sink.set_property("emit-signals", true);

    let pad = sink
        .static_pad("sink")
        .ok_or_else(|| NativeWgpuError::Video("appsink has no sink pad".to_owned()))?;
    let pad_cuda_context = Arc::clone(&cuda_context);
    let _pad_probe_id = pad.add_probe(gst::PadProbeType::QUERY_DOWNSTREAM, move |_, info| {
        let Some(query) = info.query_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        if query.type_() != gst::QueryType::Allocation {
            return gst::PadProbeReturn::Ok;
        }
        unsafe {
            let _ = native_wgpu_propose_cuda_mmap_allocation(
                query.as_mut_ptr(),
                pad_cuda_context.as_ptr(),
            );
        }
        gst::PadProbeReturn::Ok
    });

    let signal_cuda_context = Arc::clone(&cuda_context);
    let _handler_id = sink.connect("propose-allocation", false, move |values| {
        let handled = values
            .get(1)
            .and_then(|value| value.get::<&gst::QueryRef>().ok())
            .map(|query| unsafe {
                native_wgpu_propose_cuda_mmap_allocation(
                    query.as_mut_ptr(),
                    signal_cuda_context.as_ptr(),
                )
            })
            .unwrap_or(false);
        Some(handled.to_value())
    });
    Ok(())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
unsafe fn native_wgpu_propose_cuda_mmap_allocation(
    query: *mut gst::ffi::GstQuery,
    cuda_context: *mut NativeWgpuGstCudaContext,
) -> bool {
    if query.is_null() || unsafe { (*query).type_ } != gst::ffi::GST_QUERY_ALLOCATION {
        return false;
    }

    let mut caps = ptr::null_mut();
    let mut need_pool = gst::glib::ffi::GFALSE;
    unsafe {
        gst::ffi::gst_query_parse_allocation(query, &mut caps, &mut need_pool);
    }
    if caps.is_null() || cuda_context.is_null() {
        return false;
    }

    let video_info = unsafe { gst_video::ffi::gst_video_info_new() };
    if video_info.is_null() {
        return false;
    }
    let parsed_video_info = unsafe { gst_video::ffi::gst_video_info_from_caps(video_info, caps) }
        != gst::glib::ffi::GFALSE;
    if !parsed_video_info {
        unsafe {
            gst_video::ffi::gst_video_info_free(video_info);
        }
        return false;
    }
    let pool_size = unsafe { (*video_info).size };
    unsafe {
        gst_video::ffi::gst_video_info_free(video_info);
    }
    let Ok(pool_size_u32) = u32::try_from(pool_size) else {
        return false;
    };

    let pool = unsafe { gst_cuda_buffer_pool_new(cuda_context) };
    if pool.is_null() {
        return false;
    }
    let config = unsafe { gst::ffi::gst_buffer_pool_get_config(pool) };
    if config.is_null() {
        unsafe {
            gst::ffi::gst_object_unref(pool.cast::<gst::ffi::GstObject>());
        }
        return false;
    }

    const MIN_BUFFERS: u32 = 2;
    const MAX_BUFFERS: u32 = 3;
    unsafe {
        gst::ffi::gst_buffer_pool_config_set_params(
            config,
            caps,
            pool_size_u32,
            MIN_BUFFERS,
            MAX_BUFFERS,
        );
        gst_buffer_pool_config_set_cuda_alloc_method(config, GST_CUDA_MEMORY_ALLOC_MMAP);
    }
    let configured =
        unsafe { gst::ffi::gst_buffer_pool_set_config(pool, config) } != gst::glib::ffi::GFALSE;
    if !configured {
        unsafe {
            gst::ffi::gst_object_unref(pool.cast::<gst::ffi::GstObject>());
        }
        return false;
    }

    unsafe {
        gst::ffi::gst_query_add_allocation_pool(
            query,
            pool,
            pool_size_u32,
            MIN_BUFFERS,
            MAX_BUFFERS,
        );
        gst::ffi::gst_object_unref(pool.cast::<gst::ffi::GstObject>());
    }
    true
}

#[cfg(feature = "native-wgpu-gpu-video")]
struct NativeWgpuGpuVideoPlayer {
    backend: &'static str,
    events: Receiver<NativeWgpuGpuVideoDecoderEvent>,
    stop_worker: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    pending_frames: VecDeque<wgpu::Texture>,
    loop_playback: bool,
    reached_eof: bool,
    decoded_frames: u64,
    presented_frames: u64,
    bytes_read: u64,
    eos_messages: u64,
    decoder_resets: u64,
    last_frame_size: Option<(u32, u32)>,
    last_frame_format: Option<String>,
    last_error: Option<String>,
}

#[cfg(feature = "native-wgpu-gpu-video")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWgpuGpuVideoPlayerSnapshot {
    pub backend: &'static str,
    pub state: &'static str,
    pub decoded_frames: u64,
    pub presented_frames: u64,
    pub pending_frames: usize,
    pub bytes_read: u64,
    pub eos_messages: u64,
    pub decoder_resets: u64,
    pub last_frame_size: Option<(u32, u32)>,
    pub last_frame_format: Option<String>,
    pub last_error: Option<String>,
}

#[cfg(feature = "native-wgpu-gpu-video")]
impl NativeWgpuGpuVideoPlayer {
    fn new(
        source: &std::path::Path,
        loop_playback: bool,
        vulkan_device: Arc<gpu_video::VulkanDevice>,
        input_mode: NativeWgpuGpuVideoInputMode,
    ) -> Result<Self, NativeWgpuError> {
        if input_mode == NativeWgpuGpuVideoInputMode::AnnexBFile
            && !native_wgpu_is_annex_b_h264(source)
        {
            return Err(NativeWgpuError::Video(format!(
                "gpu-video backend expects Annex-B H.264 bytestream (.h264/.264), got {}",
                source.display()
            )));
        }
        File::open(source)
            .map_err(|err| NativeWgpuError::Video(format!("open {}: {err}", source.display())))?;
        let (event_tx, events) = sync_channel(NATIVE_WGPU_GPU_VIDEO_CHANNEL_FRAMES);
        let stop_worker = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop_worker);
        let worker_source = source.to_owned();
        let worker = thread::Builder::new()
            .name("gilder-gpu-video-decode".to_owned())
            .spawn(move || {
                native_wgpu_gpu_video_decode_worker(
                    worker_source,
                    loop_playback,
                    vulkan_device,
                    event_tx,
                    worker_stop,
                    input_mode,
                );
            })
            .map_err(|err| NativeWgpuError::Video(format!("spawn gpu-video worker: {err}")))?;
        Ok(Self {
            backend: input_mode.backend_label(),
            events,
            stop_worker,
            worker: Some(worker),
            pending_frames: VecDeque::new(),
            loop_playback,
            reached_eof: false,
            decoded_frames: 0,
            presented_frames: 0,
            bytes_read: 0,
            eos_messages: 0,
            decoder_resets: 0,
            last_frame_size: None,
            last_frame_format: None,
            last_error: None,
        })
    }

    fn take_next_frame(&mut self) -> Result<Option<wgpu::Texture>, NativeWgpuError> {
        self.drain_decoder_events()?;
        Ok(self.pending_frames.pop_front())
    }

    fn drain_decoder_events(&mut self) -> Result<(), NativeWgpuError> {
        let mut events = 0usize;
        while self.pending_frames.len() < NATIVE_WGPU_GPU_VIDEO_TARGET_PENDING_FRAMES
            && events < NATIVE_WGPU_GPU_VIDEO_MAX_EVENTS_PER_TICK
        {
            match self.events.try_recv() {
                Ok(NativeWgpuGpuVideoDecoderEvent::Frame(frame)) => {
                    self.decoded_frames = self.decoded_frames.saturating_add(1);
                    self.pending_frames.push_back(frame);
                    self.reached_eof = false;
                    self.last_error = None;
                }
                Ok(NativeWgpuGpuVideoDecoderEvent::BytesRead(bytes)) => {
                    self.bytes_read = self.bytes_read.saturating_add(bytes);
                }
                Ok(NativeWgpuGpuVideoDecoderEvent::Eos) => {
                    self.eos_messages = self.eos_messages.saturating_add(1);
                    self.reached_eof = true;
                }
                Ok(NativeWgpuGpuVideoDecoderEvent::Reset) => {
                    self.decoder_resets = self.decoder_resets.saturating_add(1);
                    self.reached_eof = false;
                }
                Ok(NativeWgpuGpuVideoDecoderEvent::Error(error)) => {
                    self.last_error = Some(error.clone());
                    return Err(NativeWgpuError::Video(error));
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.pending_frames.is_empty() && !self.reached_eof {
                        let error = "gpu-video decoder worker disconnected".to_owned();
                        self.last_error = Some(error.clone());
                        return Err(NativeWgpuError::Video(error));
                    }
                    break;
                }
            }
            events += 1;
        }
        Ok(())
    }

    fn record_present(&mut self, report: NativeWgpuNv12PresentReport) {
        self.presented_frames = report.presented_frames;
        self.last_frame_size = Some((report.width, report.height));
        self.last_frame_format = Some(report.format);
        self.last_error = None;
    }

    fn record_error(&mut self, error: String) {
        self.last_error = Some(error);
    }

    fn snapshot(&self) -> NativeWgpuGpuVideoPlayerSnapshot {
        NativeWgpuGpuVideoPlayerSnapshot {
            backend: self.backend,
            state: if self.reached_eof {
                if self.loop_playback {
                    "loop-boundary"
                } else {
                    "eos"
                }
            } else {
                "decoding"
            },
            decoded_frames: self.decoded_frames,
            presented_frames: self.presented_frames,
            pending_frames: self.pending_frames.len(),
            bytes_read: self.bytes_read,
            eos_messages: self.eos_messages,
            decoder_resets: self.decoder_resets,
            last_frame_size: self.last_frame_size,
            last_frame_format: self.last_frame_format.clone(),
            last_error: self.last_error.clone(),
        }
    }
}

#[cfg(feature = "native-wgpu-gpu-video")]
impl Drop for NativeWgpuGpuVideoPlayer {
    fn drop(&mut self) {
        self.stop_worker.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(feature = "native-wgpu-gpu-video")]
enum NativeWgpuGpuVideoDecoderEvent {
    Frame(wgpu::Texture),
    BytesRead(u64),
    Eos,
    Reset,
    Error(String),
}

#[cfg(feature = "native-wgpu-gpu-video")]
fn native_wgpu_gpu_video_decode_worker(
    source: std::path::PathBuf,
    loop_playback: bool,
    vulkan_device: Arc<gpu_video::VulkanDevice>,
    event_tx: SyncSender<NativeWgpuGpuVideoDecoderEvent>,
    stop: Arc<AtomicBool>,
    input_mode: NativeWgpuGpuVideoInputMode,
) {
    let result = match input_mode {
        NativeWgpuGpuVideoInputMode::AnnexBFile => native_wgpu_gpu_video_annex_b_decode_loop(
            source,
            loop_playback,
            vulkan_device,
            &event_tx,
            &stop,
        ),
        NativeWgpuGpuVideoInputMode::GstH264ByteStream => {
            native_wgpu_gpu_video_gst_h264_decode_loop(
                source,
                loop_playback,
                vulkan_device,
                &event_tx,
                &stop,
            )
        }
    };
    if let Err(err) = result {
        let _ = native_wgpu_gpu_video_send_event(
            &event_tx,
            &stop,
            NativeWgpuGpuVideoDecoderEvent::Error(err),
        );
    }
}

#[cfg(feature = "native-wgpu-gpu-video")]
fn native_wgpu_gpu_video_annex_b_decode_loop(
    source: std::path::PathBuf,
    loop_playback: bool,
    vulkan_device: Arc<gpu_video::VulkanDevice>,
    event_tx: &SyncSender<NativeWgpuGpuVideoDecoderEvent>,
    stop: &AtomicBool,
) -> Result<(), String> {
    let mut file =
        File::open(&source).map_err(|err| format!("open {}: {err}", source.display()))?;
    let mut decoder = vulkan_device
        .create_wgpu_textures_decoder_h264(gpu_video::parameters::DecoderParameters::default())
        .map_err(|err| err.to_string())?;
    let mut read_buffer = vec![0; NATIVE_WGPU_GPU_VIDEO_CHUNK_SIZE];

    while !stop.load(Ordering::Relaxed) {
        let bytes = file
            .read(&mut read_buffer)
            .map_err(|err| format!("read {}: {err}", source.display()))?;
        if bytes == 0 {
            native_wgpu_gpu_video_send_event(event_tx, stop, NativeWgpuGpuVideoDecoderEvent::Eos);
            let decoded = decoder.flush().map_err(|err| err.to_string())?;
            native_wgpu_gpu_video_send_frames(event_tx, stop, decoded);
            if !loop_playback || stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            file.seek(SeekFrom::Start(0))
                .map_err(|err| format!("seek {}: {err}", source.display()))?;
            decoder = vulkan_device
                .create_wgpu_textures_decoder_h264(
                    gpu_video::parameters::DecoderParameters::default(),
                )
                .map_err(|err| err.to_string())?;
            native_wgpu_gpu_video_send_event(event_tx, stop, NativeWgpuGpuVideoDecoderEvent::Reset);
            continue;
        }

        native_wgpu_gpu_video_send_event(
            event_tx,
            stop,
            NativeWgpuGpuVideoDecoderEvent::BytesRead(u64::try_from(bytes).unwrap_or(u64::MAX)),
        );
        let decoded = decoder
            .decode(gpu_video::EncodedInputChunk {
                data: &read_buffer[..bytes],
                pts: None,
            })
            .map_err(|err| err.to_string())?;
        native_wgpu_gpu_video_send_frames(event_tx, stop, decoded);
    }

    Ok(())
}

#[cfg(all(feature = "native-wgpu-gpu-video", feature = "video-renderer"))]
fn native_wgpu_gpu_video_gst_h264_decode_loop(
    source: std::path::PathBuf,
    loop_playback: bool,
    vulkan_device: Arc<gpu_video::VulkanDevice>,
    event_tx: &SyncSender<NativeWgpuGpuVideoDecoderEvent>,
    stop: &AtomicBool,
) -> Result<(), String> {
    gst::init().map_err(|err| err.to_string())?;
    let sink = native_wgpu_gst_h264_bytestream_appsink().map_err(|err| err.to_string())?;
    let pipeline =
        native_wgpu_gst_h264_bytestream_pipeline(&source, &sink).map_err(|err| err.to_string())?;
    let bus = pipeline
        .bus()
        .ok_or_else(|| "gst-gpu-video pipeline has no bus".to_owned())?;
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|err| err.to_string())?;

    let mut decoder = vulkan_device
        .create_wgpu_textures_decoder_h264(gpu_video::parameters::DecoderParameters::default())
        .map_err(|err| err.to_string())?;

    while !stop.load(Ordering::Relaxed) {
        while let Some(message) = bus.pop() {
            match message.view() {
                gst::MessageView::Eos(_) => {
                    native_wgpu_gpu_video_send_event(
                        event_tx,
                        stop,
                        NativeWgpuGpuVideoDecoderEvent::Eos,
                    );
                    let decoded = decoder.flush().map_err(|err| err.to_string())?;
                    native_wgpu_gpu_video_send_frames(event_tx, stop, decoded);
                    if !loop_playback || stop.load(Ordering::Relaxed) {
                        let _ = pipeline.set_state(gst::State::Null);
                        return Ok(());
                    }
                    pipeline
                        .seek_simple(gst::SeekFlags::FLUSH, gst::ClockTime::ZERO)
                        .map_err(|err| err.to_string())?;
                    pipeline
                        .set_state(gst::State::Playing)
                        .map_err(|err| err.to_string())?;
                    decoder = vulkan_device
                        .create_wgpu_textures_decoder_h264(
                            gpu_video::parameters::DecoderParameters::default(),
                        )
                        .map_err(|err| err.to_string())?;
                    native_wgpu_gpu_video_send_event(
                        event_tx,
                        stop,
                        NativeWgpuGpuVideoDecoderEvent::Reset,
                    );
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
                    let _ = pipeline.set_state(gst::State::Null);
                    return Err(message);
                }
                _ => {}
            }
        }

        let sample = sink.emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&5_000_000u64]);
        let Some(sample) = sample else {
            continue;
        };
        let buffer = sample
            .buffer()
            .ok_or_else(|| "gst-gpu-video appsink sample has no buffer".to_owned())?;
        let pts = buffer.pts().map(|pts| pts.nseconds());
        let map = buffer
            .map_readable()
            .map_err(|_| "gst-gpu-video H.264 buffer map_readable failed".to_owned())?;
        let data = map.as_slice();
        native_wgpu_gpu_video_send_event(
            event_tx,
            stop,
            NativeWgpuGpuVideoDecoderEvent::BytesRead(
                u64::try_from(data.len()).unwrap_or(u64::MAX),
            ),
        );
        let decoded = decoder
            .decode(gpu_video::EncodedInputChunk { data, pts })
            .map_err(|err| err.to_string())?;
        native_wgpu_gpu_video_send_frames(event_tx, stop, decoded);
    }

    let _ = pipeline.set_state(gst::State::Null);
    Ok(())
}

#[cfg(all(feature = "native-wgpu-gpu-video", not(feature = "video-renderer")))]
fn native_wgpu_gpu_video_gst_h264_decode_loop(
    source: std::path::PathBuf,
    loop_playback: bool,
    vulkan_device: Arc<gpu_video::VulkanDevice>,
    event_tx: &SyncSender<NativeWgpuGpuVideoDecoderEvent>,
    stop: &AtomicBool,
) -> Result<(), String> {
    let _ = (source, loop_playback, vulkan_device, event_tx, stop);
    Err("gst-gpu-video input requires building with video-renderer".to_owned())
}

#[cfg(all(feature = "native-wgpu-gpu-video", feature = "video-renderer"))]
fn native_wgpu_gst_h264_bytestream_appsink() -> Result<gst::Element, NativeWgpuError> {
    let caps = "video/x-h264,stream-format=byte-stream,alignment=au"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    gst::ElementFactory::make("appsink")
        .property("sync", false)
        .property("async", false)
        .property("emit-signals", false)
        .property("enable-last-sample", false)
        .property("wait-on-eos", false)
        .property("max-buffers", 8u32)
        .property("caps", &caps)
        .build()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))
}

#[cfg(all(feature = "native-wgpu-gpu-video", feature = "video-renderer"))]
fn native_wgpu_gst_h264_bytestream_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
) -> Result<gst::Element, NativeWgpuError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_wgpu_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_wgpu_gst_element("qtdemux")?;
    let queue = native_wgpu_gst_element("queue")?;
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", 8u32);
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 0u64);
    }
    let h264parse = native_wgpu_gst_element("h264parse")?;
    if h264parse.find_property("config-interval").is_some() {
        h264parse.set_property("config-interval", -1i32);
    }
    let capsfilter = native_wgpu_gst_element("capsfilter")?;
    let caps = "video/x-h264,stream-format=byte-stream,alignment=au"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    capsfilter.set_property("caps", &caps);

    pipeline
        .add_many([&filesrc, &demux, &queue, &h264parse, &capsfilter, sink])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;
    gst::Element::link_many([&queue, &h264parse, &capsfilter, sink])
        .map_err(|err| NativeWgpuError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeWgpuError::Video("queue has no sink pad".to_owned()))?;
    demux.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() {
            return;
        }
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let is_h264 = caps
            .structure(0)
            .map(|structure| structure.name() == "video/x-h264")
            .unwrap_or(false);
        if is_h264 {
            let _ = pad.link(&queue_sink);
        }
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "native-wgpu-gpu-video")]
fn native_wgpu_gpu_video_send_frames(
    event_tx: &SyncSender<NativeWgpuGpuVideoDecoderEvent>,
    stop: &AtomicBool,
    frames: Vec<gpu_video::OutputFrame<wgpu::Texture>>,
) {
    for frame in frames {
        if !native_wgpu_gpu_video_send_event(
            event_tx,
            stop,
            NativeWgpuGpuVideoDecoderEvent::Frame(frame.data),
        ) {
            break;
        }
    }
}

#[cfg(feature = "native-wgpu-gpu-video")]
fn native_wgpu_gpu_video_send_event(
    event_tx: &SyncSender<NativeWgpuGpuVideoDecoderEvent>,
    stop: &AtomicBool,
    mut event: NativeWgpuGpuVideoDecoderEvent,
) -> bool {
    while !stop.load(Ordering::Relaxed) {
        match event_tx.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Full(returned)) => {
                event = returned;
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
    false
}

#[cfg(feature = "native-wgpu-gpu-video")]
fn native_wgpu_is_annex_b_h264(source: &std::path::Path) -> bool {
    source
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(|extension| matches!(extension.to_ascii_lowercase().as_str(), "h264" | "264"))
        .unwrap_or(false)
}

#[cfg(feature = "native-wgpu-gpu-video")]
const NATIVE_WGPU_GPU_VIDEO_CHUNK_SIZE: usize = 1024 * 1024;
#[cfg(feature = "native-wgpu-gpu-video")]
const NATIVE_WGPU_GPU_VIDEO_CHANNEL_FRAMES: usize = 16;
#[cfg(feature = "native-wgpu-gpu-video")]
const NATIVE_WGPU_GPU_VIDEO_MAX_EVENTS_PER_TICK: usize = 64;
#[cfg(feature = "native-wgpu-gpu-video")]
const NATIVE_WGPU_GPU_VIDEO_TARGET_PENDING_FRAMES: usize = 8;

struct NativeWgpuSurfaceRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    color: NativeWgpuColor,
    render_mode: NativeWgpuRenderMode,
    started: Instant,
    #[cfg(feature = "video-renderer")]
    video: Option<NativeWgpuVideoRenderer>,
    #[cfg(any(feature = "native-wgpu-gpu-video", feature = "native-wgpu-gst-dmabuf"))]
    gpu_video: Option<NativeWgpuNv12VideoRenderer>,
    render_calls: u64,
    frames_rendered: u64,
    frames_skipped: u64,
    render_duration_us_total: u128,
    render_duration_us_max: u64,
    last_render_duration_us: Option<u64>,
    surface_suboptimal_frames: u64,
    surface_lost_skips: u64,
    surface_outdated_skips: u64,
    surface_timeout_skips: u64,
    surface_occluded_skips: u64,
    surface_validation_skips: u64,
    last_render_error: Option<String>,
}

impl NativeWgpuSurfaceRenderer {
    #[allow(unsafe_code)]
    async fn new(
        raw_display_handle: RawDisplayHandle,
        raw_window_handle: RawWindowHandle,
        size: (u32, u32),
        color: NativeWgpuColor,
        render_mode: NativeWgpuRenderMode,
    ) -> Result<Self, NativeWgpuError> {
        let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        instance_descriptor.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(instance_descriptor);
        // SAFETY: the raw wl_display and wl_surface are owned by
        // NativeWaylandHost and remain valid for the lifetime of this renderer.
        // NativeWgpuSession declares renderer before host, so the wgpu surface
        // is dropped before the Wayland objects it references. The app never
        // creates or attaches per-frame linux-dmabuf wl_buffers on this path.
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display_handle),
                raw_window_handle,
            })
        }
        .map_err(|err| NativeWgpuError::Wgpu(err.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|err| NativeWgpuError::Wgpu(err.to_string()))?;
        let required_features = native_wgpu_required_features(&adapter)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gilder-native-wgpu-device"),
                required_features,
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|err| NativeWgpuError::Wgpu(err.to_string()))?;

        Self::from_wgpu_surface(surface, adapter, device, queue, size, color, render_mode)
    }

    fn from_wgpu_surface(
        surface: wgpu::Surface<'static>,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        size: (u32, u32),
        color: NativeWgpuColor,
        render_mode: NativeWgpuRenderMode,
    ) -> Result<Self, NativeWgpuError> {
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| NativeWgpuError::Wgpu("surface reports no formats".to_owned()))?;
        let present_mode = pick_present_mode(&capabilities.present_modes);
        let alpha_mode = capabilities
            .alpha_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::CompositeAlphaMode::Opaque)
            .or_else(|| capabilities.alpha_modes.first().copied())
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode,
            alpha_mode,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            color,
            render_mode,
            started: Instant::now(),
            #[cfg(feature = "video-renderer")]
            video: None,
            #[cfg(any(feature = "native-wgpu-gpu-video", feature = "native-wgpu-gst-dmabuf"))]
            gpu_video: None,
            render_calls: 0,
            frames_rendered: 0,
            frames_skipped: 0,
            render_duration_us_total: 0,
            render_duration_us_max: 0,
            last_render_duration_us: None,
            surface_suboptimal_frames: 0,
            surface_lost_skips: 0,
            surface_outdated_skips: 0,
            surface_timeout_skips: 0,
            surface_occluded_skips: 0,
            surface_validation_skips: 0,
            last_render_error: None,
        })
    }

    fn resize(&mut self, size: (u32, u32)) {
        let width = size.0.max(1);
        let height = size.1.max(1);
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        #[cfg(feature = "video-renderer")]
        if let Some(video) = self.video.as_mut() {
            video.update_fit_uniform(&self.queue, (self.config.width, self.config.height));
        }
        #[cfg(any(feature = "native-wgpu-gpu-video", feature = "native-wgpu-gst-dmabuf"))]
        if let Some(video) = self.gpu_video.as_mut() {
            video.update_fit_uniform(&self.queue, (self.config.width, self.config.height));
        }
    }

    fn render(&mut self) -> Result<(), NativeWgpuError> {
        let render_started = Instant::now();
        self.render_calls = self.render_calls.saturating_add(1);
        let mut suboptimal = false;
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                suboptimal = true;
                frame
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                self.surface_outdated_skips = self.surface_outdated_skips.saturating_add(1);
                self.record_skip(render_started, "surface_outdated");
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                self.surface_lost_skips = self.surface_lost_skips.saturating_add(1);
                self.record_skip(render_started, "surface_lost");
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                self.surface_timeout_skips = self.surface_timeout_skips.saturating_add(1);
                self.record_skip(render_started, "surface_timeout");
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.surface_occluded_skips = self.surface_occluded_skips.saturating_add(1);
                self.record_skip(render_started, "surface_occluded");
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                self.surface_validation_skips = self.surface_validation_skips.saturating_add(1);
                self.record_skip(render_started, "surface_validation");
                return Ok(());
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gilder-native-wgpu-clear"),
            });
        self.encode_render_pass(&mut encoder, &view);
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        self.frames_rendered = self.frames_rendered.saturating_add(1);
        self.record_render_duration(render_started);
        if suboptimal {
            self.surface_suboptimal_frames = self.surface_suboptimal_frames.saturating_add(1);
            self.surface.configure(&self.device, &self.config);
            self.last_render_error = Some("surface_suboptimal".to_owned());
        } else {
            self.last_render_error = None;
        }
        Ok(())
    }

    fn record_skip(&mut self, started: Instant, reason: &str) {
        self.frames_skipped = self.frames_skipped.saturating_add(1);
        self.record_render_duration(started);
        self.last_render_error = Some(reason.to_owned());
    }

    fn record_render_duration(&mut self, started: Instant) {
        let elapsed_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        self.render_duration_us_total = self
            .render_duration_us_total
            .saturating_add(u128::from(elapsed_us));
        self.render_duration_us_max = self.render_duration_us_max.max(elapsed_us);
        self.last_render_duration_us = Some(elapsed_us);
    }

    fn render_duration_us_avg(&self) -> Option<u64> {
        if self.render_calls == 0 {
            return None;
        }
        Some((self.render_duration_us_total / u128::from(self.render_calls)) as u64)
    }

    fn render_duration_us_max(&self) -> Option<u64> {
        (self.render_calls > 0).then_some(self.render_duration_us_max)
    }

    fn clear_color(&self) -> NativeWgpuColor {
        match self.render_mode {
            NativeWgpuRenderMode::Solid => self.color,
            NativeWgpuRenderMode::Pulse => self.pulse_color(),
        }
    }

    fn pulse_color(&self) -> NativeWgpuColor {
        let elapsed = self.started.elapsed().as_secs_f64();
        let phase = (elapsed * std::f64::consts::TAU * 0.75).sin() * 0.5 + 0.5;
        let accent = NativeWgpuColor {
            red: 0.02,
            green: 0.95,
            blue: 0.62,
            alpha: self.color.alpha,
        };
        self.color.blend(accent, phase)
    }

    fn encode_render_pass(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        #[cfg(any(feature = "native-wgpu-gpu-video", feature = "native-wgpu-gst-dmabuf"))]
        if let Some(video) = self.gpu_video.as_ref().filter(|video| video.has_frame()) {
            video.encode_render_pass(encoder, view, self.clear_color().as_wgpu());
            return;
        }

        #[cfg(feature = "video-renderer")]
        if let Some(video) = self.video.as_ref().filter(|video| video.has_frame()) {
            video.encode_render_pass(encoder, view, self.clear_color().as_wgpu());
            return;
        }

        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gilder-native-wgpu-clear-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.clear_color().as_wgpu()),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }

    #[cfg(feature = "video-renderer")]
    fn upload_video_sample(
        &mut self,
        sample: &gst::Sample,
        fit: crate::core::FitMode,
    ) -> Result<NativeWgpuVideoUploadReport, NativeWgpuError> {
        let Some(buffer) = sample.buffer() else {
            return Err(NativeWgpuError::Video(
                "appsink sample has no buffer".to_owned(),
            ));
        };
        let info = native_wgpu_video_sample_info(sample, buffer)?;
        let map = buffer
            .map_readable()
            .map_err(|_| NativeWgpuError::Video("video buffer map_readable failed".to_owned()))?;
        let source = map.as_slice();

        let video = self.video.get_or_insert_with(|| {
            NativeWgpuVideoRenderer::new(&self.device, self.config.format, fit)
        });
        video.upload_rgba(
            &self.device,
            &self.queue,
            (self.config.width, self.config.height),
            &info,
            source,
            fit,
        )
    }

    #[cfg(feature = "native-wgpu-gpu-video")]
    fn present_gpu_video_frame(
        &mut self,
        frame: wgpu::Texture,
        fit: crate::core::FitMode,
    ) -> Result<NativeWgpuNv12PresentReport, NativeWgpuError> {
        let video = self.gpu_video.get_or_insert_with(|| {
            NativeWgpuNv12VideoRenderer::new(&self.device, self.config.format, fit)
        });
        video.present_texture(
            &self.device,
            &self.queue,
            (self.config.width, self.config.height),
            frame,
            fit,
        )
    }

    #[cfg(feature = "native-wgpu-gst-dmabuf")]
    fn present_gst_dmabuf_frame(
        &mut self,
        frame: NativeWgpuGstDmabufFrame,
        fit: crate::core::FitMode,
    ) -> Result<NativeWgpuGstDmabufImportReport, NativeWgpuError> {
        let texture = native_wgpu_import_gst_dmabuf_frame_as_nv12_texture(&self.device, frame)
            .map_err(NativeWgpuError::Video)?;
        let video = self.gpu_video.get_or_insert_with(|| {
            NativeWgpuNv12VideoRenderer::new(&self.device, self.config.format, fit)
        });
        let report = video.present_texture(
            &self.device,
            &self.queue,
            (self.config.width, self.config.height),
            texture,
            fit,
        )?;
        Ok(NativeWgpuGstDmabufImportReport {
            imported_frames: report.presented_frames,
        })
    }

    #[cfg(feature = "native-wgpu-gst-dmabuf")]
    fn present_gst_cuda_direct_sample(
        &mut self,
        sample: &gst::Sample,
        fit: crate::core::FitMode,
    ) -> Result<NativeWgpuGstDmabufImportReport, NativeWgpuError> {
        let video = self.gpu_video.get_or_insert_with(|| {
            NativeWgpuNv12VideoRenderer::new(&self.device, self.config.format, fit)
        });
        let report = video.present_cuda_direct_sample(
            &self.device,
            &self.queue,
            (self.config.width, self.config.height),
            sample,
            fit,
        )?;
        Ok(NativeWgpuGstDmabufImportReport {
            imported_frames: report.presented_frames,
        })
    }

    #[cfg(feature = "native-wgpu-gst-dmabuf")]
    fn present_gst_system_nv12_sample(
        &mut self,
        sample: &gst::Sample,
        fit: crate::core::FitMode,
    ) -> Result<NativeWgpuGstDmabufImportReport, NativeWgpuError> {
        let video = self.gpu_video.get_or_insert_with(|| {
            NativeWgpuNv12VideoRenderer::new(&self.device, self.config.format, fit)
        });
        let report = video.present_system_nv12_sample(
            &self.device,
            &self.queue,
            (self.config.width, self.config.height),
            sample,
            fit,
        )?;
        Ok(NativeWgpuGstDmabufImportReport {
            imported_frames: report.presented_frames,
        })
    }
}

#[cfg(feature = "video-renderer")]
struct NativeWgpuVideoRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    fit_buffer: wgpu::Buffer,
    texture: Option<wgpu::Texture>,
    bind_group: Option<wgpu::BindGroup>,
    source_size: Option<(u32, u32)>,
    fit: crate::core::FitMode,
    staging: Vec<u8>,
    uploaded_frames: u64,
}

#[cfg(feature = "video-renderer")]
impl NativeWgpuVideoRenderer {
    fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        fit: crate::core::FitMode,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gilder-native-wgpu-video-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gilder-native-wgpu-video-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gilder-native-wgpu-video-shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(NATIVE_WGPU_VIDEO_SHADER)),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gilder-native-wgpu-video-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gilder-native-wgpu-video-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let fit_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gilder-native-wgpu-video-fit-buffer"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            bind_group_layout,
            sampler,
            fit_buffer,
            texture: None,
            bind_group: None,
            source_size: None,
            fit,
            staging: Vec::new(),
            uploaded_frames: 0,
        }
    }

    fn has_frame(&self) -> bool {
        self.bind_group.is_some()
    }

    fn upload_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_size: (u32, u32),
        info: &NativeWgpuVideoSampleInfo,
        source: &[u8],
        fit: crate::core::FitMode,
    ) -> Result<NativeWgpuVideoUploadReport, NativeWgpuError> {
        if info.format != "RGBA" {
            return Err(NativeWgpuError::Video(format!(
                "expected RGBA appsink frame, got {}",
                info.format
            )));
        }
        let row_bytes = info
            .width
            .checked_mul(4)
            .ok_or_else(|| NativeWgpuError::Video("video row byte count overflow".to_owned()))?;
        if info.stride < row_bytes {
            return Err(NativeWgpuError::Video(format!(
                "video stride {} is smaller than row bytes {}",
                info.stride, row_bytes
            )));
        }
        let upload_stride = align_to(row_bytes, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let upload_len = u64::from(upload_stride)
            .checked_mul(u64::from(info.height))
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| NativeWgpuError::Video("video upload buffer too large".to_owned()))?;
        self.staging.resize(upload_len, 0);

        let row_bytes_usize = usize::try_from(row_bytes)
            .map_err(|_| NativeWgpuError::Video("video row bytes too large".to_owned()))?;
        let upload_stride_usize = usize::try_from(upload_stride)
            .map_err(|_| NativeWgpuError::Video("video upload stride too large".to_owned()))?;
        let source_stride_usize = usize::try_from(info.stride)
            .map_err(|_| NativeWgpuError::Video("video source stride too large".to_owned()))?;
        let source_offset = usize::try_from(info.offset)
            .map_err(|_| NativeWgpuError::Video("video source offset too large".to_owned()))?;

        for row in 0..usize::try_from(info.height).unwrap_or_default() {
            let source_start = source_offset
                .checked_add(row.checked_mul(source_stride_usize).ok_or_else(|| {
                    NativeWgpuError::Video("video source row offset overflow".to_owned())
                })?)
                .ok_or_else(|| {
                    NativeWgpuError::Video("video source row offset overflow".to_owned())
                })?;
            let source_end = source_start.checked_add(row_bytes_usize).ok_or_else(|| {
                NativeWgpuError::Video("video source row end overflow".to_owned())
            })?;
            if source_end > source.len() {
                return Err(NativeWgpuError::Video(format!(
                    "video buffer too small for row {row}: need {source_end}, have {}",
                    source.len()
                )));
            }
            let destination_start = row.checked_mul(upload_stride_usize).ok_or_else(|| {
                NativeWgpuError::Video("video destination row offset overflow".to_owned())
            })?;
            let destination_end =
                destination_start
                    .checked_add(row_bytes_usize)
                    .ok_or_else(|| {
                        NativeWgpuError::Video("video destination row end overflow".to_owned())
                    })?;
            self.staging[destination_start..destination_end]
                .copy_from_slice(&source[source_start..source_end]);
            if upload_stride_usize > row_bytes_usize {
                self.staging[destination_end..destination_start + upload_stride_usize].fill(0);
            }
        }

        self.ensure_texture(device, info.width, info.height);
        let texture = self
            .texture
            .as_ref()
            .expect("video texture must exist after ensure_texture");
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.staging,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(upload_stride),
                rows_per_image: Some(info.height),
            },
            wgpu::Extent3d {
                width: info.width,
                height: info.height,
                depth_or_array_layers: 1,
            },
        );
        self.fit = fit;
        self.update_fit_uniform(queue, surface_size);
        self.uploaded_frames = self.uploaded_frames.saturating_add(1);

        Ok(NativeWgpuVideoUploadReport {
            width: info.width,
            height: info.height,
            format: info.format.clone(),
            source_stride: info.stride,
            upload_stride,
            uploaded_frames: self.uploaded_frames,
        })
    }

    fn ensure_texture(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.source_size == Some((width, height)) && self.texture.is_some() {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gilder-native-wgpu-video-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gilder-native-wgpu-video-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.fit_buffer.as_entire_binding(),
                },
            ],
        });
        self.source_size = Some((width, height));
        self.texture = Some(texture);
        self.bind_group = Some(bind_group);
    }

    fn update_fit_uniform(&self, queue: &wgpu::Queue, surface_size: (u32, u32)) {
        let Some(source_size) = self.source_size else {
            return;
        };
        let (offset, scale) = video_uv_transform(self.fit, source_size, surface_size);
        let mut bytes = [0u8; 16];
        write_f32_pair(&mut bytes[0..8], offset);
        write_f32_pair(&mut bytes[8..16], scale);
        queue.write_buffer(&self.fit_buffer, 0, &bytes);
    }

    fn encode_render_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        clear_color: wgpu::Color,
    ) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gilder-native-wgpu-video-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(any(feature = "native-wgpu-gpu-video", feature = "native-wgpu-gst-dmabuf"))]
struct NativeWgpuNv12VideoRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    fit_buffer: wgpu::Buffer,
    texture: Option<wgpu::Texture>,
    bind_group: Option<wgpu::BindGroup>,
    source_size: Option<(u32, u32)>,
    texture_copy_dst: bool,
    fit_uniform_bytes: Option<[u8; 16]>,
    #[cfg(feature = "native-wgpu-gst-dmabuf")]
    cuda_direct_staging: Option<NativeWgpuCudaVulkanStagingBuffer>,
    fit: crate::core::FitMode,
    presented_frames: u64,
}

#[cfg(any(feature = "native-wgpu-gpu-video", feature = "native-wgpu-gst-dmabuf"))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeWgpuNv12PresentReport {
    width: u32,
    height: u32,
    format: String,
    presented_frames: u64,
}

#[cfg(any(feature = "native-wgpu-gpu-video", feature = "native-wgpu-gst-dmabuf"))]
impl NativeWgpuNv12VideoRenderer {
    fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        fit: crate::core::FitMode,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gilder-native-wgpu-nv12-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gilder-native-wgpu-nv12-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gilder-native-wgpu-nv12-shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(NATIVE_WGPU_NV12_SHADER)),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gilder-native-wgpu-nv12-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gilder-native-wgpu-nv12-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let fit_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gilder-native-wgpu-nv12-fit-buffer"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            bind_group_layout,
            sampler,
            fit_buffer,
            texture: None,
            bind_group: None,
            source_size: None,
            texture_copy_dst: false,
            fit_uniform_bytes: None,
            #[cfg(feature = "native-wgpu-gst-dmabuf")]
            cuda_direct_staging: None,
            fit,
            presented_frames: 0,
        }
    }

    fn has_frame(&self) -> bool {
        self.bind_group.is_some()
    }

    fn present_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_size: (u32, u32),
        texture: wgpu::Texture,
        fit: crate::core::FitMode,
    ) -> Result<NativeWgpuNv12PresentReport, NativeWgpuError> {
        if texture.format() != wgpu::TextureFormat::NV12 {
            return Err(NativeWgpuError::Video(format!(
                "expected NV12 gpu-video frame, got {:?}",
                texture.format()
            )));
        }
        let width = texture.width();
        let height = texture.height();
        if width == 0 || height == 0 {
            return Err(NativeWgpuError::Video(
                "gpu-video frame has zero dimension".to_owned(),
            ));
        }

        let y_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("gilder-native-wgpu-nv12-y-view"),
            format: Some(wgpu::TextureFormat::R8Unorm),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::Plane0,
            ..Default::default()
        });
        let uv_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("gilder-native-wgpu-nv12-uv-view"),
            format: Some(wgpu::TextureFormat::Rg8Unorm),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::Plane1,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gilder-native-wgpu-nv12-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&y_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&uv_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.fit_buffer.as_entire_binding(),
                },
            ],
        });
        self.texture = Some(texture);
        self.bind_group = Some(bind_group);
        self.source_size = Some((width, height));
        self.texture_copy_dst = false;
        self.fit = fit;
        self.update_fit_uniform(queue, surface_size);
        self.presented_frames = self.presented_frames.saturating_add(1);

        Ok(NativeWgpuNv12PresentReport {
            width,
            height,
            format: "NV12".to_owned(),
            presented_frames: self.presented_frames,
        })
    }

    #[cfg(feature = "native-wgpu-gst-dmabuf")]
    fn present_system_nv12_sample(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_size: (u32, u32),
        sample: &gst::Sample,
        fit: crate::core::FitMode,
    ) -> Result<NativeWgpuNv12PresentReport, NativeWgpuError> {
        let buffer = sample
            .buffer()
            .ok_or_else(|| NativeWgpuError::Video("appsink sample has no buffer".to_owned()))?;
        let meta =
            native_wgpu_gst_system_nv12_meta(sample, buffer).map_err(NativeWgpuError::Video)?;
        let map = buffer.map_readable().map_err(|_| {
            NativeWgpuError::Video("system NV12 buffer map_readable failed".to_owned())
        })?;
        let source = map.as_slice();

        self.ensure_system_nv12_texture(device, meta.width, meta.height)?;
        let texture = self
            .texture
            .as_ref()
            .expect("system NV12 texture must exist after ensure_system_nv12_texture");
        native_wgpu_write_system_nv12_plane(
            queue,
            texture,
            source,
            "y",
            wgpu::TextureAspect::Plane0,
            meta.y.offset,
            meta.y.stride,
            meta.y.width,
            meta.y.height,
            meta.y.row_bytes,
        )?;
        native_wgpu_write_system_nv12_plane(
            queue,
            texture,
            source,
            "uv",
            wgpu::TextureAspect::Plane1,
            meta.uv.offset,
            meta.uv.stride,
            meta.uv.width,
            meta.uv.height,
            meta.uv.row_bytes,
        )?;

        self.fit = fit;
        self.update_fit_uniform(queue, surface_size);
        self.presented_frames = self.presented_frames.saturating_add(1);

        Ok(NativeWgpuNv12PresentReport {
            width: meta.width,
            height: meta.height,
            format: "NV12".to_owned(),
            presented_frames: self.presented_frames,
        })
    }

    #[cfg(feature = "native-wgpu-gst-dmabuf")]
    fn present_cuda_direct_sample(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_size: (u32, u32),
        sample: &gst::Sample,
        fit: crate::core::FitMode,
    ) -> Result<NativeWgpuNv12PresentReport, NativeWgpuError> {
        let buffer = sample
            .buffer()
            .ok_or_else(|| NativeWgpuError::Video("appsink sample has no buffer".to_owned()))?;
        let meta =
            native_wgpu_gst_system_nv12_meta(sample, buffer).map_err(NativeWgpuError::Video)?;
        if !native_wgpu_gst_buffer_has_cuda_memory(buffer) {
            return Err(NativeWgpuError::Video(format!(
                "cuda-direct expected CUDAMemory, got {}",
                native_wgpu_gst_memory_types(buffer).join("|")
            )));
        }

        self.ensure_system_nv12_texture(device, meta.width, meta.height)?;
        let staging_layout = NativeWgpuCudaVulkanStagingLayout::for_nv12(meta.width, meta.height)?;
        let cuda_context = native_wgpu_gst_cuda_context_from_buffer(buffer)?;
        let recreate_staging = self
            .cuda_direct_staging
            .as_ref()
            .map(|staging| {
                !staging.matches(cuda_context, staging_layout.size, staging_layout.y_stride)
                    || staging.layout != staging_layout
            })
            .unwrap_or(true);
        if recreate_staging {
            self.cuda_direct_staging = Some(NativeWgpuCudaVulkanStagingBuffer::new(
                device,
                cuda_context,
                staging_layout,
            )?);
        }
        let staging = self
            .cuda_direct_staging
            .as_mut()
            .expect("cuda-direct staging must exist after ensure");
        native_wgpu_copy_gst_cuda_sample_to_vulkan_staging(buffer, &meta, staging)?;

        let texture = self
            .texture
            .as_ref()
            .expect("system NV12 texture must exist after ensure_system_nv12_texture");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gilder-native-wgpu-cuda-direct-copy"),
        });
        native_wgpu_encode_cuda_direct_staging_to_nv12_texture(&mut encoder, staging, texture)?;
        queue.submit(Some(encoder.finish()));

        self.fit = fit;
        self.update_fit_uniform(queue, surface_size);
        self.presented_frames = self.presented_frames.saturating_add(1);

        Ok(NativeWgpuNv12PresentReport {
            width: meta.width,
            height: meta.height,
            format: "NV12".to_owned(),
            presented_frames: self.presented_frames,
        })
    }

    #[cfg(feature = "native-wgpu-gst-dmabuf")]
    fn ensure_system_nv12_texture(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<(), NativeWgpuError> {
        if self.texture_copy_dst && self.source_size == Some((width, height)) {
            return Ok(());
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gilder-native-wgpu-system-nv12-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::NV12,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let y_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("gilder-native-wgpu-system-nv12-y-view"),
            format: Some(wgpu::TextureFormat::R8Unorm),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::Plane0,
            ..Default::default()
        });
        let uv_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("gilder-native-wgpu-system-nv12-uv-view"),
            format: Some(wgpu::TextureFormat::Rg8Unorm),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::Plane1,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gilder-native-wgpu-system-nv12-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&y_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&uv_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.fit_buffer.as_entire_binding(),
                },
            ],
        });

        self.texture = Some(texture);
        self.bind_group = Some(bind_group);
        self.source_size = Some((width, height));
        self.texture_copy_dst = true;
        Ok(())
    }

    fn update_fit_uniform(&mut self, queue: &wgpu::Queue, surface_size: (u32, u32)) {
        let Some(source_size) = self.source_size else {
            return;
        };
        let (offset, scale) = video_uv_transform(self.fit, source_size, surface_size);
        let mut bytes = [0u8; 16];
        write_f32_pair(&mut bytes[0..8], offset);
        write_f32_pair(&mut bytes[8..16], scale);
        if self.fit_uniform_bytes == Some(bytes) {
            return;
        }
        queue.write_buffer(&self.fit_buffer, 0, &bytes);
        self.fit_uniform_bytes = Some(bytes);
    }

    fn encode_render_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        clear_color: wgpu::Color,
    ) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gilder-native-wgpu-nv12-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuGstDmabufFrame {
    _gst_buffer: gst::Buffer,
    fds: Vec<OwnedFd>,
    width: u32,
    height: u32,
    format: u32,
    modifier: Option<u64>,
    planes: Vec<NativeWgpuGstDmabufPlane>,
    export_source: &'static str,
    memory_types: Vec<String>,
    caps_format: String,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeWgpuGstDmabufPlane {
    fd_index: usize,
    offset: u32,
    stride: u32,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuGstDmabufExport {
    source: &'static str,
    format: u32,
    fds: Vec<OwnedFd>,
    planes: Vec<NativeWgpuGstDmabufPlane>,
    modifier: Option<u64>,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeWgpuGstDmabufImportReport {
    imported_frames: u64,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeWgpuCudaVulkanStagingLayout {
    width: u32,
    height: u32,
    y_offset: u64,
    uv_offset: u64,
    y_stride: u32,
    uv_stride: u32,
    y_height: u32,
    uv_width: u32,
    uv_height: u32,
    size: u64,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl NativeWgpuCudaVulkanStagingLayout {
    fn for_nv12(width: u32, height: u32) -> Result<Self, NativeWgpuError> {
        if width == 0 || height == 0 {
            return Err(NativeWgpuError::Video(
                "cuda-direct NV12 frame has zero dimension".to_owned(),
            ));
        }
        if width % 2 != 0 || height % 2 != 0 {
            return Err(NativeWgpuError::Video(format!(
                "cuda-direct NV12 dimensions must be even, got {width}x{height}"
            )));
        }
        let y_stride = native_wgpu_align_u32(width, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            .ok_or_else(|| NativeWgpuError::Video("cuda-direct y stride overflow".to_owned()))?;
        let uv_stride = native_wgpu_align_u32(width, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            .ok_or_else(|| NativeWgpuError::Video("cuda-direct uv stride overflow".to_owned()))?;
        let y_size = u64::from(y_stride)
            .checked_mul(u64::from(height))
            .ok_or_else(|| NativeWgpuError::Video("cuda-direct y size overflow".to_owned()))?;
        let uv_height = height / 2;
        let uv_size = u64::from(uv_stride)
            .checked_mul(u64::from(uv_height))
            .ok_or_else(|| NativeWgpuError::Video("cuda-direct uv size overflow".to_owned()))?;
        let size = y_size.checked_add(uv_size).ok_or_else(|| {
            NativeWgpuError::Video("cuda-direct staging size overflow".to_owned())
        })?;
        Ok(Self {
            width,
            height,
            y_offset: 0,
            uv_offset: y_size,
            y_stride,
            uv_stride,
            y_height: height,
            uv_width: width / 2,
            uv_height,
            size,
        })
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_align_u32(value: u32, alignment: u32) -> Option<u32> {
    if alignment == 0 {
        return None;
    }
    let mask = alignment.checked_sub(1)?;
    value
        .checked_add(mask)
        .map(|value| value / alignment * alignment)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuCudaVulkanStagingBuffer {
    cuda_stream: NativeWgpuCudaStream,
    cuda_external_memory: NativeWgpuCudaExternalMemory,
    buffer: wgpu::Buffer,
    cuda_context: *mut NativeWgpuGstCudaContext,
    layout: NativeWgpuCudaVulkanStagingLayout,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl NativeWgpuCudaVulkanStagingBuffer {
    fn new(
        device: &wgpu::Device,
        cuda_context: *mut NativeWgpuGstCudaContext,
        layout: NativeWgpuCudaVulkanStagingLayout,
    ) -> Result<Self, NativeWgpuError> {
        if cuda_context.is_null() {
            return Err(NativeWgpuError::Video(
                "cuda-direct sample has null GstCudaContext".to_owned(),
            ));
        }
        let hal_device = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }.ok_or_else(|| {
            NativeWgpuError::Video("cuda-direct requires Vulkan wgpu device".to_owned())
        })?;
        let raw_device = hal_device.raw_device().clone();
        let handle_type = ash::vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD;
        let mut external_buffer_info =
            ash::vk::ExternalMemoryBufferCreateInfo::default().handle_types(handle_type);
        let buffer_info = ash::vk::BufferCreateInfo::default()
            .size(layout.size)
            .usage(ash::vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(ash::vk::SharingMode::EXCLUSIVE)
            .push_next(&mut external_buffer_info);
        let vk_buffer = unsafe { raw_device.create_buffer(&buffer_info, None) }.map_err(|err| {
            NativeWgpuError::Video(format!(
                "cuda-direct create Vulkan staging buffer failed: {err:?}"
            ))
        })?;

        let requirements = unsafe { raw_device.get_buffer_memory_requirements(vk_buffer) };
        let memory_type_index = native_wgpu_vulkan_memory_type_index(
            hal_device.shared_instance().raw_instance(),
            hal_device.raw_physical_device(),
            requirements.memory_type_bits,
            ash::vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ash::vk::MemoryPropertyFlags::empty(),
        )
        .ok_or_else(|| {
            unsafe {
                raw_device.destroy_buffer(vk_buffer, None);
            }
            NativeWgpuError::Video(format!(
                "cuda-direct no Vulkan memory type for staging buffer: bits={:#x}",
                requirements.memory_type_bits
            ))
        })?;
        let mut export_info =
            ash::vk::ExportMemoryAllocateInfo::default().handle_types(handle_type);
        let allocate_info = ash::vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index)
            .push_next(&mut export_info);
        let vk_memory = match unsafe { raw_device.allocate_memory(&allocate_info, None) } {
            Ok(memory) => memory,
            Err(err) => {
                unsafe {
                    raw_device.destroy_buffer(vk_buffer, None);
                }
                return Err(NativeWgpuError::Video(format!(
                    "cuda-direct allocate Vulkan staging memory failed: {err:?}:size={}:type={memory_type_index}",
                    requirements.size
                )));
            }
        };
        if let Err(err) = unsafe { raw_device.bind_buffer_memory(vk_buffer, vk_memory, 0) } {
            unsafe {
                raw_device.destroy_buffer(vk_buffer, None);
                raw_device.free_memory(vk_memory, None);
            }
            return Err(NativeWgpuError::Video(format!(
                "cuda-direct bind Vulkan staging memory failed: {err:?}"
            )));
        }

        let external_memory_fd = ash::khr::external_memory_fd::Device::new(
            hal_device.shared_instance().raw_instance(),
            &raw_device,
        );
        let fd_info = ash::vk::MemoryGetFdInfoKHR::default()
            .memory(vk_memory)
            .handle_type(handle_type);
        let fd = match unsafe { external_memory_fd.get_memory_fd(&fd_info) } {
            Ok(fd) => fd,
            Err(err) => {
                unsafe {
                    raw_device.destroy_buffer(vk_buffer, None);
                    raw_device.free_memory(vk_memory, None);
                }
                return Err(NativeWgpuError::Video(format!(
                    "cuda-direct export Vulkan staging fd failed: {err:?}"
                )));
            }
        };

        let (cuda_stream, cuda_external_memory) = {
            let _guard = NativeWgpuGstCudaContextPushGuard::new(cuda_context)?;
            let cuda_stream = match NativeWgpuCudaStream::new() {
                Ok(stream) => stream,
                Err(err) => {
                    unsafe {
                        drop(OwnedFd::from_raw_fd(fd));
                        raw_device.destroy_buffer(vk_buffer, None);
                        raw_device.free_memory(vk_memory, None);
                    }
                    return Err(err);
                }
            };
            let cuda_external_memory = match NativeWgpuCudaExternalMemory::import_opaque_fd(
                fd,
                requirements.size,
                layout.size,
            ) {
                Ok(memory) => memory,
                Err(err) => {
                    unsafe {
                        drop(OwnedFd::from_raw_fd(fd));
                        raw_device.destroy_buffer(vk_buffer, None);
                        raw_device.free_memory(vk_memory, None);
                    }
                    return Err(err);
                }
            };
            (cuda_stream, cuda_external_memory)
        };

        let hal_buffer = unsafe {
            wgpu::hal::vulkan::Buffer::from_raw_managed(vk_buffer, vk_memory, 0, requirements.size)
        };
        let buffer = unsafe {
            device.create_buffer_from_hal::<wgpu::hal::api::Vulkan>(
                hal_buffer,
                &wgpu::BufferDescriptor {
                    label: Some("gilder-native-wgpu-cuda-direct-staging-buffer"),
                    size: layout.size,
                    usage: wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                },
            )
        };

        Ok(Self {
            cuda_stream,
            cuda_external_memory,
            buffer,
            cuda_context,
            layout,
        })
    }

    fn matches(
        &self,
        cuda_context: *mut NativeWgpuGstCudaContext,
        size: u64,
        y_stride: u32,
    ) -> bool {
        self.cuda_context == cuda_context
            && self.layout.size >= size
            && self.layout.y_stride == y_stride
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_vulkan_memory_type_index(
    instance: &ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
    memory_type_bits: u32,
    preferred: ash::vk::MemoryPropertyFlags,
    required: ash::vk::MemoryPropertyFlags,
) -> Option<u32> {
    let properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    let mut fallback = None;
    for (index, memory_type) in properties.memory_types_as_slice().iter().enumerate() {
        let bit = 1u32 << index;
        if memory_type_bits & bit == 0 {
            continue;
        }
        if memory_type.property_flags & required != required {
            continue;
        }
        let index = u32::try_from(index).ok()?;
        if memory_type.property_flags & preferred == preferred {
            return Some(index);
        }
        fallback.get_or_insert(index);
    }
    fallback
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuCudaExternalMemory {
    handle: NativeWgpuCudaExternalMemoryHandle,
    mapped_ptr: NativeWgpuCudaDevicePtr,
    mapped_size: u64,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl NativeWgpuCudaExternalMemory {
    fn import_opaque_fd(
        fd: i32,
        allocation_size: u64,
        mapped_size: u64,
    ) -> Result<Self, NativeWgpuError> {
        let mut external_memory = ptr::null_mut();
        let desc = NativeWgpuCudaExternalMemoryHandleDesc {
            type_: CUDA_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD,
            handle: NativeWgpuCudaExternalMemoryHandleUnion { fd },
            size: allocation_size,
            flags: 0,
            reserved: [0; 16],
        };
        match native_wgpu_cuda_result(
            unsafe { CuImportExternalMemory(&mut external_memory, &desc) },
            "cuda-direct import Vulkan staging external memory",
        ) {
            Ok(()) => {}
            Err(err) => return Err(err),
        }
        if external_memory.is_null() {
            return Err(NativeWgpuError::Video(
                "cuda-direct imported external memory is null".to_owned(),
            ));
        }

        let mut mapped_ptr = 0;
        let buffer_desc = NativeWgpuCudaExternalMemoryBufferDesc {
            offset: 0,
            size: mapped_size,
            flags: 0,
            reserved: [0; 16],
        };
        if let Err(err) = native_wgpu_cuda_result(
            unsafe {
                CuExternalMemoryGetMappedBuffer(&mut mapped_ptr, external_memory, &buffer_desc)
            },
            "cuda-direct map Vulkan staging external memory",
        ) {
            let _ = unsafe { CuDestroyExternalMemory(external_memory) };
            return Err(err);
        }
        if mapped_ptr == 0 {
            let _ = unsafe { CuDestroyExternalMemory(external_memory) };
            return Err(NativeWgpuError::Video(
                "cuda-direct mapped external memory pointer is null".to_owned(),
            ));
        }
        Ok(Self {
            handle: external_memory,
            mapped_ptr,
            mapped_size,
        })
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl Drop for NativeWgpuCudaExternalMemory {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            let _ = unsafe { CuDestroyExternalMemory(self.handle) };
        }
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuCudaStream {
    handle: NativeWgpuCudaStreamHandle,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl NativeWgpuCudaStream {
    fn new() -> Result<Self, NativeWgpuError> {
        let mut handle = ptr::null_mut();
        native_wgpu_cuda_result(
            unsafe { CuStreamCreate(&mut handle, CUDA_STREAM_NON_BLOCKING) },
            "cuda-direct create copy stream",
        )?;
        if handle.is_null() {
            return Err(NativeWgpuError::Video(
                "cuda-direct copy stream is null".to_owned(),
            ));
        }
        Ok(Self { handle })
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl Drop for NativeWgpuCudaStream {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            let _ = unsafe { CuStreamDestroy(self.handle) };
        }
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuGstCudaContextPushGuard;

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl NativeWgpuGstCudaContextPushGuard {
    fn new(context: *mut NativeWgpuGstCudaContext) -> Result<Self, NativeWgpuError> {
        if context.is_null() {
            return Err(NativeWgpuError::Video(
                "cuda-direct cannot push null GstCudaContext".to_owned(),
            ));
        }
        let pushed = unsafe { gst_cuda_context_push(context) } != gst::glib::ffi::GFALSE;
        if !pushed {
            return Err(NativeWgpuError::Video(
                "cuda-direct failed to push GstCudaContext".to_owned(),
            ));
        }
        Ok(Self)
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl Drop for NativeWgpuGstCudaContextPushGuard {
    fn drop(&mut self) {
        let mut context = ptr::null_mut();
        let _ = unsafe { gst_cuda_context_pop(&mut context) };
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl Drop for NativeWgpuCudaVulkanStagingBuffer {
    fn drop(&mut self) {
        if self.cuda_context.is_null() || self.cuda_stream.handle.is_null() {
            return;
        }
        if let Ok(_guard) = NativeWgpuGstCudaContextPushGuard::new(self.cuda_context) {
            let _ = unsafe { CuStreamSynchronize(self.cuda_stream.handle) };
        }
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_encode_cuda_direct_staging_to_nv12_texture(
    encoder: &mut wgpu::CommandEncoder,
    staging: &NativeWgpuCudaVulkanStagingBuffer,
    texture: &wgpu::Texture,
) -> Result<(), NativeWgpuError> {
    let layout = staging.layout;
    encoder.copy_buffer_to_texture(
        wgpu::TexelCopyBufferInfo {
            buffer: &staging.buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: layout.y_offset,
                bytes_per_row: Some(layout.y_stride),
                rows_per_image: Some(layout.y_height),
            },
        },
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::Plane0,
        },
        wgpu::Extent3d {
            width: layout.width,
            height: layout.height,
            depth_or_array_layers: 1,
        },
    );
    encoder.copy_buffer_to_texture(
        wgpu::TexelCopyBufferInfo {
            buffer: &staging.buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: layout.uv_offset,
                bytes_per_row: Some(layout.uv_stride),
                rows_per_image: Some(layout.uv_height),
            },
        },
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::Plane1,
        },
        wgpu::Extent3d {
            width: layout.uv_width,
            height: layout.uv_height,
            depth_or_array_layers: 1,
        },
    );
    Ok(())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_import_gst_dmabuf_frame_as_nv12_texture(
    device: &wgpu::Device,
    mut frame: NativeWgpuGstDmabufFrame,
) -> Result<wgpu::Texture, String> {
    let frame_summary = native_wgpu_gst_dmabuf_frame_summary(&frame);
    if frame.format != DRM_FORMAT_NV12 {
        return Err(format!(
            "vulkan_import_expected_nv12_fourcc_{}_got_{}:{}",
            DRM_FORMAT_NV12, frame.format, frame_summary
        ));
    }
    if frame.width == 0 || frame.height == 0 {
        return Err(format!("vulkan_import_invalid_zero_size:{frame_summary}"));
    }
    if frame.width % 2 != 0 || frame.height % 2 != 0 {
        return Err(format!(
            "vulkan_import_nv12_requires_even_size:{}x{}:{}",
            frame.width, frame.height, frame_summary
        ));
    }
    if frame.fds.len() != 1 {
        return Err(format!(
            "vulkan_import_multi_fd_dmabuf_not_yet_supported:fd_count={}:{}",
            frame.fds.len(),
            frame_summary
        ));
    }
    let drm_modifier = frame
        .modifier
        .filter(|modifier| *modifier != DRM_FORMAT_MOD_INVALID);
    let drm_plane_layouts = drm_modifier
        .map(|modifier| native_wgpu_nv12_drm_plane_layouts(&frame, modifier))
        .transpose()?;

    let desc = wgpu::TextureDescriptor {
        label: Some("gilder-native-wgpu-gst-dmabuf-nv12-texture"),
        size: wgpu::Extent3d {
            width: frame.width,
            height: frame.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::NV12,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    let hal_desc = wgpu::hal::TextureDescriptor {
        label: desc.label,
        size: desc.size,
        mip_level_count: desc.mip_level_count,
        sample_count: desc.sample_count,
        dimension: desc.dimension,
        format: desc.format,
        usage: wgpu::TextureUses::RESOURCE,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: Vec::new(),
    };

    let hal_device = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }
        .ok_or_else(|| "vulkan_import_wgpu_device_is_not_vulkan".to_owned())?;
    let raw_device = hal_device.raw_device().clone();
    let external_memory_fd = ash::khr::external_memory_fd::Device::new(
        hal_device.shared_instance().raw_instance(),
        &raw_device,
    );
    let import_fd_raw = frame.fds.remove(0).into_raw_fd();
    let prefer_dma_buf_handle = frame.export_source.contains("dmabuf")
        || frame.caps_format == "DMA_DRM"
        || drm_modifier.is_some();
    let handle_type_candidates =
        native_wgpu_external_memory_handle_candidates(prefer_dma_buf_handle);
    let mut fd_probe_reports = Vec::with_capacity(handle_type_candidates.len());
    let mut selected_fd_properties = None;
    for (label, candidate) in handle_type_candidates {
        let mut fd_properties = ash::vk::MemoryFdPropertiesKHR::default();
        match unsafe {
            external_memory_fd.get_memory_fd_properties(
                candidate,
                import_fd_raw,
                &mut fd_properties,
            )
        } {
            Ok(()) => {
                fd_probe_reports.push(format!(
                    "{label}=ok:memory_type_bits={:#x}",
                    fd_properties.memory_type_bits
                ));
                if fd_properties.memory_type_bits != 0 {
                    selected_fd_properties = Some((label, candidate, fd_properties));
                    break;
                }
            }
            Err(err) => fd_probe_reports.push(format!("{label}={err:?}")),
        }
    }
    let Some((handle_type_label, handle_type, fd_properties)) = selected_fd_properties else {
        drop(unsafe { OwnedFd::from_raw_fd(import_fd_raw) });
        return Err(format!(
            "vulkan_import_fd_properties_failed:{}:{}",
            fd_probe_reports.join("|"),
            frame_summary
        ));
    };

    let mut external_image_info =
        ash::vk::ExternalMemoryImageCreateInfo::default().handle_types(handle_type);
    let mut drm_modifier_info = ash::vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default();
    if let (Some(modifier), Some(plane_layouts)) = (drm_modifier, drm_plane_layouts.as_ref()) {
        drm_modifier_info = drm_modifier_info
            .drm_format_modifier(modifier)
            .plane_layouts(plane_layouts);
    }
    let image_tiling = if drm_plane_layouts.is_some() {
        ash::vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT
    } else {
        ash::vk::ImageTiling::LINEAR
    };
    let initial_layout = if drm_plane_layouts.is_some() {
        ash::vk::ImageLayout::UNDEFINED
    } else {
        ash::vk::ImageLayout::PREINITIALIZED
    };
    let mut image_info = ash::vk::ImageCreateInfo::default()
        .image_type(ash::vk::ImageType::TYPE_2D)
        .format(ash::vk::Format::G8_B8R8_2PLANE_420_UNORM)
        .extent(ash::vk::Extent3D {
            width: frame.width,
            height: frame.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(ash::vk::SampleCountFlags::TYPE_1)
        .tiling(image_tiling)
        .usage(ash::vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(ash::vk::SharingMode::EXCLUSIVE)
        .initial_layout(initial_layout)
        .push_next(&mut external_image_info);
    if drm_plane_layouts.is_some() {
        image_info = image_info.push_next(&mut drm_modifier_info);
    }
    let image = unsafe { raw_device.create_image(&image_info, None) }.map_err(|err| {
        // SAFETY: Vulkan did not take ownership of the fd when image creation failed.
        drop(unsafe { OwnedFd::from_raw_fd(import_fd_raw) });
        format!(
            "vulkan_import_create_image_failed:{err:?}:handle={handle_type_label}:tiling={image_tiling:?}:modifier={}:{}",
            native_wgpu_optional_drm_modifier_label(drm_modifier),
            frame_summary
        )
    })?;

    let requirements = unsafe { raw_device.get_image_memory_requirements(image) };
    let memory_type_bits = requirements.memory_type_bits & fd_properties.memory_type_bits;
    let memory_type_index = (0..32)
        .find(|index| (memory_type_bits & (1u32 << index)) != 0)
        .ok_or_else(|| {
            unsafe {
                raw_device.destroy_image(image, None);
            }
            drop(unsafe { OwnedFd::from_raw_fd(import_fd_raw) });
            format!(
                "vulkan_import_no_compatible_memory_type:handle={handle_type_label}:image_bits={:#x}:fd_bits={:#x}:probes={}:modifier={}:{}",
                requirements.memory_type_bits,
                fd_properties.memory_type_bits,
                fd_probe_reports.join("|"),
                native_wgpu_optional_drm_modifier_label(drm_modifier),
                frame_summary
            )
        })?;

    let mut dedicated_info = ash::vk::MemoryDedicatedAllocateInfo::default().image(image);
    let mut import_info = ash::vk::ImportMemoryFdInfoKHR::default()
        .handle_type(handle_type)
        .fd(import_fd_raw);
    import_info.p_next = (&mut dedicated_info as *mut ash::vk::MemoryDedicatedAllocateInfo).cast();
    let allocate_info = ash::vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index)
        .push_next(&mut import_info);
    let memory = match unsafe { raw_device.allocate_memory(&allocate_info, None) } {
        Ok(memory) => memory,
        Err(err) => {
            unsafe {
                raw_device.destroy_image(image, None);
            }
            drop(unsafe { OwnedFd::from_raw_fd(import_fd_raw) });
            return Err(format!(
                "vulkan_import_allocate_memory_failed:{err:?}:handle={handle_type_label}:size={}:type={}:{}",
                requirements.size, memory_type_index, frame_summary
            ));
        }
    };
    if let Err(err) = unsafe { raw_device.bind_image_memory(image, memory, 0) } {
        unsafe {
            raw_device.free_memory(memory, None);
            raw_device.destroy_image(image, None);
        }
        return Err(format!(
            "vulkan_import_bind_image_memory_failed:{err:?}:handle={handle_type_label}:{}",
            frame_summary
        ));
    }

    let drop_device = raw_device.clone();
    let drop_callback: wgpu::hal::DropCallback = Box::new(move || {
        unsafe {
            drop_device.destroy_image(image, None);
            drop_device.free_memory(memory, None);
        }
        drop(frame);
    });
    let hal_texture = unsafe {
        hal_device.texture_from_raw(
            image,
            &hal_desc,
            Some(drop_callback),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };
    let texture =
        unsafe { device.create_texture_from_hal::<wgpu::hal::api::Vulkan>(hal_texture, &desc) };
    Ok(texture)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_external_memory_handle_candidates(
    prefer_dma_buf: bool,
) -> [(&'static str, ash::vk::ExternalMemoryHandleTypeFlags); 2] {
    let opaque_fd = (
        "OPAQUE_FD",
        ash::vk::ExternalMemoryHandleTypeFlags::OPAQUE_FD,
    );
    let dma_buf = (
        "DMA_BUF_EXT",
        ash::vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
    );
    if prefer_dma_buf {
        [dma_buf, opaque_fd]
    } else {
        [opaque_fd, dma_buf]
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_nv12_drm_plane_layouts(
    frame: &NativeWgpuGstDmabufFrame,
    modifier: u64,
) -> Result<Vec<ash::vk::SubresourceLayout>, String> {
    if frame.planes.len() < 2 {
        return Err(format!(
            "vulkan_import_modifier_requires_nv12_two_planes:modifier={}:{}",
            native_wgpu_drm_modifier_label(modifier),
            native_wgpu_gst_dmabuf_frame_summary(frame)
        ));
    }
    let y = frame.planes[0];
    let uv = frame.planes[1];
    if y.fd_index >= frame.fds.len() || uv.fd_index >= frame.fds.len() {
        return Err(format!(
            "vulkan_import_invalid_plane_fd_index:{}",
            native_wgpu_gst_dmabuf_frame_summary(frame)
        ));
    }
    let y_size = if y.fd_index == uv.fd_index && uv.offset > y.offset {
        u64::from(uv.offset - y.offset)
    } else {
        native_wgpu_drm_plane_size_bytes(y.stride, frame.height)?
    };
    let uv_size = native_wgpu_drm_plane_size_bytes(uv.stride, frame.height / 2)?;
    Ok(vec![
        ash::vk::SubresourceLayout {
            offset: u64::from(y.offset),
            size: y_size,
            row_pitch: u64::from(y.stride),
            array_pitch: 0,
            depth_pitch: 0,
        },
        ash::vk::SubresourceLayout {
            offset: u64::from(uv.offset),
            size: uv_size,
            row_pitch: u64::from(uv.stride),
            array_pitch: 0,
            depth_pitch: 0,
        },
    ])
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_drm_plane_size_bytes(stride: u32, rows: u32) -> Result<u64, String> {
    u64::from(stride)
        .checked_mul(u64::from(rows))
        .ok_or_else(|| format!("vulkan_import_plane_size_overflow:stride={stride}:rows={rows}"))
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_dmabuf_frame_summary(frame: &NativeWgpuGstDmabufFrame) -> String {
    format!(
        "source={}:caps_format={}:memory_types={}:fds={}:planes={}:modifier={}:plane_layout={}",
        frame.export_source,
        frame.caps_format,
        frame.memory_types.join("|"),
        frame.fds.len(),
        frame.planes.len(),
        native_wgpu_optional_drm_modifier_label(frame.modifier),
        native_wgpu_gst_dmabuf_plane_summary(&frame.planes)
    )
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_dmabuf_plane_summary(planes: &[NativeWgpuGstDmabufPlane]) -> String {
    planes
        .iter()
        .enumerate()
        .map(|(index, plane)| {
            format!(
                "{}:fd{}+{}@{}",
                index, plane.fd_index, plane.offset, plane.stride
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_optional_drm_modifier_label(modifier: Option<u64>) -> String {
    modifier
        .map(native_wgpu_drm_modifier_label)
        .unwrap_or_else(|| "none".to_owned())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_drm_modifier_label(modifier: u64) -> String {
    format!("{modifier:#018x}")
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuGstDmabufMeta {
    format: gst_video::VideoFormat,
    caps_format: String,
    width: u32,
    height: u32,
    n_planes: u32,
    offsets: Vec<usize>,
    strides: Vec<i32>,
    drm_fourcc: Option<u32>,
    drm_modifier: Option<u64>,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeWgpuGstSystemNv12Plane {
    offset: usize,
    stride: u32,
    width: u32,
    height: u32,
    row_bytes: u32,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeWgpuGstSystemNv12Meta {
    width: u32,
    height: u32,
    y: NativeWgpuGstSystemNv12Plane,
    uv: NativeWgpuGstSystemNv12Plane,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_dmabuf_frame_from_sample(
    sample: &gst::Sample,
) -> Result<NativeWgpuGstDmabufFrame, String> {
    let buffer = sample
        .buffer()
        .ok_or_else(|| "appsink sample has no buffer".to_owned())?;
    let meta = native_wgpu_gst_dmabuf_meta(sample.caps(), buffer)?;
    let memory_types = native_wgpu_gst_memory_types(buffer);
    let export = native_wgpu_dmabuf_export_from_buffer(buffer, &meta)?;
    let gst_buffer = sample
        .buffer_owned()
        .ok_or_else(|| "missing owned GstBuffer for dmabuf lifetime".to_owned())?;

    Ok(NativeWgpuGstDmabufFrame {
        _gst_buffer: gst_buffer,
        fds: export.fds,
        width: meta.width,
        height: meta.height,
        format: export.format,
        modifier: meta.drm_modifier.or(export.modifier),
        planes: export.planes,
        export_source: export.source,
        memory_types,
        caps_format: meta.caps_format,
    })
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_buffer_has_cuda_memory(buffer: &gst::BufferRef) -> bool {
    (0..buffer.n_memory()).any(|index| native_wgpu_is_cuda_memory(buffer.peek_memory(index)))
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_cuda_context_from_buffer(
    buffer: &gst::BufferRef,
) -> Result<*mut NativeWgpuGstCudaContext, NativeWgpuError> {
    for memory_index in 0..buffer.n_memory() {
        let memory = buffer.peek_memory(memory_index);
        if !native_wgpu_is_cuda_memory(memory) {
            continue;
        }
        let cuda_memory = memory.as_ptr().cast_mut().cast::<NativeWgpuGstCudaMemory>();
        let context = unsafe { (*cuda_memory).context };
        if !context.is_null() {
            return Ok(context);
        }
    }
    Err(NativeWgpuError::Video(
        "cuda-direct buffer has no GstCudaContext".to_owned(),
    ))
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_copy_gst_cuda_sample_to_vulkan_staging(
    buffer: &gst::BufferRef,
    meta: &NativeWgpuGstSystemNv12Meta,
    staging: &mut NativeWgpuCudaVulkanStagingBuffer,
) -> Result<(), NativeWgpuError> {
    if staging.cuda_external_memory.mapped_size < staging.layout.size {
        return Err(NativeWgpuError::Video(format!(
            "cuda-direct staging mapped size {} smaller than layout {}",
            staging.cuda_external_memory.mapped_size, staging.layout.size
        )));
    }
    let _guard = NativeWgpuGstCudaContextPushGuard::new(staging.cuda_context)?;
    let y_map = native_wgpu_copy_gst_cuda_plane_to_staging(
        buffer,
        0,
        meta.y.offset,
        meta.y.stride,
        meta.y.row_bytes,
        meta.y.height,
        staging.cuda_context,
        staging.cuda_stream.handle,
        staging.cuda_external_memory.mapped_ptr + staging.layout.y_offset,
        staging.layout.y_stride,
        "y",
    )?;
    let uv_map = match native_wgpu_copy_gst_cuda_plane_to_staging(
        buffer,
        1,
        meta.uv.offset,
        meta.uv.stride,
        meta.uv.row_bytes,
        meta.uv.height,
        staging.cuda_context,
        staging.cuda_stream.handle,
        staging.cuda_external_memory.mapped_ptr + staging.layout.uv_offset,
        staging.layout.uv_stride,
        "uv",
    ) {
        Ok(map) => map,
        Err(err) => {
            let sync_result = native_wgpu_cuda_result(
                unsafe { CuStreamSynchronize(staging.cuda_stream.handle) },
                "cuda-direct synchronize copy stream after failed uv copy",
            );
            drop(y_map);
            sync_result?;
            return Err(err);
        }
    };
    let sync_result = native_wgpu_cuda_result(
        unsafe { CuStreamSynchronize(staging.cuda_stream.handle) },
        "cuda-direct synchronize copy stream",
    );
    drop(uv_map);
    drop(y_map);
    sync_result?;
    Ok(())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[allow(clippy::too_many_arguments)]
fn native_wgpu_copy_gst_cuda_plane_to_staging(
    buffer: &gst::BufferRef,
    plane_index: usize,
    plane_offset: usize,
    source_stride: u32,
    row_bytes: u32,
    height: u32,
    expected_context: *mut NativeWgpuGstCudaContext,
    stream: NativeWgpuCudaStreamHandle,
    destination: NativeWgpuCudaDevicePtr,
    destination_stride: u32,
    label: &str,
) -> Result<NativeWgpuCudaMemoryMap, NativeWgpuError> {
    let plane_end = plane_offset
        .checked_add(1)
        .ok_or_else(|| NativeWgpuError::Video(format!("cuda-direct {label} offset overflow")))?;
    let (memory_range, memory_skip) =
        buffer.find_memory(plane_offset..plane_end).ok_or_else(|| {
            NativeWgpuError::Video(format!("cuda-direct {label} plane has no memory"))
        })?;
    let memory_index = memory_range.start;
    if memory_index >= buffer.n_memory() {
        return Err(NativeWgpuError::Video(format!(
            "cuda-direct {label} memory index out of range"
        )));
    }
    let memory = buffer.peek_memory(memory_index);
    if !native_wgpu_is_cuda_memory(memory) {
        return Err(NativeWgpuError::Video(format!(
            "cuda-direct {label} plane memory is not CUDAMemory: {}",
            native_wgpu_gst_memory_type(memory)
        )));
    }
    let cuda_memory = memory.as_ptr().cast_mut().cast::<NativeWgpuGstCudaMemory>();
    let context = unsafe { (*cuda_memory).context };
    if context != expected_context {
        return Err(NativeWgpuError::Video(format!(
            "cuda-direct {label} plane context changed"
        )));
    }
    unsafe {
        gst_cuda_memory_sync(cuda_memory);
    }
    let map = NativeWgpuCudaMemoryMap::new(memory).map_err(|err| {
        NativeWgpuError::Video(format!("cuda-direct {label} CUDA map failed: {err}"))
    })?;
    let source = map
        .device_ptr()
        .checked_add(u64::try_from(memory_skip).map_err(|_| {
            NativeWgpuError::Video(format!("cuda-direct {label} memory skip too large"))
        })?)
        .ok_or_else(|| NativeWgpuError::Video(format!("cuda-direct {label} source overflow")))?;
    let copy = NativeWgpuCudaMemcpy2D {
        src_x_in_bytes: 0,
        src_y: 0,
        src_memory_type: CUDA_MEMORYTYPE_DEVICE,
        src_host: ptr::null(),
        src_device: source,
        src_array: ptr::null_mut(),
        src_pitch: usize::try_from(source_stride).map_err(|_| {
            NativeWgpuError::Video(format!("cuda-direct {label} source stride too large"))
        })?,
        dst_x_in_bytes: 0,
        dst_y: 0,
        dst_memory_type: CUDA_MEMORYTYPE_DEVICE,
        dst_host: ptr::null_mut(),
        dst_device: destination,
        dst_array: ptr::null_mut(),
        dst_pitch: usize::try_from(destination_stride).map_err(|_| {
            NativeWgpuError::Video(format!("cuda-direct {label} destination stride too large"))
        })?,
        width_in_bytes: usize::try_from(row_bytes).map_err(|_| {
            NativeWgpuError::Video(format!("cuda-direct {label} row bytes too large"))
        })?,
        height: usize::try_from(height)
            .map_err(|_| NativeWgpuError::Video(format!("cuda-direct {label} height too large")))?,
    };
    native_wgpu_cuda_result(
        unsafe { CuMemcpy2DAsync(&copy, stream) },
        &format!("cuda-direct copy {label} plane {plane_index}"),
    )?;
    Ok(map)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuCudaMemoryMap {
    memory: *mut gst::ffi::GstMemory,
    info: gst::ffi::GstMapInfo,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl NativeWgpuCudaMemoryMap {
    fn new(memory: &gst::MemoryRef) -> Result<Self, String> {
        let memory_ptr = memory.as_ptr().cast_mut();
        let mut info = std::mem::MaybeUninit::<gst::ffi::GstMapInfo>::zeroed();
        let mapped =
            unsafe { gst::ffi::gst_memory_map(memory_ptr, info.as_mut_ptr(), GST_MAP_READ_CUDA) }
                != gst::glib::ffi::GFALSE;
        if !mapped {
            return Err(native_wgpu_gst_memory_type(memory));
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

    fn device_ptr(&self) -> NativeWgpuCudaDevicePtr {
        self.info.data as usize as NativeWgpuCudaDevicePtr
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
impl Drop for NativeWgpuCudaMemoryMap {
    fn drop(&mut self) {
        unsafe {
            gst::ffi::gst_memory_unmap(self.memory, &mut self.info);
        }
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_system_nv12_meta(
    sample: &gst::Sample,
    buffer: &gst::BufferRef,
) -> Result<NativeWgpuGstSystemNv12Meta, String> {
    let meta = match native_wgpu_gst_dmabuf_meta(sample.caps(), buffer) {
        Ok(meta) => meta,
        Err(meta_err) => native_wgpu_gst_system_nv12_meta_from_caps(sample)
            .map_err(|caps_err| format!("{meta_err};caps_fallback:{caps_err}"))?,
    };
    if meta.format != gst_video::VideoFormat::Nv12 && meta.caps_format != "NV12" {
        return Err(format!(
            "expected system NV12 appsink frame, got {}",
            meta.caps_format
        ));
    }
    if meta.width == 0 || meta.height == 0 {
        return Err("system NV12 frame has zero dimension".to_owned());
    }
    if meta.width % 2 != 0 || meta.height % 2 != 0 {
        return Err(format!(
            "system NV12 frame dimensions must be even, got {}x{}",
            meta.width, meta.height
        ));
    }
    if meta.offsets.len() < 2 || meta.strides.len() < 2 {
        return Err(format!(
            "system NV12 frame needs 2 planes, got offsets={} strides={}",
            meta.offsets.len(),
            meta.strides.len()
        ));
    }

    let y_stride = native_wgpu_positive_stride("system NV12 y", meta.strides[0])?;
    let uv_stride = native_wgpu_positive_stride("system NV12 uv", meta.strides[1])?;
    let y_row_bytes = meta.width;
    let uv_row_bytes = meta.width;
    if y_stride < y_row_bytes {
        return Err(format!(
            "system NV12 y stride {y_stride} smaller than row bytes {y_row_bytes}"
        ));
    }
    if uv_stride < uv_row_bytes {
        return Err(format!(
            "system NV12 uv stride {uv_stride} smaller than row bytes {uv_row_bytes}"
        ));
    }

    Ok(NativeWgpuGstSystemNv12Meta {
        width: meta.width,
        height: meta.height,
        y: NativeWgpuGstSystemNv12Plane {
            offset: meta.offsets[0],
            stride: y_stride,
            width: meta.width,
            height: meta.height,
            row_bytes: y_row_bytes,
        },
        uv: NativeWgpuGstSystemNv12Plane {
            offset: meta.offsets[1],
            stride: uv_stride,
            width: meta.width / 2,
            height: meta.height / 2,
            row_bytes: uv_row_bytes,
        },
    })
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_system_nv12_meta_from_caps(
    sample: &gst::Sample,
) -> Result<NativeWgpuGstDmabufMeta, String> {
    let caps = sample
        .caps()
        .ok_or_else(|| "appsink sample has no caps".to_owned())?;
    let structure = caps
        .structure(0)
        .ok_or_else(|| "appsink caps has no structure".to_owned())?;
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
    let caps_format = structure
        .get::<String>("format")
        .unwrap_or_else(|_| "unknown".to_owned());
    let y_size = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| "system NV12 plane offset overflow".to_owned())?;
    let stride = i32::try_from(width).map_err(|_| "system NV12 stride too large".to_owned())?;

    Ok(NativeWgpuGstDmabufMeta {
        format: gst_video::VideoFormat::Nv12,
        caps_format,
        width,
        height,
        n_planes: 2,
        offsets: vec![0, y_size],
        strides: vec![stride, stride],
        drm_fourcc: Some(DRM_FORMAT_NV12),
        drm_modifier: None,
    })
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_positive_stride(label: &str, stride: i32) -> Result<u32, String> {
    u32::try_from(stride)
        .ok()
        .filter(|stride| *stride > 0)
        .ok_or_else(|| format!("{label} stride must be positive, got {stride}"))
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_write_system_nv12_plane(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    source: &[u8],
    label: &str,
    aspect: wgpu::TextureAspect,
    offset: usize,
    stride: u32,
    width: u32,
    height: u32,
    row_bytes: u32,
) -> Result<(), NativeWgpuError> {
    if width == 0 || height == 0 {
        return Err(NativeWgpuError::Video(format!(
            "system NV12 {label} plane has zero dimension"
        )));
    }
    native_wgpu_validate_system_nv12_plane_range(
        source.len(),
        label,
        offset,
        stride,
        row_bytes,
        height,
    )?;
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect,
        },
        source,
        wgpu::TexelCopyBufferLayout {
            offset: u64::try_from(offset).map_err(|_| {
                NativeWgpuError::Video(format!("system NV12 {label} offset too large"))
            })?,
            bytes_per_row: Some(stride),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    Ok(())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_validate_system_nv12_plane_range(
    source_len: usize,
    label: &str,
    offset: usize,
    stride: u32,
    row_bytes: u32,
    height: u32,
) -> Result<(), NativeWgpuError> {
    let stride = usize::try_from(stride)
        .map_err(|_| NativeWgpuError::Video(format!("system NV12 {label} stride too large")))?;
    let row_bytes = usize::try_from(row_bytes)
        .map_err(|_| NativeWgpuError::Video(format!("system NV12 {label} row bytes too large")))?;
    if stride < row_bytes {
        return Err(NativeWgpuError::Video(format!(
            "system NV12 {label} stride {stride} smaller than row bytes {row_bytes}"
        )));
    }
    let last_row = usize::try_from(height.saturating_sub(1))
        .ok()
        .and_then(|row| row.checked_mul(stride))
        .and_then(|row_offset| offset.checked_add(row_offset))
        .ok_or_else(|| {
            NativeWgpuError::Video(format!("system NV12 {label} plane offset overflow"))
        })?;
    let end = last_row
        .checked_add(row_bytes)
        .ok_or_else(|| NativeWgpuError::Video(format!("system NV12 {label} plane end overflow")))?;
    if end > source_len {
        return Err(NativeWgpuError::Video(format!(
            "system NV12 {label} plane exceeds mapped buffer: need {end}, have {source_len}"
        )));
    }
    Ok(())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_dmabuf_meta(
    caps: Option<&gst::CapsRef>,
    buffer: &gst::BufferRef,
) -> Result<NativeWgpuGstDmabufMeta, String> {
    let meta = buffer
        .meta::<gst_video::VideoMeta>()
        .ok_or_else(|| "appsink buffer has no GstVideoMeta".to_owned())?;
    let format = meta.format();
    let caps_format = caps
        .and_then(|caps| {
            caps.structure(0)
                .and_then(|structure| structure.get::<String>("format").ok())
        })
        .unwrap_or_else(|| format.to_str().to_string());
    let caps_drm_format = caps.and_then(native_wgpu_caps_drm_format_string);
    let caps_drm = caps_drm_format
        .as_deref()
        .and_then(native_wgpu_drm_fourcc_modifier_from_caps_format);
    let drm_fourcc = caps_drm
        .map(|(fourcc, _)| fourcc)
        .or_else(|| native_wgpu_drm_fourcc_from_video_format(format));
    let drm_modifier = caps_drm.and_then(|(_, modifier)| modifier).or_else(|| {
        caps_drm_format
            .as_deref()
            .and_then(native_wgpu_drm_modifier_from_caps_format)
    });
    Ok(NativeWgpuGstDmabufMeta {
        format,
        caps_format,
        width: meta.width(),
        height: meta.height(),
        n_planes: meta.n_planes(),
        offsets: meta.offset().to_vec(),
        strides: meta.stride().to_vec(),
        drm_fourcc,
        drm_modifier,
    })
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_dmabuf_export_from_buffer(
    buffer: &gst::BufferRef,
    meta: &NativeWgpuGstDmabufMeta,
) -> Result<NativeWgpuGstDmabufExport, String> {
    match native_wgpu_dmabuf_export_from_dmabuf_memory(buffer, meta) {
        Ok(export) => Ok(export),
        Err(dmabuf_err) => match native_wgpu_dmabuf_export_from_cuda_memory(buffer, meta) {
            Ok(export) => Ok(export),
            Err(cuda_err) => match native_wgpu_dmabuf_export_from_gl_memory_egl(buffer, meta) {
                Ok(export) => Ok(export),
                Err(gl_err) => Err(format!(
                    "dmabuf_memory:{dmabuf_err};cuda_memory:{cuda_err};gl_memory:{gl_err}"
                )),
            },
        },
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_required_drm_fourcc(meta: &NativeWgpuGstDmabufMeta) -> Result<u32, String> {
    meta.drm_fourcc.ok_or_else(|| {
        format!(
            "missing_drm_fourcc:caps_format={}:video_format={}",
            meta.caps_format,
            meta.format.to_str()
        )
    })
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_dmabuf_export_from_dmabuf_memory(
    buffer: &gst::BufferRef,
    meta: &NativeWgpuGstDmabufMeta,
) -> Result<NativeWgpuGstDmabufExport, String> {
    let format = native_wgpu_required_drm_fourcc(meta)?;
    let memory_count = buffer.n_memory();
    if memory_count == 0 {
        return Err("buffer_has_no_memory".to_owned());
    }

    let mut fds = Vec::with_capacity(memory_count);
    let mut memory_fd_indices = Vec::with_capacity(memory_count);
    for memory_index in 0..memory_count {
        let memory = buffer.peek_memory(memory_index);
        let fd = native_wgpu_dmabuf_memory_fd(memory).ok_or_else(|| {
            format!(
                "memory_{memory_index}_not_dmabuf:{}",
                native_wgpu_gst_memory_type(memory)
            )
        })?;
        // SAFETY: GStreamer owns the fd returned by gst_dmabuf_memory_get_fd.
        // Clone it so the imported GPU texture can outlive this borrowed view.
        let owned_fd = unsafe { BorrowedFd::borrow_raw(fd) }
            .try_clone_to_owned()
            .map_err(|err| format!("memory_{memory_index}_fd_clone_failed:{err}"))?;
        memory_fd_indices.push(fds.len());
        fds.push(owned_fd);
    }

    let planes = native_wgpu_dmabuf_planes_from_buffer_layout(buffer, meta, |memory_index| {
        memory_fd_indices.get(memory_index).copied()
    })
    .ok_or_else(|| "invalid_dmabuf_plane_layout".to_owned())?;
    Ok(NativeWgpuGstDmabufExport {
        source: "gst-dmabuf-memory",
        format,
        fds,
        planes,
        modifier: meta.drm_modifier,
    })
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_dmabuf_export_from_cuda_memory(
    buffer: &gst::BufferRef,
    meta: &NativeWgpuGstDmabufMeta,
) -> Result<NativeWgpuGstDmabufExport, String> {
    let format = native_wgpu_required_drm_fourcc(meta)?;
    let memory_count = buffer.n_memory();
    if memory_count == 0 {
        return Err("buffer_has_no_memory".to_owned());
    }

    let mut fds = Vec::with_capacity(memory_count);
    let mut memory_fd_indices = Vec::with_capacity(memory_count);
    for memory_index in 0..memory_count {
        let memory = buffer.peek_memory(memory_index);
        if !native_wgpu_is_cuda_memory(memory) {
            return Err(format!(
                "memory_{memory_index}_not_cuda:{}",
                native_wgpu_gst_memory_type(memory)
            ));
        }
        let cuda_memory = memory.as_ptr().cast_mut().cast::<NativeWgpuGstCudaMemory>();
        let alloc_method = unsafe { gst_cuda_memory_get_alloc_method(cuda_memory) };
        if alloc_method != GST_CUDA_MEMORY_ALLOC_MMAP {
            return Err(format!(
                "memory_{memory_index}_cuda_alloc_method_not_mmap:{}",
                native_wgpu_cuda_alloc_method_label(alloc_method)
            ));
        }
        let mut fd = -1;
        let exported =
            unsafe { gst_cuda_memory_export(cuda_memory, (&mut fd as *mut i32).cast::<c_void>()) }
                != gst::glib::ffi::GFALSE;
        if !exported || fd < 0 {
            return Err(format!(
                "memory_{memory_index}_cuda_export_failed:exported={exported}:fd={fd}"
            ));
        }
        memory_fd_indices.push(fds.len());
        // SAFETY: gst_cuda_memory_export returns a newly-opened POSIX fd for
        // CUDA mmap memory.
        fds.push(unsafe { OwnedFd::from_raw_fd(fd) });
    }

    let planes = native_wgpu_dmabuf_planes_from_buffer_layout(buffer, meta, |memory_index| {
        memory_fd_indices.get(memory_index).copied()
    })
    .ok_or_else(|| "invalid_cuda_plane_layout".to_owned())?;
    Ok(NativeWgpuGstDmabufExport {
        source: "gst-cuda-memory-export",
        format,
        fds,
        planes,
        modifier: meta.drm_modifier,
    })
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_dmabuf_export_from_gl_memory_egl(
    buffer: &gst::BufferRef,
    meta: &NativeWgpuGstDmabufMeta,
) -> Result<NativeWgpuGstDmabufExport, String> {
    let format = native_wgpu_required_drm_fourcc(meta)?;
    let plane_count = native_wgpu_video_meta_plane_count(meta)
        .ok_or_else(|| "invalid_video_plane_meta".to_owned())?;
    if buffer.n_memory() < plane_count {
        return Err(format!(
            "memory_count_{}_less_than_plane_count_{plane_count}",
            buffer.n_memory()
        ));
    }

    let mut fds = Vec::with_capacity(plane_count);
    let mut planes = Vec::with_capacity(plane_count);
    let mut source = None;
    for plane_index in 0..plane_count {
        let memory = buffer.peek_memory(plane_index);
        let export = native_wgpu_gl_memory_export_dmabuf(memory)
            .map_err(|err| format!("plane_{plane_index}:{err}"))?;
        let stride = u32::try_from(export.stride)
            .map_err(|_| format!("plane_{plane_index}:invalid_export_stride"))?;
        let offset = u32::try_from(export.offset)
            .map_err(|_| format!("plane_{plane_index}:invalid_export_offset"))?;
        source = Some(export.source);
        fds.push(export.fd);
        planes.push(NativeWgpuGstDmabufPlane {
            fd_index: plane_index,
            offset,
            stride,
        });
    }

    Ok(NativeWgpuGstDmabufExport {
        source: source.unwrap_or("gst-gl-memory"),
        format,
        fds,
        planes,
        modifier: Some(DRM_FORMAT_MOD_INVALID),
    })
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_dmabuf_planes_from_buffer_layout<F>(
    buffer: &gst::BufferRef,
    meta: &NativeWgpuGstDmabufMeta,
    mut fd_index_for_memory: F,
) -> Option<Vec<NativeWgpuGstDmabufPlane>>
where
    F: FnMut(usize) -> Option<usize>,
{
    let plane_count = native_wgpu_video_meta_plane_count(meta)?;
    let mut planes = Vec::with_capacity(plane_count);
    for plane_index in 0..plane_count {
        let plane_offset = meta.offsets[plane_index];
        let plane_stride = u32::try_from(meta.strides[plane_index]).ok()?;
        if plane_stride == 0 {
            return None;
        }
        let (memory_range, memory_skip) =
            buffer.find_memory(plane_offset..plane_offset.saturating_add(1))?;
        let memory_index = memory_range.start;
        let memory = buffer.peek_memory(memory_index);
        let (_, memory_offset, _) = memory.sizes();
        let offset = u32::try_from(memory_offset.checked_add(memory_skip)?).ok()?;
        let fd_index = fd_index_for_memory(memory_index)?;
        planes.push(NativeWgpuGstDmabufPlane {
            fd_index,
            offset,
            stride: plane_stride,
        });
    }
    Some(planes)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_video_meta_plane_count(meta: &NativeWgpuGstDmabufMeta) -> Option<usize> {
    let plane_count = usize::try_from(meta.n_planes).ok()?;
    if plane_count == 0 || meta.offsets.len() < plane_count || meta.strides.len() < plane_count {
        return None;
    }
    Some(plane_count)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_dmabuf_memory_fd(memory: &gst::MemoryRef) -> Option<i32> {
    let is_dmabuf =
        unsafe { gst_is_dmabuf_memory(memory.as_ptr().cast_mut()) } != gst::glib::ffi::GFALSE;
    if !is_dmabuf {
        return None;
    }
    let fd = unsafe { gst_dmabuf_memory_get_fd(memory.as_ptr().cast_mut()) };
    (fd >= 0).then_some(fd)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_is_cuda_memory(memory: &gst::MemoryRef) -> bool {
    if memory.is_type("CUDAMemory") || memory.is_type("gst.cuda.memory") {
        return true;
    }
    let is_cuda = unsafe { gst_is_cuda_memory(memory.as_ptr().cast_mut()) };
    is_cuda != gst::glib::ffi::GFALSE
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_cuda_alloc_method_label(method: i32) -> &'static str {
    match method {
        1 => "malloc",
        2 => "mmap",
        _ => "unknown",
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_cuda_result(result: i32, label: &str) -> Result<(), NativeWgpuError> {
    if result == CUDA_SUCCESS {
        return Ok(());
    }
    Err(NativeWgpuError::Video(format!(
        "{label} failed: {}",
        native_wgpu_cuda_error_label(result)
    )))
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_cuda_error_label(result: i32) -> String {
    let mut name = ptr::null();
    let mut description = ptr::null();
    let name_result = unsafe { CuGetErrorName(result, &mut name) };
    let description_result = unsafe { CuGetErrorString(result, &mut description) };
    let name = if name_result == CUDA_SUCCESS && !name.is_null() {
        unsafe { std::ffi::CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    } else {
        "unknown".to_owned()
    };
    let description = if description_result == CUDA_SUCCESS && !description.is_null() {
        unsafe { std::ffi::CStr::from_ptr(description) }
            .to_string_lossy()
            .into_owned()
    } else {
        "no description".to_owned()
    };
    format!("{result}:{name}:{description}")
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuGlMemoryEglDmabufExport {
    source: &'static str,
    fd: OwnedFd,
    stride: i32,
    offset: usize,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gl_memory_export_dmabuf(
    memory: &gst::MemoryRef,
) -> Result<NativeWgpuGlMemoryEglDmabufExport, String> {
    match native_wgpu_gl_memory_egl_export_dmabuf(memory) {
        Ok(export) => Ok(export),
        Err(egl_err) => match native_wgpu_gl_memory_texture_export_dmabuf(memory) {
            Ok(export) => Ok(export),
            Err(texture_err) => Err(format!("egl:{egl_err};texture:{texture_err}")),
        },
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gl_memory_egl_export_dmabuf(
    memory: &gst::MemoryRef,
) -> Result<NativeWgpuGlMemoryEglDmabufExport, String> {
    let is_gl_egl =
        unsafe { gst_is_gl_memory_egl(memory.as_ptr().cast_mut()) } != gst::glib::ffi::GFALSE;
    if !is_gl_egl {
        return Err(format!(
            "not_gst_gl_memory_egl:{}",
            native_wgpu_gst_memory_type(memory)
        ));
    }
    let image = unsafe { gst_gl_memory_egl_get_image(memory.as_ptr().cast_mut().cast()) }
        .cast::<NativeWgpuGstEGLImage>();
    if image.is_null() {
        return Err("gst_gl_memory_egl_get_image_null".to_owned());
    }
    let mut fd = -1;
    let mut stride = 0;
    let mut offset = 0usize;
    let exported = unsafe { gst_egl_image_export_dmabuf(image, &mut fd, &mut stride, &mut offset) }
        != gst::glib::ffi::GFALSE;
    if !exported || fd < 0 || stride <= 0 {
        return Err(format!(
            "gst_egl_image_export_dmabuf_failed:exported={exported}:fd={fd}:stride={stride}:offset={offset}"
        ));
    }
    Ok(NativeWgpuGlMemoryEglDmabufExport {
        source: "gst-gl-memory-egl",
        // SAFETY: gst_egl_image_export_dmabuf returns a newly-opened dmabuf fd.
        fd: unsafe { OwnedFd::from_raw_fd(fd) },
        stride,
        offset,
    })
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
struct NativeWgpuGlMemoryTextureExportState {
    gl_memory: *mut NativeWgpuGstGLMemory,
    fd: i32,
    stride: i32,
    offset: usize,
    success: bool,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
unsafe extern "C" fn native_wgpu_gl_memory_texture_export_thread(
    context: *mut c_void,
    data: *mut c_void,
) {
    let state = unsafe { &mut *(data.cast::<NativeWgpuGlMemoryTextureExportState>()) };
    let image = unsafe {
        gst_egl_image_from_texture(
            context.cast::<NativeWgpuGstGLContext>(),
            state.gl_memory,
            ptr::null_mut(),
        )
    };
    if image.is_null() {
        return;
    }
    state.success = unsafe {
        gst_egl_image_export_dmabuf(image, &mut state.fd, &mut state.stride, &mut state.offset)
    } != gst::glib::ffi::GFALSE;
    unsafe {
        gst::ffi::gst_mini_object_unref(image.cast::<gst::ffi::GstMiniObject>());
    }
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gl_memory_texture_export_dmabuf(
    memory: &gst::MemoryRef,
) -> Result<NativeWgpuGlMemoryEglDmabufExport, String> {
    let is_gl_memory =
        unsafe { gst_is_gl_memory(memory.as_ptr().cast_mut()) } != gst::glib::ffi::GFALSE;
    if !is_gl_memory {
        return Err(format!(
            "not_gst_gl_memory:{}",
            native_wgpu_gst_memory_type(memory)
        ));
    }
    let gl_memory = memory.as_ptr().cast_mut().cast::<NativeWgpuGstGLMemory>();
    let context = unsafe { (*gl_memory).base.context };
    if context.is_null() {
        return Err("gl_memory_context_null".to_owned());
    }
    let mut state = NativeWgpuGlMemoryTextureExportState {
        gl_memory,
        fd: -1,
        stride: 0,
        offset: 0,
        success: false,
    };
    unsafe {
        gst_gl_context_thread_add(
            context.cast::<c_void>(),
            Some(native_wgpu_gl_memory_texture_export_thread),
            (&mut state as *mut NativeWgpuGlMemoryTextureExportState).cast::<c_void>(),
        );
    }
    if !state.success || state.fd < 0 || state.stride <= 0 {
        return Err(format!(
            "texture_export_failed:success={}:fd={}:stride={}:offset={}",
            state.success, state.fd, state.stride, state.offset
        ));
    }
    Ok(NativeWgpuGlMemoryEglDmabufExport {
        source: "gst-gl-memory-texture",
        // SAFETY: gst_egl_image_export_dmabuf returns a newly-opened dmabuf fd.
        fd: unsafe { OwnedFd::from_raw_fd(state.fd) },
        stride: state.stride,
        offset: state.offset,
    })
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_drm_fourcc_from_video_format(format: gst_video::VideoFormat) -> Option<u32> {
    use gst::glib::translate::IntoGlib;

    let fourcc = unsafe { gst_video_dma_drm_fourcc_from_format(format.into_glib()) };
    (fourcc != 0).then_some(fourcc)
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_caps_drm_format_string(caps: &gst::CapsRef) -> Option<String> {
    caps.structure(0)
        .and_then(|structure| structure.get::<String>("drm-format").ok())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_drm_modifier_from_caps_format(format: &str) -> Option<u64> {
    let (_, modifier) = format.rsplit_once(':')?;
    let modifier = modifier
        .strip_prefix("0x")
        .or_else(|| modifier.strip_prefix("0X"))
        .unwrap_or(modifier);
    u64::from_str_radix(modifier, 16).ok()
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_drm_fourcc_modifier_from_caps_format(format: &str) -> Option<(u32, Option<u64>)> {
    let format = CString::new(format).ok()?;
    let mut modifier = 0u64;
    let fourcc = unsafe { gst_video_dma_drm_fourcc_from_string(format.as_ptr(), &mut modifier) };
    (fourcc != 0).then_some((
        fourcc,
        (modifier != DRM_FORMAT_MOD_INVALID).then_some(modifier),
    ))
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_memory_types(buffer: &gst::BufferRef) -> Vec<String> {
    (0..buffer.n_memory())
        .map(|index| native_wgpu_gst_memory_type(buffer.peek_memory(index)))
        .collect()
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_gst_memory_type(memory: &gst::MemoryRef) -> String {
    for memory_type in ["CUDAMemory", "GLMemory", "DMABuf", "SystemMemory"] {
        if memory.is_type(memory_type) {
            return memory_type.to_owned();
        }
    }
    memory
        .allocator()
        .map(|allocator| allocator.memory_type().to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
const DRM_FORMAT_NV12: u32 = 0x3231_564e;
#[cfg(feature = "native-wgpu-gst-dmabuf")]
const DRM_FORMAT_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[link(name = "gstvideo-1.0")]
unsafe extern "C" {
    fn gst_video_dma_drm_fourcc_from_format(format: i32) -> u32;
    fn gst_video_dma_drm_fourcc_from_string(format_str: *const c_char, modifier: *mut u64) -> u32;
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[link(name = "gstallocators-1.0")]
unsafe extern "C" {
    fn gst_is_dmabuf_memory(mem: *mut gst::ffi::GstMemory) -> gst::glib::ffi::gboolean;
    fn gst_dmabuf_memory_get_fd(mem: *mut gst::ffi::GstMemory) -> i32;
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
struct NativeWgpuGstGLMemoryEGL {
    _private: [u8; 0],
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
struct NativeWgpuGstCudaMemory {
    mem: gst::ffi::GstMemory,
    context: *mut NativeWgpuGstCudaContext,
    info: gst_video::ffi::GstVideoInfo,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
struct NativeWgpuGstCudaContext {
    _private: [u8; 0],
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
struct NativeWgpuGstGLBaseMemory {
    mem: gst::ffi::GstMemory,
    context: *mut NativeWgpuGstGLContext,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
struct NativeWgpuGstGLMemory {
    base: NativeWgpuGstGLBaseMemory,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
struct NativeWgpuGstGLContext {
    _private: [u8; 0],
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
struct NativeWgpuGstEGLImage {
    _private: [u8; 0],
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[link(name = "gstgl-1.0")]
unsafe extern "C" {
    fn gst_is_gl_memory(mem: *mut gst::ffi::GstMemory) -> gst::glib::ffi::gboolean;
    fn gst_is_gl_memory_egl(mem: *mut gst::ffi::GstMemory) -> gst::glib::ffi::gboolean;
    fn gst_gl_memory_egl_get_image(mem: *mut NativeWgpuGstGLMemoryEGL) -> *mut c_void;
    fn gst_egl_image_from_texture(
        context: *mut NativeWgpuGstGLContext,
        gl_mem: *mut NativeWgpuGstGLMemory,
        attribs: *mut usize,
    ) -> *mut NativeWgpuGstEGLImage;
    fn gst_egl_image_export_dmabuf(
        image: *mut NativeWgpuGstEGLImage,
        fd: *mut i32,
        stride: *mut i32,
        offset: *mut usize,
    ) -> gst::glib::ffi::gboolean;
    fn gst_gl_context_thread_add(
        context: *mut c_void,
        func: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
        data: *mut c_void,
    );
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
const GST_CUDA_MEMORY_ALLOC_MMAP: i32 = 2;
#[cfg(feature = "native-wgpu-gst-dmabuf")]
const GST_MAP_READ_CUDA: gst::ffi::GstMapFlags =
    gst::ffi::GST_MAP_READ | (gst::ffi::GST_MAP_FLAG_LAST << 1);
#[cfg(feature = "native-wgpu-gst-dmabuf")]
const CUDA_SUCCESS: i32 = 0;
#[cfg(feature = "native-wgpu-gst-dmabuf")]
const CUDA_MEMORYTYPE_DEVICE: u32 = 2;
#[cfg(feature = "native-wgpu-gst-dmabuf")]
const CUDA_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD: u32 = 1;
#[cfg(feature = "native-wgpu-gst-dmabuf")]
const CUDA_STREAM_NON_BLOCKING: u32 = 1;

#[cfg(feature = "native-wgpu-gst-dmabuf")]
type NativeWgpuCudaDevicePtr = u64;
#[cfg(feature = "native-wgpu-gst-dmabuf")]
type NativeWgpuCudaExternalMemoryHandle = *mut c_void;
#[cfg(feature = "native-wgpu-gst-dmabuf")]
type NativeWgpuCudaArrayHandle = *mut c_void;
#[cfg(feature = "native-wgpu-gst-dmabuf")]
type NativeWgpuCudaStreamHandle = *mut c_void;

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeWgpuCudaExternalMemoryWin32Handle {
    handle: *mut c_void,
    name: *const c_void,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
union NativeWgpuCudaExternalMemoryHandleUnion {
    fd: i32,
    win32: NativeWgpuCudaExternalMemoryWin32Handle,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
struct NativeWgpuCudaExternalMemoryHandleDesc {
    type_: u32,
    handle: NativeWgpuCudaExternalMemoryHandleUnion,
    size: u64,
    flags: u32,
    reserved: [u32; 16],
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
struct NativeWgpuCudaExternalMemoryBufferDesc {
    offset: u64,
    size: u64,
    flags: u32,
    reserved: [u32; 16],
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[repr(C)]
struct NativeWgpuCudaMemcpy2D {
    src_x_in_bytes: usize,
    src_y: usize,
    src_memory_type: u32,
    src_host: *const c_void,
    src_device: NativeWgpuCudaDevicePtr,
    src_array: NativeWgpuCudaArrayHandle,
    src_pitch: usize,
    dst_x_in_bytes: usize,
    dst_y: usize,
    dst_memory_type: u32,
    dst_host: *mut c_void,
    dst_device: NativeWgpuCudaDevicePtr,
    dst_array: NativeWgpuCudaArrayHandle,
    dst_pitch: usize,
    width_in_bytes: usize,
    height: usize,
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
#[link(name = "gstcuda-1.0")]
#[allow(clashing_extern_declarations)]
unsafe extern "C" {
    fn CuGetErrorName(error: i32, p_str: *mut *const c_char) -> i32;
    fn CuGetErrorString(error: i32, p_str: *mut *const c_char) -> i32;
    fn CuMemcpy2DAsync(
        copy: *const NativeWgpuCudaMemcpy2D,
        stream: NativeWgpuCudaStreamHandle,
    ) -> i32;
    fn CuStreamCreate(stream_out: *mut NativeWgpuCudaStreamHandle, flags: u32) -> i32;
    fn CuStreamDestroy(stream: NativeWgpuCudaStreamHandle) -> i32;
    fn CuStreamSynchronize(stream: NativeWgpuCudaStreamHandle) -> i32;
    fn CuImportExternalMemory(
        ext_mem_out: *mut NativeWgpuCudaExternalMemoryHandle,
        mem_handle_desc: *const NativeWgpuCudaExternalMemoryHandleDesc,
    ) -> i32;
    fn CuExternalMemoryGetMappedBuffer(
        dev_ptr: *mut NativeWgpuCudaDevicePtr,
        ext_mem: NativeWgpuCudaExternalMemoryHandle,
        buffer_desc: *const NativeWgpuCudaExternalMemoryBufferDesc,
    ) -> i32;
    fn CuDestroyExternalMemory(ext_mem: NativeWgpuCudaExternalMemoryHandle) -> i32;
    fn gst_cuda_load_library() -> gst::glib::ffi::gboolean;
    fn gst_cuda_context_new(device_id: u32) -> *mut NativeWgpuGstCudaContext;
    fn gst_cuda_context_push(ctx: *mut NativeWgpuGstCudaContext) -> gst::glib::ffi::gboolean;
    fn gst_cuda_context_pop(cuda_ctx: *mut *mut c_void) -> gst::glib::ffi::gboolean;
    fn gst_context_new_cuda_context(
        cuda_ctx: *mut NativeWgpuGstCudaContext,
    ) -> *mut gst::ffi::GstContext;
    fn gst_cuda_buffer_pool_new(
        context: *mut NativeWgpuGstCudaContext,
    ) -> *mut gst::ffi::GstBufferPool;
    fn gst_buffer_pool_config_set_cuda_alloc_method(
        config: *mut gst::ffi::GstStructure,
        method: i32,
    );
    fn gst_is_cuda_memory(mem: *mut gst::ffi::GstMemory) -> gst::glib::ffi::gboolean;
    fn gst_cuda_memory_sync(mem: *mut NativeWgpuGstCudaMemory);
    fn gst_cuda_memory_get_alloc_method(mem: *mut NativeWgpuGstCudaMemory) -> i32;
    fn gst_cuda_memory_export(mem: *mut NativeWgpuGstCudaMemory, os_handle: *mut c_void) -> i32;
}

#[cfg(feature = "video-renderer")]
struct NativeWgpuVideoSampleInfo {
    width: u32,
    height: u32,
    format: String,
    offset: u64,
    stride: u32,
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeWgpuVideoUploadReport {
    width: u32,
    height: u32,
    format: String,
    source_stride: u32,
    upload_stride: u32,
    uploaded_frames: u64,
}

#[cfg(feature = "video-renderer")]
fn native_wgpu_video_sample_info(
    sample: &gst::Sample,
    buffer: &gst::BufferRef,
) -> Result<NativeWgpuVideoSampleInfo, NativeWgpuError> {
    let caps = sample
        .caps()
        .ok_or_else(|| NativeWgpuError::Video("appsink sample has no caps".to_owned()))?;
    let structure = caps
        .structure(0)
        .ok_or_else(|| NativeWgpuError::Video("appsink caps has no structure".to_owned()))?;
    let width = structure
        .get::<i32>("width")
        .map_err(|_| NativeWgpuError::Video("appsink caps missing width".to_owned()))?;
    let height = structure
        .get::<i32>("height")
        .map_err(|_| NativeWgpuError::Video("appsink caps missing height".to_owned()))?;
    let format = structure
        .get::<String>("format")
        .unwrap_or_else(|_| "unknown".to_owned());
    let width = u32::try_from(width)
        .ok()
        .filter(|width| *width > 0)
        .ok_or_else(|| NativeWgpuError::Video("invalid appsink frame width".to_owned()))?;
    let height = u32::try_from(height)
        .ok()
        .filter(|height| *height > 0)
        .ok_or_else(|| NativeWgpuError::Video("invalid appsink frame height".to_owned()))?;
    let row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| NativeWgpuError::Video("video row byte count overflow".to_owned()))?;
    let (offset, stride) = buffer
        .meta::<gst_video::VideoMeta>()
        .and_then(|meta| {
            let offset = meta.offset().first().copied()?;
            let stride = meta.stride().first().copied()?;
            let stride = u32::try_from(stride).ok()?;
            Some((u64::try_from(offset).ok()?, stride))
        })
        .unwrap_or((0, row_bytes));
    Ok(NativeWgpuVideoSampleInfo {
        width,
        height,
        format,
        offset,
        stride,
    })
}

#[cfg(any(
    feature = "video-renderer",
    feature = "native-wgpu-gpu-video",
    feature = "native-wgpu-gst-dmabuf"
))]
fn video_uv_transform(
    fit: crate::core::FitMode,
    source_size: (u32, u32),
    surface_size: (u32, u32),
) -> ([f32; 2], [f32; 2]) {
    if matches!(fit, crate::core::FitMode::Stretch) {
        return ([0.0, 0.0], [1.0, 1.0]);
    }
    let source_aspect = source_size.0 as f32 / source_size.1.max(1) as f32;
    let surface_aspect = surface_size.0.max(1) as f32 / surface_size.1.max(1) as f32;
    if matches!(
        fit,
        crate::core::FitMode::Contain | crate::core::FitMode::Center
    ) {
        return ([0.0, 0.0], [1.0, 1.0]);
    }
    if source_aspect > surface_aspect {
        let width = (surface_aspect / source_aspect).clamp(0.0, 1.0);
        ([(1.0 - width) * 0.5, 0.0], [width, 1.0])
    } else {
        let height = (source_aspect / surface_aspect).clamp(0.0, 1.0);
        ([0.0, (1.0 - height) * 0.5], [1.0, height])
    }
}

#[cfg(any(
    feature = "video-renderer",
    feature = "native-wgpu-gpu-video",
    feature = "native-wgpu-gst-dmabuf"
))]
fn write_f32_pair(destination: &mut [u8], pair: [f32; 2]) {
    destination[0..4].copy_from_slice(&pair[0].to_ne_bytes());
    destination[4..8].copy_from_slice(&pair[1].to_ne_bytes());
}

#[cfg(feature = "video-renderer")]
const NATIVE_WGPU_VIDEO_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Fit {
    offset: vec2<f32>,
    scale: vec2<f32>,
};

@group(0) @binding(0) var video_texture: texture_2d<f32>;
@group(0) @binding(1) var video_sampler: sampler;
@group(0) @binding(2) var<uniform> fit: Fit;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 2.0),
        vec2<f32>(2.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = fit.offset + uvs[vertex_index] * fit.scale;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(video_texture, video_sampler, input.uv);
}
"#;

#[cfg(any(feature = "native-wgpu-gpu-video", feature = "native-wgpu-gst-dmabuf"))]
const NATIVE_WGPU_NV12_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Fit {
    offset: vec2<f32>,
    scale: vec2<f32>,
};

@group(0) @binding(0) var y_texture: texture_2d<f32>;
@group(0) @binding(1) var uv_texture: texture_2d<f32>;
@group(0) @binding(2) var video_sampler: sampler;
@group(0) @binding(3) var<uniform> fit: Fit;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(3.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 2.0),
        vec2<f32>(2.0, 0.0),
        vec2<f32>(0.0, 0.0),
    );

    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = fit.offset + uvs[vertex_index] * fit.scale;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var y = textureSample(y_texture, video_sampler, input.uv).x;
    var uv = textureSample(uv_texture, video_sampler, input.uv).xy;
    var u = uv.x;
    var v = uv.y;

    y = clamp((y - (16.0 / 255.0)) / 0.85882352941, 0.0, 1.0);
    u = clamp((u - (16.0 / 255.0)) / 0.87843137254, 0.0, 1.0);
    v = clamp((v - (16.0 / 255.0)) / 0.87843137254, 0.0, 1.0);

    let r = y + 1.5748 * (v - 0.5);
    let g = y - 0.1873 * (u - 0.5) - 0.4681 * (v - 0.5);
    let b = y + 1.8556 * (u - 0.5);

    return vec4<f32>(
        clamp(r, 0.0, 1.0),
        clamp(g, 0.0, 1.0),
        clamp(b, 0.0, 1.0),
        1.0,
    );
}
"#;

fn blend_channel(from: f64, to: f64, amount: f64) -> f64 {
    (from + (to - from) * amount).clamp(0.0, 1.0)
}

#[cfg(feature = "video-renderer")]
fn align_to(value: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return value;
    }
    value.div_ceil(alignment) * alignment
}

fn average_fps(frames: u64, elapsed: Duration) -> f64 {
    let elapsed = elapsed.as_secs_f64();
    if elapsed <= f64::EPSILON {
        return 0.0;
    }
    frames as f64 / elapsed
}

fn pick_present_mode(supported: &[wgpu::PresentMode]) -> wgpu::PresentMode {
    const PREFERRED: &[wgpu::PresentMode] = &[
        wgpu::PresentMode::FifoRelaxed,
        wgpu::PresentMode::Fifo,
        wgpu::PresentMode::Mailbox,
        wgpu::PresentMode::Immediate,
    ];
    for mode in PREFERRED {
        if supported.contains(mode) {
            return *mode;
        }
    }
    supported
        .first()
        .copied()
        .unwrap_or(wgpu::PresentMode::Fifo)
}

fn native_wgpu_required_features(
    adapter: &wgpu::Adapter,
) -> Result<wgpu::Features, NativeWgpuError> {
    let features = native_wgpu_required_feature_flags();
    let adapter_features = adapter.features();
    if !adapter_features.contains(features) {
        return Err(NativeWgpuError::Wgpu(format!(
            "adapter missing required wgpu features: {:?}",
            features - adapter_features
        )));
    }
    Ok(features)
}

fn native_wgpu_required_feature_flags() -> wgpu::Features {
    wgpu::Features::empty() | native_wgpu_required_nv12_feature()
}

#[cfg(feature = "native-wgpu-gst-dmabuf")]
fn native_wgpu_required_nv12_feature() -> wgpu::Features {
    wgpu::Features::TEXTURE_FORMAT_NV12
}

#[cfg(not(feature = "native-wgpu-gst-dmabuf"))]
fn native_wgpu_required_nv12_feature() -> wgpu::Features {
    wgpu::Features::empty()
}
