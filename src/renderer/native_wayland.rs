//! Native Wayland layer-shell host for future non-GTK renderers.
//!
//! This module owns only the Wayland surface lifecycle. Content renderers
//! such as video, web, shader, or scene runtimes should be layered on top of
//! the surface host here.

#[cfg(feature = "video-renderer")]
use gst::prelude::*;
#[cfg(feature = "video-renderer")]
use gst_video::prelude::*;
#[cfg(feature = "video-renderer")]
use gstreamer as gst;
#[cfg(feature = "video-renderer")]
use gstreamer_video as gst_video;
use serde::Serialize;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_dmabuf, delegate_layer, delegate_output, delegate_registry,
    delegate_seat, delegate_shm,
    dmabuf::{DmabufFeedback, DmabufHandler, DmabufState},
    output::{OutputHandler, OutputInfo, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{
        Shm, ShmHandler,
        slot::{Buffer, SlotPool},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString, c_void},
    fmt,
    fs::{self, File, OpenOptions},
    io::ErrorKind,
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd},
        raw::c_char,
        unix::fs::MetadataExt,
    },
    path::PathBuf,
    ptr::{self, NonNull},
    sync::{Arc, Mutex},
    time::Instant,
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    backend::WaylandError,
    globals::registry_queue_init,
    protocol::{wl_buffer, wl_output, wl_seat, wl_shm, wl_surface},
};
use wayland_protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    linux_dmabuf::zv1::client::{zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1},
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeWaylandHostOptions {
    pub namespace: String,
    pub layer: NativeWaylandLayer,
    pub output_name: Option<String>,
    pub opaque_region: bool,
    pub input_passthrough: bool,
}

impl Default for NativeWaylandHostOptions {
    fn default() -> Self {
        Self {
            namespace: "gilder-wallpaper-native".to_owned(),
            layer: NativeWaylandLayer::Bottom,
            output_name: None,
            opaque_region: true,
            input_passthrough: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeWaylandLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

impl NativeWaylandLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Overlay => "overlay",
        }
    }
}

impl std::str::FromStr for NativeWaylandLayer {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "background" => Ok(Self::Background),
            "bottom" => Ok(Self::Bottom),
            "top" => Ok(Self::Top),
            "overlay" => Ok(Self::Overlay),
            other => Err(format!("unsupported native Wayland layer: {other}")),
        }
    }
}

impl From<NativeWaylandLayer> for Layer {
    fn from(layer: NativeWaylandLayer) -> Self {
        match layer {
            NativeWaylandLayer::Background => Layer::Background,
            NativeWaylandLayer::Bottom => Layer::Bottom,
            NativeWaylandLayer::Top => Layer::Top,
            NativeWaylandLayer::Overlay => Layer::Overlay,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeWaylandCapabilities {
    pub built: bool,
    pub experimental: bool,
    pub owns_wlr_layer_shell_surface: bool,
    pub exports_raw_wayland_handles: bool,
    pub native_video_overlay: bool,
    pub supports_fractional_scale_protocol: bool,
    pub supports_viewporter_protocol: bool,
    pub probes_linux_dmabuf_protocol: bool,
    pub native_dmabuf_buffer_attach: bool,
    pub consumes_render_sync: bool,
    pub unsafe_policy: &'static str,
}

pub fn capabilities() -> NativeWaylandCapabilities {
    NativeWaylandCapabilities {
        built: true,
        experimental: true,
        owns_wlr_layer_shell_surface: true,
        exports_raw_wayland_handles: true,
        native_video_overlay: cfg!(feature = "video-renderer"),
        supports_fractional_scale_protocol: true,
        supports_viewporter_protocol: true,
        probes_linux_dmabuf_protocol: true,
        native_dmabuf_buffer_attach: cfg!(feature = "video-renderer"),
        consumes_render_sync: false,
        unsafe_policy: "unsafe is allowed but must stay behind audited native Wayland/GPU boundaries",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeWaylandSurfaceSnapshot {
    pub logical_size: Option<(u32, u32)>,
    pub scale_num: u32,
    pub scale_den: u32,
    pub configured: bool,
    pub surface_protocol_id: u32,
    pub layer: NativeWaylandLayer,
    pub requested_output_name: Option<String>,
    pub selected_output: Option<NativeWaylandOutputSnapshot>,
    pub known_outputs: Vec<NativeWaylandOutputSnapshot>,
    pub parent_mapping_buffer_attached: bool,
    pub opaque_region_enabled: bool,
    pub input_passthrough_enabled: bool,
    pub dmabuf: NativeWaylandDmabufSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeWaylandDmabufSnapshot {
    pub supports_linux_dmabuf_protocol: bool,
    pub linux_dmabuf_version: Option<u32>,
    pub linux_dmabuf_modifier_count: usize,
    pub linux_dmabuf_modifier_samples: Vec<NativeWaylandDmabufFormatSnapshot>,
    pub linux_dmabuf_feedback_requested: bool,
    pub linux_dmabuf_default_feedback_requested: bool,
    pub linux_dmabuf_surface_feedback_requested: bool,
    pub linux_dmabuf_feedback_received: bool,
    pub linux_dmabuf_feedback_count: u64,
    pub linux_dmabuf_feedback: Option<NativeWaylandDmabufFeedbackSnapshot>,
    pub dmabuf_buffers_created: u64,
    pub dmabuf_buffer_create_failures: u64,
    pub dmabuf_buffers_released: u64,
    pub dmabuf_frames_submitted: u64,
    pub dmabuf_frames_attached: u64,
    pub dmabuf_frame_attach_failures: u64,
    pub dmabuf_frame_attach_skips: u64,
    pub dmabuf_buffers_pending: usize,
    pub dmabuf_buffers_in_flight: usize,
    pub dmabuf_last_frame_format: Option<u32>,
    pub dmabuf_last_frame_modifier: Option<u64>,
    pub dmabuf_last_attach_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeWaylandDmabufFeedbackSnapshot {
    pub source: NativeWaylandDmabufFeedbackSource,
    pub main_device: u64,
    pub format_count: usize,
    pub format_fourcc_count: usize,
    pub format_fourccs: Vec<u32>,
    pub format_table: Vec<NativeWaylandDmabufFormatSnapshot>,
    pub format_samples: Vec<NativeWaylandDmabufFormatSnapshot>,
    pub tranche_count: usize,
    pub tranche_samples: Vec<NativeWaylandDmabufTrancheSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeWaylandDmabufFeedbackSource {
    Default,
    Surface,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeWaylandDmabufFormatSnapshot {
    pub format: u32,
    pub modifier: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeWaylandDmabufTrancheSnapshot {
    pub device: u64,
    pub flags: String,
    pub format_count: usize,
    pub format_indices_sample: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeWaylandOutputSnapshot {
    pub id: u32,
    pub name: Option<String>,
    pub description: Option<String>,
    pub make: String,
    pub model: String,
    pub logical_position: Option<(i32, i32)>,
    pub logical_size: Option<(i32, i32)>,
    pub scale_factor: i32,
    pub current_mode: Option<NativeWaylandOutputModeSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeWaylandOutputModeSnapshot {
    pub width: i32,
    pub height: i32,
    pub refresh_millihertz: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWaylandSurfaceHandles {
    pub display: NonNull<c_void>,
    pub surface: NonNull<c_void>,
    pub logical_size: (u32, u32),
    pub dmabuf_main_device: Option<u64>,
}

impl NativeWaylandSurfaceHandles {
    pub fn window_handle(self) -> usize {
        self.surface.as_ptr() as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeWaylandError {
    Wayland(String),
    MissingRawHandle(&'static str),
    Timeout(String),
    #[cfg(feature = "video-renderer")]
    GStreamer(String),
}

impl fmt::Display for NativeWaylandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(err) => write!(f, "wayland error: {err}"),
            Self::MissingRawHandle(handle) => write!(f, "missing Wayland {handle} handle"),
            Self::Timeout(message) => write!(f, "timeout: {message}"),
            #[cfg(feature = "video-renderer")]
            Self::GStreamer(err) => write!(f, "GStreamer error: {err}"),
        }
    }
}

impl std::error::Error for NativeWaylandError {}

pub struct NativeWaylandHost {
    connection: Connection,
    event_queue: EventQueue<NativeWaylandState>,
    state: NativeWaylandState,
}

impl NativeWaylandHost {
    pub fn connect(options: NativeWaylandHostOptions) -> Result<Self, NativeWaylandError> {
        let connection = Connection::connect_to_env()
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let (globals, mut event_queue) = registry_queue_init(&connection)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let qh = event_queue.handle();

        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let shm =
            Shm::bind(&globals, &qh).map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let surface = compositor.create_surface(&qh);

        let fractional_manager: Option<WpFractionalScaleManagerV1> =
            globals.bind(&qh, 1..=1, NativeWaylandProtocolData).ok();
        let fractional_scale = fractional_manager
            .as_ref()
            .map(|manager| manager.get_fractional_scale(&surface, &qh, NativeWaylandProtocolData));
        let viewporter: Option<WpViewporter> =
            globals.bind(&qh, 1..=1, NativeWaylandProtocolData).ok();
        let viewport = viewporter
            .as_ref()
            .map(|viewporter| viewporter.get_viewport(&surface, &qh, NativeWaylandProtocolData));

        let mut state = NativeWaylandState {
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            dmabuf_state: DmabufState::new(&globals, &qh),
            shm,
            compositor,
            layer: None,
            layer_kind: options.layer,
            requested_output_name: options.output_name.clone(),
            selected_output_id: None,
            scale: NativeScaleState::new(
                fractional_manager,
                fractional_scale,
                viewporter,
                viewport,
            ),
            logical_size: None,
            configured: false,
            opaque_region_enabled: options.opaque_region,
            input_passthrough_enabled: options.input_passthrough,
            opaque_region: None,
            input_region: None,
            parent_mapping_buffer: None,
            dmabuf_runtime: NativeDmabufRuntimeState::default(),
        };

        event_queue
            .roundtrip(&mut state)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let output = select_native_output(&state.output_state, options.output_name.as_deref())?;
        state.selected_output_id = output
            .as_ref()
            .and_then(|output| state.output_state.info(output).map(|info| info.id));

        let layer = layer_shell.create_layer_surface(
            &qh,
            surface,
            options.layer.into(),
            Some(options.namespace.clone()),
            output.as_ref(),
        );
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(-1);
        layer.set_anchor(Anchor::all());
        layer.set_size(0, 0);
        layer.commit();
        state.layer = Some(layer);
        state.request_dmabuf_feedback(&qh);

        event_queue
            .roundtrip(&mut state)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;

        Ok(Self {
            connection,
            event_queue,
            state,
        })
    }

    pub fn dispatch_pending(&mut self) -> Result<(), NativeWaylandError> {
        self.dispatch_queued_events().map(|_| ())
    }

    fn flush_events(&self) -> Result<(), NativeWaylandError> {
        match self.event_queue.flush() {
            Ok(()) => Ok(()),
            Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => Ok(()),
            Err(err) => Err(NativeWaylandError::Wayland(err.to_string())),
        }
    }

    fn dispatch_queued_events(&mut self) -> Result<usize, NativeWaylandError> {
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))
    }

    pub fn pump_events(&mut self) -> Result<(), NativeWaylandError> {
        self.dispatch_queued_events()?;
        self.flush_events()?;

        for _ in 0..8 {
            let Some(read_guard) = self.event_queue.prepare_read() else {
                self.dispatch_queued_events()?;
                continue;
            };

            let fd = read_guard.connection_fd();
            let mut fds = [rustix::event::PollFd::new(
                &fd,
                rustix::event::PollFlags::IN | rustix::event::PollFlags::ERR,
            )];
            let timeout = rustix::event::Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            let ready = loop {
                match rustix::event::poll(&mut fds, Some(&timeout)) {
                    Ok(ready) => break ready,
                    Err(rustix::io::Errno::INTR) => continue,
                    Err(err) => {
                        return Err(NativeWaylandError::Wayland(format!(
                            "poll Wayland fd: {err}"
                        )));
                    }
                }
            };
            if ready == 0 {
                return Ok(());
            }

            match read_guard.read() {
                Ok(_) => {
                    self.dispatch_queued_events()?;
                    self.flush_events()?;
                }
                Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => {
                    return Ok(());
                }
                Err(err) => return Err(NativeWaylandError::Wayland(err.to_string())),
            }
        }

        Ok(())
    }

    pub fn blocking_dispatch(&mut self) -> Result<(), NativeWaylandError> {
        self.event_queue
            .blocking_dispatch(&mut self.state)
            .map(|_| ())
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))
    }

    pub fn roundtrip(&mut self) -> Result<(), NativeWaylandError> {
        self.event_queue
            .roundtrip(&mut self.state)
            .map(|_| ())
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))
    }

    pub fn wait_until_configured(&mut self, rounds: usize) -> Result<(), NativeWaylandError> {
        for _ in 0..rounds {
            if self.state.configured {
                return Ok(());
            }
            self.roundtrip()?;
        }
        Err(NativeWaylandError::Timeout(format!(
            "native Wayland layer surface was not configured after {rounds} roundtrips"
        )))
    }

    pub fn snapshot(&self) -> NativeWaylandSurfaceSnapshot {
        self.state.snapshot()
    }

    pub fn surface_handles(&self) -> Result<NativeWaylandSurfaceHandles, NativeWaylandError> {
        let display = NonNull::new(self.connection.backend().display_ptr().cast::<c_void>())
            .ok_or(NativeWaylandError::MissingRawHandle("display"))?;
        let layer = self
            .state
            .layer
            .as_ref()
            .ok_or(NativeWaylandError::MissingRawHandle("layer surface"))?;
        let surface = NonNull::new(layer.wl_surface().id().as_ptr().cast::<c_void>())
            .ok_or(NativeWaylandError::MissingRawHandle("surface"))?;
        let logical_size = self
            .state
            .logical_size
            .ok_or(NativeWaylandError::MissingRawHandle(
                "configured surface size",
            ))?;
        Ok(NativeWaylandSurfaceHandles {
            display,
            surface,
            logical_size,
            dmabuf_main_device: self
                .state
                .dmabuf_runtime
                .latest_feedback
                .as_ref()
                .map(|feedback| feedback.main_device),
        })
    }

    #[cfg(feature = "video-renderer")]
    fn present_dmabuf_frame(
        &mut self,
        frame: NativeWaylandDmabufFrame,
    ) -> Result<(), NativeWaylandError> {
        let qh = self.event_queue.handle();
        self.state.present_dmabuf_frame(&qh, frame)?;
        self.connection
            .flush()
            .map(|_| ())
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))
    }

    #[cfg(feature = "video-renderer")]
    fn can_accept_dmabuf_frame(&self) -> bool {
        self.state.can_accept_dmabuf_frame()
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeWaylandVideoOptions {
    pub host: NativeWaylandHostOptions,
    pub output_name: String,
    pub fit: crate::core::FitMode,
    pub muted: bool,
    pub loop_playback: bool,
    pub target_max_fps: Option<u32>,
    pub sink_throttle: bool,
    pub decoder_policy: crate::config::VideoDecoderPolicy,
    pub start_offset_ms: u64,
    pub pipeline: NativeWaylandVideoPipeline,
    pub debug_visible_frame: bool,
}

#[cfg(feature = "video-renderer")]
impl Default for NativeWaylandVideoOptions {
    fn default() -> Self {
        Self {
            host: NativeWaylandHostOptions::default(),
            output_name: "native-wayland".to_owned(),
            fit: crate::core::FitMode::Cover,
            muted: true,
            loop_playback: true,
            target_max_fps: Some(240),
            sink_throttle: false,
            decoder_policy: crate::config::VideoDecoderPolicy::HardwarePreferred,
            start_offset_ms: 0,
            pipeline: NativeWaylandVideoPipeline::AppsinkDmabufPresent,
            debug_visible_frame: false,
        }
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeWaylandVideoPipeline {
    Playbin,
    Playbin3,
    ExplicitH264Gl,
    AppsinkProbe,
    AppsinkMmapProbe,
    AppsinkDmabufPresent,
}

#[cfg(feature = "video-renderer")]
impl NativeWaylandVideoPipeline {
    pub fn playbin_element_name(self) -> Option<&'static str> {
        match self {
            Self::Playbin => Some("playbin"),
            Self::Playbin3 => Some("playbin3"),
            Self::ExplicitH264Gl
            | Self::AppsinkProbe
            | Self::AppsinkMmapProbe
            | Self::AppsinkDmabufPresent => None,
        }
    }

    pub fn uses_legacy_waylandsink(self) -> bool {
        matches!(self, Self::Playbin | Self::Playbin3)
    }

    pub fn uses_gst_wayland_surface_context(self) -> bool {
        matches!(self, Self::Playbin | Self::Playbin3 | Self::ExplicitH264Gl)
    }
}

#[cfg(feature = "video-renderer")]
impl std::str::FromStr for NativeWaylandVideoPipeline {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "playbin" => Ok(Self::Playbin),
            "playbin3" => Ok(Self::Playbin3),
            "explicit-h264-gl" | "h264-gl" | "gl-h264" => Ok(Self::ExplicitH264Gl),
            "appsink-probe" | "appsink" | "probe" => Ok(Self::AppsinkProbe),
            "appsink-mmap-probe" | "appsink-mmap" | "mmap-probe" => Ok(Self::AppsinkMmapProbe),
            "appsink-dmabuf-present" | "dmabuf-present" | "present" => {
                Ok(Self::AppsinkDmabufPresent)
            }
            other => Err(format!("unsupported native video pipeline: {other}")),
        }
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWaylandVideoSnapshot {
    pub runtime_elapsed_ms: u64,
    pub surface: NativeWaylandSurfaceSnapshot,
    pub pipeline: crate::renderer::video::VideoPipelineSnapshot,
    pub sink_stats: NativeWaylandSinkStats,
    pub video_pipeline: NativeWaylandVideoPipeline,
    pub video_sink: String,
    pub video_fit: crate::core::FitMode,
    pub render_rectangle: Option<NativeWaylandRenderRectangle>,
    pub appsink_probe: Option<NativeWaylandAppsinkProbeSnapshot>,
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWaylandVideoRuntimeSnapshot {
    pub runtime_elapsed_ms: u64,
    pub surface: NativeWaylandSurfaceSnapshot,
    pub pipeline: NativeWaylandVideoRuntimePipelineSnapshot,
    pub sink_stats: NativeWaylandSinkStats,
    pub video_pipeline: NativeWaylandVideoPipeline,
    pub video_sink: String,
    pub video_fit: crate::core::FitMode,
    pub render_rectangle: Option<NativeWaylandRenderRectangle>,
    pub appsink_probe: Option<NativeWaylandAppsinkProbeSnapshot>,
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeWaylandVideoRuntimePipelineSnapshot {
    pub gst_state: String,
    pub frame_stats: crate::renderer::video::VideoFrameStats,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct NativeWaylandSinkStats {
    pub rendered: Option<u64>,
    pub dropped: Option<u64>,
    pub average_rate: Option<f64>,
    pub raw: Option<String>,
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeWaylandRenderRectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct NativeWaylandAppsinkProbeSnapshot {
    pub pulled_samples: u64,
    pub last_caps: Option<String>,
    pub last_memory_count: usize,
    pub last_memory_types: Vec<String>,
    pub last_buffer_size: Option<usize>,
    pub last_video_meta: Option<NativeWaylandAppsinkVideoMetaSnapshot>,
    pub last_cuda_alloc_method: Option<String>,
    pub last_cuda_export_fd: Option<i32>,
    pub cuda_export_successes: u64,
    pub cuda_export_failures: u64,
    pub cuda_export_fds_closed: u64,
    pub last_dmabuf_export_source: Option<String>,
    pub last_dmabuf_export_fd_count: usize,
    pub last_dmabuf_export_plane_count: usize,
    pub last_dmabuf_export_error: Option<String>,
    pub last_dmabuf_copy_fallback_error: Option<String>,
    pub dmabuf_export_successes: u64,
    pub dmabuf_export_failures: u64,
    pub allocation_queries: u64,
    pub allocation_caps: Option<String>,
    pub allocation_need_pool: Option<bool>,
    pub allocation_pool_size: Option<usize>,
    pub allocation_mmap_pool_proposals: u64,
    pub allocation_mmap_pool_failures: u64,
    pub allocation_last_result: Option<String>,
    pub memory_type_counts: BTreeMap<String, u64>,
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct NativeWaylandAppsinkVideoMetaSnapshot {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub n_planes: u32,
    pub offsets: Vec<usize>,
    pub strides: Vec<i32>,
    pub caps_drm_format: Option<String>,
    pub drm_fourcc: Option<u32>,
    pub drm_modifier: Option<u64>,
    pub exported_cuda_fd_count: usize,
    pub exported_dmabuf_fd_count: usize,
    pub dmabuf_export_source: Option<String>,
    pub dmabuf_export_error: Option<String>,
    pub dmabuf_attach_ready: bool,
    pub dmabuf_attach_blockers: Vec<String>,
}

#[cfg(feature = "video-renderer")]
struct NativeWaylandDmabufFrame {
    fds: Vec<OwnedFd>,
    gst_buffer: gst::Buffer,
    gbm_bo: Option<NativeGbmBo>,
    export_source: &'static str,
    width: u32,
    height: u32,
    format: u32,
    modifier: Option<u64>,
    planes: Vec<NativeWaylandDmabufPlane>,
}

#[cfg(feature = "video-renderer")]
struct NativeWaylandDmabufPlane {
    fd_index: usize,
    offset: u32,
    stride: u32,
}

#[cfg(feature = "video-renderer")]
struct NativeWaylandDmabufExport {
    source: &'static str,
    format: u32,
    fds: Vec<OwnedFd>,
    gbm_bo: Option<NativeGbmBo>,
    planes: Vec<NativeWaylandDmabufPlane>,
    modifier: Option<u64>,
}

#[cfg(feature = "video-renderer")]
struct NativeGbmDmabufAllocator {
    main_device: Option<u64>,
    device: Option<NativeGbmDevice>,
    last_copy_fallback_error: Option<String>,
}

#[cfg(feature = "video-renderer")]
impl NativeGbmDmabufAllocator {
    fn new(main_device: Option<u64>) -> Self {
        Self {
            main_device,
            device: None,
            last_copy_fallback_error: None,
        }
    }

    fn export_copy(
        &mut self,
        buffer: &gst::BufferRef,
        meta: &NativeWaylandAppsinkVideoMetaSnapshot,
        debug_visible_frame: bool,
    ) -> Result<NativeWaylandDmabufExport, String> {
        if meta.format != "NV12" {
            return Err(format!("unsupported_copy_source_format:{}", meta.format));
        }
        self.last_copy_fallback_error = None;
        if debug_visible_frame {
            return self.export_debug_xrgb_pattern(meta.width, meta.height);
        }
        let map = buffer
            .map_readable()
            .map_err(|_| "source_buffer_map_read_failed".to_owned())?;
        let source = map.as_slice();
        self.export_nv12_copy(source, meta).or_else(|nv12_err| {
            self.last_copy_fallback_error = Some(format!("nv12_copy:{nv12_err}"));
            self.export_xrgb_copy(source, meta)
                .map_err(|xrgb_err| format!("nv12_copy:{nv12_err};xrgb_copy:{xrgb_err}"))
        })
    }

    fn last_copy_fallback_error(&self) -> Option<String> {
        self.last_copy_fallback_error.clone()
    }

    fn export_nv12_copy(
        &mut self,
        source: &[u8],
        meta: &NativeWaylandAppsinkVideoMetaSnapshot,
    ) -> Result<NativeWaylandDmabufExport, String> {
        let layout = NativeNv12CopyLayout::from_meta(meta, source.len())?;
        let mut bo = self.create_bo(meta.width, meta.height, DRM_FORMAT_NV12)?;
        if bo.plane_count() < 2 {
            return Err(format!(
                "gbm_nv12_bo_plane_count_less_than_2:{}",
                bo.plane_count()
            ));
        }
        let dst_y_stride = bo.plane_stride(0);
        let dst_uv_stride = bo.plane_stride(1);
        let dst_y_offset = bo.plane_offset(0);
        let dst_uv_offset = bo.plane_offset(1);

        let map_result = bo.map_write(meta.width, meta.height, |dst_base, map_stride| unsafe {
            let dst_y_stride = dst_y_stride.unwrap_or(map_stride);
            let dst_uv_stride = dst_uv_stride.unwrap_or(dst_y_stride);
            let dst_y_offset = dst_y_offset.unwrap_or(0);
            let dst_uv_offset =
                dst_uv_offset.unwrap_or(dst_y_offset + dst_y_stride as usize * layout.height);
            native_copy_plane_rows(
                source,
                dst_base,
                layout.y_offset,
                layout.y_stride,
                dst_y_offset,
                dst_y_stride as usize,
                layout.width,
                layout.height,
            )?;
            native_copy_plane_rows(
                source,
                dst_base,
                layout.uv_offset,
                layout.uv_stride,
                dst_uv_offset,
                dst_uv_stride as usize,
                layout.width,
                layout.height / 2,
            )
        });
        if let Err(map_err) = map_result {
            let dst_y_stride =
                dst_y_stride.ok_or_else(|| format!("{map_err};gbm_nv12_y_stride_unavailable"))?;
            let dst_uv_stride = dst_uv_stride.unwrap_or(dst_y_stride);
            let dst_y_offset = dst_y_offset.unwrap_or(0);
            let dst_uv_offset =
                dst_uv_offset.unwrap_or(dst_y_offset + dst_y_stride as usize * layout.height);
            bo.mmap_write_nv12(
                source,
                &layout,
                dst_y_offset,
                dst_y_stride as usize,
                dst_uv_offset,
                dst_uv_stride as usize,
            )
            .map_err(|mmap_err| format!("{map_err};mmap_nv12:{mmap_err}"))?;
        }

        bo.export("gbm-linear-nv12-copy", DRM_FORMAT_NV12)
    }

    fn export_xrgb_copy(
        &mut self,
        source: &[u8],
        meta: &NativeWaylandAppsinkVideoMetaSnapshot,
    ) -> Result<NativeWaylandDmabufExport, String> {
        let layout = NativeNv12CopyLayout::from_meta(meta, source.len())?;
        let mut bo = self.create_bo(meta.width, meta.height, DRM_FORMAT_XRGB8888)?;
        let dst_stride = bo.plane_stride(0);
        bo.map_write(meta.width, meta.height, |dst_base, map_stride| unsafe {
            let dst_stride = dst_stride.unwrap_or(map_stride) as usize;
            native_copy_nv12_to_xrgb(source, dst_base, dst_stride, &layout)
        })?;
        bo.export("gbm-linear-xrgb-copy", DRM_FORMAT_XRGB8888)
    }

    fn export_debug_xrgb_pattern(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<NativeWaylandDmabufExport, String> {
        let width = usize::try_from(width).map_err(|_| "debug_width_overflow".to_owned())?;
        let height = usize::try_from(height).map_err(|_| "debug_height_overflow".to_owned())?;
        let mut bo = self.create_bo(width as u32, height as u32, DRM_FORMAT_XRGB8888)?;
        let dst_stride = bo.plane_stride(0);
        bo.map_write(width as u32, height as u32, |dst_base, map_stride| unsafe {
            let dst_stride = dst_stride.unwrap_or(map_stride) as usize;
            native_fill_xrgb_debug_pattern(dst_base, dst_stride, width, height)
        })?;
        bo.export("gbm-debug-xrgb-pattern", DRM_FORMAT_XRGB8888)
    }

    fn create_bo(&mut self, width: u32, height: u32, format: u32) -> Result<NativeGbmBo, String> {
        let device = self.device()?;
        let flag_sets = [
            GBM_BO_USE_LINEAR | GBM_BO_USE_RENDERING | GBM_BO_USE_WRITE,
            GBM_BO_USE_LINEAR | GBM_BO_USE_WRITE,
            GBM_BO_USE_LINEAR | GBM_BO_USE_RENDERING,
            GBM_BO_USE_LINEAR,
        ];
        let mut unsupported = Vec::new();
        for flags in flag_sets {
            let supported = unsafe {
                gbm_device_is_format_supported(device.ptr.as_ptr(), format, flags)
                    != gst::glib::ffi::GFALSE
            };
            if !supported {
                unsupported.push(format!("flags=0x{flags:x}"));
            }
            let ptr = unsafe { gbm_bo_create(device.ptr.as_ptr(), width, height, format, flags) };
            if let Some(ptr) = NonNull::new(ptr) {
                return Ok(NativeGbmBo { ptr });
            }
        }
        Err(format!(
            "gbm_bo_create_failed:path={}:format={format}:{}x{}:{}",
            device.path.display(),
            width,
            height,
            unsupported.join("|")
        ))
    }

    fn device(&mut self) -> Result<&mut NativeGbmDevice, String> {
        if self.device.is_none() {
            self.device = Some(NativeGbmDevice::open(self.main_device)?);
        }
        self.device
            .as_mut()
            .ok_or_else(|| "gbm_device_unavailable".to_owned())
    }
}

#[cfg(feature = "video-renderer")]
unsafe impl Send for NativeGbmDmabufAllocator {}

#[cfg(feature = "video-renderer")]
struct NativeGbmDevice {
    _file: File,
    path: PathBuf,
    ptr: NonNull<NativeGbmDeviceRaw>,
}

#[cfg(feature = "video-renderer")]
impl NativeGbmDevice {
    fn open(main_device: Option<u64>) -> Result<Self, String> {
        let path = native_gbm_render_node_path(main_device)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|err| format!("open_gbm_render_node_failed:{}:{err}", path.display()))?;
        let ptr = unsafe { gbm_create_device(file.as_raw_fd()) };
        let ptr = NonNull::new(ptr)
            .ok_or_else(|| format!("gbm_create_device_failed:{}", path.display()))?;
        Ok(Self {
            _file: file,
            path,
            ptr,
        })
    }
}

#[cfg(feature = "video-renderer")]
impl Drop for NativeGbmDevice {
    fn drop(&mut self) {
        unsafe {
            gbm_device_destroy(self.ptr.as_ptr());
        }
    }
}

#[cfg(feature = "video-renderer")]
struct NativeGbmBo {
    ptr: NonNull<NativeGbmBoRaw>,
}

#[cfg(feature = "video-renderer")]
impl NativeGbmBo {
    fn plane_count(&self) -> u32 {
        unsafe { gbm_bo_get_plane_count(self.ptr.as_ptr()) }
    }

    fn plane_stride(&self, plane: i32) -> Option<u32> {
        let stride = unsafe { gbm_bo_get_stride_for_plane(self.ptr.as_ptr(), plane) };
        (stride > 0).then_some(stride)
    }

    fn plane_offset(&self, plane: i32) -> Option<usize> {
        let offset = unsafe { gbm_bo_get_offset(self.ptr.as_ptr(), plane) };
        usize::try_from(offset).ok()
    }

    fn modifier(&self) -> u64 {
        unsafe { gbm_bo_get_modifier(self.ptr.as_ptr()) }
    }

    fn map_write<F>(&mut self, width: u32, height: u32, mut copy: F) -> Result<(), String>
    where
        F: FnMut(*mut u8, u32) -> Result<(), String>,
    {
        let mut stride = 0u32;
        let mut map_data = ptr::null_mut::<c_void>();
        let data = unsafe {
            gbm_bo_map(
                self.ptr.as_ptr(),
                0,
                0,
                width,
                height,
                GBM_BO_TRANSFER_WRITE,
                &mut stride,
                &mut map_data,
            )
        };
        if data.is_null() || map_data.is_null() || stride == 0 {
            return Err(format!(
                "gbm_bo_map_write_failed:data_null={}:map_data_null={}:stride={stride}",
                data.is_null(),
                map_data.is_null()
            ));
        }
        let result = copy(data.cast::<u8>(), stride);
        unsafe {
            gbm_bo_unmap(self.ptr.as_ptr(), map_data);
        }
        result
    }

    fn export(
        self,
        source: &'static str,
        _format: u32,
    ) -> Result<NativeWaylandDmabufExport, String> {
        let plane_count = self.plane_count();
        if plane_count == 0 || plane_count > GBM_MAX_PLANES {
            return Err(format!("invalid_gbm_plane_count:{plane_count}"));
        }
        let modifier = self.modifier();
        let mut fds = Vec::with_capacity(plane_count as usize);
        let mut planes = Vec::with_capacity(plane_count as usize);
        for plane in 0..plane_count {
            let plane_i32 =
                i32::try_from(plane).map_err(|_| "gbm_plane_index_overflow".to_owned())?;
            let stride = self
                .plane_stride(plane_i32)
                .ok_or_else(|| format!("missing_gbm_plane_stride:{plane}"))?;
            let offset = self
                .plane_offset(plane_i32)
                .ok_or_else(|| format!("missing_gbm_plane_offset:{plane}"))?;
            let offset = u32::try_from(offset)
                .map_err(|_| format!("gbm_plane_offset_overflow:{plane}:{offset}"))?;
            fds.push(self.plane_fd(plane)?);
            planes.push(NativeWaylandDmabufPlane {
                fd_index: plane as usize,
                offset,
                stride,
            });
        }
        Ok(NativeWaylandDmabufExport {
            source,
            format: _format,
            fds,
            gbm_bo: Some(self),
            planes,
            modifier: Some(modifier),
        })
    }

    fn mmap_write_nv12(
        &self,
        source: &[u8],
        layout: &NativeNv12CopyLayout,
        dst_y_offset: usize,
        dst_y_stride: usize,
        dst_uv_offset: usize,
        dst_uv_stride: usize,
    ) -> Result<(), String> {
        let y_fd = self.plane_fd(0)?;
        let uv_fd = self.plane_fd(1)?;
        unsafe {
            native_mmap_copy_plane_rows(
                source,
                y_fd.as_fd(),
                layout.y_offset,
                layout.y_stride,
                dst_y_offset,
                dst_y_stride,
                layout.width,
                layout.height,
            )?;
            native_mmap_copy_plane_rows(
                source,
                uv_fd.as_fd(),
                layout.uv_offset,
                layout.uv_stride,
                dst_uv_offset,
                dst_uv_stride,
                layout.width,
                layout.height / 2,
            )
        }
    }

    fn plane_fd(&self, plane: u32) -> Result<OwnedFd, String> {
        let plane_i32 =
            i32::try_from(plane).map_err(|_| format!("gbm_plane_index_overflow:{plane}"))?;
        let fd = unsafe { gbm_bo_get_fd_for_plane(self.ptr.as_ptr(), plane_i32) };
        if fd < 0 {
            return Err(format!("gbm_bo_get_fd_for_plane_failed:{plane}"));
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

#[cfg(feature = "video-renderer")]
impl Drop for NativeGbmBo {
    fn drop(&mut self) {
        unsafe {
            gbm_bo_destroy(self.ptr.as_ptr());
        }
    }
}

#[cfg(feature = "video-renderer")]
unsafe impl Send for NativeGbmBo {}

#[cfg(feature = "video-renderer")]
struct NativeNv12CopyLayout {
    width: usize,
    height: usize,
    y_offset: usize,
    uv_offset: usize,
    y_stride: usize,
    uv_stride: usize,
}

#[cfg(feature = "video-renderer")]
impl NativeNv12CopyLayout {
    fn from_meta(
        meta: &NativeWaylandAppsinkVideoMetaSnapshot,
        source_len: usize,
    ) -> Result<Self, String> {
        let width = usize::try_from(meta.width).map_err(|_| "width_overflow".to_owned())?;
        let height = usize::try_from(meta.height).map_err(|_| "height_overflow".to_owned())?;
        if width == 0 || height == 0 || height % 2 != 0 {
            return Err(format!("invalid_nv12_size:{}x{}", meta.width, meta.height));
        }
        if meta.offsets.len() < 2 || meta.strides.len() < 2 {
            return Err("missing_nv12_plane_meta".to_owned());
        }
        let y_stride = usize::try_from(meta.strides[0])
            .ok()
            .filter(|stride| *stride >= width)
            .ok_or_else(|| format!("invalid_nv12_y_stride:{}", meta.strides[0]))?;
        let uv_stride = usize::try_from(meta.strides[1])
            .ok()
            .filter(|stride| *stride >= width)
            .ok_or_else(|| format!("invalid_nv12_uv_stride:{}", meta.strides[1]))?;
        native_validate_plane_range(meta.offsets[0], y_stride, width, height, source_len)
            .map_err(|err| format!("source_y:{err}"))?;
        native_validate_plane_range(meta.offsets[1], uv_stride, width, height / 2, source_len)
            .map_err(|err| format!("source_uv:{err}"))?;
        Ok(Self {
            width,
            height,
            y_offset: meta.offsets[0],
            uv_offset: meta.offsets[1],
            y_stride,
            uv_stride,
        })
    }
}

#[cfg(feature = "video-renderer")]
fn native_gbm_render_node_path(main_device: Option<u64>) -> Result<PathBuf, String> {
    let mut fallback = Vec::new();
    let entries = fs::read_dir("/dev/dri").map_err(|err| format!("read_dev_dri_failed:{err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read_dev_dri_entry_failed:{err}"))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("renderD") {
            continue;
        }
        let rdev = entry
            .metadata()
            .map_err(|err| format!("stat_dri_node_failed:{}:{err}", path.display()))?
            .rdev();
        if main_device == Some(rdev) {
            return Ok(path);
        }
        fallback.push(path);
    }
    fallback.sort();
    fallback
        .into_iter()
        .next()
        .ok_or_else(|| format!("no_render_node_for_dmabuf_main_device:{main_device:?}"))
}

#[cfg(feature = "video-renderer")]
fn native_validate_plane_range(
    offset: usize,
    stride: usize,
    row_bytes: usize,
    rows: usize,
    len: usize,
) -> Result<(), String> {
    if rows == 0 {
        return Ok(());
    }
    let last_row = rows - 1;
    let end = offset
        .checked_add(
            stride
                .checked_mul(last_row)
                .ok_or_else(|| "stride_rows_overflow".to_owned())?,
        )
        .and_then(|value| value.checked_add(row_bytes))
        .ok_or_else(|| "plane_end_overflow".to_owned())?;
    if end > len {
        return Err(format!("plane_out_of_bounds:end={end}:len={len}"));
    }
    Ok(())
}

#[cfg(feature = "video-renderer")]
fn native_plane_required_len(
    offset: usize,
    stride: usize,
    row_bytes: usize,
    rows: usize,
) -> Result<usize, String> {
    if rows == 0 {
        return Ok(offset);
    }
    let last_row = rows - 1;
    offset
        .checked_add(
            stride
                .checked_mul(last_row)
                .ok_or_else(|| "stride_rows_overflow".to_owned())?,
        )
        .and_then(|value| value.checked_add(row_bytes))
        .ok_or_else(|| "plane_end_overflow".to_owned())
}

#[cfg(feature = "video-renderer")]
unsafe fn native_copy_plane_rows(
    source: &[u8],
    dst_base: *mut u8,
    src_offset: usize,
    src_stride: usize,
    dst_offset: usize,
    dst_stride: usize,
    row_bytes: usize,
    rows: usize,
) -> Result<(), String> {
    native_validate_plane_range(src_offset, src_stride, row_bytes, rows, source.len())?;
    native_validate_plane_range(dst_offset, dst_stride, row_bytes, rows, usize::MAX)?;
    if dst_base.is_null() {
        return Err("copy_destination_null".to_owned());
    }
    for row in 0..rows {
        let src_start = src_offset + src_stride * row;
        let dst_start = dst_offset + dst_stride * row;
        unsafe {
            ptr::copy_nonoverlapping(
                source.as_ptr().add(src_start),
                dst_base.add(dst_start),
                row_bytes,
            );
        }
    }
    Ok(())
}

#[cfg(feature = "video-renderer")]
unsafe fn native_mmap_copy_plane_rows(
    source: &[u8],
    fd: BorrowedFd<'_>,
    src_offset: usize,
    src_stride: usize,
    dst_offset: usize,
    dst_stride: usize,
    row_bytes: usize,
    rows: usize,
) -> Result<(), String> {
    native_validate_plane_range(src_offset, src_stride, row_bytes, rows, source.len())?;
    let map_len = native_plane_required_len(dst_offset, dst_stride, row_bytes, rows)?;
    if map_len == 0 {
        return Ok(());
    }
    let data = unsafe {
        libc::mmap(
            ptr::null_mut(),
            map_len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd.as_raw_fd(),
            0,
        )
    };
    if data == libc::MAP_FAILED {
        return Err(format!(
            "dmabuf_mmap_failed:len={map_len}:{}",
            std::io::Error::last_os_error()
        ));
    }
    let result = unsafe {
        native_copy_plane_rows(
            source,
            data.cast::<u8>(),
            src_offset,
            src_stride,
            dst_offset,
            dst_stride,
            row_bytes,
            rows,
        )
    };
    let unmap_result = unsafe { libc::munmap(data, map_len) };
    if unmap_result != 0 {
        return Err(format!(
            "dmabuf_munmap_failed:len={map_len}:{}",
            std::io::Error::last_os_error()
        ));
    }
    result
}

#[cfg(feature = "video-renderer")]
unsafe fn native_copy_nv12_to_xrgb(
    source: &[u8],
    dst_base: *mut u8,
    dst_stride: usize,
    layout: &NativeNv12CopyLayout,
) -> Result<(), String> {
    if dst_stride < layout.width * 4 {
        return Err(format!(
            "xrgb_destination_stride_too_small:{dst_stride}<{}",
            layout.width * 4
        ));
    }
    native_validate_plane_range(0, dst_stride, layout.width * 4, layout.height, usize::MAX)?;
    for y in 0..layout.height {
        let y_row = layout.y_offset + layout.y_stride * y;
        let uv_row = layout.uv_offset + layout.uv_stride * (y / 2);
        let dst_row = dst_stride * y;
        for x in 0..layout.width {
            let y_value = source[y_row + x] as i32;
            let uv_index = uv_row + (x / 2) * 2;
            let u_value = source[uv_index] as i32;
            let v_value = source[uv_index + 1] as i32;
            let (r, g, b) = native_yuv_to_rgb(y_value, u_value, v_value);
            let dst = dst_row + x * 4;
            unsafe {
                *dst_base.add(dst) = b;
                *dst_base.add(dst + 1) = g;
                *dst_base.add(dst + 2) = r;
                *dst_base.add(dst + 3) = 0xff;
            }
        }
    }
    Ok(())
}

#[cfg(feature = "video-renderer")]
unsafe fn native_fill_xrgb_debug_pattern(
    dst_base: *mut u8,
    dst_stride: usize,
    width: usize,
    height: usize,
) -> Result<(), String> {
    if dst_base.is_null() {
        return Err("debug_pattern_destination_null".to_owned());
    }
    if dst_stride < width * 4 {
        return Err(format!(
            "debug_pattern_stride_too_small:{dst_stride}<{}",
            width * 4
        ));
    }

    let colors = [
        (0xffu8, 0x00u8, 0xffu8),
        (0x00, 0xff, 0xff),
        (0xff, 0xff, 0x00),
        (0xff, 0x20, 0x20),
        (0x20, 0xff, 0x20),
        (0x20, 0x80, 0xff),
    ];
    let bar_width = (width / colors.len()).max(1);
    for y in 0..height {
        let dst_row = dst_stride * y;
        for x in 0..width {
            let mut color = colors[(x / bar_width).min(colors.len() - 1)];
            if x < 24 || y < 24 || x + 24 >= width || y + 24 >= height {
                color = (0xff, 0xff, 0xff);
            }
            let dst = dst_row + x * 4;
            unsafe {
                *dst_base.add(dst) = color.2;
                *dst_base.add(dst + 1) = color.1;
                *dst_base.add(dst + 2) = color.0;
                *dst_base.add(dst + 3) = 0xff;
            }
        }
    }
    Ok(())
}

#[cfg(feature = "video-renderer")]
fn native_yuv_to_rgb(y: i32, u: i32, v: i32) -> (u8, u8, u8) {
    let c = (y - 16).max(0);
    let d = u - 128;
    let e = v - 128;
    let r = (298 * c + 409 * e + 128) >> 8;
    let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
    let b = (298 * c + 516 * d + 128) >> 8;
    (
        r.clamp(0, 255) as u8,
        g.clamp(0, 255) as u8,
        b.clamp(0, 255) as u8,
    )
}

#[cfg(feature = "video-renderer")]
pub struct NativeWaylandVideoSession {
    player: NativeWaylandVideoPlayer,
    host: NativeWaylandHost,
    started: Instant,
}

#[cfg(feature = "video-renderer")]
impl NativeWaylandVideoSession {
    pub fn new(
        source: &std::path::Path,
        options: NativeWaylandVideoOptions,
    ) -> Result<Self, NativeWaylandError> {
        let mut host = NativeWaylandHost::connect(options.host.clone())?;
        host.wait_until_configured(8)?;
        let handles = host.surface_handles()?;
        let player = NativeWaylandVideoPlayer::new(source, handles, options)?;
        Ok(Self {
            player,
            host,
            started: Instant::now(),
        })
    }

    pub fn play(&self) -> Result<(), NativeWaylandError> {
        self.player.play()
    }

    pub fn tick(&mut self) -> Result<(), NativeWaylandError> {
        self.host.pump_events()?;
        let can_present_dmabuf = self.host.can_accept_dmabuf_frame();
        let frames = self.player.poll_bus(can_present_dmabuf)?;
        for frame in frames {
            self.host.present_dmabuf_frame(frame)?;
        }
        self.host.pump_events()?;
        Ok(())
    }

    pub fn snapshot(&self) -> NativeWaylandSurfaceSnapshot {
        self.host.snapshot()
    }

    pub fn runtime_snapshot(&self) -> NativeWaylandVideoSnapshot {
        NativeWaylandVideoSnapshot {
            runtime_elapsed_ms: u64::try_from(self.started.elapsed().as_millis())
                .unwrap_or(u64::MAX),
            surface: self.host.snapshot(),
            pipeline: self.player.snapshot(),
            sink_stats: self.player.sink_stats(),
            video_pipeline: self.player.pipeline_kind,
            video_sink: self.player.sink_name.clone(),
            video_fit: self.player.fit,
            render_rectangle: self.player.render_rectangle,
            appsink_probe: self.player.appsink_probe_snapshot(),
        }
    }

    pub fn runtime_sample_snapshot(&self) -> NativeWaylandVideoRuntimeSnapshot {
        NativeWaylandVideoRuntimeSnapshot {
            runtime_elapsed_ms: u64::try_from(self.started.elapsed().as_millis())
                .unwrap_or(u64::MAX),
            surface: self.host.snapshot(),
            pipeline: self.player.runtime_pipeline_snapshot(),
            sink_stats: self.player.sink_stats(),
            video_pipeline: self.player.pipeline_kind,
            video_sink: self.player.sink_name.clone(),
            video_fit: self.player.fit,
            render_rectangle: self.player.render_rectangle,
            appsink_probe: self.player.appsink_probe_snapshot(),
        }
    }
}

#[cfg(feature = "video-renderer")]
struct NativeWaylandVideoPlayer {
    pipeline: gst::Element,
    bus: gst::Bus,
    sink: gst::Element,
    overlay: Option<gst_video::VideoOverlay>,
    sink_name: String,
    sink_tuning: crate::renderer::video::VideoSinkTuningReport,
    observed_decoder_reports: BTreeMap<String, crate::renderer::video::VideoDecoderReport>,
    observed_caps_reports: crate::renderer::video::VideoCapsReportStore,
    observed_queue_elements: crate::renderer::video::VideoQueueElementStore,
    frame_stats: crate::renderer::video::VideoFrameStats,
    source: String,
    output_name: String,
    muted: bool,
    target_max_fps: Option<u32>,
    sink_throttle: bool,
    decoder_policy: crate::config::VideoDecoderPolicy,
    loop_playback: bool,
    start_offset: gst::ClockTime,
    start_offset_ms: u64,
    pipeline_kind: NativeWaylandVideoPipeline,
    fit: crate::core::FitMode,
    logical_size: (u32, u32),
    render_source_size: Option<(u32, u32)>,
    render_rectangle: Option<NativeWaylandRenderRectangle>,
    appsink_probe: Option<Arc<Mutex<NativeWaylandAppsinkProbeState>>>,
}

#[cfg(feature = "video-renderer")]
impl NativeWaylandVideoPlayer {
    pub fn new(
        source: &std::path::Path,
        handles: NativeWaylandSurfaceHandles,
        options: NativeWaylandVideoOptions,
    ) -> Result<Self, NativeWaylandError> {
        use gst::glib::translate::from_glib_full;
        gst::init().map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;

        crate::renderer::video::apply_decoder_rank_policy(options.decoder_policy);
        let sink_bundle = native_video_sink(
            options.pipeline,
            options.fit,
            options.target_max_fps,
            handles.dmabuf_main_device,
            options.debug_visible_frame,
        )?;
        let NativeVideoSinkBundle {
            pipeline_sink,
            render_sink,
            stats_sink,
            overlay,
            tuning,
            sink_name,
            appsink_probe,
            cuda_context,
        } = sink_bundle;
        if options.sink_throttle {
            configure_native_sink_frame_limiter(&pipeline_sink, options.target_max_fps)?;
        }

        let display_context = if options.pipeline.uses_gst_wayland_surface_context() {
            // SAFETY: the display pointer comes from NativeWaylandHost's live
            // wayland-client connection. NativeWaylandVideoSession owns the
            // host for at least as long as a GStreamer overlay sink can use
            // this context.
            Some(unsafe {
                let context = gst_wl_display_handle_context_new(handles.display.as_ptr());
                if context.is_null() {
                    return Err(NativeWaylandError::GStreamer(
                        "failed to create Wayland display context".to_owned(),
                    ));
                }
                from_glib_full(context)
            })
        } else {
            None
        };
        if let Some(display_context) = display_context.as_ref() {
            pipeline_sink.set_context(display_context);
            render_sink.set_context(display_context);
        }

        let window_handle = handles.window_handle();
        let render_rectangle = if let Some(overlay) = overlay.as_ref() {
            // SAFETY: window_handle is the wl_surface proxy owned by the live
            // NativeWaylandHost. NativeWaylandVideoSession drops the player
            // before dropping the host, so the sink cannot outlive the surface.
            unsafe {
                overlay.set_window_handle(window_handle);
            }
            Some(set_overlay_render_rectangle(
                overlay,
                handles.logical_size,
                None,
                options.fit,
            )?)
        } else {
            None
        };

        let pipeline = native_video_pipeline(
            source,
            &pipeline_sink,
            options.pipeline,
            options.muted,
            cuda_context.as_deref(),
        )?;
        if let Some(cuda_context) = cuda_context.as_deref() {
            let context = cuda_context.gst_context()?;
            pipeline.set_context(&context);
            pipeline_sink.set_context(&context);
            render_sink.set_context(&context);
        }
        if let Some(display_context) = display_context.as_ref() {
            pipeline.set_context(display_context);
        }
        crate::renderer::video::configure_video_pipeline_low_memory(&pipeline);
        let observed_caps_reports = crate::renderer::video::video_caps_report_store();
        let observed_queue_elements = crate::renderer::video::video_queue_element_store();
        crate::renderer::video::install_video_caps_observers(&pipeline, &observed_caps_reports);
        crate::renderer::video::install_video_queue_observers(&pipeline, &observed_queue_elements);

        let bus = pipeline.bus().ok_or_else(|| {
            NativeWaylandError::GStreamer("native video pipeline has no bus".to_owned())
        })?;
        let fit = options.fit;
        let has_overlay = overlay.is_some();
        bus.set_sync_handler(move |_, message| {
            if has_overlay && gst_video::is_video_overlay_prepare_window_handle_message(message) {
                if let Some(src) = message.src()
                    && let Ok(overlay) = src.clone().dynamic_cast::<gst_video::VideoOverlay>()
                {
                    // SAFETY: same lifetime argument as above; the sync
                    // handler only repeats the handle handoff requested by
                    // GstVideoOverlay for the already-owned pipeline.
                    unsafe {
                        overlay.set_window_handle(window_handle);
                    }
                    let _ = set_overlay_render_rectangle(&overlay, handles.logical_size, None, fit);
                }
                gst::BusSyncReply::Drop
            } else {
                gst::BusSyncReply::Pass
            }
        });

        let player = Self {
            pipeline,
            bus,
            sink: stats_sink,
            overlay,
            sink_name,
            sink_tuning: tuning,
            observed_decoder_reports: BTreeMap::new(),
            observed_caps_reports,
            observed_queue_elements,
            frame_stats: crate::renderer::video::VideoFrameStats::default(),
            source: source.to_string_lossy().into_owned(),
            output_name: options.output_name,
            muted: options.muted,
            target_max_fps: options.target_max_fps,
            sink_throttle: options.sink_throttle,
            decoder_policy: options.decoder_policy,
            loop_playback: options.loop_playback,
            start_offset: gst::ClockTime::from_mseconds(options.start_offset_ms),
            start_offset_ms: options.start_offset_ms,
            pipeline_kind: options.pipeline,
            fit: options.fit,
            logical_size: handles.logical_size,
            render_source_size: None,
            render_rectangle,
            appsink_probe,
        };
        if options.start_offset_ms > 0 {
            player.seek_to_start_offset()?;
        }
        Ok(player)
    }

    pub fn play(&self) -> Result<(), NativeWaylandError> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map(|_| ())
            .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))
    }

    pub fn poll_bus(
        &mut self,
        can_present_dmabuf: bool,
    ) -> Result<Vec<NativeWaylandDmabufFrame>, NativeWaylandError> {
        while let Some(message) = self.bus.pop() {
            if let Some(report) = crate::renderer::video::decoder_report_from_message(&message) {
                self.observed_decoder_reports
                    .entry(report.element.clone())
                    .or_insert(report);
            }
            match message.view() {
                gst::MessageView::Eos(_) if self.loop_playback => {
                    self.seek_to_start_offset()?;
                    self.pipeline
                        .set_state(gst::State::Playing)
                        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
                }
                gst::MessageView::Eos(_) => {
                    return Err(NativeWaylandError::GStreamer(
                        "native video pipeline reached EOS".to_owned(),
                    ));
                }
                gst::MessageView::Error(err) => {
                    return Err(NativeWaylandError::GStreamer(format!(
                        "{} ({:?})",
                        err.error(),
                        err.debug()
                    )));
                }
                gst::MessageView::Qos(qos) => {
                    let (processed, dropped) = qos.stats();
                    let (jitter, proportion, _) = qos.values();
                    self.frame_stats.record_qos_values(
                        processed.format().to_string(),
                        processed.value(),
                        dropped.value(),
                        jitter,
                        proportion,
                    );
                }
                _ => {}
            }
        }
        let frames = self.pull_appsink_samples(can_present_dmabuf);
        self.refresh_render_rectangle()?;
        Ok(frames)
    }

    fn snapshot(&self) -> crate::renderer::video::VideoPipelineSnapshot {
        let current_decoder_reports =
            crate::renderer::video::actual_decoder_reports(&self.pipeline);
        let actual_decoder_reports = crate::renderer::video::merge_decoder_reports(
            current_decoder_reports,
            self.observed_decoder_reports.values().cloned(),
        );
        let current_caps_reports = crate::renderer::video::video_caps_reports(&self.pipeline);
        let caps_reports = crate::renderer::video::merge_caps_reports(
            current_caps_reports,
            crate::renderer::video::observed_video_caps_reports(&self.observed_caps_reports),
        );
        let allocation_reports = crate::renderer::video::video_allocation_reports(&self.pipeline);
        let current_queue_reports = crate::renderer::video::video_queue_reports(&self.pipeline);
        let queue_reports = crate::renderer::video::merge_queue_reports(
            current_queue_reports,
            crate::renderer::video::observed_video_queue_reports(&self.observed_queue_elements),
        );
        let zero_copy_evidence =
            crate::renderer::video::zero_copy_evidence(&actual_decoder_reports, &caps_reports);
        let memory_path =
            crate::renderer::video::video_memory_path(&actual_decoder_reports, &caps_reports);
        let retention_report = crate::renderer::video::video_memory_retention_report(
            &memory_path,
            &allocation_reports,
            &self.sink_tuning,
        );
        crate::renderer::video::VideoPipelineSnapshot {
            output_name: self.output_name.clone(),
            source: self.source.clone(),
            mode: crate::policy::RenderMode::Active,
            gst_state: self.pipeline.current_state().name().to_string(),
            loop_playback: self.loop_playback,
            muted: self.muted,
            target_max_fps: self.target_max_fps,
            sink_tuning: self.sink_tuning.clone(),
            frame_limiter_enabled: self.sink_throttle && self.target_max_fps.is_some(),
            frame_limiter_max_fps: self.sink_throttle.then_some(self.target_max_fps).flatten(),
            frame_stats: self.frame_stats.clone(),
            decoder_policy: self.decoder_policy,
            decoder_policy_status: crate::renderer::video::decoder_policy_status(
                self.decoder_policy,
                &actual_decoder_reports,
            ),
            start_offset_ms: self.start_offset_ms,
            position_ms: crate::renderer::video::playback_position_ms(&self.pipeline),
            duration_ms: crate::renderer::video::playback_duration_ms(&self.pipeline),
            actual_decoders: actual_decoder_reports
                .iter()
                .map(|report| report.element.clone())
                .collect(),
            actual_decoder_reports,
            caps_reports,
            allocation_reports,
            queue_reports,
            zero_copy_evidence,
            memory_path,
            retention_report,
        }
    }

    fn runtime_pipeline_snapshot(&self) -> NativeWaylandVideoRuntimePipelineSnapshot {
        NativeWaylandVideoRuntimePipelineSnapshot {
            gst_state: self.pipeline.current_state().name().to_string(),
            frame_stats: self.frame_stats.clone(),
            position_ms: crate::renderer::video::playback_position_ms(&self.pipeline),
            duration_ms: crate::renderer::video::playback_duration_ms(&self.pipeline),
        }
    }

    fn sink_stats(&self) -> NativeWaylandSinkStats {
        if self.sink.find_property("stats").is_some() {
            let stats = self.sink.property::<gst::Structure>("stats");
            return NativeWaylandSinkStats {
                rendered: stats.get::<u64>("rendered").ok(),
                dropped: stats.get::<u64>("dropped").ok(),
                average_rate: stats.get::<f64>("average-rate").ok(),
                raw: Some(stats.to_string()),
            };
        }
        if self.sink.find_property("frames-rendered").is_some() {
            let rendered = self.sink.property::<u32>("frames-rendered");
            let dropped = self
                .sink
                .find_property("frames-dropped")
                .map(|_| self.sink.property::<u32>("frames-dropped"));
            let raw = self
                .sink
                .find_property("last-message")
                .and_then(|_| self.sink.property::<Option<String>>("last-message"));
            return NativeWaylandSinkStats {
                rendered: Some(u64::from(rendered)),
                dropped: dropped.map(u64::from),
                average_rate: None,
                raw,
            };
        }
        NativeWaylandSinkStats::default()
    }

    fn refresh_render_rectangle(&mut self) -> Result<(), NativeWaylandError> {
        let Some(overlay) = self.overlay.as_ref() else {
            return Ok(());
        };
        if self.render_source_size.is_none() {
            let reports =
                crate::renderer::video::observed_video_caps_reports(&self.observed_caps_reports);
            self.render_source_size = native_video_source_size(&reports);
        }
        let rectangle =
            render_rectangle_for_fit(self.fit, self.logical_size, self.render_source_size);
        if self.render_rectangle == Some(rectangle) {
            return Ok(());
        }
        apply_overlay_render_rectangle(overlay, rectangle)?;
        self.render_rectangle = Some(rectangle);
        Ok(())
    }

    fn appsink_probe_snapshot(&self) -> Option<NativeWaylandAppsinkProbeSnapshot> {
        self.appsink_probe.as_ref().map(|probe| {
            probe
                .lock()
                .map(|probe| probe.snapshot())
                .unwrap_or_default()
        })
    }

    fn pull_appsink_samples(&mut self, can_present_dmabuf: bool) -> Vec<NativeWaylandDmabufFrame> {
        let Some(probe) = self.appsink_probe.as_ref() else {
            return Vec::new();
        };
        let present_dmabuf = self.pipeline_kind == NativeWaylandVideoPipeline::AppsinkDmabufPresent;
        if present_dmabuf {
            let mut latest_sample = None;
            let mut pulled_samples = 0u64;
            loop {
                let sample = self
                    .sink
                    .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&0u64]);
                let Some(sample) = sample else {
                    break;
                };
                pulled_samples = pulled_samples.saturating_add(1);
                latest_sample = Some(sample);
            }
            let Some(sample) = latest_sample else {
                return Vec::new();
            };
            return probe
                .lock()
                .ok()
                .and_then(|mut probe| {
                    probe
                        .record_sample(&sample, can_present_dmabuf, pulled_samples)
                        .map(|frame| vec![frame])
                })
                .unwrap_or_default();
        }

        let mut frames = Vec::new();
        loop {
            let sample = self
                .sink
                .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&0u64]);
            let Some(sample) = sample else {
                break;
            };
            if let Ok(mut probe) = probe.lock() {
                if let Some(frame) = probe.record_sample(&sample, present_dmabuf, 1) {
                    frames.push(frame);
                }
            }
        }
        frames
    }

    fn seek_to_start_offset(&self) -> Result<(), NativeWaylandError> {
        self.pipeline
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                self.start_offset,
            )
            .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Default)]
struct NativeWaylandAppsinkProbeState {
    dmabuf_allocator: Option<NativeGbmDmabufAllocator>,
    debug_visible_frame: bool,
    pulled_samples: u64,
    last_caps: Option<String>,
    last_memory_count: usize,
    last_memory_types: Vec<String>,
    last_buffer_size: Option<usize>,
    last_video_meta: Option<NativeWaylandAppsinkVideoMetaSnapshot>,
    last_cuda_alloc_method: Option<String>,
    last_cuda_export_fd: Option<i32>,
    cuda_export_successes: u64,
    cuda_export_failures: u64,
    cuda_export_fds_closed: u64,
    last_dmabuf_export_source: Option<String>,
    last_dmabuf_export_fd_count: usize,
    last_dmabuf_export_plane_count: usize,
    last_dmabuf_export_error: Option<String>,
    last_dmabuf_copy_fallback_error: Option<String>,
    dmabuf_export_successes: u64,
    dmabuf_export_failures: u64,
    allocation_queries: u64,
    allocation_caps: Option<String>,
    allocation_need_pool: Option<bool>,
    allocation_pool_size: Option<usize>,
    allocation_mmap_pool_proposals: u64,
    allocation_mmap_pool_failures: u64,
    allocation_last_result: Option<String>,
    memory_type_counts: BTreeMap<String, u64>,
}

#[cfg(feature = "video-renderer")]
impl NativeWaylandAppsinkProbeState {
    fn new(dmabuf_main_device: Option<u64>, debug_visible_frame: bool) -> Self {
        Self {
            dmabuf_allocator: Some(NativeGbmDmabufAllocator::new(dmabuf_main_device)),
            debug_visible_frame,
            ..Self::default()
        }
    }

    fn record_sample(
        &mut self,
        sample: &gst::Sample,
        present_dmabuf: bool,
        pulled_sample_count: u64,
    ) -> Option<NativeWaylandDmabufFrame> {
        self.pulled_samples = self
            .pulled_samples
            .saturating_add(pulled_sample_count.max(1));
        self.last_caps = sample.caps().map(|caps| caps.to_string());

        let Some(buffer) = sample.buffer() else {
            self.last_memory_count = 0;
            self.last_memory_types.clear();
            self.last_buffer_size = None;
            self.last_video_meta = None;
            return None;
        };

        self.last_buffer_size = Some(buffer.size());
        self.last_memory_count = buffer.n_memory();
        let mut exported_cuda_fd_count = 0usize;
        let memory_types: Vec<String> = buffer
            .iter_memories()
            .map(|memory| {
                if let Some(report) = native_cuda_memory_export_report(memory) {
                    self.last_cuda_alloc_method = Some(report.alloc_method);
                    self.last_cuda_export_fd = report.export_fd;
                    if report.export_fd.is_some() {
                        self.cuda_export_successes += 1;
                        exported_cuda_fd_count += 1;
                    } else {
                        self.cuda_export_failures += 1;
                    }
                    if report.export_fd_closed {
                        self.cuda_export_fds_closed += 1;
                    }
                }
                native_gst_memory_type(memory)
            })
            .collect();
        for memory_type in &memory_types {
            *self
                .memory_type_counts
                .entry(memory_type.clone())
                .or_default() += 1;
        }
        self.last_memory_types = memory_types;
        let mut video_meta =
            native_appsink_video_meta_snapshot(sample.caps(), buffer, exported_cuda_fd_count);
        let (frame, dmabuf_export_error) = if present_dmabuf {
            match video_meta.as_ref() {
                Some(meta) => match native_dmabuf_frame_from_sample(
                    sample,
                    buffer,
                    meta,
                    self.dmabuf_allocator.as_mut(),
                    self.debug_visible_frame,
                ) {
                    Ok(frame) => (Some(frame), None),
                    Err(err) => (None, Some(err)),
                },
                None => (None, Some("missing_video_meta".to_owned())),
            }
        } else {
            (None, None)
        };
        if present_dmabuf {
            self.last_dmabuf_copy_fallback_error = self
                .dmabuf_allocator
                .as_ref()
                .and_then(NativeGbmDmabufAllocator::last_copy_fallback_error);
        }
        if present_dmabuf {
            if let Some(frame) = frame.as_ref() {
                self.dmabuf_export_successes += 1;
                self.last_dmabuf_export_source = Some(frame.export_source.to_owned());
                self.last_dmabuf_export_fd_count = frame.fds.len();
                self.last_dmabuf_export_plane_count = frame.planes.len();
                self.last_dmabuf_export_error = None;
            } else {
                self.dmabuf_export_failures += 1;
                self.last_dmabuf_export_source = None;
                self.last_dmabuf_export_fd_count = 0;
                self.last_dmabuf_export_plane_count = 0;
                self.last_dmabuf_export_error = dmabuf_export_error.clone();
            }
        }
        if let Some(meta) = video_meta.as_mut() {
            meta.exported_dmabuf_fd_count = self.last_dmabuf_export_fd_count;
            meta.dmabuf_export_source = self.last_dmabuf_export_source.clone();
            meta.dmabuf_export_error = dmabuf_export_error;
            meta.dmabuf_attach_blockers = native_dmabuf_attach_blockers(meta);
            meta.dmabuf_attach_ready = meta.dmabuf_attach_blockers.is_empty();
        }
        self.last_video_meta = video_meta;
        frame
    }

    fn snapshot(&self) -> NativeWaylandAppsinkProbeSnapshot {
        NativeWaylandAppsinkProbeSnapshot {
            pulled_samples: self.pulled_samples,
            last_caps: self.last_caps.clone(),
            last_memory_count: self.last_memory_count,
            last_memory_types: self.last_memory_types.clone(),
            last_buffer_size: self.last_buffer_size,
            last_video_meta: self.last_video_meta.clone(),
            last_cuda_alloc_method: self.last_cuda_alloc_method.clone(),
            last_cuda_export_fd: self.last_cuda_export_fd,
            cuda_export_successes: self.cuda_export_successes,
            cuda_export_failures: self.cuda_export_failures,
            cuda_export_fds_closed: self.cuda_export_fds_closed,
            last_dmabuf_export_source: self.last_dmabuf_export_source.clone(),
            last_dmabuf_export_fd_count: self.last_dmabuf_export_fd_count,
            last_dmabuf_export_plane_count: self.last_dmabuf_export_plane_count,
            last_dmabuf_export_error: self.last_dmabuf_export_error.clone(),
            last_dmabuf_copy_fallback_error: self.last_dmabuf_copy_fallback_error.clone(),
            dmabuf_export_successes: self.dmabuf_export_successes,
            dmabuf_export_failures: self.dmabuf_export_failures,
            allocation_queries: self.allocation_queries,
            allocation_caps: self.allocation_caps.clone(),
            allocation_need_pool: self.allocation_need_pool,
            allocation_pool_size: self.allocation_pool_size,
            allocation_mmap_pool_proposals: self.allocation_mmap_pool_proposals,
            allocation_mmap_pool_failures: self.allocation_mmap_pool_failures,
            allocation_last_result: self.allocation_last_result.clone(),
            memory_type_counts: self.memory_type_counts.clone(),
        }
    }

    fn record_allocation_result(&mut self, report: NativeAppsinkAllocationReport) {
        self.allocation_queries += 1;
        self.allocation_caps = report.caps;
        self.allocation_need_pool = report.need_pool;
        self.allocation_pool_size = report.pool_size;
        if report.proposed_mmap_pool {
            self.allocation_mmap_pool_proposals += 1;
        } else {
            self.allocation_mmap_pool_failures += 1;
        }
        self.allocation_last_result = Some(report.result);
    }
}

#[cfg(feature = "video-renderer")]
struct NativeCudaMemoryExportReport {
    alloc_method: String,
    export_fd: Option<i32>,
    export_fd_closed: bool,
}

#[cfg(feature = "video-renderer")]
struct NativeAppsinkAllocationReport {
    caps: Option<String>,
    need_pool: Option<bool>,
    pool_size: Option<usize>,
    proposed_mmap_pool: bool,
    result: String,
}

#[cfg(feature = "video-renderer")]
struct NativeCudaContextHandle {
    ptr: NonNull<NativeGstCudaContext>,
}

#[cfg(feature = "video-renderer")]
impl NativeCudaContextHandle {
    fn new(device_id: u32) -> Result<Self, NativeWaylandError> {
        let loaded = unsafe { gst_cuda_load_library() } != gst::glib::ffi::GFALSE;
        if !loaded {
            return Err(NativeWaylandError::GStreamer(
                "failed to load CUDA library for shared CUDA context".to_owned(),
            ));
        }
        let ptr = unsafe { gst_cuda_context_new(device_id) };
        let ptr = NonNull::new(ptr).ok_or_else(|| {
            NativeWaylandError::GStreamer(format!(
                "failed to create shared CUDA context for device {device_id}"
            ))
        })?;
        Ok(Self { ptr })
    }

    fn as_ptr(&self) -> *mut NativeGstCudaContext {
        self.ptr.as_ptr()
    }

    fn gst_context(&self) -> Result<gst::Context, NativeWaylandError> {
        let context = unsafe { gst_context_new_cuda_context(self.as_ptr()) };
        if context.is_null() {
            return Err(NativeWaylandError::GStreamer(
                "failed to create GstContext for shared CUDA context".to_owned(),
            ));
        }
        Ok(unsafe { gst::glib::translate::from_glib_full(context) })
    }
}

#[cfg(feature = "video-renderer")]
impl Drop for NativeCudaContextHandle {
    fn drop(&mut self) {
        unsafe {
            gst::ffi::gst_object_unref(self.ptr.as_ptr().cast::<gst::ffi::GstObject>());
        }
    }
}

#[cfg(feature = "video-renderer")]
unsafe impl Send for NativeCudaContextHandle {}

#[cfg(feature = "video-renderer")]
unsafe impl Sync for NativeCudaContextHandle {}

#[cfg(feature = "video-renderer")]
fn install_appsink_cuda_mmap_allocation_probe(
    sink: &gst::Element,
    probe: Arc<Mutex<NativeWaylandAppsinkProbeState>>,
    cuda_context: Arc<NativeCudaContextHandle>,
) -> Result<(), NativeWaylandError> {
    sink.set_property("emit-signals", true);

    let pad = sink.static_pad("sink").ok_or_else(|| {
        NativeWaylandError::GStreamer("appsink has no sink pad for allocation probe".to_owned())
    })?;
    let pad_probe = Arc::clone(&probe);
    let pad_cuda_context = Arc::clone(&cuda_context);
    let _pad_probe_id = pad.add_probe(gst::PadProbeType::QUERY_DOWNSTREAM, move |_, info| {
        let Some(query) = info.query_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        if query.type_() != gst::QueryType::Allocation {
            return gst::PadProbeReturn::Ok;
        }

        // SAFETY: QUERY_DOWNSTREAM pad probes receive a borrowed, mutable
        // GstQuery for the duration of the probe callback. The helper only
        // appends an allocation pool before the query continues downstream.
        let report =
            unsafe { propose_cuda_mmap_allocation(query.as_mut_ptr(), pad_cuda_context.as_ptr()) };
        if let Ok(mut probe) = pad_probe.lock() {
            probe.record_allocation_result(report);
        }
        gst::PadProbeReturn::Ok
    });

    let signal_probe = Arc::clone(&probe);
    let signal_cuda_context = Arc::clone(&cuda_context);
    let _handler_id = sink.connect("propose-allocation", false, move |values| {
        let report = values
            .get(1)
            .and_then(|value| value.get::<&gst::QueryRef>().ok())
            .map(|query| {
                // SAFETY: GstAppSink emits this signal with a borrowed,
                // writable GstQuery for the duration of the signal emission.
                // propose_cuda_mmap_allocation only mutates that query by
                // adding an allocation pool before the signal returns.
                unsafe {
                    propose_cuda_mmap_allocation(query.as_mut_ptr(), signal_cuda_context.as_ptr())
                }
            })
            .unwrap_or_else(|| NativeAppsinkAllocationReport {
                caps: None,
                need_pool: None,
                pool_size: None,
                proposed_mmap_pool: false,
                result: "missing propose-allocation GstQuery signal value".to_owned(),
            });
        let handled = report.proposed_mmap_pool;
        if let Ok(mut probe) = signal_probe.lock() {
            probe.record_allocation_result(report);
        }
        Some(handled.to_value())
    });
    Ok(())
}

#[cfg(feature = "video-renderer")]
unsafe fn propose_cuda_mmap_allocation(
    query: *mut gst::ffi::GstQuery,
    cuda_context: *mut NativeGstCudaContext,
) -> NativeAppsinkAllocationReport {
    if query.is_null() {
        return NativeAppsinkAllocationReport {
            caps: None,
            need_pool: None,
            pool_size: None,
            proposed_mmap_pool: false,
            result: "missing allocation query".to_owned(),
        };
    }
    if unsafe { (*query).type_ } != gst::ffi::GST_QUERY_ALLOCATION {
        return NativeAppsinkAllocationReport {
            caps: None,
            need_pool: None,
            pool_size: None,
            proposed_mmap_pool: false,
            result: "not an allocation query".to_owned(),
        };
    }

    let mut caps = ptr::null_mut();
    let mut need_pool = gst::glib::ffi::GFALSE;
    unsafe {
        gst::ffi::gst_query_parse_allocation(query, &mut caps, &mut need_pool);
    }
    let caps_string = unsafe { native_gst_caps_string(caps) };
    if caps.is_null() {
        return NativeAppsinkAllocationReport {
            caps: caps_string,
            need_pool: Some(need_pool != gst::glib::ffi::GFALSE),
            pool_size: None,
            proposed_mmap_pool: false,
            result: "allocation query has no caps".to_owned(),
        };
    }

    let video_info = unsafe { gst_video::ffi::gst_video_info_new() };
    if video_info.is_null() {
        return NativeAppsinkAllocationReport {
            caps: caps_string,
            need_pool: Some(need_pool != gst::glib::ffi::GFALSE),
            pool_size: None,
            proposed_mmap_pool: false,
            result: "failed to allocate GstVideoInfo".to_owned(),
        };
    }
    let parsed_video_info = unsafe { gst_video::ffi::gst_video_info_from_caps(video_info, caps) }
        != gst::glib::ffi::GFALSE;
    if !parsed_video_info {
        unsafe {
            gst_video::ffi::gst_video_info_free(video_info);
        }
        return NativeAppsinkAllocationReport {
            caps: caps_string,
            need_pool: Some(need_pool != gst::glib::ffi::GFALSE),
            pool_size: None,
            proposed_mmap_pool: false,
            result: "caps cannot be converted to GstVideoInfo".to_owned(),
        };
    }
    let pool_size = unsafe { (*video_info).size };
    unsafe {
        gst_video::ffi::gst_video_info_free(video_info);
    }
    let Ok(pool_size_u32) = u32::try_from(pool_size) else {
        return NativeAppsinkAllocationReport {
            caps: caps_string,
            need_pool: Some(need_pool != gst::glib::ffi::GFALSE),
            pool_size: Some(pool_size),
            proposed_mmap_pool: false,
            result: "video frame size exceeds GstBufferPool u32 size".to_owned(),
        };
    };

    if cuda_context.is_null() {
        return NativeAppsinkAllocationReport {
            caps: caps_string,
            need_pool: Some(need_pool != gst::glib::ffi::GFALSE),
            pool_size: Some(pool_size),
            proposed_mmap_pool: false,
            result: "missing shared CUDA context for mmap pool".to_owned(),
        };
    }
    let pool = unsafe { gst_cuda_buffer_pool_new(cuda_context) };
    if pool.is_null() {
        return NativeAppsinkAllocationReport {
            caps: caps_string,
            need_pool: Some(need_pool != gst::glib::ffi::GFALSE),
            pool_size: Some(pool_size),
            proposed_mmap_pool: false,
            result: "failed to create CUDA buffer pool".to_owned(),
        };
    }

    let config = unsafe { gst::ffi::gst_buffer_pool_get_config(pool) };
    if config.is_null() {
        unsafe {
            gst::ffi::gst_object_unref(pool.cast::<gst::ffi::GstObject>());
        }
        return NativeAppsinkAllocationReport {
            caps: caps_string,
            need_pool: Some(need_pool != gst::glib::ffi::GFALSE),
            pool_size: Some(pool_size),
            proposed_mmap_pool: false,
            result: "CUDA buffer pool returned no config".to_owned(),
        };
    }

    const MIN_BUFFERS: u32 = 2;
    const MAX_BUFFERS: u32 = 2;
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
        return NativeAppsinkAllocationReport {
            caps: caps_string,
            need_pool: Some(need_pool != gst::glib::ffi::GFALSE),
            pool_size: Some(pool_size),
            proposed_mmap_pool: false,
            result: "failed to configure CUDA mmap buffer pool".to_owned(),
        };
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

    NativeAppsinkAllocationReport {
        caps: caps_string,
        need_pool: Some(need_pool != gst::glib::ffi::GFALSE),
        pool_size: Some(pool_size),
        proposed_mmap_pool: true,
        result: "proposed CUDA mmap buffer pool".to_owned(),
    }
}

#[cfg(feature = "video-renderer")]
unsafe fn native_gst_caps_string(caps: *mut gst::ffi::GstCaps) -> Option<String> {
    if caps.is_null() {
        return None;
    }
    let value = unsafe { gst::ffi::gst_caps_to_string(caps) };
    if value.is_null() {
        return None;
    }
    let string = unsafe { CStr::from_ptr(value) }
        .to_string_lossy()
        .into_owned();
    unsafe {
        gst::glib::ffi::g_free(value.cast::<c_void>());
    }
    Some(string)
}

#[cfg(feature = "video-renderer")]
fn native_cuda_memory_export_report(
    memory: &gst::MemoryRef,
) -> Option<NativeCudaMemoryExportReport> {
    if !memory.is_type("gst.cuda.memory") && !memory.is_type("CUDAMemory") {
        return None;
    }
    let cuda_memory = memory.as_ptr().cast_mut().cast::<NativeGstCudaMemory>();
    let alloc_method = unsafe { gst_cuda_memory_get_alloc_method(cuda_memory) };
    let mut fd = -1;
    let exported =
        unsafe { gst_cuda_memory_export(cuda_memory, (&mut fd as *mut i32).cast::<c_void>()) != 0 };
    let export_fd = (exported && fd >= 0).then_some(fd);
    let export_fd_closed = if let Some(fd) = export_fd {
        // SAFETY: gst_cuda_memory_export returns a new OS fd for mmap CUDA
        // memory. This path is diagnostic only; display uses real dmabuf
        // exports, so close the CUDA fd immediately.
        let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
        drop(owned_fd);
        true
    } else {
        false
    };
    Some(NativeCudaMemoryExportReport {
        alloc_method: native_cuda_alloc_method_label(alloc_method).to_owned(),
        export_fd,
        export_fd_closed,
    })
}

#[cfg(feature = "video-renderer")]
fn native_appsink_video_meta_snapshot(
    caps: Option<&gst::CapsRef>,
    buffer: &gst::BufferRef,
    exported_cuda_fd_count: usize,
) -> Option<NativeWaylandAppsinkVideoMetaSnapshot> {
    let meta = buffer.meta::<gst_video::VideoMeta>()?;
    let format = meta.format();
    let caps_drm_format = caps.and_then(native_caps_drm_format_string);
    let caps_drm = caps_drm_format
        .as_deref()
        .and_then(native_drm_fourcc_modifier_from_caps_format);
    let drm_fourcc = caps_drm
        .map(|(fourcc, _)| fourcc)
        .or_else(|| native_drm_fourcc_from_video_format(format));
    let drm_modifier = caps_drm.and_then(|(_, modifier)| modifier).or_else(|| {
        caps_drm_format
            .as_deref()
            .and_then(native_drm_modifier_from_caps_format)
    });

    let mut snapshot = NativeWaylandAppsinkVideoMetaSnapshot {
        format: format.to_str().to_string(),
        width: meta.width(),
        height: meta.height(),
        n_planes: meta.n_planes(),
        offsets: meta.offset().to_vec(),
        strides: meta.stride().to_vec(),
        caps_drm_format,
        drm_fourcc,
        drm_modifier,
        exported_cuda_fd_count,
        exported_dmabuf_fd_count: 0,
        dmabuf_export_source: None,
        dmabuf_export_error: None,
        dmabuf_attach_ready: false,
        dmabuf_attach_blockers: Vec::new(),
    };
    snapshot.dmabuf_attach_blockers = native_dmabuf_attach_blockers(&snapshot);
    snapshot.dmabuf_attach_ready = snapshot.dmabuf_attach_blockers.is_empty();
    Some(snapshot)
}

#[cfg(feature = "video-renderer")]
fn native_dmabuf_attach_blockers(meta: &NativeWaylandAppsinkVideoMetaSnapshot) -> Vec<String> {
    let mut blockers = Vec::new();
    if meta.exported_dmabuf_fd_count == 0 {
        blockers.push("missing_dmabuf_export_fd".to_owned());
    }
    if meta.drm_fourcc.is_none() {
        blockers.push("missing_drm_fourcc".to_owned());
    }
    let plane_count = usize::try_from(meta.n_planes).unwrap_or_default();
    if plane_count == 0 || meta.offsets.len() < plane_count || meta.strides.len() < plane_count {
        blockers.push("invalid_video_plane_meta".to_owned());
    }
    blockers
}

#[cfg(feature = "video-renderer")]
fn native_dmabuf_frame_from_sample(
    sample: &gst::Sample,
    buffer: &gst::BufferRef,
    meta: &NativeWaylandAppsinkVideoMetaSnapshot,
    dmabuf_allocator: Option<&mut NativeGbmDmabufAllocator>,
    debug_visible_frame: bool,
) -> Result<NativeWaylandDmabufFrame, String> {
    let _format = meta
        .drm_fourcc
        .ok_or_else(|| "missing_drm_fourcc".to_owned())?;
    if meta.width == 0 || meta.height == 0 {
        return Err("invalid_video_size".to_owned());
    }
    let export =
        native_dmabuf_export_from_buffer(buffer, meta, dmabuf_allocator, debug_visible_frame)?;
    let gst_buffer = sample
        .buffer_owned()
        .ok_or_else(|| "missing_owned_gst_buffer".to_owned())?;
    Some(NativeWaylandDmabufFrame {
        fds: export.fds,
        gst_buffer,
        gbm_bo: export.gbm_bo,
        export_source: export.source,
        width: meta.width,
        height: meta.height,
        format: export.format,
        modifier: meta.drm_modifier.or(export.modifier),
        planes: export.planes,
    })
    .ok_or_else(|| "unreachable_dmabuf_frame_build".to_owned())
}

#[cfg(feature = "video-renderer")]
fn native_dmabuf_export_from_buffer(
    buffer: &gst::BufferRef,
    meta: &NativeWaylandAppsinkVideoMetaSnapshot,
    dmabuf_allocator: Option<&mut NativeGbmDmabufAllocator>,
    debug_visible_frame: bool,
) -> Result<NativeWaylandDmabufExport, String> {
    let mut dmabuf_allocator = dmabuf_allocator;
    if debug_visible_frame {
        return dmabuf_allocator
            .as_deref_mut()
            .map(|allocator| allocator.export_debug_xrgb_pattern(meta.width, meta.height))
            .unwrap_or_else(|| Err("missing_gbm_dmabuf_allocator".to_owned()));
    }

    match native_dmabuf_export_from_dmabuf_memory(buffer, meta) {
        Ok(export) => return Ok(export),
        Err(dmabuf_err) => match native_dmabuf_export_from_gl_memory_egl(buffer, meta) {
            Ok(export) => Ok(export),
            Err(gl_err) => {
                let gbm_result = dmabuf_allocator
                    .as_deref_mut()
                    .map(|allocator| allocator.export_copy(buffer, meta, false))
                    .unwrap_or_else(|| Err("missing_gbm_dmabuf_allocator".to_owned()));
                match gbm_result {
                    Ok(export) => Ok(export),
                    Err(gbm_err) => Err(format!(
                        "dmabuf_memory:{dmabuf_err};gl_memory:{gl_err};gbm_copy:{gbm_err}"
                    )),
                }
            }
        },
    }
}

#[cfg(feature = "video-renderer")]
fn native_dmabuf_export_from_dmabuf_memory(
    buffer: &gst::BufferRef,
    meta: &NativeWaylandAppsinkVideoMetaSnapshot,
) -> Result<NativeWaylandDmabufExport, String> {
    let memory_count = buffer.n_memory();
    if memory_count == 0 {
        return Err("buffer_has_no_memory".to_owned());
    }

    let mut fds = Vec::with_capacity(memory_count);
    let mut memory_fd_indices = Vec::with_capacity(memory_count);
    for memory_index in 0..memory_count {
        let memory = buffer.peek_memory(memory_index);
        let fd = native_dmabuf_memory_fd(memory).ok_or_else(|| {
            format!(
                "memory_{memory_index}_not_dmabuf:{}",
                native_gst_memory_type(memory)
            )
        })?;
        // SAFETY: gst_dmabuf_memory_get_fd returns a borrowed fd owned by the
        // GstMemory. We immediately duplicate it so the Wayland in-flight
        // buffer can outlive this borrowed view.
        let owned_fd = unsafe { BorrowedFd::borrow_raw(fd) }
            .try_clone_to_owned()
            .map_err(|err| format!("memory_{memory_index}_fd_clone_failed:{err}"))?;
        memory_fd_indices.push(fds.len());
        fds.push(owned_fd);
    }

    let planes = native_dmabuf_planes_from_buffer_layout(buffer, meta, |memory_index| {
        memory_fd_indices.get(memory_index).copied()
    })
    .ok_or_else(|| "invalid_dmabuf_plane_layout".to_owned())?;
    Ok(NativeWaylandDmabufExport {
        source: "gst-dmabuf-memory",
        format: meta.drm_fourcc.unwrap_or(DRM_FORMAT_NV12),
        fds,
        gbm_bo: None,
        planes,
        modifier: meta.drm_modifier,
    })
}

#[cfg(feature = "video-renderer")]
fn native_dmabuf_export_from_cuda_memory(
    buffer: &gst::BufferRef,
    meta: &NativeWaylandAppsinkVideoMetaSnapshot,
) -> Result<NativeWaylandDmabufExport, String> {
    let memory_count = buffer.n_memory();
    if memory_count == 0 {
        return Err("buffer_has_no_memory".to_owned());
    }

    let mut fds = Vec::with_capacity(memory_count);
    let mut memory_fd_indices = Vec::with_capacity(memory_count);
    for memory_index in 0..memory_count {
        let memory = buffer.peek_memory(memory_index);
        if !memory.is_type("gst.cuda.memory") && !memory.is_type("CUDAMemory") {
            return Err(format!(
                "memory_{memory_index}_not_cuda:{}",
                native_gst_memory_type(memory)
            ));
        }

        let cuda_memory = memory.as_ptr().cast_mut().cast::<NativeGstCudaMemory>();
        let alloc_method = unsafe { gst_cuda_memory_get_alloc_method(cuda_memory) };
        if alloc_method != GST_CUDA_MEMORY_ALLOC_MMAP {
            return Err(format!(
                "memory_{memory_index}_cuda_alloc_method_not_mmap:{}",
                native_cuda_alloc_method_label(alloc_method)
            ));
        }

        let mut fd = -1;
        let exported = unsafe {
            gst_cuda_memory_export(cuda_memory, (&mut fd as *mut i32).cast::<c_void>()) != 0
        };
        if !exported || fd < 0 {
            return Err(format!(
                "memory_{memory_index}_cuda_export_failed:exported={exported}:fd={fd}"
            ));
        }

        memory_fd_indices.push(fds.len());
        // SAFETY: gst_cuda_memory_export returns a newly-opened POSIX fd for
        // CUDA mmap memory. The compositor import below verifies whether it is
        // a DRM PRIME dmabuf accepted by Wayland.
        fds.push(unsafe { OwnedFd::from_raw_fd(fd) });
    }

    let planes = native_dmabuf_planes_from_buffer_layout(buffer, meta, |memory_index| {
        memory_fd_indices.get(memory_index).copied()
    })
    .ok_or_else(|| "invalid_cuda_plane_layout".to_owned())?;
    Ok(NativeWaylandDmabufExport {
        source: "gst-cuda-memory-export",
        format: meta.drm_fourcc.unwrap_or(DRM_FORMAT_NV12),
        fds,
        gbm_bo: None,
        planes,
        modifier: meta.drm_modifier,
    })
}

#[cfg(feature = "video-renderer")]
fn native_dmabuf_export_from_gl_memory_egl(
    buffer: &gst::BufferRef,
    meta: &NativeWaylandAppsinkVideoMetaSnapshot,
) -> Result<NativeWaylandDmabufExport, String> {
    let plane_count =
        native_video_meta_plane_count(meta).ok_or_else(|| "invalid_video_plane_meta".to_owned())?;
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
        let export = native_gl_memory_export_dmabuf(memory)
            .map_err(|err| format!("plane_{plane_index}:{err}"))?;
        let stride = u32::try_from(export.stride)
            .map_err(|_| format!("plane_{plane_index}:invalid_export_stride"))?;
        let offset = u32::try_from(export.offset)
            .map_err(|_| format!("plane_{plane_index}:invalid_export_offset"))?;
        source = Some(export.source);
        fds.push(export.fd);
        planes.push(NativeWaylandDmabufPlane {
            fd_index: plane_index,
            offset,
            stride,
        });
    }

    Ok(NativeWaylandDmabufExport {
        source: source.unwrap_or("gst-gl-memory"),
        format: meta.drm_fourcc.unwrap_or(DRM_FORMAT_XRGB8888),
        fds,
        gbm_bo: None,
        planes,
        modifier: Some(DRM_FORMAT_MOD_INVALID),
    })
}

#[cfg(feature = "video-renderer")]
fn native_dmabuf_planes_from_buffer_layout<F>(
    buffer: &gst::BufferRef,
    meta: &NativeWaylandAppsinkVideoMetaSnapshot,
    mut fd_index_for_memory: F,
) -> Option<Vec<NativeWaylandDmabufPlane>>
where
    F: FnMut(usize) -> Option<usize>,
{
    let plane_count = native_video_meta_plane_count(meta)?;
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
        planes.push(NativeWaylandDmabufPlane {
            fd_index,
            offset,
            stride: plane_stride,
        });
    }
    Some(planes)
}

#[cfg(feature = "video-renderer")]
fn native_video_meta_plane_count(meta: &NativeWaylandAppsinkVideoMetaSnapshot) -> Option<usize> {
    let plane_count = usize::try_from(meta.n_planes).ok()?;
    if plane_count == 0 || meta.offsets.len() < plane_count || meta.strides.len() < plane_count {
        return None;
    }
    Some(plane_count)
}

#[cfg(feature = "video-renderer")]
fn native_dmabuf_memory_fd(memory: &gst::MemoryRef) -> Option<i32> {
    let is_dmabuf =
        unsafe { gst_is_dmabuf_memory(memory.as_ptr().cast_mut()) } != gst::glib::ffi::GFALSE;
    if !is_dmabuf {
        return None;
    }
    let fd = unsafe { gst_dmabuf_memory_get_fd(memory.as_ptr().cast_mut()) };
    (fd >= 0).then_some(fd)
}

#[cfg(feature = "video-renderer")]
struct NativeGlMemoryEglDmabufExport {
    source: &'static str,
    fd: OwnedFd,
    stride: i32,
    offset: usize,
}

#[cfg(feature = "video-renderer")]
fn native_gl_memory_export_dmabuf(
    memory: &gst::MemoryRef,
) -> Result<NativeGlMemoryEglDmabufExport, String> {
    match native_gl_memory_egl_export_dmabuf(memory) {
        Ok(export) => Ok(export),
        Err(egl_err) => match native_gl_memory_texture_export_dmabuf(memory) {
            Ok(export) => Ok(export),
            Err(texture_err) => Err(format!("egl:{egl_err};texture:{texture_err}")),
        },
    }
}

#[cfg(feature = "video-renderer")]
fn native_gl_memory_egl_export_dmabuf(
    memory: &gst::MemoryRef,
) -> Result<NativeGlMemoryEglDmabufExport, String> {
    let is_gl_egl =
        unsafe { gst_is_gl_memory_egl(memory.as_ptr().cast_mut()) } != gst::glib::ffi::GFALSE;
    if !is_gl_egl {
        return Err(format!(
            "not_gst_gl_memory_egl:{}",
            native_gst_memory_type(memory)
        ));
    }
    let image = unsafe { gst_gl_memory_egl_get_image(memory.as_ptr().cast_mut().cast()) }
        .cast::<NativeGstEGLImage>();
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
    Ok(NativeGlMemoryEglDmabufExport {
        source: "gst-gl-memory-egl",
        // SAFETY: gst_egl_image_export_dmabuf returns a newly-opened dmabuf fd
        // on success. This OwnedFd closes it after the Wayland buffer release.
        fd: unsafe { OwnedFd::from_raw_fd(fd) },
        stride,
        offset,
    })
}

#[cfg(feature = "video-renderer")]
struct NativeGlMemoryTextureExportState {
    gl_memory: *mut NativeGstGLMemory,
    fd: i32,
    stride: i32,
    offset: usize,
    success: bool,
}

#[cfg(feature = "video-renderer")]
unsafe extern "C" fn native_gl_memory_texture_export_thread(
    context: *mut NativeGstGLContext,
    data: *mut c_void,
) {
    let state = unsafe { &mut *(data.cast::<NativeGlMemoryTextureExportState>()) };
    let image = unsafe { gst_egl_image_from_texture(context, state.gl_memory, ptr::null_mut()) };
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

#[cfg(feature = "video-renderer")]
fn native_gl_memory_texture_export_dmabuf(
    memory: &gst::MemoryRef,
) -> Result<NativeGlMemoryEglDmabufExport, String> {
    let is_gl_memory =
        unsafe { gst_is_gl_memory(memory.as_ptr().cast_mut()) } != gst::glib::ffi::GFALSE;
    if !is_gl_memory {
        return Err(format!(
            "not_gst_gl_memory:{}",
            native_gst_memory_type(memory)
        ));
    }
    let gl_memory = memory.as_ptr().cast_mut().cast::<NativeGstGLMemory>();
    let context = unsafe { (*gl_memory).base.context };
    if context.is_null() {
        return Err("gl_memory_context_null".to_owned());
    }
    let mut state = NativeGlMemoryTextureExportState {
        gl_memory,
        fd: -1,
        stride: 0,
        offset: 0,
        success: false,
    };
    unsafe {
        gst_gl_context_thread_add(
            context,
            Some(native_gl_memory_texture_export_thread),
            (&mut state as *mut NativeGlMemoryTextureExportState).cast::<c_void>(),
        );
    }
    if !state.success || state.fd < 0 || state.stride <= 0 {
        return Err(format!(
            "texture_export_failed:success={}:fd={}:stride={}:offset={}",
            state.success, state.fd, state.stride, state.offset
        ));
    }
    Ok(NativeGlMemoryEglDmabufExport {
        source: "gst-gl-memory-texture",
        // SAFETY: gst_egl_image_export_dmabuf returns a newly-opened dmabuf fd
        // on success. This OwnedFd closes it after the Wayland buffer release.
        fd: unsafe { OwnedFd::from_raw_fd(state.fd) },
        stride: state.stride,
        offset: state.offset,
    })
}

#[cfg(feature = "video-renderer")]
fn native_drm_fourcc_from_video_format(format: gst_video::VideoFormat) -> Option<u32> {
    use gst::glib::translate::IntoGlib;

    // SAFETY: gst_video_dma_drm_fourcc_from_format is a pure conversion helper
    // for a GStreamer video format enum value and returns 0 for unsupported
    // formats.
    let fourcc = unsafe { gst_video_dma_drm_fourcc_from_format(format.into_glib()) };
    (fourcc != 0).then_some(fourcc)
}

#[cfg(feature = "video-renderer")]
fn native_caps_drm_format_string(caps: &gst::CapsRef) -> Option<String> {
    caps.structure(0)
        .and_then(|structure| structure.get::<String>("drm-format").ok())
}

#[cfg(feature = "video-renderer")]
fn native_drm_modifier_from_caps_format(format: &str) -> Option<u64> {
    let (_, modifier) = format.rsplit_once(':')?;
    let modifier = modifier
        .strip_prefix("0x")
        .or_else(|| modifier.strip_prefix("0X"))
        .unwrap_or(modifier);
    u64::from_str_radix(modifier, 16).ok()
}

#[cfg(feature = "video-renderer")]
fn native_drm_fourcc_modifier_from_caps_format(format: &str) -> Option<(u32, Option<u64>)> {
    let format = CString::new(format).ok()?;
    let mut modifier = 0u64;
    let fourcc = unsafe { gst_video_dma_drm_fourcc_from_string(format.as_ptr(), &mut modifier) };
    (fourcc != 0).then_some((
        fourcc,
        (modifier != DRM_FORMAT_MOD_INVALID).then_some(modifier),
    ))
}

#[cfg(feature = "video-renderer")]
fn native_cuda_alloc_method_label(method: i32) -> &'static str {
    match method {
        1 => "malloc",
        2 => "mmap",
        _ => "unknown",
    }
}

#[cfg(feature = "video-renderer")]
fn native_gst_memory_type(memory: &gst::MemoryRef) -> String {
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

#[cfg(feature = "video-renderer")]
struct NativeVideoSinkBundle {
    pipeline_sink: gst::Element,
    render_sink: gst::Element,
    stats_sink: gst::Element,
    overlay: Option<gst_video::VideoOverlay>,
    tuning: crate::renderer::video::VideoSinkTuningReport,
    sink_name: String,
    appsink_probe: Option<Arc<Mutex<NativeWaylandAppsinkProbeState>>>,
    cuda_context: Option<Arc<NativeCudaContextHandle>>,
}

#[cfg(feature = "video-renderer")]
fn native_video_sink(
    pipeline_kind: NativeWaylandVideoPipeline,
    fit: crate::core::FitMode,
    target_max_fps: Option<u32>,
    dmabuf_main_device: Option<u64>,
    debug_visible_frame: bool,
) -> Result<NativeVideoSinkBundle, NativeWaylandError> {
    match pipeline_kind {
        NativeWaylandVideoPipeline::Playbin | NativeWaylandVideoPipeline::Playbin3 => {
            native_wayland_sink(fit, target_max_fps)
        }
        NativeWaylandVideoPipeline::ExplicitH264Gl => native_gl_video_sink(fit, target_max_fps),
        NativeWaylandVideoPipeline::AppsinkProbe => native_appsink_probe_sink(
            target_max_fps,
            NativeAppsinkAllocationMode::Passive,
            dmabuf_main_device,
            debug_visible_frame,
        ),
        NativeWaylandVideoPipeline::AppsinkMmapProbe => native_appsink_probe_sink(
            target_max_fps,
            NativeAppsinkAllocationMode::ForceCudaMmap,
            dmabuf_main_device,
            debug_visible_frame,
        ),
        NativeWaylandVideoPipeline::AppsinkDmabufPresent => native_appsink_probe_sink(
            target_max_fps,
            NativeAppsinkAllocationMode::GbmDmabufPresent,
            dmabuf_main_device,
            debug_visible_frame,
        ),
    }
}

#[cfg(feature = "video-renderer")]
fn native_wayland_sink(
    fit: crate::core::FitMode,
    target_max_fps: Option<u32>,
) -> Result<NativeVideoSinkBundle, NativeWaylandError> {
    let sink = gst::ElementFactory::make("waylandsink")
        .property("sync", true)
        .property("enable-last-sample", false)
        .build()
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    if sink.find_property("fullscreen").is_some() {
        sink.set_property("fullscreen", false);
    }
    if sink.find_property("force-aspect-ratio").is_some() {
        sink.set_property(
            "force-aspect-ratio",
            !matches!(fit, crate::core::FitMode::Stretch),
        );
    }
    if sink.find_property("handle-events").is_some() {
        sink.set_property("handle-events", false);
    }
    let overlay = sink
        .clone()
        .dynamic_cast::<gst_video::VideoOverlay>()
        .map_err(|_| {
            NativeWaylandError::GStreamer(
                "waylandsink does not implement GstVideoOverlay".to_owned(),
            )
        })?;
    let mut tuning = crate::renderer::video::configure_video_sink_low_memory(&sink, target_max_fps);
    tuning.sink_element = Some("waylandsink+native-layer-surface".to_owned());
    Ok(NativeVideoSinkBundle {
        pipeline_sink: sink.clone(),
        render_sink: sink.clone(),
        stats_sink: sink,
        overlay: Some(overlay),
        tuning,
        sink_name: "wayland".to_owned(),
        appsink_probe: None,
        cuda_context: None,
    })
}

#[cfg(feature = "video-renderer")]
fn native_gl_video_sink(
    fit: crate::core::FitMode,
    target_max_fps: Option<u32>,
) -> Result<NativeVideoSinkBundle, NativeWaylandError> {
    let render_sink = gst::ElementFactory::make("glimagesink")
        .property("sync", false)
        .property("enable-last-sample", false)
        .property(
            "force-aspect-ratio",
            !matches!(fit, crate::core::FitMode::Stretch),
        )
        .property("handle-events", false)
        .build()
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    let stats_sink = gst::ElementFactory::make("fpsdisplaysink")
        .property("video-sink", &render_sink)
        .property("sync", false)
        .property("text-overlay", false)
        .property("silent", false)
        .build()
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    let overlay = render_sink
        .clone()
        .dynamic_cast::<gst_video::VideoOverlay>()
        .map_err(|_| {
            NativeWaylandError::GStreamer(
                "glimagesink does not implement GstVideoOverlay".to_owned(),
            )
        })?;
    let mut tuning =
        crate::renderer::video::configure_video_sink_low_memory(&render_sink, target_max_fps);
    tuning.sink_element = Some("explicit-h264-gl+fpsdisplaysink+glimagesink".to_owned());
    Ok(NativeVideoSinkBundle {
        pipeline_sink: stats_sink.clone(),
        render_sink,
        stats_sink,
        overlay: Some(overlay),
        tuning,
        sink_name: "glimage".to_owned(),
        appsink_probe: None,
        cuda_context: None,
    })
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeAppsinkAllocationMode {
    Passive,
    ForceCudaMmap,
    GbmDmabufPresent,
}

#[cfg(feature = "video-renderer")]
fn native_appsink_probe_sink(
    target_max_fps: Option<u32>,
    allocation_mode: NativeAppsinkAllocationMode,
    dmabuf_main_device: Option<u64>,
    debug_visible_frame: bool,
) -> Result<NativeVideoSinkBundle, NativeWaylandError> {
    let sink = gst::ElementFactory::make("appsink")
        .property("sync", false)
        .property("async", false)
        .property("emit-signals", false)
        .property("enable-last-sample", false)
        .property("wait-on-eos", false)
        .property("max-buffers", 2u32)
        .property_from_str("leaky-type", "downstream")
        .build()
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    let appsink_probe = Arc::new(Mutex::new(NativeWaylandAppsinkProbeState::new(
        dmabuf_main_device,
        debug_visible_frame,
    )));
    let cuda_context = if matches!(allocation_mode, NativeAppsinkAllocationMode::ForceCudaMmap) {
        Some(Arc::new(NativeCudaContextHandle::new(0)?))
    } else {
        None
    };
    if matches!(allocation_mode, NativeAppsinkAllocationMode::ForceCudaMmap) {
        let cuda_context = cuda_context
            .as_ref()
            .expect("CUDA mmap allocation mode must create a shared CUDA context");
        install_appsink_cuda_mmap_allocation_probe(
            &sink,
            Arc::clone(&appsink_probe),
            Arc::clone(cuda_context),
        )?;
    }
    let mut tuning = crate::renderer::video::configure_video_sink_low_memory(&sink, target_max_fps);
    tuning.sink_element = Some(match allocation_mode {
        NativeAppsinkAllocationMode::Passive => "appsink-probe+nvh264dec".to_owned(),
        NativeAppsinkAllocationMode::ForceCudaMmap => "appsink-mmap-probe+nvh264dec".to_owned(),
        NativeAppsinkAllocationMode::GbmDmabufPresent => {
            "appsink-dmabuf-present+gbm-nv12-copy".to_owned()
        }
    });
    Ok(NativeVideoSinkBundle {
        pipeline_sink: sink.clone(),
        render_sink: sink.clone(),
        stats_sink: sink,
        overlay: None,
        tuning,
        sink_name: match allocation_mode {
            NativeAppsinkAllocationMode::Passive => "appsink-probe",
            NativeAppsinkAllocationMode::ForceCudaMmap => "appsink-mmap-probe",
            NativeAppsinkAllocationMode::GbmDmabufPresent => "appsink-dmabuf-present",
        }
        .to_owned(),
        appsink_probe: Some(appsink_probe),
        cuda_context,
    })
}

#[cfg(feature = "video-renderer")]
fn native_video_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
    pipeline_kind: NativeWaylandVideoPipeline,
    muted: bool,
    cuda_context: Option<&NativeCudaContextHandle>,
) -> Result<gst::Element, NativeWaylandError> {
    match pipeline_kind {
        NativeWaylandVideoPipeline::Playbin | NativeWaylandVideoPipeline::Playbin3 => {
            let uri = gst::glib::filename_to_uri(source, None::<&str>)
                .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
            let flags = if muted { "video" } else { "video+audio" };
            let element_name = pipeline_kind.playbin_element_name().ok_or_else(|| {
                NativeWaylandError::GStreamer("missing playbin element".to_owned())
            })?;
            gst::ElementFactory::make(element_name)
                .property("uri", uri.as_str())
                .property_from_str("flags", flags)
                .property("video-sink", sink)
                .build()
                .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))
        }
        NativeWaylandVideoPipeline::ExplicitH264Gl => {
            native_explicit_h264_gl_pipeline(source, sink, cuda_context)
        }
        NativeWaylandVideoPipeline::AppsinkProbe | NativeWaylandVideoPipeline::AppsinkMmapProbe => {
            native_explicit_h264_appsink_pipeline(source, sink, cuda_context)
        }
        NativeWaylandVideoPipeline::AppsinkDmabufPresent => {
            native_explicit_h264_cuda_download_appsink_pipeline(source, sink, cuda_context)
        }
    }
}

#[cfg(feature = "video-renderer")]
fn native_explicit_h264_gl_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
    cuda_context: Option<&NativeCudaContextHandle>,
) -> Result<gst::Element, NativeWaylandError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_gst_element("qtdemux")?;
    let h264parse = native_gst_element("h264parse")?;
    let decoder = native_gst_element("nvh264dec")?;
    if let Some(cuda_context) = cuda_context {
        decoder.set_context(&cuda_context.gst_context()?);
    }
    let glupload = native_gst_element("glupload")?;
    let glconvert = native_gst_element("glcolorconvert")?;

    pipeline
        .add_many([
            &filesrc, &demux, &h264parse, &decoder, &glupload, &glconvert, sink,
        ])
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    gst::Element::link_many([&h264parse, &decoder, &glupload, &glconvert, sink])
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;

    let parse_sink = h264parse
        .static_pad("sink")
        .ok_or_else(|| NativeWaylandError::GStreamer("h264parse has no sink pad".to_owned()))?;
    demux.connect_pad_added(move |_, pad| {
        if parse_sink.is_linked() || !native_demux_pad_is_video(pad) {
            return;
        }
        let _ = pad.link(&parse_sink);
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "video-renderer")]
fn native_explicit_h264_gl_appsink_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
    cuda_context: Option<&NativeCudaContextHandle>,
) -> Result<gst::Element, NativeWaylandError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_gst_element("qtdemux")?;
    let queue = native_gst_element("queue")?;
    let h264parse = native_gst_element("h264parse")?;
    let decoder = native_gst_element("nvh264dec")?;
    if let Some(cuda_context) = cuda_context {
        decoder.set_context(&cuda_context.gst_context()?);
    }
    let glupload = native_gst_element("glupload")?;
    let capsfilter = native_gst_element("capsfilter")?;
    let caps = "video/x-raw(memory:GLMemory),format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    capsfilter.set_property("caps", &caps);

    pipeline
        .add_many([
            &filesrc,
            &demux,
            &queue,
            &h264parse,
            &decoder,
            &glupload,
            &capsfilter,
            sink,
        ])
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    gst::Element::link_many([&queue, &h264parse, &decoder, &glupload, &capsfilter, sink])
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeWaylandError::GStreamer("queue has no sink pad".to_owned()))?;
    demux.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() || !native_demux_pad_is_video(pad) {
            return;
        }
        let _ = pad.link(&queue_sink);
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "video-renderer")]
fn native_explicit_h264_appsink_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
    cuda_context: Option<&NativeCudaContextHandle>,
) -> Result<gst::Element, NativeWaylandError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_gst_element("qtdemux")?;
    let h264parse = native_gst_element("h264parse")?;
    let decoder = native_gst_element("nvh264dec")?;
    if let Some(cuda_context) = cuda_context {
        decoder.set_context(&cuda_context.gst_context()?);
    }

    pipeline
        .add_many([&filesrc, &demux, &h264parse, &decoder, sink])
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    gst::Element::link_many([&h264parse, &decoder, sink])
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;

    let parse_sink = h264parse
        .static_pad("sink")
        .ok_or_else(|| NativeWaylandError::GStreamer("h264parse has no sink pad".to_owned()))?;
    demux.connect_pad_added(move |_, pad| {
        if parse_sink.is_linked() || !native_demux_pad_is_video(pad) {
            return;
        }
        let _ = pad.link(&parse_sink);
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "video-renderer")]
fn native_explicit_h264_cuda_download_appsink_pipeline(
    source: &std::path::Path,
    sink: &gst::Element,
    cuda_context: Option<&NativeCudaContextHandle>,
) -> Result<gst::Element, NativeWaylandError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_gst_element("qtdemux")?;
    let queue = native_gst_element("queue")?;
    configure_native_low_latency_queue(&queue);
    let h264parse = native_gst_element("h264parse")?;
    let decoder = native_gst_element("nvh264dec")?;
    if decoder.find_property("num-output-surfaces").is_some() {
        decoder.set_property("num-output-surfaces", 2u32);
    }
    if decoder.find_property("max-display-delay").is_some() {
        decoder.set_property("max-display-delay", 0i32);
    }
    if let Some(cuda_context) = cuda_context {
        decoder.set_context(&cuda_context.gst_context()?);
    }
    let cudadownload = native_gst_element("cudadownload")?;
    let capsfilter = native_gst_element("capsfilter")?;
    let caps = "video/x-raw,format=NV12"
        .parse::<gst::Caps>()
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
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
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
    gst::Element::link_many([
        &queue,
        &h264parse,
        &decoder,
        &cudadownload,
        &capsfilter,
        sink,
    ])
    .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeWaylandError::GStreamer("queue has no sink pad".to_owned()))?;
    demux.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() || !native_demux_pad_is_video(pad) {
            return;
        }
        let _ = pad.link(&queue_sink);
    });

    Ok(pipeline.upcast::<gst::Element>())
}

#[cfg(feature = "video-renderer")]
fn configure_native_low_latency_queue(queue: &gst::Element) {
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", 2u32);
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 0u64);
    }
    if queue.find_property("leaky").is_some() {
        queue.set_property_from_str("leaky", "downstream");
    }
}

#[cfg(feature = "video-renderer")]
fn native_gst_element(name: &str) -> Result<gst::Element, NativeWaylandError> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))
}

#[cfg(feature = "video-renderer")]
fn native_demux_pad_is_video(pad: &gst::Pad) -> bool {
    pad.current_caps()
        .or_else(|| Some(pad.query_caps(None)))
        .and_then(|caps| {
            caps.structure(0)
                .map(|structure| structure.name().starts_with("video/"))
        })
        .unwrap_or(false)
}

#[cfg(feature = "video-renderer")]
impl Drop for NativeWaylandVideoPlayer {
    fn drop(&mut self) {
        self.bus.unset_sync_handler();
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(feature = "video-renderer")]
fn set_overlay_render_rectangle(
    overlay: &gst_video::VideoOverlay,
    logical_size: (u32, u32),
    source_size: Option<(u32, u32)>,
    fit: crate::core::FitMode,
) -> Result<NativeWaylandRenderRectangle, NativeWaylandError> {
    let rectangle = render_rectangle_for_fit(fit, logical_size, source_size);
    apply_overlay_render_rectangle(overlay, rectangle)?;
    Ok(rectangle)
}

#[cfg(feature = "video-renderer")]
fn apply_overlay_render_rectangle(
    overlay: &gst_video::VideoOverlay,
    rectangle: NativeWaylandRenderRectangle,
) -> Result<(), NativeWaylandError> {
    overlay
        .set_render_rectangle(rectangle.x, rectangle.y, rectangle.width, rectangle.height)
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))
}

#[cfg(feature = "video-renderer")]
fn render_rectangle_for_fit(
    fit: crate::core::FitMode,
    logical_size: (u32, u32),
    source_size: Option<(u32, u32)>,
) -> NativeWaylandRenderRectangle {
    let target_width = logical_size.0.max(1);
    let target_height = logical_size.1.max(1);
    let full = NativeWaylandRenderRectangle {
        x: 0,
        y: 0,
        width: target_width as i32,
        height: target_height as i32,
    };
    if matches!(fit, crate::core::FitMode::Stretch) {
        return full;
    }
    let Some((source_width, source_height)) =
        source_size.filter(|(width, height)| *width > 0 && *height > 0)
    else {
        return full;
    };

    let width_scale = target_width as f64 / source_width as f64;
    let height_scale = target_height as f64 / source_height as f64;
    let scale = match fit {
        crate::core::FitMode::Cover => width_scale.max(height_scale),
        crate::core::FitMode::Contain | crate::core::FitMode::Tile => width_scale.min(height_scale),
        crate::core::FitMode::Center => width_scale.min(height_scale).min(1.0),
        crate::core::FitMode::Stretch => unreachable!(),
    };
    let width = (source_width as f64 * scale).ceil().max(1.0) as i32;
    let height = (source_height as f64 * scale).ceil().max(1.0) as i32;
    NativeWaylandRenderRectangle {
        x: (target_width as i32 - width) / 2,
        y: (target_height as i32 - height) / 2,
        width,
        height,
    }
}

#[cfg(feature = "video-renderer")]
fn native_video_source_size(
    reports: &[crate::renderer::video::VideoCapsReport],
) -> Option<(u32, u32)> {
    reports
        .iter()
        .flat_map(|report| report.structures.iter())
        .filter_map(|structure| {
            let width = u32::try_from(structure.width?).ok()?;
            let height = u32::try_from(structure.height?).ok()?;
            (width > 0 && height > 0).then_some((width, height))
        })
        .max_by_key(|(width, height)| u64::from(*width) * u64::from(*height))
}

#[cfg(feature = "video-renderer")]
fn configure_native_sink_frame_limiter(
    sink: &gst::Element,
    target_max_fps: Option<u32>,
) -> Result<(), NativeWaylandError> {
    let Some(target_max_fps) = target_max_fps.filter(|target_max_fps| *target_max_fps > 0) else {
        return Ok(());
    };
    if sink.find_property("throttle-time").is_none() {
        return Ok(());
    }
    let throttle_time_ns = 1_000_000_000u64 / u64::from(target_max_fps);
    sink.set_property("throttle-time", throttle_time_ns);
    Ok(())
}

fn select_native_output(
    output_state: &OutputState,
    requested_name: Option<&str>,
) -> Result<Option<wl_output::WlOutput>, NativeWaylandError> {
    let Some(requested_name) = requested_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return Ok(None);
    };

    let mut case_insensitive_match = None;
    for output in output_state.outputs() {
        let Some(info) = output_state.info(&output) else {
            continue;
        };
        if native_output_info_matches(&info, requested_name, true) {
            return Ok(Some(output));
        }
        if case_insensitive_match.is_none()
            && native_output_info_matches(&info, requested_name, false)
        {
            case_insensitive_match = Some(output);
        }
    }

    if let Some(output) = case_insensitive_match {
        return Ok(Some(output));
    }

    let known_outputs = native_output_snapshots(output_state);
    Err(NativeWaylandError::Wayland(format!(
        "native Wayland output {requested_name:?} was not found; known outputs: {}",
        native_output_labels(&known_outputs)
    )))
}

fn native_output_info_matches(info: &OutputInfo, requested_name: &str, exact: bool) -> bool {
    let combined_make_model = format!("{} {}", info.make, info.model);
    let candidates = [
        info.name.as_deref(),
        info.description.as_deref(),
        Some(info.make.as_str()),
        Some(info.model.as_str()),
        Some(combined_make_model.as_str()),
    ];
    let id = info.id.to_string();

    candidates
        .into_iter()
        .flatten()
        .chain(std::iter::once(id.as_str()))
        .any(|candidate| {
            if exact {
                candidate == requested_name
            } else {
                candidate.eq_ignore_ascii_case(requested_name)
            }
        })
}

fn native_output_snapshots(output_state: &OutputState) -> Vec<NativeWaylandOutputSnapshot> {
    output_state
        .outputs()
        .filter_map(|output| {
            output_state
                .info(&output)
                .map(|info| native_output_snapshot(&info))
        })
        .collect()
}

fn native_output_snapshot(info: &OutputInfo) -> NativeWaylandOutputSnapshot {
    let current_mode =
        info.modes
            .iter()
            .find(|mode| mode.current)
            .map(|mode| NativeWaylandOutputModeSnapshot {
                width: mode.dimensions.0,
                height: mode.dimensions.1,
                refresh_millihertz: mode.refresh_rate,
            });
    NativeWaylandOutputSnapshot {
        id: info.id,
        name: info.name.clone(),
        description: info.description.clone(),
        make: info.make.clone(),
        model: info.model.clone(),
        logical_position: info.logical_position,
        logical_size: info.logical_size,
        scale_factor: info.scale_factor,
        current_mode,
    }
}

fn native_output_labels(outputs: &[NativeWaylandOutputSnapshot]) -> String {
    if outputs.is_empty() {
        return "none".to_owned();
    }
    outputs
        .iter()
        .map(|output| {
            let name = output.name.as_deref().unwrap_or("<unnamed>");
            let description = output.description.as_deref().unwrap_or("");
            if description.is_empty() {
                format!("{name}#{}", output.id)
            } else {
                format!("{name}#{} ({description})", output.id)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(feature = "video-renderer")]
#[link(name = "gstwayland-1.0")]
unsafe extern "C" {
    fn gst_wl_display_handle_context_new(display: *mut c_void) -> *mut gst::ffi::GstContext;
}

#[cfg(feature = "video-renderer")]
#[link(name = "gstvideo-1.0")]
unsafe extern "C" {
    fn gst_video_dma_drm_fourcc_from_format(format: i32) -> u32;
    fn gst_video_dma_drm_fourcc_from_string(format_str: *const c_char, modifier: *mut u64) -> u32;
}

#[cfg(feature = "video-renderer")]
#[repr(C)]
struct NativeGstCudaMemory {
    _private: [u8; 0],
}

#[cfg(feature = "video-renderer")]
#[repr(C)]
struct NativeGstCudaContext {
    _private: [u8; 0],
}

#[cfg(feature = "video-renderer")]
#[repr(C)]
struct NativeGstGLMemoryEGL {
    _private: [u8; 0],
}

#[cfg(feature = "video-renderer")]
#[repr(C)]
struct NativeGstGLBaseMemory {
    mem: gst::ffi::GstMemory,
    context: *mut NativeGstGLContext,
}

#[cfg(feature = "video-renderer")]
#[repr(C)]
struct NativeGstGLMemory {
    base: NativeGstGLBaseMemory,
}

#[cfg(feature = "video-renderer")]
#[repr(C)]
struct NativeGstGLContext {
    _private: [u8; 0],
}

#[cfg(feature = "video-renderer")]
#[repr(C)]
struct NativeGstEGLImage {
    _private: [u8; 0],
}

#[cfg(feature = "video-renderer")]
#[link(name = "gstcuda-1.0")]
unsafe extern "C" {
    fn gst_cuda_load_library() -> gst::glib::ffi::gboolean;
    fn gst_cuda_context_new(device_id: u32) -> *mut NativeGstCudaContext;
    fn gst_context_new_cuda_context(
        cuda_ctx: *mut NativeGstCudaContext,
    ) -> *mut gst::ffi::GstContext;
    fn gst_cuda_buffer_pool_new(context: *mut NativeGstCudaContext)
    -> *mut gst::ffi::GstBufferPool;
    fn gst_buffer_pool_config_set_cuda_alloc_method(
        config: *mut gst::ffi::GstStructure,
        method: i32,
    );
    fn gst_cuda_memory_get_alloc_method(mem: *mut NativeGstCudaMemory) -> i32;
    fn gst_cuda_memory_export(mem: *mut NativeGstCudaMemory, os_handle: *mut c_void) -> i32;
}

#[cfg(feature = "video-renderer")]
const GST_CUDA_MEMORY_ALLOC_MMAP: i32 = 2;

#[cfg(feature = "video-renderer")]
#[link(name = "gstallocators-1.0")]
unsafe extern "C" {
    fn gst_is_dmabuf_memory(mem: *mut gst::ffi::GstMemory) -> gst::glib::ffi::gboolean;
    fn gst_dmabuf_memory_get_fd(mem: *mut gst::ffi::GstMemory) -> i32;
}

#[cfg(feature = "video-renderer")]
#[link(name = "gstgl-1.0")]
unsafe extern "C" {
    fn gst_is_gl_memory(mem: *mut gst::ffi::GstMemory) -> gst::glib::ffi::gboolean;
    fn gst_is_gl_memory_egl(mem: *mut gst::ffi::GstMemory) -> gst::glib::ffi::gboolean;
    fn gst_gl_memory_egl_get_image(mem: *mut NativeGstGLMemoryEGL) -> *mut c_void;
    fn gst_egl_image_from_texture(
        context: *mut NativeGstGLContext,
        gl_mem: *mut NativeGstGLMemory,
        attribs: *mut usize,
    ) -> *mut NativeGstEGLImage;
    fn gst_egl_image_export_dmabuf(
        image: *mut NativeGstEGLImage,
        fd: *mut i32,
        stride: *mut i32,
        offset: *mut usize,
    ) -> gst::glib::ffi::gboolean;
    fn gst_gl_context_thread_add(
        context: *mut NativeGstGLContext,
        func: Option<unsafe extern "C" fn(*mut NativeGstGLContext, *mut c_void)>,
        data: *mut c_void,
    );
}

#[cfg(feature = "video-renderer")]
#[repr(C)]
struct NativeGbmDeviceRaw {
    _private: [u8; 0],
}

#[cfg(feature = "video-renderer")]
#[repr(C)]
struct NativeGbmBoRaw {
    _private: [u8; 0],
}

#[cfg(feature = "video-renderer")]
#[link(name = "gbm")]
unsafe extern "C" {
    fn gbm_create_device(fd: RawFd) -> *mut NativeGbmDeviceRaw;
    fn gbm_device_destroy(gbm: *mut NativeGbmDeviceRaw);
    fn gbm_device_is_format_supported(gbm: *mut NativeGbmDeviceRaw, format: u32, flags: u32)
    -> i32;
    fn gbm_bo_create(
        gbm: *mut NativeGbmDeviceRaw,
        width: u32,
        height: u32,
        format: u32,
        flags: u32,
    ) -> *mut NativeGbmBoRaw;
    fn gbm_bo_map(
        bo: *mut NativeGbmBoRaw,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        flags: u32,
        stride: *mut u32,
        map_data: *mut *mut c_void,
    ) -> *mut c_void;
    fn gbm_bo_unmap(bo: *mut NativeGbmBoRaw, map_data: *mut c_void);
    fn gbm_bo_get_stride_for_plane(bo: *mut NativeGbmBoRaw, plane: i32) -> u32;
    fn gbm_bo_get_offset(bo: *mut NativeGbmBoRaw, plane: i32) -> u32;
    fn gbm_bo_get_modifier(bo: *mut NativeGbmBoRaw) -> u64;
    fn gbm_bo_get_plane_count(bo: *mut NativeGbmBoRaw) -> u32;
    fn gbm_bo_get_fd_for_plane(bo: *mut NativeGbmBoRaw, plane: i32) -> RawFd;
    fn gbm_bo_destroy(bo: *mut NativeGbmBoRaw);
}

#[cfg(feature = "video-renderer")]
const GBM_BO_USE_RENDERING: u32 = 1 << 2;
#[cfg(feature = "video-renderer")]
const GBM_BO_USE_WRITE: u32 = 1 << 3;
#[cfg(feature = "video-renderer")]
const GBM_BO_USE_LINEAR: u32 = 1 << 4;
#[cfg(feature = "video-renderer")]
const GBM_BO_TRANSFER_WRITE: u32 = 1 << 1;
#[cfg(feature = "video-renderer")]
const GBM_MAX_PLANES: u32 = 4;

#[cfg(feature = "video-renderer")]
const DRM_FORMAT_XRGB8888: u32 = 0x3432_5258;
#[cfg(feature = "video-renderer")]
const DRM_FORMAT_NV12: u32 = 0x3231_564e;

#[cfg(feature = "video-renderer")]
const DRM_FORMAT_MOD_LINEAR: u64 = 0;
#[cfg(feature = "video-renderer")]
const DRM_FORMAT_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

struct NativeWaylandState {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    dmabuf_state: DmabufState,
    shm: Shm,
    compositor: CompositorState,
    layer: Option<LayerSurface>,
    layer_kind: NativeWaylandLayer,
    requested_output_name: Option<String>,
    selected_output_id: Option<u32>,
    scale: NativeScaleState,
    logical_size: Option<(u32, u32)>,
    configured: bool,
    opaque_region_enabled: bool,
    input_passthrough_enabled: bool,
    opaque_region: Option<Region>,
    input_region: Option<Region>,
    parent_mapping_buffer: Option<NativeWaylandParentMappingBuffer>,
    dmabuf_runtime: NativeDmabufRuntimeState,
}

impl NativeWaylandState {
    fn reconfigure(&mut self) {
        let Some((width, height)) = self.logical_size else {
            return;
        };

        if !self.scale.received {
            let outputs: Vec<wl_output::WlOutput> = self.output_state.outputs().collect();
            for output in &outputs {
                if self
                    .scale
                    .compute_from_output(&self.output_state, output, self.logical_size)
                {
                    break;
                }
            }
        }

        self.apply_surface_regions(width, height);
        self.attach_parent_mapping_buffer();

        if let Some(viewport) = &self.scale.viewport {
            viewport.set_destination(width as i32, height as i32);
        }

        let Some(layer) = self.layer.as_ref() else {
            return;
        };
        layer.set_size(width, height);
        let _ = layer.set_buffer_scale(1);
        layer.commit();
        self.configured = true;
    }

    fn apply_surface_regions(&mut self, width: u32, height: u32) {
        let Some(layer) = self.layer.as_ref() else {
            return;
        };

        if self.opaque_region_enabled {
            if let Ok(region) = Region::new(&self.compositor) {
                region.add(0, 0, width as i32, height as i32);
                layer.set_opaque_region(Some(region.wl_region()));
                self.opaque_region = Some(region);
            }
        } else {
            layer.set_opaque_region(None);
            self.opaque_region = None;
        }

        if self.input_passthrough_enabled {
            if let Ok(region) = Region::new(&self.compositor) {
                layer.set_input_region(Some(region.wl_region()));
                self.input_region = Some(region);
            }
        } else {
            layer.set_input_region(None);
            self.input_region = None;
        }
    }

    fn snapshot(&self) -> NativeWaylandSurfaceSnapshot {
        let known_outputs = native_output_snapshots(&self.output_state);
        let selected_output = self
            .selected_output_id
            .and_then(|id| known_outputs.iter().find(|output| output.id == id).cloned());
        NativeWaylandSurfaceSnapshot {
            logical_size: self.logical_size,
            scale_num: self.scale.num,
            scale_den: NativeScaleState::DENOMINATOR,
            configured: self.configured,
            surface_protocol_id: self
                .layer
                .as_ref()
                .map(|layer| layer.wl_surface().id().protocol_id())
                .unwrap_or_default(),
            layer: self.layer_kind,
            requested_output_name: self.requested_output_name.clone(),
            selected_output,
            known_outputs,
            parent_mapping_buffer_attached: self.parent_mapping_buffer.is_some(),
            opaque_region_enabled: self.opaque_region_enabled,
            input_passthrough_enabled: self.input_passthrough_enabled,
            dmabuf: self.dmabuf_runtime.snapshot(&self.dmabuf_state),
        }
    }

    fn attach_parent_mapping_buffer(&mut self) {
        let Some(layer) = self.layer.as_ref() else {
            return;
        };
        if self.parent_mapping_buffer.is_some() {
            return;
        }

        let mut pool = match SlotPool::new(4, &self.shm) {
            Ok(pool) => pool,
            Err(_) => return,
        };
        let (buffer, canvas) = match pool.create_buffer(1, 1, 4, wl_shm::Format::Argb8888) {
            Ok(buffer) => buffer,
            Err(_) => return,
        };
        // Transparent ARGB pixel. This maps the parent layer surface without
        // drawing visible content if waylandsink renders on a child surface.
        for pixel in canvas.chunks_exact_mut(4) {
            pixel.copy_from_slice(&0x00000000u32.to_le_bytes());
        }
        layer.wl_surface().damage_buffer(0, 0, 1, 1);
        if buffer.attach_to(layer.wl_surface()).is_err() {
            return;
        }
        layer.commit();
        self.parent_mapping_buffer = Some(NativeWaylandParentMappingBuffer {
            _pool: pool,
            _buffer: buffer,
        });
    }

    fn request_dmabuf_feedback(&mut self, qh: &QueueHandle<Self>) {
        if self.dmabuf_runtime.feedback_requested {
            return;
        }

        if let Ok(feedback) = self.dmabuf_state.get_default_feedback(qh) {
            self.dmabuf_runtime.default_feedback_id = Some(feedback.id().protocol_id());
            self.dmabuf_runtime.default_feedback_requested = true;
            self.dmabuf_runtime.default_feedback = Some(feedback);
        }

        if let Some(layer) = self.layer.as_ref()
            && let Ok(feedback) = self
                .dmabuf_state
                .get_surface_feedback(layer.wl_surface(), qh)
        {
            self.dmabuf_runtime.surface_feedback_id = Some(feedback.id().protocol_id());
            self.dmabuf_runtime.surface_feedback_requested = true;
            self.dmabuf_runtime.surface_feedback = Some(feedback);
        }

        self.dmabuf_runtime.feedback_requested = self.dmabuf_runtime.default_feedback_requested
            || self.dmabuf_runtime.surface_feedback_requested;
    }

    #[cfg(feature = "video-renderer")]
    fn can_accept_dmabuf_frame(&self) -> bool {
        self.dmabuf_runtime.buffers_busy_len() < NativeDmabufRuntimeState::MAX_BUFFERS_IN_FLIGHT
    }

    #[cfg(feature = "video-renderer")]
    fn present_dmabuf_frame(
        &mut self,
        qh: &QueueHandle<Self>,
        frame: NativeWaylandDmabufFrame,
    ) -> Result<(), NativeWaylandError> {
        self.dmabuf_runtime.frames_submitted += 1;
        self.dmabuf_runtime.last_frame_format = Some(frame.format);

        if self.layer.is_none() {
            self.dmabuf_runtime.frame_attach_failures += 1;
            self.dmabuf_runtime.last_attach_error = Some("missing_layer_surface".to_owned());
            return Ok(());
        };

        if self.dmabuf_runtime.buffers_busy_len() >= NativeDmabufRuntimeState::MAX_BUFFERS_IN_FLIGHT
        {
            self.dmabuf_runtime.frame_attach_skips += 1;
            self.dmabuf_runtime.last_attach_error = Some("dmabuf_buffers_busy_full".to_owned());
            return Ok(());
        }

        let modifier = match frame.modifier {
            Some(modifier)
                if self
                    .dmabuf_runtime
                    .supports_format_modifier(frame.format, modifier) =>
            {
                modifier
            }
            Some(modifier) => {
                self.dmabuf_runtime.frame_attach_skips += 1;
                self.dmabuf_runtime.last_attach_error =
                    Some(format!("unsupported_format_modifier:{modifier}"));
                return Ok(());
            }
            None if self
                .dmabuf_runtime
                .supports_format_modifier(frame.format, DRM_FORMAT_MOD_INVALID) =>
            {
                DRM_FORMAT_MOD_INVALID
            }
            None if self
                .dmabuf_runtime
                .supports_format_modifier(frame.format, DRM_FORMAT_MOD_LINEAR) =>
            {
                DRM_FORMAT_MOD_LINEAR
            }
            None => {
                self.dmabuf_runtime.frame_attach_skips += 1;
                self.dmabuf_runtime.last_attach_error =
                    Some("missing_supported_implicit_or_linear_modifier".to_owned());
                return Ok(());
            }
        };
        self.dmabuf_runtime.last_frame_modifier = Some(modifier);

        if frame.fds.is_empty()
            || frame.planes.is_empty()
            || frame
                .planes
                .iter()
                .any(|plane| plane.stride == 0 || plane.fd_index >= frame.fds.len())
            || frame.width > i32::MAX as u32
            || frame.height > i32::MAX as u32
        {
            self.dmabuf_runtime.frame_attach_failures += 1;
            self.dmabuf_runtime.last_attach_error = Some("invalid_dmabuf_frame_layout".to_owned());
            return Ok(());
        }

        let params = self
            .dmabuf_state
            .create_params(qh)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let mut wayland_fds = Vec::with_capacity(frame.fds.len());
        for fd in &frame.fds {
            match fd.try_clone() {
                Ok(fd) => wayland_fds.push(fd),
                Err(err) => {
                    self.dmabuf_runtime.frame_attach_failures += 1;
                    self.dmabuf_runtime.last_attach_error =
                        Some(format!("invalid_exported_fd:{err}"));
                    return Ok(());
                }
            }
        }
        for (plane_idx, plane) in frame.planes.iter().enumerate() {
            let Some(fd) = wayland_fds.get(plane.fd_index) else {
                self.dmabuf_runtime.frame_attach_failures += 1;
                self.dmabuf_runtime.last_attach_error =
                    Some("invalid_dmabuf_plane_fd_index".to_owned());
                return Ok(());
            };
            params.add(
                fd.as_fd(),
                plane_idx as u32,
                plane.offset,
                plane.stride,
                modifier,
            );
        }
        let params_proxy = params.create(
            frame.width as i32,
            frame.height as i32,
            frame.format,
            zwp_linux_buffer_params_v1::Flags::empty(),
        );
        let params_id = params_proxy.id().protocol_id();

        self.dmabuf_runtime.last_attach_error = None;
        self.dmabuf_runtime
            .buffers_pending
            .push(NativeWaylandDmabufPendingBuffer {
                params_id,
                _params: params_proxy,
                frame,
            });
        Ok(())
    }

    #[cfg(feature = "video-renderer")]
    fn attach_created_dmabuf_buffer(
        &mut self,
        params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        buffer: wl_buffer::WlBuffer,
    ) {
        self.dmabuf_runtime.buffers_created += 1;
        let params_id = params.id().protocol_id();
        let Some(index) = self
            .dmabuf_runtime
            .buffers_pending
            .iter()
            .position(|pending| pending.params_id == params_id)
        else {
            self.dmabuf_runtime.frame_attach_failures += 1;
            self.dmabuf_runtime.last_attach_error =
                Some(format!("unknown_dmabuf_params_created:{params_id}"));
            return;
        };
        let NativeWaylandDmabufPendingBuffer { frame, _params, .. } =
            self.dmabuf_runtime.buffers_pending.swap_remove(index);

        let Some(layer) = self.layer.as_ref() else {
            self.dmabuf_runtime.frame_attach_failures += 1;
            self.dmabuf_runtime.last_attach_error =
                Some("missing_layer_surface_for_created_dmabuf".to_owned());
            return;
        };

        let buffer_id = buffer.id().protocol_id();
        let surface = layer.wl_surface();
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, frame.width as i32, frame.height as i32);
        layer.commit();

        self.dmabuf_runtime.frames_attached += 1;
        self.dmabuf_runtime.last_attach_error = None;
        self.dmabuf_runtime
            .buffers_in_flight
            .push(NativeWaylandDmabufAttachedBuffer {
                buffer_id,
                _buffer: buffer,
                _params,
                _fds: frame.fds,
                _gst_buffer: frame.gst_buffer,
                _gbm_bo: frame.gbm_bo,
            });
    }

    #[cfg(feature = "video-renderer")]
    fn fail_pending_dmabuf_buffer(
        &mut self,
        params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    ) {
        self.dmabuf_runtime.buffer_create_failures += 1;
        self.dmabuf_runtime.frame_attach_failures += 1;
        let params_id = params.id().protocol_id();
        if let Some(index) = self
            .dmabuf_runtime
            .buffers_pending
            .iter()
            .position(|pending| pending.params_id == params_id)
        {
            let pending = self.dmabuf_runtime.buffers_pending.swap_remove(index);
            self.dmabuf_runtime.last_attach_error = Some(format!(
                "dmabuf_create_failed:format={}:modifier={}:{}x{}",
                pending.frame.format,
                pending.frame.modifier.unwrap_or(0),
                pending.frame.width,
                pending.frame.height
            ));
        } else {
            self.dmabuf_runtime.last_attach_error =
                Some(format!("unknown_dmabuf_params_failed:{params_id}"));
        }
    }
}

struct NativeWaylandParentMappingBuffer {
    _pool: SlotPool,
    _buffer: Buffer,
}

#[cfg(feature = "video-renderer")]
struct NativeWaylandDmabufAttachedBuffer {
    buffer_id: u32,
    _buffer: wl_buffer::WlBuffer,
    _params: zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    _fds: Vec<OwnedFd>,
    _gst_buffer: gst::Buffer,
    _gbm_bo: Option<NativeGbmBo>,
}

#[cfg(feature = "video-renderer")]
struct NativeWaylandDmabufPendingBuffer {
    params_id: u32,
    _params: zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    frame: NativeWaylandDmabufFrame,
}

#[derive(Default)]
struct NativeDmabufRuntimeState {
    default_feedback: Option<zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1>,
    surface_feedback: Option<zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1>,
    default_feedback_id: Option<u32>,
    surface_feedback_id: Option<u32>,
    feedback_requested: bool,
    default_feedback_requested: bool,
    surface_feedback_requested: bool,
    feedback_count: u64,
    latest_feedback: Option<NativeWaylandDmabufFeedbackSnapshot>,
    buffers_created: u64,
    buffer_create_failures: u64,
    buffers_released: u64,
    frames_submitted: u64,
    frames_attached: u64,
    frame_attach_failures: u64,
    frame_attach_skips: u64,
    last_frame_format: Option<u32>,
    last_frame_modifier: Option<u64>,
    last_attach_error: Option<String>,
    #[cfg(feature = "video-renderer")]
    buffers_pending: Vec<NativeWaylandDmabufPendingBuffer>,
    #[cfg(feature = "video-renderer")]
    buffers_in_flight: Vec<NativeWaylandDmabufAttachedBuffer>,
}

impl NativeDmabufRuntimeState {
    const SAMPLE_LIMIT: usize = 8;
    const MAX_BUFFERS_IN_FLIGHT: usize = 4;

    fn snapshot(&self, dmabuf_state: &DmabufState) -> NativeWaylandDmabufSnapshot {
        NativeWaylandDmabufSnapshot {
            supports_linux_dmabuf_protocol: dmabuf_state.version().is_some(),
            linux_dmabuf_version: dmabuf_state.version(),
            linux_dmabuf_modifier_count: dmabuf_state.modifiers().len(),
            linux_dmabuf_modifier_samples: dmabuf_state
                .modifiers()
                .iter()
                .take(Self::SAMPLE_LIMIT)
                .map(NativeWaylandDmabufFormatSnapshot::from)
                .collect(),
            linux_dmabuf_feedback_requested: self.feedback_requested,
            linux_dmabuf_default_feedback_requested: self.default_feedback_requested,
            linux_dmabuf_surface_feedback_requested: self.surface_feedback_requested,
            linux_dmabuf_feedback_received: self.feedback_count > 0,
            linux_dmabuf_feedback_count: self.feedback_count,
            linux_dmabuf_feedback: self.latest_feedback.clone(),
            dmabuf_buffers_created: self.buffers_created,
            dmabuf_buffer_create_failures: self.buffer_create_failures,
            dmabuf_buffers_released: self.buffers_released,
            dmabuf_frames_submitted: self.frames_submitted,
            dmabuf_frames_attached: self.frames_attached,
            dmabuf_frame_attach_failures: self.frame_attach_failures,
            dmabuf_frame_attach_skips: self.frame_attach_skips,
            dmabuf_buffers_pending: self.buffers_pending_len(),
            dmabuf_buffers_in_flight: self.buffers_in_flight_len(),
            dmabuf_last_frame_format: self.last_frame_format,
            dmabuf_last_frame_modifier: self.last_frame_modifier,
            dmabuf_last_attach_error: self.last_attach_error.clone(),
        }
    }

    fn buffers_pending_len(&self) -> usize {
        #[cfg(feature = "video-renderer")]
        {
            self.buffers_pending.len()
        }
        #[cfg(not(feature = "video-renderer"))]
        {
            0
        }
    }

    fn buffers_in_flight_len(&self) -> usize {
        #[cfg(feature = "video-renderer")]
        {
            self.buffers_in_flight.len()
        }
        #[cfg(not(feature = "video-renderer"))]
        {
            0
        }
    }

    fn buffers_busy_len(&self) -> usize {
        self.buffers_pending_len() + self.buffers_in_flight_len()
    }

    fn feedback_source(
        &self,
        proxy: &zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
    ) -> NativeWaylandDmabufFeedbackSource {
        let id = proxy.id().protocol_id();
        if self.surface_feedback_id == Some(id) {
            NativeWaylandDmabufFeedbackSource::Surface
        } else if self.default_feedback_id == Some(id) {
            NativeWaylandDmabufFeedbackSource::Default
        } else {
            NativeWaylandDmabufFeedbackSource::Unknown
        }
    }

    fn record_feedback(
        &mut self,
        proxy: &zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
        feedback: DmabufFeedback,
    ) {
        let source = self.feedback_source(proxy);
        let snapshot = NativeWaylandDmabufFeedbackSnapshot::from_feedback(source, &feedback);
        self.feedback_count += 1;

        let current_is_surface = self
            .latest_feedback
            .as_ref()
            .map(|feedback| feedback.source == NativeWaylandDmabufFeedbackSource::Surface)
            .unwrap_or(false);
        if source == NativeWaylandDmabufFeedbackSource::Surface || !current_is_surface {
            self.latest_feedback = Some(snapshot);
        }
    }

    fn supports_format_modifier(&self, format: u32, modifier: u64) -> bool {
        if let Some(feedback) = self.latest_feedback.as_ref()
            && feedback
                .format_table
                .iter()
                .any(|entry| entry.format == format && entry.modifier == modifier)
        {
            return true;
        }
        false
    }

    fn release_buffer(&mut self, buffer: &wl_buffer::WlBuffer) {
        self.buffers_released += 1;
        #[cfg(feature = "video-renderer")]
        {
            let buffer_id = buffer.id().protocol_id();
            if let Some(index) = self
                .buffers_in_flight
                .iter()
                .position(|in_flight| in_flight.buffer_id == buffer_id)
            {
                self.buffers_in_flight.swap_remove(index);
            }
        }
    }
}

impl NativeWaylandDmabufFeedbackSnapshot {
    fn from_feedback(source: NativeWaylandDmabufFeedbackSource, feedback: &DmabufFeedback) -> Self {
        let format_table = feedback.format_table();
        let format_fourccs: Vec<u32> = format_table
            .iter()
            .map(|format| format.format)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Self {
            source,
            main_device: feedback.main_device() as u64,
            format_count: format_table.len(),
            format_fourcc_count: format_fourccs.len(),
            format_fourccs,
            format_table: format_table
                .iter()
                .map(NativeWaylandDmabufFormatSnapshot::from)
                .collect(),
            format_samples: format_table
                .iter()
                .take(NativeDmabufRuntimeState::SAMPLE_LIMIT)
                .map(NativeWaylandDmabufFormatSnapshot::from)
                .collect(),
            tranche_count: feedback.tranches().len(),
            tranche_samples: feedback
                .tranches()
                .iter()
                .take(NativeDmabufRuntimeState::SAMPLE_LIMIT)
                .map(|tranche| NativeWaylandDmabufTrancheSnapshot {
                    device: tranche.device as u64,
                    flags: format!("{:?}", tranche.flags),
                    format_count: tranche.formats.len(),
                    format_indices_sample: tranche
                        .formats
                        .iter()
                        .take(NativeDmabufRuntimeState::SAMPLE_LIMIT)
                        .copied()
                        .collect(),
                })
                .collect(),
        }
    }
}

impl From<&smithay_client_toolkit::dmabuf::DmabufFormat> for NativeWaylandDmabufFormatSnapshot {
    fn from(format: &smithay_client_toolkit::dmabuf::DmabufFormat) -> Self {
        Self {
            format: format.format,
            modifier: format.modifier,
        }
    }
}

impl CompositorHandler for NativeWaylandState {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for NativeWaylandState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.reconfigure();
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.reconfigure();
    }

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for NativeWaylandState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl DmabufHandler for NativeWaylandState {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_feedback(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        proxy: &zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
        feedback: DmabufFeedback,
    ) {
        self.dmabuf_runtime.record_feedback(proxy, feedback);
    }

    fn created(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        buffer: wl_buffer::WlBuffer,
    ) {
        self.attach_created_dmabuf_buffer(params, buffer);
    }

    fn failed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    ) {
        self.fail_pending_dmabuf_buffer(params);
    }

    fn released(&mut self, _: &Connection, _: &QueueHandle<Self>, buffer: &wl_buffer::WlBuffer) {
        self.dmabuf_runtime.release_buffer(buffer);
    }
}

impl SeatHandler for NativeWaylandState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl LayerShellHandler for NativeWaylandState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {}

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let (width, height) = configure.new_size;
        if width == 0 || height == 0 {
            return;
        }
        self.logical_size = Some((width, height));
        self.reconfigure();
    }
}

delegate_compositor!(NativeWaylandState);
delegate_output!(NativeWaylandState);
delegate_shm!(NativeWaylandState);
delegate_dmabuf!(NativeWaylandState);
delegate_seat!(NativeWaylandState);
delegate_layer!(NativeWaylandState);
delegate_registry!(NativeWaylandState);

impl ProvidesRegistryState for NativeWaylandState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

#[derive(Debug)]
struct NativeWaylandProtocolData;

impl Dispatch<WpFractionalScaleManagerV1, NativeWaylandProtocolData, NativeWaylandState>
    for NativeWaylandState
{
    fn event(
        _: &mut NativeWaylandState,
        _: &WpFractionalScaleManagerV1,
        _: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _: &NativeWaylandProtocolData,
        _: &Connection,
        _: &QueueHandle<NativeWaylandState>,
    ) {
    }
}

impl Dispatch<WpFractionalScaleV1, NativeWaylandProtocolData, NativeWaylandState>
    for NativeWaylandState
{
    fn event(
        state: &mut NativeWaylandState,
        _: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as Proxy>::Event,
        _: &NativeWaylandProtocolData,
        _: &Connection,
        _: &QueueHandle<NativeWaylandState>,
    ) {
        if let wp_fractional_scale_v1::Event::PreferredScale { scale } = event {
            state.scale.handle_preferred_scale(scale);
            state.reconfigure();
        }
    }
}

impl Dispatch<WpViewporter, NativeWaylandProtocolData, NativeWaylandState> for NativeWaylandState {
    fn event(
        _: &mut NativeWaylandState,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &NativeWaylandProtocolData,
        _: &Connection,
        _: &QueueHandle<NativeWaylandState>,
    ) {
    }
}

impl Dispatch<WpViewport, NativeWaylandProtocolData, NativeWaylandState> for NativeWaylandState {
    fn event(
        _: &mut NativeWaylandState,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &NativeWaylandProtocolData,
        _: &Connection,
        _: &QueueHandle<NativeWaylandState>,
    ) {
    }
}

struct NativeScaleState {
    #[allow(dead_code)]
    fractional_manager: Option<WpFractionalScaleManagerV1>,
    #[allow(dead_code)]
    fractional_scale: Option<WpFractionalScaleV1>,
    #[allow(dead_code)]
    viewporter: Option<WpViewporter>,
    viewport: Option<WpViewport>,
    num: u32,
    received: bool,
}

impl NativeScaleState {
    const DENOMINATOR: u32 = 120;

    fn new(
        fractional_manager: Option<WpFractionalScaleManagerV1>,
        fractional_scale: Option<WpFractionalScaleV1>,
        viewporter: Option<WpViewporter>,
        viewport: Option<WpViewport>,
    ) -> Self {
        Self {
            fractional_manager,
            fractional_scale,
            viewporter,
            viewport,
            num: Self::DENOMINATOR,
            received: false,
        }
    }

    fn handle_preferred_scale(&mut self, scale: u32) {
        self.num = scale;
        self.received = true;
    }

    fn compute_from_output(
        &mut self,
        output_state: &OutputState,
        output: &wl_output::WlOutput,
        fallback_logical: Option<(u32, u32)>,
    ) -> bool {
        if self.received {
            return false;
        }
        let Some(info) = output_state.info(output) else {
            return false;
        };
        let Some(mode) = info.modes.iter().find(|mode| mode.current) else {
            return false;
        };
        let (logical_width, logical_height) = match info.logical_size {
            Some((width, height)) if width > 0 && height > 0 => (width, height),
            _ => match fallback_logical {
                Some((width, height)) => (width as i32, height as i32),
                None => return false,
            },
        };
        if logical_width <= 0 || logical_height <= 0 {
            return false;
        }

        let width_scale = mode.dimensions.0 as f64 / logical_width as f64;
        let height_scale = mode.dimensions.1 as f64 / logical_height as f64;
        let computed = ((width_scale + height_scale) / 2.0 * Self::DENOMINATOR as f64).round();
        let computed = computed.max(Self::DENOMINATOR as f64) as u32;
        self.num = computed;
        self.received = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_experimental_native_capabilities() {
        let capabilities = capabilities();
        assert!(capabilities.built);
        assert!(capabilities.experimental);
        assert!(capabilities.owns_wlr_layer_shell_surface);
        assert!(capabilities.exports_raw_wayland_handles);
        assert_eq!(
            capabilities.native_video_overlay,
            cfg!(feature = "video-renderer")
        );
        assert!(capabilities.probes_linux_dmabuf_protocol);
        assert_eq!(
            capabilities.native_dmabuf_buffer_attach,
            cfg!(feature = "video-renderer")
        );
        assert!(!capabilities.consumes_render_sync);
    }

    #[cfg(feature = "video-renderer")]
    #[test]
    fn native_video_cover_rectangle_crops_instead_of_letterboxing() {
        let rectangle = render_rectangle_for_fit(
            crate::core::FitMode::Cover,
            (1707, 1067),
            Some((3840, 2160)),
        );
        assert_eq!(
            rectangle,
            NativeWaylandRenderRectangle {
                x: -95,
                y: 0,
                width: 1897,
                height: 1067,
            }
        );
    }

    #[cfg(feature = "video-renderer")]
    #[test]
    fn native_video_contain_rectangle_preserves_letterbox_geometry() {
        let rectangle = render_rectangle_for_fit(
            crate::core::FitMode::Contain,
            (1707, 1067),
            Some((3840, 2160)),
        );
        assert_eq!(
            rectangle,
            NativeWaylandRenderRectangle {
                x: 0,
                y: 53,
                width: 1707,
                height: 961,
            }
        );
    }

    #[cfg(feature = "video-renderer")]
    #[test]
    fn parses_drm_modifier_from_gstreamer_caps_format() {
        assert_eq!(
            native_drm_modifier_from_caps_format("NV12:0x0100000000000002"),
            Some(0x0100_0000_0000_0002)
        );
        assert_eq!(native_drm_modifier_from_caps_format("NV12"), None);
    }
}
