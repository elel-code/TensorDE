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

#[cfg(feature = "native-wgpu-gpu-video")]
use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek, SeekFrom},
    sync::{
        Arc,
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
        unsafe_policy: "unsafe is limited to raw Wayland handle surface creation",
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
        while started.elapsed() < duration && !self.host.is_closed() {
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
                    let message = format!(
                        "{}: {}",
                        err.src()
                            .map(|src| src.path_string())
                            .unwrap_or_else(|| "gstreamer".into()),
                        err.error()
                    );
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

#[cfg(feature = "video-renderer")]
fn native_wgpu_gst_element(name: &str) -> Result<gst::Element, NativeWgpuError> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|err| NativeWgpuError::Video(err.to_string()))
}

#[cfg(feature = "video-renderer")]
fn native_wgpu_pad_is_video(pad: &gst::Pad) -> bool {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    caps.structure(0)
        .map(|structure| structure.name().starts_with("video/"))
        .unwrap_or(false)
}

#[cfg(feature = "native-wgpu-gpu-video")]
struct NativeWgpuGpuVideoPlayer {
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
    ) -> Result<Self, NativeWgpuError> {
        if !native_wgpu_is_annex_b_h264(source) {
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
                );
            })
            .map_err(|err| NativeWgpuError::Video(format!("spawn gpu-video worker: {err}")))?;
        Ok(Self {
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
            backend: "gpu-video",
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
) {
    if let Err(err) =
        native_wgpu_gpu_video_decode_loop(source, loop_playback, vulkan_device, &event_tx, &stop)
    {
        let _ = native_wgpu_gpu_video_send_event(
            &event_tx,
            &stop,
            NativeWgpuGpuVideoDecoderEvent::Error(err),
        );
    }
}

#[cfg(feature = "native-wgpu-gpu-video")]
fn native_wgpu_gpu_video_decode_loop(
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
    #[cfg(feature = "native-wgpu-gpu-video")]
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
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gilder-native-wgpu-device"),
                required_features: wgpu::Features::empty(),
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
            #[cfg(feature = "native-wgpu-gpu-video")]
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
        #[cfg(feature = "native-wgpu-gpu-video")]
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
        #[cfg(feature = "native-wgpu-gpu-video")]
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

#[cfg(feature = "native-wgpu-gpu-video")]
struct NativeWgpuNv12VideoRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    fit_buffer: wgpu::Buffer,
    texture: Option<wgpu::Texture>,
    bind_group: Option<wgpu::BindGroup>,
    source_size: Option<(u32, u32)>,
    fit: crate::core::FitMode,
    presented_frames: u64,
}

#[cfg(feature = "native-wgpu-gpu-video")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeWgpuNv12PresentReport {
    width: u32,
    height: u32,
    format: String,
    presented_frames: u64,
}

#[cfg(feature = "native-wgpu-gpu-video")]
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

#[cfg(any(feature = "video-renderer", feature = "native-wgpu-gpu-video"))]
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

#[cfg(any(feature = "video-renderer", feature = "native-wgpu-gpu-video"))]
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

#[cfg(feature = "native-wgpu-gpu-video")]
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
