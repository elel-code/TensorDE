//! Native Wayland layer-shell host for future non-GTK renderers.
//!
//! This module owns only the Wayland surface lifecycle. Content renderers
//! such as video, web, shader, or scene runtimes should be layered on top of
//! the surface host here.

#![allow(unsafe_code)]

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
    collections::BTreeSet, ffi::c_void, fmt, io::ErrorKind, ptr::NonNull, thread, time::Duration,
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
    pub attach_parent_mapping_buffer: bool,
}

impl Default for NativeWaylandHostOptions {
    fn default() -> Self {
        Self {
            namespace: "gilder-wallpaper-native".to_owned(),
            layer: NativeWaylandLayer::Bottom,
            output_name: None,
            opaque_region: true,
            input_passthrough: true,
            attach_parent_mapping_buffer: true,
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
        native_video_overlay: false,
        supports_fractional_scale_protocol: true,
        supports_viewporter_protocol: true,
        probes_linux_dmabuf_protocol: true,
        native_dmabuf_buffer_attach: false,
        consumes_render_sync: false,
        unsafe_policy: "unsafe is allowed but must stay behind audited native Wayland/GPU boundaries",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeWaylandSurfaceSnapshot {
    pub logical_size: Option<(u32, u32)>,
    pub buffer_size: Option<(u32, u32)>,
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
    pub buffer_size: (u32, u32),
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
}

impl fmt::Display for NativeWaylandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wayland(err) => write!(f, "wayland error: {err}"),
            Self::MissingRawHandle(handle) => write!(f, "missing Wayland {handle} handle"),
            Self::Timeout(message) => write!(f, "timeout: {message}"),
        }
    }
}

impl std::error::Error for NativeWaylandError {}

fn native_wayland_connect_to_env() -> Result<Connection, NativeWaylandError> {
    const RETRY_DELAYS_MS: [u64; 3] = [5, 20, 50];
    let mut last_error = None::<String>;

    for attempt in 0..=RETRY_DELAYS_MS.len() {
        match Connection::connect_to_env() {
            Ok(connection) => return Ok(connection),
            Err(err) => {
                let message = err.to_string();
                let retryable = message.contains("Could not find wayland compositor");
                last_error = Some(message);
                if !retryable || attempt == RETRY_DELAYS_MS.len() {
                    break;
                }
                thread::sleep(Duration::from_millis(RETRY_DELAYS_MS[attempt]));
            }
        }
    }

    Err(NativeWaylandError::Wayland(last_error.unwrap_or_else(
        || "failed to connect to Wayland compositor".to_owned(),
    )))
}

pub struct NativeWaylandHost {
    connection: Connection,
    event_queue: EventQueue<NativeWaylandState>,
    state: NativeWaylandState,
}

impl NativeWaylandHost {
    pub fn connect(options: NativeWaylandHostOptions) -> Result<Self, NativeWaylandError> {
        let connection = native_wayland_connect_to_env()?;
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
            closed: false,
            opaque_region_enabled: options.opaque_region,
            input_passthrough_enabled: options.input_passthrough,
            parent_mapping_buffer_enabled: options.attach_parent_mapping_buffer,
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

    pub fn logical_size(&self) -> Option<(u32, u32)> {
        self.state.logical_size
    }

    pub fn is_closed(&self) -> bool {
        self.state.closed
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
            buffer_size: self.state.scale.buffer_size(logical_size),
            dmabuf_main_device: self
                .state
                .dmabuf_runtime
                .latest_feedback
                .as_ref()
                .map(|feedback| feedback.main_device),
        })
    }
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
    closed: bool,
    opaque_region_enabled: bool,
    input_passthrough_enabled: bool,
    parent_mapping_buffer_enabled: bool,
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
            buffer_size: self
                .logical_size
                .map(|logical_size| self.scale.buffer_size(logical_size)),
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
        if !self.parent_mapping_buffer_enabled {
            return;
        }
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
        // drawing visible content.
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
}

struct NativeWaylandParentMappingBuffer {
    _pool: SlotPool,
    _buffer: Buffer,
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
}

impl NativeDmabufRuntimeState {
    const SAMPLE_LIMIT: usize = 8;

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
            dmabuf_buffers_pending: 0,
            dmabuf_buffers_in_flight: 0,
            dmabuf_last_frame_format: self.last_frame_format,
            dmabuf_last_frame_modifier: self.last_frame_modifier,
            dmabuf_last_attach_error: self.last_attach_error.clone(),
        }
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
        let _ = params;
        let _ = buffer;
    }

    fn failed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    ) {
        let _ = params;
    }

    fn released(&mut self, _: &Connection, _: &QueueHandle<Self>, buffer: &wl_buffer::WlBuffer) {
        let _ = buffer;
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
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.closed = true;
    }

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

    fn buffer_size(&self, logical_size: (u32, u32)) -> (u32, u32) {
        (
            native_scaled_buffer_dimension(logical_size.0, self.num, Self::DENOMINATOR),
            native_scaled_buffer_dimension(logical_size.1, self.num, Self::DENOMINATOR),
        )
    }
}

fn native_scaled_buffer_dimension(value: u32, scale_num: u32, scale_den: u32) -> u32 {
    if value == 0 || scale_num == 0 || scale_den == 0 {
        return value.max(1);
    }
    let scaled = u64::from(value)
        .saturating_mul(u64::from(scale_num))
        .div_ceil(u64::from(scale_den));
    scaled.min(u64::from(u32::MAX)).max(1) as u32
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
        assert!(!capabilities.native_video_overlay);
        assert!(capabilities.probes_linux_dmabuf_protocol);
        assert!(!capabilities.native_dmabuf_buffer_attach);
        assert!(!capabilities.consumes_render_sync);
    }
}
