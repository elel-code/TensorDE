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
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
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
};
use std::{collections::BTreeMap, ffi::c_void, fmt, ptr::NonNull};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_seat, wl_surface},
};
use wayland_protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeWaylandHostOptions {
    pub namespace: String,
    pub layer: NativeWaylandLayer,
    pub opaque_region: bool,
    pub input_passthrough: bool,
}

impl Default for NativeWaylandHostOptions {
    fn default() -> Self {
        Self {
            namespace: "gilder-wallpaper-native".to_owned(),
            layer: NativeWaylandLayer::Background,
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
    pub opaque_region_enabled: bool,
    pub input_passthrough_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWaylandSurfaceHandles {
    pub display: NonNull<c_void>,
    pub surface: NonNull<c_void>,
    pub logical_size: (u32, u32),
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
        let (globals, event_queue) = registry_queue_init(&connection)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let qh = event_queue.handle();

        let compositor = CompositorState::bind(&globals, &qh)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))?;
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

        let layer = layer_shell.create_layer_surface(
            &qh,
            surface,
            options.layer.into(),
            Some(options.namespace.clone()),
            None,
        );
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(-1);
        layer.set_anchor(Anchor::all());
        layer.set_size(0, 0);
        layer.commit();

        let state = NativeWaylandState {
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            compositor,
            layer,
            layer_kind: options.layer,
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
        };

        Ok(Self {
            connection,
            event_queue,
            state,
        })
    }

    pub fn dispatch_pending(&mut self) -> Result<(), NativeWaylandError> {
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map(|_| ())
            .map_err(|err| NativeWaylandError::Wayland(err.to_string()))
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
        let surface = NonNull::new(self.state.layer.wl_surface().id().as_ptr().cast::<c_void>())
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
        })
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeWaylandVideoOptions {
    pub host: NativeWaylandHostOptions,
    pub output_name: String,
    pub muted: bool,
    pub loop_playback: bool,
    pub target_max_fps: Option<u32>,
    pub sink_throttle: bool,
    pub decoder_policy: crate::config::VideoDecoderPolicy,
    pub start_offset_ms: u64,
    pub pipeline: NativeWaylandVideoPipeline,
}

#[cfg(feature = "video-renderer")]
impl Default for NativeWaylandVideoOptions {
    fn default() -> Self {
        Self {
            host: NativeWaylandHostOptions::default(),
            output_name: "native-wayland".to_owned(),
            muted: true,
            loop_playback: true,
            target_max_fps: Some(240),
            sink_throttle: false,
            decoder_policy: crate::config::VideoDecoderPolicy::HardwarePreferred,
            start_offset_ms: 0,
            pipeline: NativeWaylandVideoPipeline::Playbin,
        }
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeWaylandVideoPipeline {
    Playbin,
    Playbin3,
}

#[cfg(feature = "video-renderer")]
impl NativeWaylandVideoPipeline {
    pub fn element_name(self) -> &'static str {
        match self {
            Self::Playbin => "playbin",
            Self::Playbin3 => "playbin3",
        }
    }
}

#[cfg(feature = "video-renderer")]
impl std::str::FromStr for NativeWaylandVideoPipeline {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "playbin" => Ok(Self::Playbin),
            "playbin3" => Ok(Self::Playbin3),
            other => Err(format!("unsupported native video pipeline: {other}")),
        }
    }
}

#[cfg(feature = "video-renderer")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeWaylandVideoSnapshot {
    pub surface: NativeWaylandSurfaceSnapshot,
    pub pipeline: crate::renderer::video::VideoPipelineSnapshot,
    pub sink_stats: NativeWaylandSinkStats,
    pub video_pipeline: NativeWaylandVideoPipeline,
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
pub struct NativeWaylandVideoSession {
    player: NativeWaylandVideoPlayer,
    host: NativeWaylandHost,
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
        Ok(Self { player, host })
    }

    pub fn play(&self) -> Result<(), NativeWaylandError> {
        self.player.play()
    }

    pub fn tick(&mut self) -> Result<(), NativeWaylandError> {
        self.host.dispatch_pending()?;
        self.player.poll_bus()
    }

    pub fn snapshot(&self) -> NativeWaylandSurfaceSnapshot {
        self.host.snapshot()
    }

    pub fn runtime_snapshot(&self) -> NativeWaylandVideoSnapshot {
        NativeWaylandVideoSnapshot {
            surface: self.host.snapshot(),
            pipeline: self.player.snapshot(),
            sink_stats: self.player.sink_stats(),
            video_pipeline: self.player.pipeline_kind,
        }
    }
}

#[cfg(feature = "video-renderer")]
struct NativeWaylandVideoPlayer {
    pipeline: gst::Element,
    bus: gst::Bus,
    sink: gst::Element,
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

        let uri = gst::glib::filename_to_uri(source, None::<&str>)
            .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
        crate::renderer::video::apply_decoder_rank_policy(options.decoder_policy);
        let sink = gst::ElementFactory::make("waylandsink")
            .property("sync", true)
            .property("fullscreen", false)
            .property("force-aspect-ratio", true)
            .property("enable-last-sample", false)
            .build()
            .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
        let mut tuning =
            crate::renderer::video::configure_video_sink_low_memory(&sink, options.target_max_fps);
        tuning.sink_element = Some("waylandsink+native-layer-surface".to_owned());
        if options.sink_throttle {
            configure_native_sink_frame_limiter(&sink, options.target_max_fps)?;
        }

        // SAFETY: the display pointer comes from NativeWaylandHost's live
        // wayland-client connection. NativeWaylandVideoSession owns the host
        // for at least as long as the GStreamer pipeline can use this context.
        let display_context = unsafe {
            let context = gst_wl_display_handle_context_new(handles.display.as_ptr());
            if context.is_null() {
                return Err(NativeWaylandError::GStreamer(
                    "failed to create Wayland display context".to_owned(),
                ));
            }
            from_glib_full(context)
        };
        sink.set_context(&display_context);

        let overlay = sink
            .clone()
            .dynamic_cast::<gst_video::VideoOverlay>()
            .map_err(|_| {
                NativeWaylandError::GStreamer(
                    "waylandsink does not implement GstVideoOverlay".to_owned(),
                )
            })?;
        let window_handle = handles.window_handle();
        // SAFETY: window_handle is the wl_surface proxy owned by the live
        // NativeWaylandHost. NativeWaylandVideoSession drops the player before
        // dropping the host, so waylandsink cannot outlive the surface.
        unsafe {
            overlay.set_window_handle(window_handle);
        }
        set_overlay_render_rectangle(&overlay, handles.logical_size)?;

        let flags = if options.muted {
            "video"
        } else {
            "video+audio"
        };
        let pipeline = gst::ElementFactory::make(options.pipeline.element_name())
            .property("uri", uri.as_str())
            .property_from_str("flags", flags)
            .property("video-sink", &sink)
            .build()
            .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))?;
        pipeline.set_context(&display_context);
        crate::renderer::video::configure_video_pipeline_low_memory(&pipeline);
        let observed_caps_reports = crate::renderer::video::video_caps_report_store();
        let observed_queue_elements = crate::renderer::video::video_queue_element_store();
        crate::renderer::video::install_video_caps_observers(&pipeline, &observed_caps_reports);
        crate::renderer::video::install_video_queue_observers(&pipeline, &observed_queue_elements);

        let bus = pipeline.bus().ok_or_else(|| {
            NativeWaylandError::GStreamer("native video pipeline has no bus".to_owned())
        })?;
        bus.set_sync_handler(move |_, message| {
            if gst_video::is_video_overlay_prepare_window_handle_message(message) {
                if let Some(src) = message.src()
                    && let Ok(overlay) = src.clone().dynamic_cast::<gst_video::VideoOverlay>()
                {
                    // SAFETY: same lifetime argument as above; the sync
                    // handler only repeats the handle handoff requested by
                    // GstVideoOverlay for the already-owned pipeline.
                    unsafe {
                        overlay.set_window_handle(window_handle);
                    }
                    let _ = set_overlay_render_rectangle(&overlay, handles.logical_size);
                }
                gst::BusSyncReply::Drop
            } else {
                gst::BusSyncReply::Pass
            }
        });

        let player = Self {
            pipeline,
            bus,
            sink,
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

    pub fn poll_bus(&mut self) -> Result<(), NativeWaylandError> {
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
        Ok(())
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

    fn sink_stats(&self) -> NativeWaylandSinkStats {
        if self.sink.find_property("stats").is_none() {
            return NativeWaylandSinkStats::default();
        }
        let stats = self.sink.property::<gst::Structure>("stats");
        NativeWaylandSinkStats {
            rendered: stats.get::<u64>("rendered").ok(),
            dropped: stats.get::<u64>("dropped").ok(),
            average_rate: stats.get::<f64>("average-rate").ok(),
            raw: Some(stats.to_string()),
        }
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
) -> Result<(), NativeWaylandError> {
    overlay
        .set_render_rectangle(0, 0, logical_size.0 as i32, logical_size.1 as i32)
        .map_err(|err| NativeWaylandError::GStreamer(err.to_string()))
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

#[cfg(feature = "video-renderer")]
#[link(name = "gstwayland-1.0")]
unsafe extern "C" {
    fn gst_wl_display_handle_context_new(display: *mut c_void) -> *mut gst::ffi::GstContext;
}

struct NativeWaylandState {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    layer: LayerSurface,
    layer_kind: NativeWaylandLayer,
    scale: NativeScaleState,
    logical_size: Option<(u32, u32)>,
    configured: bool,
    opaque_region_enabled: bool,
    input_passthrough_enabled: bool,
    opaque_region: Option<Region>,
    input_region: Option<Region>,
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

        if let Some(viewport) = &self.scale.viewport {
            viewport.set_destination(width as i32, height as i32);
        }

        self.layer.set_size(width, height);
        let _ = self.layer.set_buffer_scale(1);
        self.layer.commit();
        self.configured = true;
    }

    fn apply_surface_regions(&mut self, width: u32, height: u32) {
        if self.opaque_region_enabled {
            if let Ok(region) = Region::new(&self.compositor) {
                region.add(0, 0, width as i32, height as i32);
                self.layer.set_opaque_region(Some(region.wl_region()));
                self.opaque_region = Some(region);
            }
        } else {
            self.layer.set_opaque_region(None);
            self.opaque_region = None;
        }

        if self.input_passthrough_enabled {
            if let Ok(region) = Region::new(&self.compositor) {
                self.layer.set_input_region(Some(region.wl_region()));
                self.input_region = Some(region);
            }
        } else {
            self.layer.set_input_region(None);
            self.input_region = None;
        }
    }

    fn snapshot(&self) -> NativeWaylandSurfaceSnapshot {
        NativeWaylandSurfaceSnapshot {
            logical_size: self.logical_size,
            scale_num: self.scale.num,
            scale_den: NativeScaleState::DENOMINATOR,
            configured: self.configured,
            surface_protocol_id: self.layer.wl_surface().id().protocol_id(),
            layer: self.layer_kind,
            opaque_region_enabled: self.opaque_region_enabled,
            input_passthrough_enabled: self.input_passthrough_enabled,
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
        assert!(!capabilities.consumes_render_sync);
    }
}
