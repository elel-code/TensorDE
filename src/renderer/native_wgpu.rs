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
}

impl fmt::Display for NativeWgpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(err) => write!(f, "wayland error: {err}"),
            Self::Timeout(err) => write!(f, "timeout: {err}"),
            Self::Wgpu(err) => write!(f, "wgpu error: {err}"),
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

struct NativeWgpuSurfaceRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    color: NativeWgpuColor,
    render_mode: NativeWgpuRenderMode,
    started: Instant,
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
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gilder-native-wgpu-clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
}

fn blend_channel(from: f64, to: f64, amount: f64) -> f64 {
    (from + (to - from) * amount).clamp(0.0, 1.0)
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
