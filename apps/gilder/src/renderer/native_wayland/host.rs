//! Single-runtime Gilder host over `wayland-client-runtime::NativeShell`.

use std::{thread, time::Duration};

use wayland_client_runtime::{
    LayerAnchor, LayerKeyboardInteractivity, LayerMargins, LayerSurfaceState, LogicalSize,
    NativeShell, NativeShellEvent, NativeSurfaceId, OutputInfo, SurfaceRegion,
};

#[cfg(feature = "native-vulkan-renderer")]
use crate::engine::scene::SceneEventQueue;

#[cfg(feature = "native-vulkan-renderer")]
use super::event_source::NativeWaylandEventSource;
use super::snapshot::{
    NativeDmabufRuntimeState, NativeWaylandDmabufFeedbackSource, NativeWaylandFrameCallbackState,
    output_labels, output_snapshot, scaled_buffer_size,
};
use super::{
    NativeWaylandError, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
    NativeWaylandSurfaceSnapshot,
};

pub struct NativeWaylandHost {
    shell: NativeShell,
    surface: NativeSurfaceId,
    surface_protocol_id: u32,
    options: NativeWaylandHostOptions,
    selected_output_id: Option<u32>,
    configure_readiness: NativeWaylandConfigureReadiness,
    closed: bool,
    frame_callback: NativeWaylandFrameCallbackState,
    dmabuf: NativeDmabufRuntimeState,
    #[cfg(feature = "native-vulkan-renderer")]
    event_source: NativeWaylandEventSource,
    events: Vec<NativeShellEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeWaylandConfigureReadiness {
    fractional_scale_expected: bool,
    fractional_scale_received: bool,
}

impl NativeWaylandConfigureReadiness {
    pub(super) fn new(fractional_scale_expected: bool) -> Self {
        Self {
            fractional_scale_expected,
            fractional_scale_received: false,
        }
    }

    pub(super) fn record_preferred_scale(&mut self) {
        self.fractional_scale_received = true;
    }

    pub(super) fn is_ready(self, logical_configured: bool) -> bool {
        logical_configured && (!self.fractional_scale_expected || self.fractional_scale_received)
    }

    fn scale_is_ready(self) -> bool {
        !self.fractional_scale_expected || self.fractional_scale_received
    }
}

impl NativeWaylandHost {
    pub fn connect(options: NativeWaylandHostOptions) -> Result<Self, NativeWaylandError> {
        let mut shell = connect_to_env()?;
        shell.roundtrip()?;

        let selected_output_id = select_output(&shell.outputs(), options.output_name.as_deref())?;
        let layer_state = LayerSurfaceState {
            size: LogicalSize::new(0, 0),
            anchor: LayerAnchor::TOP | LayerAnchor::BOTTOM | LayerAnchor::LEFT | LayerAnchor::RIGHT,
            exclusive_zone: -1,
            exclusive_edge: None,
            margins: LayerMargins::default(),
            keyboard_interactivity: LayerKeyboardInteractivity::None,
            layer: options.layer.into(),
        };
        let surface = shell.create_layer_surface_gpu(
            options.namespace.clone(),
            selected_output_id,
            layer_state,
        )?;
        let surface_protocol_id = shell.surface_handle(surface)?.protocol_id();
        let configure_readiness =
            NativeWaylandConfigureReadiness::new(shell.has_fractional_scale());

        let mut dmabuf = NativeDmabufRuntimeState::default();
        if shell
            .linux_dmabuf_version()
            .is_some_and(|version| version >= 4)
        {
            shell.request_dmabuf_default_feedback()?;
            dmabuf.default_feedback_requested = true;
            shell.request_dmabuf_surface_feedback(surface)?;
            dmabuf.surface_feedback_requested = true;
        }

        let mut host = Self {
            shell,
            surface,
            surface_protocol_id,
            options,
            selected_output_id,
            configure_readiness,
            closed: false,
            frame_callback: NativeWaylandFrameCallbackState::default(),
            dmabuf,
            #[cfg(feature = "native-vulkan-renderer")]
            event_source: NativeWaylandEventSource::default(),
            events: Vec::with_capacity(32),
        };
        host.roundtrip()?;
        Ok(host)
    }

    pub fn dispatch_pending(&mut self) -> Result<(), NativeWaylandError> {
        self.shell.dispatch_pending()?;
        self.process_events()
    }

    pub fn pump_events(&mut self) -> Result<(), NativeWaylandError> {
        self.shell.try_read_and_dispatch()?;
        self.process_events()
    }

    #[cfg(feature = "native-vulkan-renderer")]
    pub(crate) fn publish_scene_events(&mut self, queue: &mut SceneEventQueue) {
        self.event_source.publish_to(queue);
    }

    #[cfg(feature = "native-vulkan-renderer")]
    pub(crate) fn discard_scene_events(&mut self) {
        self.event_source.discard_pending();
    }

    pub fn request_frame_callback(&mut self) -> Result<(), NativeWaylandError> {
        if !self.shell.is_frame_pending(self.surface) {
            self.frame_callback.request();
        }
        self.shell.request_frame(self.surface)?;
        self.shell.flush()?;
        Ok(())
    }

    pub fn roundtrip(&mut self) -> Result<(), NativeWaylandError> {
        self.shell.roundtrip()?;
        self.process_events()
    }

    pub fn wait_until_configured(&mut self, rounds: usize) -> Result<(), NativeWaylandError> {
        if self
            .configure_readiness
            .is_ready(self.logical_size().is_some())
        {
            return Ok(());
        }
        for _ in 0..rounds {
            self.roundtrip()?;
            if self
                .configure_readiness
                .is_ready(self.logical_size().is_some())
            {
                return Ok(());
            }
        }
        let missing = if self.logical_size().is_none() {
            "layer-surface configure"
        } else {
            "wp_fractional_scale_v1.preferred_scale"
        };
        Err(NativeWaylandError::Timeout(format!(
            "native Wayland surface did not receive {missing} after {rounds} roundtrips"
        )))
    }

    pub fn snapshot(&self) -> NativeWaylandSurfaceSnapshot {
        let logical_size = self.logical_size();
        let scale_factor = self.shell.scale_factor(self.surface).unwrap_or(1.0);
        let known_outputs = self
            .shell
            .outputs()
            .iter()
            .map(output_snapshot)
            .collect::<Vec<_>>();
        let selected_output = self
            .selected_output_id
            .and_then(|id| known_outputs.iter().find(|output| output.id == id).cloned());
        NativeWaylandSurfaceSnapshot {
            logical_size,
            buffer_size: logical_size
                .filter(|_| self.configure_readiness.scale_is_ready())
                .map(|size| {
                    scaled_buffer_size(size, scale_factor, self.options.fractional_scale_rounding)
                }),
            scale_num: (scale_factor * 120.0).round().max(1.0) as u32,
            scale_den: 120,
            fractional_scale_rounding: self.options.fractional_scale_rounding,
            configured: logical_size.is_some(),
            render_ready: self.configure_readiness.is_ready(logical_size.is_some()),
            fractional_scale_expected: self.configure_readiness.fractional_scale_expected,
            fractional_scale_received: self.configure_readiness.fractional_scale_received,
            surface_protocol_id: self.surface_protocol_id,
            layer: self.options.layer,
            requested_output_name: self.options.output_name.clone(),
            selected_output,
            known_outputs,
            opaque_region_enabled: self.options.opaque_region,
            input_passthrough_enabled: self.options.input_passthrough,
            frame_callback: self
                .frame_callback
                .snapshot(self.shell.is_frame_pending(self.surface)),
            dmabuf: self.dmabuf.snapshot(&self.shell),
        }
    }

    pub fn logical_size(&self) -> Option<(u32, u32)> {
        self.shell
            .logical_size(self.surface)
            .map(|size| (size.width, size.height))
            .filter(|(width, height)| *width > 0 && *height > 0)
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn surface_handles(&self) -> Result<NativeWaylandSurfaceHandles, NativeWaylandError> {
        let logical_size = self
            .logical_size()
            .ok_or(NativeWaylandError::MissingRawHandle(
                "configured surface size",
            ))?;
        if !self.configure_readiness.scale_is_ready() {
            return Err(NativeWaylandError::MissingPreferredFractionalScale);
        }
        let scale_factor = self.shell.scale_factor(self.surface).unwrap_or(1.0);
        let handle = self.shell.surface_handle(self.surface)?;
        Ok(NativeWaylandSurfaceHandles {
            display: handle.display_ptr(),
            surface: handle.surface_ptr(),
            logical_size,
            buffer_size: scaled_buffer_size(
                logical_size,
                scale_factor,
                self.options.fractional_scale_rounding,
            ),
            dmabuf_main_device: self
                .dmabuf
                .latest_feedback
                .as_ref()
                .map(|feedback| feedback.main_device),
        })
    }

    fn process_events(&mut self) -> Result<(), NativeWaylandError> {
        self.shell.drain_events_into(&mut self.events);
        let mut apply_surface_policy = false;
        let mut events = std::mem::take(&mut self.events);
        for event in events.drain(..) {
            match &event {
                NativeShellEvent::LayerConfigure { surface, .. } if *surface == self.surface => {
                    apply_surface_policy = true;
                }
                NativeShellEvent::LayerClosed { surface } if *surface == self.surface => {
                    self.closed = true;
                }
                NativeShellEvent::ScaleFactorChanged { surface, .. }
                    if *surface == self.surface =>
                {
                    self.configure_readiness.record_preferred_scale();
                }
                NativeShellEvent::Frame { surface, time } if *surface == self.surface => {
                    self.frame_callback.complete(*time);
                }
                NativeShellEvent::DmabufFeedback { surface, feedback }
                    if surface.is_none() || *surface == Some(self.surface) =>
                {
                    let source = if surface.is_some() {
                        NativeWaylandDmabufFeedbackSource::Surface
                    } else {
                        NativeWaylandDmabufFeedbackSource::Default
                    };
                    self.dmabuf.record_feedback(source, feedback);
                }
                NativeShellEvent::DmabufBufferCreated { .. } => {
                    self.dmabuf.buffers_created = self.dmabuf.buffers_created.saturating_add(1);
                }
                NativeShellEvent::DmabufBufferFailed => {
                    self.dmabuf.buffer_create_failures =
                        self.dmabuf.buffer_create_failures.saturating_add(1);
                }
                NativeShellEvent::DmabufBufferReleased { .. } => {
                    self.dmabuf.buffers_released = self.dmabuf.buffers_released.saturating_add(1);
                }
                _ => {}
            }
            #[cfg(feature = "native-vulkan-renderer")]
            if let Some(size) = self.logical_size() {
                self.event_source.push_native_event(
                    self.surface,
                    self.surface_protocol_id,
                    size,
                    &event,
                );
            }
        }
        self.events = events;
        if apply_surface_policy {
            self.apply_surface_policy()?;
        }
        Ok(())
    }

    fn apply_surface_policy(&mut self) -> Result<(), NativeWaylandError> {
        let Some((width, height)) = self.logical_size() else {
            return Ok(());
        };
        let opaque = if self.options.opaque_region {
            SurfaceRegion::full(width, height)
        } else {
            SurfaceRegion::Default
        };
        let input = if self.options.input_passthrough {
            SurfaceRegion::Empty
        } else {
            SurfaceRegion::Default
        };
        self.shell.set_opaque_region(self.surface, opaque)?;
        self.shell.set_input_region(self.surface, input)?;
        self.shell.commit_surface(self.surface)?;
        Ok(())
    }
}

fn connect_to_env() -> Result<NativeShell, NativeWaylandError> {
    const RETRY_DELAYS_MS: [u64; 3] = [5, 20, 50];
    let mut last_error = None;
    for retry_delay_ms in RETRY_DELAYS_MS.into_iter().map(Some).chain([None]) {
        match NativeShell::connect_to_env() {
            Ok(shell) => return Ok(shell),
            Err(error) => {
                let message = error.to_string();
                let retryable = message.contains("Could not find wayland compositor")
                    || message.contains("No such file or directory");
                last_error = Some(message);
                let Some(retry_delay_ms) = retry_delay_ms.filter(|_| retryable) else {
                    break;
                };
                thread::sleep(Duration::from_millis(retry_delay_ms));
            }
        }
    }
    Err(NativeWaylandError::Wayland(last_error.unwrap_or_else(
        || "failed to connect to Wayland compositor".to_owned(),
    )))
}

fn select_output(
    outputs: &[OutputInfo],
    requested_name: Option<&str>,
) -> Result<Option<u32>, NativeWaylandError> {
    let Some(requested_name) = requested_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return Ok(None);
    };
    if let Some(output) = outputs
        .iter()
        .find(|output| output_matches(output, requested_name, true))
        .or_else(|| {
            outputs
                .iter()
                .find(|output| output_matches(output, requested_name, false))
        })
    {
        return Ok(Some(output.id.get()));
    }
    let snapshots = outputs.iter().map(output_snapshot).collect::<Vec<_>>();
    Err(NativeWaylandError::Wayland(format!(
        "native Wayland output {requested_name:?} was not found; known outputs: {}",
        output_labels(&snapshots)
    )))
}

fn output_matches(output: &OutputInfo, requested_name: &str, exact: bool) -> bool {
    let combined_make_model = format!("{} {}", output.make, output.model);
    let id = output.id.get().to_string();
    [
        output.name.as_deref(),
        output.description.as_deref(),
        Some(output.make.as_str()),
        Some(output.model.as_str()),
        Some(combined_make_model.as_str()),
        Some(id.as_str()),
    ]
    .into_iter()
    .flatten()
    .any(|candidate| {
        if exact {
            candidate == requested_name
        } else {
            candidate.eq_ignore_ascii_case(requested_name)
        }
    })
}
