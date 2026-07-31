//! Stable diagnostics derived from the shared Wayland runtime state.

use std::collections::BTreeSet;

use serde::Serialize;
use wayland_client_runtime::{DmabufFeedback, DmabufFormat, NativeShell, OutputInfo};

use super::NativeWaylandFractionalScaleRounding;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeWaylandSurfaceSnapshot {
    pub logical_size: Option<(u32, u32)>,
    pub buffer_size: Option<(u32, u32)>,
    pub scale_num: u32,
    pub scale_den: u32,
    pub fractional_scale_rounding: NativeWaylandFractionalScaleRounding,
    pub configured: bool,
    pub render_ready: bool,
    pub fractional_scale_expected: bool,
    pub fractional_scale_received: bool,
    pub surface_protocol_id: u32,
    pub layer: super::NativeWaylandLayer,
    pub requested_output_name: Option<String>,
    pub selected_output: Option<NativeWaylandOutputSnapshot>,
    pub known_outputs: Vec<NativeWaylandOutputSnapshot>,
    pub opaque_region_enabled: bool,
    pub input_passthrough_enabled: bool,
    pub frame_callback: NativeWaylandFrameCallbackSnapshot,
    pub dmabuf: NativeWaylandDmabufSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeWaylandFrameCallbackSnapshot {
    pub requested_count: u64,
    pub completed_count: u64,
    pub pending: bool,
    pub last_time_millis: Option<u32>,
    pub last_interval_millis: Option<u32>,
    pub min_interval_millis: Option<u32>,
    pub max_interval_millis: Option<u32>,
    pub avg_interval_millis: Option<u32>,
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

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct NativeWaylandFrameCallbackState {
    requested_count: u64,
    completed_count: u64,
    last_time_millis: Option<u32>,
    last_interval_millis: Option<u32>,
    min_interval_millis: Option<u32>,
    max_interval_millis: Option<u32>,
    interval_total_millis: u64,
    interval_count: u64,
}

impl NativeWaylandFrameCallbackState {
    pub(super) fn request(&mut self) {
        self.requested_count = self.requested_count.saturating_add(1);
    }

    pub(super) fn complete(&mut self, time_millis: u32) {
        if let Some(previous) = self.last_time_millis {
            let interval = time_millis.wrapping_sub(previous);
            self.last_interval_millis = Some(interval);
            self.min_interval_millis = Some(
                self.min_interval_millis
                    .map_or(interval, |v| v.min(interval)),
            );
            self.max_interval_millis = Some(
                self.max_interval_millis
                    .map_or(interval, |v| v.max(interval)),
            );
            self.interval_total_millis = self
                .interval_total_millis
                .saturating_add(u64::from(interval));
            self.interval_count = self.interval_count.saturating_add(1);
        }
        self.last_time_millis = Some(time_millis);
        self.completed_count = self.completed_count.saturating_add(1);
    }

    pub(super) fn snapshot(self, pending: bool) -> NativeWaylandFrameCallbackSnapshot {
        NativeWaylandFrameCallbackSnapshot {
            requested_count: self.requested_count,
            completed_count: self.completed_count,
            pending,
            last_time_millis: self.last_time_millis,
            last_interval_millis: self.last_interval_millis,
            min_interval_millis: self.min_interval_millis,
            max_interval_millis: self.max_interval_millis,
            avg_interval_millis: (self.interval_count > 0).then(|| {
                (self.interval_total_millis / self.interval_count).min(u64::from(u32::MAX)) as u32
            }),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct NativeDmabufRuntimeState {
    pub(super) default_feedback_requested: bool,
    pub(super) surface_feedback_requested: bool,
    pub(super) feedback_count: u64,
    pub(super) latest_feedback: Option<NativeWaylandDmabufFeedbackSnapshot>,
    pub(super) buffers_created: u64,
    pub(super) buffer_create_failures: u64,
    pub(super) buffers_released: u64,
}

impl NativeDmabufRuntimeState {
    const SAMPLE_LIMIT: usize = 8;

    pub(super) fn record_feedback(
        &mut self,
        source: NativeWaylandDmabufFeedbackSource,
        feedback: &DmabufFeedback,
    ) {
        self.feedback_count = self.feedback_count.saturating_add(1);
        let current_is_surface = self
            .latest_feedback
            .as_ref()
            .is_some_and(|value| value.source == NativeWaylandDmabufFeedbackSource::Surface);
        if source == NativeWaylandDmabufFeedbackSource::Surface || !current_is_surface {
            self.latest_feedback = Some(feedback_snapshot(source, feedback));
        }
    }

    pub(super) fn snapshot(&self, shell: &NativeShell) -> NativeWaylandDmabufSnapshot {
        let modifiers = shell.dmabuf_modifiers();
        NativeWaylandDmabufSnapshot {
            supports_linux_dmabuf_protocol: shell.has_linux_dmabuf(),
            linux_dmabuf_version: shell.linux_dmabuf_version(),
            linux_dmabuf_modifier_count: modifiers.len(),
            linux_dmabuf_modifier_samples: modifiers
                .iter()
                .take(Self::SAMPLE_LIMIT)
                .copied()
                .map(NativeWaylandDmabufFormatSnapshot::from)
                .collect(),
            linux_dmabuf_feedback_requested: self.default_feedback_requested
                || self.surface_feedback_requested,
            linux_dmabuf_default_feedback_requested: self.default_feedback_requested,
            linux_dmabuf_surface_feedback_requested: self.surface_feedback_requested,
            linux_dmabuf_feedback_received: self.feedback_count > 0,
            linux_dmabuf_feedback_count: self.feedback_count,
            linux_dmabuf_feedback: self.latest_feedback.clone(),
            dmabuf_buffers_created: self.buffers_created,
            dmabuf_buffer_create_failures: self.buffer_create_failures,
            dmabuf_buffers_released: self.buffers_released,
            dmabuf_frames_submitted: 0,
            dmabuf_frames_attached: 0,
            dmabuf_frame_attach_failures: 0,
            dmabuf_frame_attach_skips: 0,
            dmabuf_buffers_pending: 0,
            dmabuf_buffers_in_flight: 0,
            dmabuf_last_frame_format: None,
            dmabuf_last_frame_modifier: None,
            dmabuf_last_attach_error: None,
        }
    }
}

impl From<DmabufFormat> for NativeWaylandDmabufFormatSnapshot {
    fn from(format: DmabufFormat) -> Self {
        Self {
            format: format.format(),
            modifier: format.modifier(),
        }
    }
}

fn feedback_snapshot(
    source: NativeWaylandDmabufFeedbackSource,
    feedback: &DmabufFeedback,
) -> NativeWaylandDmabufFeedbackSnapshot {
    let format_fourccs = feedback
        .formats()
        .iter()
        .map(|format| format.format())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    NativeWaylandDmabufFeedbackSnapshot {
        source,
        main_device: feedback.main_device(),
        format_count: feedback.formats().len(),
        format_fourcc_count: format_fourccs.len(),
        format_fourccs,
        format_table: feedback.formats().iter().copied().map(Into::into).collect(),
        format_samples: feedback
            .formats()
            .iter()
            .take(NativeDmabufRuntimeState::SAMPLE_LIMIT)
            .copied()
            .map(Into::into)
            .collect(),
        tranche_count: feedback.tranches().len(),
        tranche_samples: feedback
            .tranches()
            .iter()
            .take(NativeDmabufRuntimeState::SAMPLE_LIMIT)
            .map(|tranche| NativeWaylandDmabufTrancheSnapshot {
                device: tranche.device,
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

pub(super) fn output_snapshot(info: &OutputInfo) -> NativeWaylandOutputSnapshot {
    let logical_size = info
        .logical_size
        .map(|size| (size.width as i32, size.height as i32));
    NativeWaylandOutputSnapshot {
        id: info.id.get(),
        name: info.name.clone(),
        description: info.description.clone(),
        make: info.make.clone(),
        model: info.model.clone(),
        logical_position: info
            .logical_position
            .map(|position| (position.x, position.y)),
        logical_size,
        scale_factor: info.scale_factor,
        current_mode: logical_size.map(|(width, height)| NativeWaylandOutputModeSnapshot {
            width,
            height,
            refresh_millihertz: info.refresh_mhz.unwrap_or_default(),
        }),
    }
}

pub(super) fn output_labels(outputs: &[NativeWaylandOutputSnapshot]) -> String {
    if outputs.is_empty() {
        return "none".to_owned();
    }
    outputs
        .iter()
        .map(|output| {
            let name = output.name.as_deref().unwrap_or("<unnamed>");
            match output
                .description
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                Some(description) => format!("{name}#{} ({description})", output.id),
                None => format!("{name}#{}", output.id),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn scaled_buffer_size(
    logical_size: (u32, u32),
    scale_factor: f64,
    rounding: NativeWaylandFractionalScaleRounding,
) -> (u32, u32) {
    let scale_num = (scale_factor.max(0.01) * 120.0).round().max(1.0) as u32;
    (
        native_scaled_buffer_dimension(logical_size.0, scale_num, 120, rounding),
        native_scaled_buffer_dimension(logical_size.1, scale_num, 120, rounding),
    )
}

pub(super) fn native_scaled_buffer_dimension(
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
