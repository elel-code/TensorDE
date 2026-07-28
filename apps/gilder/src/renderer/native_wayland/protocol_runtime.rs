#[derive(Debug, Clone, Copy, Default)]
struct NativeWaylandFrameCallbackState {
    requested_count: u64,
    completed_count: u64,
    pending: bool,
    last_time_millis: Option<u32>,
    last_interval_millis: Option<u32>,
    min_interval_millis: Option<u32>,
    max_interval_millis: Option<u32>,
    interval_total_millis: u64,
    interval_count: u64,
}

impl NativeWaylandFrameCallbackState {
    fn request(&mut self) {
        self.requested_count = self.requested_count.saturating_add(1);
        self.pending = true;
    }

    fn complete(&mut self, time_millis: u32) {
        if let Some(previous_time_millis) = self.last_time_millis {
            let interval_millis = time_millis.wrapping_sub(previous_time_millis);
            self.last_interval_millis = Some(interval_millis);
            self.min_interval_millis = Some(
                self.min_interval_millis
                    .map_or(interval_millis, |current| current.min(interval_millis)),
            );
            self.max_interval_millis = Some(
                self.max_interval_millis
                    .map_or(interval_millis, |current| current.max(interval_millis)),
            );
            self.interval_total_millis = self
                .interval_total_millis
                .saturating_add(u64::from(interval_millis));
            self.interval_count = self.interval_count.saturating_add(1);
        }
        self.last_time_millis = Some(time_millis);
        self.completed_count = self.completed_count.saturating_add(1);
        self.pending = false;
    }

    fn snapshot(&self) -> NativeWaylandFrameCallbackSnapshot {
        NativeWaylandFrameCallbackSnapshot {
            requested_count: self.requested_count,
            completed_count: self.completed_count,
            pending: self.pending,
            last_time_millis: self.last_time_millis,
            last_interval_millis: self.last_interval_millis,
            min_interval_millis: self.min_interval_millis,
            max_interval_millis: self.max_interval_millis,
            avg_interval_millis: if self.interval_count == 0 {
                None
            } else {
                Some(
                    (self.interval_total_millis / self.interval_count).min(u64::from(u32::MAX))
                        as u32,
                )
            },
        }
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

    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        time: u32,
    ) {
        self.frame_callback.complete(time);
    }

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
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for NativeWaylandState {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(layer) = self.layer.as_ref() else {
            return;
        };
        let surface = layer.wl_surface();
        let surface_id = u64::from(surface.id().protocol_id());
        let surface_size = self
            .logical_size
            .map(|(width, height)| [width, height])
            .unwrap_or([0; 2]);
        for event in events.iter().filter(|event| &event.surface == surface) {
            self.event_source
                .push_pointer_event(surface_id, surface_size, event);
        }
    }
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

delegate_dispatch2!(NativeWaylandState);
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
    rounding: NativeWaylandFractionalScaleRounding,
    received: bool,
}

impl NativeScaleState {
    const DENOMINATOR: u32 = 120;

    fn new(
        fractional_manager: Option<WpFractionalScaleManagerV1>,
        fractional_scale: Option<WpFractionalScaleV1>,
        viewporter: Option<WpViewporter>,
        viewport: Option<WpViewport>,
        rounding: NativeWaylandFractionalScaleRounding,
    ) -> Self {
        Self {
            fractional_manager,
            fractional_scale,
            viewporter,
            viewport,
            num: Self::DENOMINATOR,
            rounding,
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
            native_scaled_buffer_dimension(
                logical_size.0,
                self.num,
                Self::DENOMINATOR,
                self.rounding,
            ),
            native_scaled_buffer_dimension(
                logical_size.1,
                self.num,
                Self::DENOMINATOR,
                self.rounding,
            ),
        )
    }
}

fn native_scaled_buffer_dimension(
    value: u32,
    scale_num: u32,
    scale_den: u32,
    rounding: NativeWaylandFractionalScaleRounding,
) -> u32 {
    if value == 0 || scale_num == 0 || scale_den == 0 {
        return value.max(1);
    }
    let scaled_num = u64::from(value).saturating_mul(u64::from(scale_num));
    let scale_den = u64::from(scale_den);
    let scaled = match rounding {
        NativeWaylandFractionalScaleRounding::Ceil => scaled_num.div_ceil(scale_den),
        NativeWaylandFractionalScaleRounding::Nearest => {
            scaled_num.saturating_add(scale_den / 2) / scale_den
        }
        NativeWaylandFractionalScaleRounding::Floor => scaled_num / scale_den,
    };
    scaled.min(u64::from(u32::MAX)).max(1) as u32
}

#[cfg(test)]
mod tests;
