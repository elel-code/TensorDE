use std::path::PathBuf;
use std::time::{Duration, Instant};

use gst::prelude::*;
use gstreamer as gst;
use serde::Serialize;

use super::NativeVulkanError;

const NATIVE_VULKAN_AUDIO_POSITION_EARLY_TOLERANCE_NS: u64 = 250_000_000;
const NATIVE_VULKAN_AUDIO_POSITION_LATE_TOLERANCE_NS: u64 = 500_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanAudioClockProbeOptions {
    pub source: PathBuf,
    pub duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAudioClockProbeSnapshot {
    pub source: PathBuf,
    pub probe_duration_ms: u64,
    pub runtime_elapsed_ms: u64,
    pub pipeline: &'static str,
    pub audio_stream_codec: Option<String>,
    pub audio_raw_caps: Option<String>,
    pub audio_sample_format: Option<String>,
    pub audio_sample_rate: Option<u32>,
    pub audio_channels: Option<u32>,
    pub gst_state: Option<String>,
    pub gst_position_ns: Option<u64>,
    pub gst_duration_ns: Option<u64>,
    pub gst_new_clock_count: u32,
    pub gst_clock_names: Vec<String>,
    pub gst_clock_lost_count: u32,
    pub gst_stream_start_count: u32,
    pub gst_async_done_count: u32,
    pub gst_duration_changed_count: u32,
    pub gst_latency_count: u32,
    pub gst_state_playing_count: u32,
    pub gst_eos_count: u32,
    pub audio_buffer_count: u32,
    pub audio_first_pts_ns: Option<u64>,
    pub audio_last_pts_ns: Option<u64>,
    pub audio_pts_delta_min_ns: Option<u64>,
    pub audio_pts_delta_max_ns: Option<u64>,
    pub audio_duration_min_ns: Option<u64>,
    pub audio_duration_max_ns: Option<u64>,
    pub audio_decoders: Vec<String>,
    pub video_decoders: Vec<String>,
    pub reached_clocked_playback: bool,
    pub no_video_decoder_instantiated: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAudioClockRuntimeSnapshot {
    pub source: PathBuf,
    pub runtime_elapsed_ms: u64,
    pub pipeline: &'static str,
    pub audio_raw_caps: Option<String>,
    pub audio_sample_format: Option<String>,
    pub audio_sample_rate: Option<u32>,
    pub audio_channels: Option<u32>,
    pub gst_state: Option<String>,
    pub gst_position_ns: Option<u64>,
    pub gst_duration_ns: Option<u64>,
    pub gst_new_clock_count: u32,
    pub gst_clock_names: Vec<String>,
    pub gst_clock_lost_count: u32,
    pub gst_stream_start_count: u32,
    pub gst_async_done_count: u32,
    pub gst_duration_changed_count: u32,
    pub gst_latency_count: u32,
    pub gst_state_playing_count: u32,
    pub gst_eos_count: u32,
    pub audio_buffer_count: u32,
    pub audio_first_pts_ns: Option<u64>,
    pub audio_last_pts_ns: Option<u64>,
    pub audio_pts_delta_min_ns: Option<u64>,
    pub audio_pts_delta_max_ns: Option<u64>,
    pub audio_duration_min_ns: Option<u64>,
    pub audio_duration_max_ns: Option<u64>,
    pub audio_decoders: Vec<String>,
    pub video_decoders: Vec<String>,
    pub reached_clocked_playback: bool,
    pub no_video_decoder_instantiated: bool,
    pub audio_loop_seek_count: u32,
    pub audio_loop_seek_error_count: u32,
    pub audio_loop_restart_count: u32,
    pub audio_last_loop_seek_position_ms: Option<u64>,
    pub audio_playback_started: bool,
    pub audio_clock_serial: u32,
    pub audio_initial_position_ms: Option<u64>,
    pub audio_segment_start_position_ns: Option<u64>,
    pub audio_segment_elapsed_ns: Option<u64>,
    pub audio_position_stale_count: u32,
    pub audio_sample_stale_count: u32,
    pub audio_master_clock_estimate_ns: Option<u64>,
    pub sampled_video_frame_count: u32,
    pub audio_position_query_count: u32,
    pub audio_position_query_hit_count: u32,
    pub first_video_pts_ns: Option<u64>,
    pub first_audio_position_ns: Option<u64>,
    pub first_video_clock_ns: Option<u64>,
    pub latest_video_pts_ns: Option<u64>,
    pub latest_video_clock_ns: Option<u64>,
    pub latest_audio_position_ns: Option<u64>,
    pub audio_video_zero_based_drift_latest_ns: Option<i64>,
    pub audio_video_zero_based_drift_min_ns: Option<i64>,
    pub audio_video_zero_based_drift_max_ns: Option<i64>,
    pub audio_video_zero_based_drift_abs_max_ns: Option<u64>,
    pub audio_video_clock_drift_latest_ns: Option<i64>,
    pub audio_video_clock_drift_min_ns: Option<i64>,
    pub audio_video_clock_drift_max_ns: Option<i64>,
    pub audio_video_clock_drift_abs_max_ns: Option<u64>,
    pub audio_video_master_clock_drift_latest_ns: Option<i64>,
    pub audio_video_master_clock_drift_min_ns: Option<i64>,
    pub audio_video_master_clock_drift_max_ns: Option<i64>,
    pub audio_video_master_clock_drift_abs_max_ns: Option<u64>,
    pub audio_video_raw_drift_latest_ns: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Default)]
struct NativeVulkanAudioClockProbeStats {
    audio_raw_caps: Option<String>,
    audio_sample_format: Option<String>,
    audio_sample_rate: Option<u32>,
    audio_channels: Option<u32>,
    new_clock_count: u32,
    clock_names: Vec<String>,
    clock_lost_count: u32,
    stream_start_count: u32,
    async_done_count: u32,
    duration_changed_count: u32,
    latency_count: u32,
    state_playing_count: u32,
    eos_count: u32,
    audio_buffer_count: u32,
    first_pts_ns: Option<u64>,
    last_pts_ns: Option<u64>,
    previous_pts_ns: Option<u64>,
    pts_delta_min_ns: Option<u64>,
    pts_delta_max_ns: Option<u64>,
    duration_min_ns: Option<u64>,
    duration_max_ns: Option<u64>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct NativeVulkanAudioClockSegment {
    serial: u32,
    start_position_ns: u64,
    started_at: Instant,
}

pub(super) struct NativeVulkanAudioClockRuntimeProbe {
    source: PathBuf,
    pipeline: gst::Pipeline,
    sink: gst::Element,
    bus: gst::Bus,
    started_at: Instant,
    stats: NativeVulkanAudioClockProbeStats,
    playback_started: bool,
    audio_clock_serial: u32,
    audio_initial_position_ms: Option<u64>,
    audio_segment: Option<NativeVulkanAudioClockSegment>,
    audio_position_stale_count: u32,
    audio_sample_stale_count: u32,
    audio_master_clock_position_ns: Option<u64>,
    audio_master_clock_updated_at: Option<Instant>,
    sampled_video_frame_count: u32,
    audio_position_query_count: u32,
    audio_position_query_hit_count: u32,
    audio_loop_seek_count: u32,
    audio_loop_seek_error_count: u32,
    audio_loop_restart_count: u32,
    audio_last_loop_seek_position_ms: Option<u64>,
    first_video_pts_ns: Option<u64>,
    first_audio_position_ns: Option<u64>,
    first_video_clock_ns: Option<u64>,
    latest_video_pts_ns: Option<u64>,
    latest_video_clock_ns: Option<u64>,
    latest_audio_position_ns: Option<u64>,
    zero_based_drift_latest_ns: Option<i64>,
    zero_based_drift_min_ns: Option<i64>,
    zero_based_drift_max_ns: Option<i64>,
    zero_based_drift_abs_max_ns: Option<u64>,
    clock_drift_latest_ns: Option<i64>,
    clock_drift_min_ns: Option<i64>,
    clock_drift_max_ns: Option<i64>,
    clock_drift_abs_max_ns: Option<u64>,
    master_clock_drift_latest_ns: Option<i64>,
    master_clock_drift_min_ns: Option<i64>,
    master_clock_drift_max_ns: Option<i64>,
    master_clock_drift_abs_max_ns: Option<u64>,
    raw_drift_latest_ns: Option<i64>,
}

impl NativeVulkanAudioClockProbeStats {
    fn record_caps(&mut self, caps: &gst::CapsRef) {
        self.audio_raw_caps = Some(caps.to_string());
        let Some(structure) = caps.structure(0) else {
            return;
        };
        self.audio_sample_format = structure.get::<String>("format").ok();
        self.audio_sample_rate = structure
            .get::<i32>("rate")
            .ok()
            .and_then(|value| u32::try_from(value).ok());
        self.audio_channels = structure
            .get::<i32>("channels")
            .ok()
            .and_then(|value| u32::try_from(value).ok());
    }

    fn record_sample(&mut self, sample: &gst::Sample) {
        self.audio_buffer_count = self.audio_buffer_count.saturating_add(1);
        if let Some(caps) = sample.caps() {
            self.record_caps(caps);
        }
        let Some(buffer) = sample.buffer() else {
            return;
        };
        let pts_ns = native_vulkan_audio_clock_time_ns(buffer.pts());
        if self.first_pts_ns.is_none() {
            self.first_pts_ns = pts_ns;
        }
        if let Some(pts_ns) = pts_ns {
            if let Some(previous) = self.previous_pts_ns
                && let Some(delta) = pts_ns.checked_sub(previous)
            {
                native_vulkan_audio_update_min(&mut self.pts_delta_min_ns, delta);
                native_vulkan_audio_update_max(&mut self.pts_delta_max_ns, delta);
            }
            self.previous_pts_ns = Some(pts_ns);
            self.last_pts_ns = Some(pts_ns);
        }
        if let Some(duration_ns) = native_vulkan_audio_clock_time_ns(buffer.duration()) {
            native_vulkan_audio_update_min(&mut self.duration_min_ns, duration_ns);
            native_vulkan_audio_update_max(&mut self.duration_max_ns, duration_ns);
        }
    }
}

impl NativeVulkanAudioClockRuntimeProbe {
    pub(super) fn start(source: &PathBuf) -> Result<Self, NativeVulkanError> {
        if !source.is_file() {
            return Err(NativeVulkanError::Video(format!(
                "audio clock runtime source does not exist: {}",
                source.display()
            )));
        }
        gst::init().map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        let pipeline = native_vulkan_audio_clock_aac_pipeline(source)?;
        let sink = pipeline
            .by_name("gilder-native-vulkan-audio-appsink")
            .ok_or_else(|| NativeVulkanError::Video("audio appsink not found".to_owned()))?;
        let bus = pipeline.bus().ok_or_else(|| {
            NativeVulkanError::Video("audio runtime pipeline has no bus".to_owned())
        })?;
        Ok(Self {
            source: source.clone(),
            pipeline,
            sink,
            bus,
            started_at: Instant::now(),
            stats: NativeVulkanAudioClockProbeStats::default(),
            playback_started: false,
            audio_clock_serial: 0,
            audio_initial_position_ms: None,
            audio_segment: None,
            audio_position_stale_count: 0,
            audio_sample_stale_count: 0,
            audio_master_clock_position_ns: None,
            audio_master_clock_updated_at: None,
            sampled_video_frame_count: 0,
            audio_position_query_count: 0,
            audio_position_query_hit_count: 0,
            audio_loop_seek_count: 0,
            audio_loop_seek_error_count: 0,
            audio_loop_restart_count: 0,
            audio_last_loop_seek_position_ms: None,
            first_video_pts_ns: None,
            first_audio_position_ns: None,
            first_video_clock_ns: None,
            latest_video_pts_ns: None,
            latest_video_clock_ns: None,
            latest_audio_position_ns: None,
            zero_based_drift_latest_ns: None,
            zero_based_drift_min_ns: None,
            zero_based_drift_max_ns: None,
            zero_based_drift_abs_max_ns: None,
            clock_drift_latest_ns: None,
            clock_drift_min_ns: None,
            clock_drift_max_ns: None,
            clock_drift_abs_max_ns: None,
            master_clock_drift_latest_ns: None,
            master_clock_drift_min_ns: None,
            master_clock_drift_max_ns: None,
            master_clock_drift_abs_max_ns: None,
            raw_drift_latest_ns: None,
        })
    }

    pub(super) fn poll(&mut self) -> Result<(), NativeVulkanError> {
        self.poll_bus()?;
        self.drain_audio_samples();
        Ok(())
    }

    fn poll_bus(&mut self) -> Result<(), NativeVulkanError> {
        while let Some(message) = self.bus.pop() {
            native_vulkan_audio_clock_record_message(&self.pipeline, &mut self.stats, &message)?;
        }
        Ok(())
    }

    fn drain_audio_samples(&mut self) {
        while let Some(sample) = self
            .sink
            .emit_by_name::<Option<gst::Sample>>("try-pull-sample", &[&0u64])
        {
            if self.sample_belongs_to_current_segment(&sample) {
                self.stats.record_sample(&sample);
            } else {
                self.audio_sample_stale_count = self.audio_sample_stale_count.saturating_add(1);
            }
        }
    }

    fn ensure_clocked_playback(&mut self, position_ms: u64) -> Result<(), NativeVulkanError> {
        if self.playback_started {
            return Ok(());
        }
        self.audio_initial_position_ms = Some(position_ms);
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        self.playback_started = true;
        let _ = self.pipeline.state(gst::ClockTime::from_mseconds(500));
        self.seek_clocked_playback_to(position_ms)?;
        self.poll_bus()?;
        self.begin_segment(position_ms);
        self.poll()
    }

    fn restart_clocked_playback_at(&mut self, position_ms: u64) -> Result<(), NativeVulkanError> {
        let _ = self.pipeline.set_state(gst::State::Null);
        let pipeline = native_vulkan_audio_clock_aac_pipeline(&self.source)?;
        let sink = pipeline
            .by_name("gilder-native-vulkan-audio-appsink")
            .ok_or_else(|| NativeVulkanError::Video("audio appsink not found".to_owned()))?;
        let bus = pipeline.bus().ok_or_else(|| {
            NativeVulkanError::Video("audio runtime pipeline has no bus".to_owned())
        })?;
        self.pipeline = pipeline;
        self.sink = sink;
        self.bus = bus;
        self.pipeline
            .set_state(gst::State::Playing)
            .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        self.playback_started = true;
        let _ = self.pipeline.state(gst::ClockTime::from_mseconds(500));
        self.seek_clocked_playback_to(position_ms)?;
        self.poll_bus()
    }

    fn seek_clocked_playback_to(&self, position_ms: u64) -> Result<(), NativeVulkanError> {
        if position_ms == 0 {
            return Ok(());
        }
        self.pipeline
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::ClockTime::from_mseconds(position_ms),
            )
            .map_err(|err| {
                NativeVulkanError::Video(format!(
                    "seek audio clock runtime to {position_ms}ms: {err}"
                ))
            })?;
        let _ = self.pipeline.state(gst::ClockTime::from_mseconds(500));
        Ok(())
    }

    fn begin_segment(&mut self, position_ms: u64) {
        self.audio_clock_serial = self.audio_clock_serial.saturating_add(1);
        self.audio_segment = Some(NativeVulkanAudioClockSegment {
            serial: self.audio_clock_serial,
            start_position_ns: position_ms.saturating_mul(1_000_000),
            started_at: Instant::now(),
        });
        self.reset_segment_measurements();
    }

    fn reset_segment_measurements(&mut self) {
        self.first_audio_position_ns = None;
        self.first_video_pts_ns = None;
        self.first_video_clock_ns = None;
        self.latest_video_pts_ns = None;
        self.latest_video_clock_ns = None;
        self.latest_audio_position_ns = None;
        self.audio_master_clock_position_ns = None;
        self.audio_master_clock_updated_at = None;
        self.zero_based_drift_latest_ns = None;
        self.zero_based_drift_min_ns = None;
        self.zero_based_drift_max_ns = None;
        self.zero_based_drift_abs_max_ns = None;
        self.clock_drift_latest_ns = None;
        self.clock_drift_min_ns = None;
        self.clock_drift_max_ns = None;
        self.clock_drift_abs_max_ns = None;
        self.master_clock_drift_latest_ns = None;
        self.master_clock_drift_min_ns = None;
        self.master_clock_drift_max_ns = None;
        self.master_clock_drift_abs_max_ns = None;
        self.raw_drift_latest_ns = None;
    }

    fn sample_belongs_to_current_segment(&self, sample: &gst::Sample) -> bool {
        let Some(buffer) = sample.buffer() else {
            return true;
        };
        let Some(position_ns) = native_vulkan_audio_clock_time_ns(buffer.pts()) else {
            return true;
        };
        self.audio_position_belongs_to_current_segment(position_ns)
    }

    fn audio_position_belongs_to_current_segment(&self, position_ns: u64) -> bool {
        match self.audio_segment {
            Some(segment) => native_vulkan_audio_position_belongs_to_segment(
                position_ns,
                segment.start_position_ns,
                native_vulkan_audio_duration_ns(segment.started_at.elapsed()),
            ),
            None => true,
        }
    }

    fn audio_segment_elapsed_ns(&self) -> Option<u64> {
        self.audio_segment
            .map(|segment| native_vulkan_audio_duration_ns(segment.started_at.elapsed()))
    }

    pub(super) fn audio_master_clock_estimate_ns(&self) -> Option<u64> {
        let position_ns = self.audio_master_clock_position_ns?;
        let updated_at = self.audio_master_clock_updated_at?;
        Some(position_ns.saturating_add(native_vulkan_audio_duration_ns(updated_at.elapsed())))
    }

    fn record_master_clock_drift(&mut self, drift_ns: i64) {
        self.master_clock_drift_latest_ns = Some(drift_ns);
        native_vulkan_audio_update_min_i64(&mut self.master_clock_drift_min_ns, drift_ns);
        native_vulkan_audio_update_max_i64(&mut self.master_clock_drift_max_ns, drift_ns);
        native_vulkan_audio_update_max(
            &mut self.master_clock_drift_abs_max_ns,
            drift_ns.unsigned_abs(),
        );
    }

    pub(super) fn sample_video_pts_ms(
        &mut self,
        video_pts_ms: Option<u64>,
        video_clock_ns: Option<u64>,
    ) -> Result<(), NativeVulkanError> {
        let start_position_ms = video_pts_ms
            .or_else(|| video_clock_ns.map(|value| value / 1_000_000))
            .unwrap_or(0);
        self.ensure_clocked_playback(start_position_ms)?;
        self.poll()?;
        self.sampled_video_frame_count = self.sampled_video_frame_count.saturating_add(1);
        self.audio_position_query_count = self.audio_position_query_count.saturating_add(1);
        let audio_position_ns = self
            .pipeline
            .query_position::<gst::ClockTime>()
            .map(|value| value.nseconds());
        let Some(audio_position_ns) = audio_position_ns else {
            return Ok(());
        };
        if !self.audio_position_belongs_to_current_segment(audio_position_ns) {
            self.audio_position_stale_count = self.audio_position_stale_count.saturating_add(1);
            return Ok(());
        }
        self.audio_position_query_hit_count = self.audio_position_query_hit_count.saturating_add(1);
        self.latest_audio_position_ns = Some(audio_position_ns);
        self.first_audio_position_ns
            .get_or_insert(audio_position_ns);
        let audio_master_updated_at = Instant::now();
        self.audio_master_clock_position_ns = Some(audio_position_ns);
        self.audio_master_clock_updated_at = Some(audio_master_updated_at);
        let audio_master_estimate_ns = self.audio_master_clock_estimate_ns();
        if let Some(video_clock_ns) = video_clock_ns {
            self.latest_video_clock_ns = Some(video_clock_ns);
            self.first_video_clock_ns.get_or_insert(video_clock_ns);
            let video_elapsed_ns =
                video_clock_ns.saturating_sub(self.first_video_clock_ns.unwrap_or(video_clock_ns));
            let audio_elapsed_ns = audio_position_ns
                .saturating_sub(self.first_audio_position_ns.unwrap_or(audio_position_ns));
            if let Some(drift_ns) = native_vulkan_audio_i128_to_i64(
                i128::from(audio_elapsed_ns) - i128::from(video_elapsed_ns),
            ) {
                self.clock_drift_latest_ns = Some(drift_ns);
                native_vulkan_audio_update_min_i64(&mut self.clock_drift_min_ns, drift_ns);
                native_vulkan_audio_update_max_i64(&mut self.clock_drift_max_ns, drift_ns);
                native_vulkan_audio_update_max(
                    &mut self.clock_drift_abs_max_ns,
                    drift_ns.unsigned_abs(),
                );
            }
            if let Some(audio_master_estimate_ns) = audio_master_estimate_ns {
                let audio_master_elapsed_ns = audio_master_estimate_ns
                    .saturating_sub(self.first_audio_position_ns.unwrap_or(audio_position_ns));
                if let Some(drift_ns) = native_vulkan_audio_i128_to_i64(
                    i128::from(audio_master_elapsed_ns) - i128::from(video_elapsed_ns),
                ) {
                    self.record_master_clock_drift(drift_ns);
                }
            }
        }
        let Some(video_pts_ms) = video_pts_ms else {
            return Ok(());
        };
        let video_pts_ns = video_pts_ms.saturating_mul(1_000_000);
        self.latest_video_pts_ns = Some(video_pts_ns);
        self.first_video_pts_ns.get_or_insert(video_pts_ns);

        self.raw_drift_latest_ns = native_vulkan_audio_i128_to_i64(
            i128::from(audio_position_ns) - i128::from(video_pts_ns),
        );
        let video_elapsed_ns =
            video_pts_ns.saturating_sub(self.first_video_pts_ns.unwrap_or(video_pts_ns));
        let audio_elapsed_ns = audio_position_ns
            .saturating_sub(self.first_audio_position_ns.unwrap_or(audio_position_ns));
        if let Some(drift_ns) = native_vulkan_audio_i128_to_i64(
            i128::from(audio_elapsed_ns) - i128::from(video_elapsed_ns),
        ) {
            self.zero_based_drift_latest_ns = Some(drift_ns);
            native_vulkan_audio_update_min_i64(&mut self.zero_based_drift_min_ns, drift_ns);
            native_vulkan_audio_update_max_i64(&mut self.zero_based_drift_max_ns, drift_ns);
            native_vulkan_audio_update_max(
                &mut self.zero_based_drift_abs_max_ns,
                drift_ns.unsigned_abs(),
            );
        }
        Ok(())
    }

    pub(super) fn seek_for_video_loop(
        &mut self,
        position_ms: u64,
    ) -> Result<(), NativeVulkanError> {
        self.ensure_clocked_playback(position_ms)?;
        self.poll()?;
        self.audio_loop_seek_count = self.audio_loop_seek_count.saturating_add(1);
        self.audio_last_loop_seek_position_ms = Some(position_ms);
        if let Err(err) = self.restart_clocked_playback_at(position_ms) {
            self.audio_loop_seek_error_count = self.audio_loop_seek_error_count.saturating_add(1);
            return Err(err);
        }
        self.audio_loop_restart_count = self.audio_loop_restart_count.saturating_add(1);
        self.begin_segment(position_ms);
        self.poll()
    }

    pub(super) fn snapshot(
        &mut self,
    ) -> Result<NativeVulkanAudioClockRuntimeSnapshot, NativeVulkanError> {
        self.poll()?;
        let gst_state = Some(
            self.pipeline
                .state(gst::ClockTime::ZERO)
                .1
                .name()
                .to_string(),
        );
        let gst_position_ns = self
            .pipeline
            .query_position::<gst::ClockTime>()
            .map(|value| value.nseconds());
        let gst_duration_ns = self
            .pipeline
            .query_duration::<gst::ClockTime>()
            .map(|value| value.nseconds());
        let audio_decoders = native_vulkan_audio_decoder_elements(self.pipeline.upcast_ref());
        let video_decoders = native_vulkan_video_decoder_elements(self.pipeline.upcast_ref());
        let reached_clocked_playback = self.stats.new_clock_count > 0
            && self.stats.stream_start_count > 0
            && self.stats.state_playing_count > 0
            && self.stats.audio_buffer_count > 0;
        let no_video_decoder_instantiated = video_decoders.is_empty();

        Ok(NativeVulkanAudioClockRuntimeSnapshot {
            source: self.source.clone(),
            runtime_elapsed_ms: native_vulkan_audio_duration_ms(self.started_at.elapsed()),
            pipeline: "qtdemux-aacparse-avdec_aac-appsink-clock-probe",
            audio_raw_caps: self.stats.audio_raw_caps.clone(),
            audio_sample_format: self.stats.audio_sample_format.clone(),
            audio_sample_rate: self.stats.audio_sample_rate,
            audio_channels: self.stats.audio_channels,
            gst_state,
            gst_position_ns,
            gst_duration_ns,
            gst_new_clock_count: self.stats.new_clock_count,
            gst_clock_names: self.stats.clock_names.clone(),
            gst_clock_lost_count: self.stats.clock_lost_count,
            gst_stream_start_count: self.stats.stream_start_count,
            gst_async_done_count: self.stats.async_done_count,
            gst_duration_changed_count: self.stats.duration_changed_count,
            gst_latency_count: self.stats.latency_count,
            gst_state_playing_count: self.stats.state_playing_count,
            gst_eos_count: self.stats.eos_count,
            audio_buffer_count: self.stats.audio_buffer_count,
            audio_first_pts_ns: self.stats.first_pts_ns,
            audio_last_pts_ns: self.stats.last_pts_ns,
            audio_pts_delta_min_ns: self.stats.pts_delta_min_ns,
            audio_pts_delta_max_ns: self.stats.pts_delta_max_ns,
            audio_duration_min_ns: self.stats.duration_min_ns,
            audio_duration_max_ns: self.stats.duration_max_ns,
            audio_decoders,
            video_decoders,
            reached_clocked_playback,
            no_video_decoder_instantiated,
            audio_loop_seek_count: self.audio_loop_seek_count,
            audio_loop_seek_error_count: self.audio_loop_seek_error_count,
            audio_loop_restart_count: self.audio_loop_restart_count,
            audio_last_loop_seek_position_ms: self.audio_last_loop_seek_position_ms,
            audio_playback_started: self.playback_started,
            audio_clock_serial: self
                .audio_segment
                .map(|segment| segment.serial)
                .unwrap_or(self.audio_clock_serial),
            audio_initial_position_ms: self.audio_initial_position_ms,
            audio_segment_start_position_ns: self
                .audio_segment
                .map(|segment| segment.start_position_ns),
            audio_segment_elapsed_ns: self.audio_segment_elapsed_ns(),
            audio_position_stale_count: self.audio_position_stale_count,
            audio_sample_stale_count: self.audio_sample_stale_count,
            audio_master_clock_estimate_ns: self.audio_master_clock_estimate_ns(),
            sampled_video_frame_count: self.sampled_video_frame_count,
            audio_position_query_count: self.audio_position_query_count,
            audio_position_query_hit_count: self.audio_position_query_hit_count,
            first_video_pts_ns: self.first_video_pts_ns,
            first_audio_position_ns: self.first_audio_position_ns,
            first_video_clock_ns: self.first_video_clock_ns,
            latest_video_pts_ns: self.latest_video_pts_ns,
            latest_video_clock_ns: self.latest_video_clock_ns,
            latest_audio_position_ns: self.latest_audio_position_ns,
            audio_video_zero_based_drift_latest_ns: self.zero_based_drift_latest_ns,
            audio_video_zero_based_drift_min_ns: self.zero_based_drift_min_ns,
            audio_video_zero_based_drift_max_ns: self.zero_based_drift_max_ns,
            audio_video_zero_based_drift_abs_max_ns: self.zero_based_drift_abs_max_ns,
            audio_video_clock_drift_latest_ns: self.clock_drift_latest_ns,
            audio_video_clock_drift_min_ns: self.clock_drift_min_ns,
            audio_video_clock_drift_max_ns: self.clock_drift_max_ns,
            audio_video_clock_drift_abs_max_ns: self.clock_drift_abs_max_ns,
            audio_video_master_clock_drift_latest_ns: self.master_clock_drift_latest_ns,
            audio_video_master_clock_drift_min_ns: self.master_clock_drift_min_ns,
            audio_video_master_clock_drift_max_ns: self.master_clock_drift_max_ns,
            audio_video_master_clock_drift_abs_max_ns: self.master_clock_drift_abs_max_ns,
            audio_video_raw_drift_latest_ns: self.raw_drift_latest_ns,
            last_error: self.stats.last_error.clone(),
        })
    }
}

impl Drop for NativeVulkanAudioClockRuntimeProbe {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

pub fn probe_native_vulkan_audio_clock(
    options: NativeVulkanAudioClockProbeOptions,
) -> Result<NativeVulkanAudioClockProbeSnapshot, NativeVulkanError> {
    if !options.source.is_file() {
        return Err(NativeVulkanError::Video(format!(
            "audio clock probe source does not exist: {}",
            options.source.display()
        )));
    }
    if options.duration.is_zero() {
        return Err(NativeVulkanError::Video(
            "audio clock probe duration must be non-zero".to_owned(),
        ));
    }

    let mut runtime = NativeVulkanAudioClockRuntimeProbe::start(&options.source)?;
    runtime.ensure_clocked_playback(0)?;
    runtime.started_at = Instant::now();
    let started_at = runtime.started_at;
    let deadline = started_at + options.duration;

    while Instant::now() < deadline {
        while let Some(message) = runtime
            .bus
            .timed_pop(Some(gst::ClockTime::from_mseconds(10)))
        {
            native_vulkan_audio_clock_record_message(
                &runtime.pipeline,
                &mut runtime.stats,
                &message,
            )?;
            if Instant::now() >= deadline {
                break;
            }
        }
        runtime.poll()?;
    }
    let runtime_snapshot = runtime.snapshot()?;

    let runtime_elapsed_ms =
        native_vulkan_audio_duration_ms(started_at.elapsed().min(options.duration));

    Ok(NativeVulkanAudioClockProbeSnapshot {
        source: options.source,
        probe_duration_ms: native_vulkan_audio_duration_ms(options.duration),
        runtime_elapsed_ms,
        pipeline: "qtdemux-aacparse-avdec_aac-appsink-clock-probe",
        audio_stream_codec: None,
        audio_raw_caps: runtime_snapshot.audio_raw_caps,
        audio_sample_format: runtime_snapshot.audio_sample_format,
        audio_sample_rate: runtime_snapshot.audio_sample_rate,
        audio_channels: runtime_snapshot.audio_channels,
        gst_state: runtime_snapshot.gst_state,
        gst_position_ns: runtime_snapshot.gst_position_ns,
        gst_duration_ns: runtime_snapshot.gst_duration_ns,
        gst_new_clock_count: runtime_snapshot.gst_new_clock_count,
        gst_clock_names: runtime_snapshot.gst_clock_names,
        gst_clock_lost_count: runtime_snapshot.gst_clock_lost_count,
        gst_stream_start_count: runtime_snapshot.gst_stream_start_count,
        gst_async_done_count: runtime_snapshot.gst_async_done_count,
        gst_duration_changed_count: runtime_snapshot.gst_duration_changed_count,
        gst_latency_count: runtime_snapshot.gst_latency_count,
        gst_state_playing_count: runtime_snapshot.gst_state_playing_count,
        gst_eos_count: runtime_snapshot.gst_eos_count,
        audio_buffer_count: runtime_snapshot.audio_buffer_count,
        audio_first_pts_ns: runtime_snapshot.audio_first_pts_ns,
        audio_last_pts_ns: runtime_snapshot.audio_last_pts_ns,
        audio_pts_delta_min_ns: runtime_snapshot.audio_pts_delta_min_ns,
        audio_pts_delta_max_ns: runtime_snapshot.audio_pts_delta_max_ns,
        audio_duration_min_ns: runtime_snapshot.audio_duration_min_ns,
        audio_duration_max_ns: runtime_snapshot.audio_duration_max_ns,
        audio_decoders: runtime_snapshot.audio_decoders,
        video_decoders: runtime_snapshot.video_decoders,
        reached_clocked_playback: runtime_snapshot.reached_clocked_playback,
        no_video_decoder_instantiated: runtime_snapshot.no_video_decoder_instantiated,
        last_error: runtime_snapshot.last_error,
    })
}

fn native_vulkan_audio_clock_aac_pipeline(
    source: &PathBuf,
) -> Result<gst::Pipeline, NativeVulkanError> {
    let pipeline = gst::Pipeline::new();
    let filesrc = native_vulkan_audio_gst_element("filesrc")?;
    filesrc.set_property("location", source.to_string_lossy().as_ref());
    let demux = native_vulkan_audio_gst_element("qtdemux")?;
    demux.set_property("name", "gilder-native-vulkan-audio-demux");
    let queue = native_vulkan_audio_gst_element("queue")?;
    native_vulkan_audio_configure_queue(&queue);
    let parser = native_vulkan_audio_gst_element("aacparse")?;
    let decoder = native_vulkan_audio_gst_element("avdec_aac")?;
    let convert = native_vulkan_audio_gst_element("audioconvert")?;
    let resample = native_vulkan_audio_gst_element("audioresample")?;
    let sink = native_vulkan_audio_gst_element("appsink")?;
    native_vulkan_audio_configure_appsink(&sink);

    pipeline
        .add_many([
            &filesrc, &demux, &queue, &parser, &decoder, &convert, &resample, &sink,
        ])
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    filesrc
        .link(&demux)
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
    gst::Element::link_many([&queue, &parser, &decoder, &convert, &resample, &sink])
        .map_err(|err| NativeVulkanError::Video(err.to_string()))?;

    let queue_sink = queue
        .static_pad("sink")
        .ok_or_else(|| NativeVulkanError::Video("audio queue has no sink pad".to_owned()))?;
    demux.connect_pad_added(move |_, pad| {
        if queue_sink.is_linked() {
            return;
        }
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let is_aac_audio = caps.structure(0).is_some_and(|structure| {
            structure.name() == "audio/mpeg"
                && structure
                    .get::<i32>("mpegversion")
                    .is_ok_and(|version| version == 4)
        });
        if is_aac_audio {
            let _ = pad.link(&queue_sink);
        }
    });

    Ok(pipeline)
}

fn native_vulkan_audio_clock_record_message(
    pipeline: &gst::Pipeline,
    stats: &mut NativeVulkanAudioClockProbeStats,
    message: &gst::Message,
) -> Result<(), NativeVulkanError> {
    match message.view() {
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
            stats.last_error = Some(message.clone());
            return Err(NativeVulkanError::Video(message));
        }
        gst::MessageView::Eos(_) => {
            stats.eos_count = stats.eos_count.saturating_add(1);
        }
        gst::MessageView::NewClock(new_clock) => {
            stats.new_clock_count = stats.new_clock_count.saturating_add(1);
            if let Some(clock) = new_clock.clock() {
                stats.clock_names.push(clock.name().to_string());
            }
        }
        gst::MessageView::ClockLost(_) => {
            stats.clock_lost_count = stats.clock_lost_count.saturating_add(1);
        }
        gst::MessageView::StreamStart(_) => {
            stats.stream_start_count = stats.stream_start_count.saturating_add(1);
        }
        gst::MessageView::AsyncDone(_) => {
            stats.async_done_count = stats.async_done_count.saturating_add(1);
        }
        gst::MessageView::DurationChanged(_) => {
            stats.duration_changed_count = stats.duration_changed_count.saturating_add(1);
        }
        gst::MessageView::Latency(_) => {
            stats.latency_count = stats.latency_count.saturating_add(1);
            pipeline
                .recalculate_latency()
                .map_err(|err| NativeVulkanError::Video(err.to_string()))?;
        }
        gst::MessageView::StateChanged(state) => {
            if state.current() == gst::State::Playing {
                stats.state_playing_count = stats.state_playing_count.saturating_add(1);
            }
        }
        _ => {}
    }
    Ok(())
}

fn native_vulkan_audio_gst_element(name: &str) -> Result<gst::Element, NativeVulkanError> {
    gst::ElementFactory::make(name)
        .build()
        .map_err(|err| NativeVulkanError::Video(format!("create {name}: {err}")))
}

fn native_vulkan_audio_configure_queue(queue: &gst::Element) {
    if queue.find_property("max-size-buffers").is_some() {
        queue.set_property("max-size-buffers", 8u32);
    }
    if queue.find_property("max-size-bytes").is_some() {
        queue.set_property("max-size-bytes", 0u32);
    }
    if queue.find_property("max-size-time").is_some() {
        queue.set_property("max-size-time", 250_000_000u64);
    }
}

fn native_vulkan_audio_configure_appsink(sink: &gst::Element) {
    sink.set_property("name", "gilder-native-vulkan-audio-appsink");
    if sink.find_property("sync").is_some() {
        sink.set_property("sync", true);
    }
    if sink.find_property("async").is_some() {
        sink.set_property("async", false);
    }
    if sink.find_property("emit-signals").is_some() {
        sink.set_property("emit-signals", false);
    }
    if sink.find_property("enable-last-sample").is_some() {
        sink.set_property("enable-last-sample", false);
    }
    if sink.find_property("wait-on-eos").is_some() {
        sink.set_property("wait-on-eos", false);
    }
    if sink.find_property("max-buffers").is_some() {
        sink.set_property("max-buffers", 1u32);
    }
    if sink.find_property("drop").is_some() {
        sink.set_property("drop", true);
    }
    if sink.find_property("qos").is_some() {
        sink.set_property("qos", true);
    }
}

fn native_vulkan_audio_clock_time_ns(value: Option<gst::ClockTime>) -> Option<u64> {
    value.map(|value| value.nseconds())
}

fn native_vulkan_audio_position_belongs_to_segment(
    position_ns: u64,
    segment_start_position_ns: u64,
    segment_elapsed_ns: u64,
) -> bool {
    let lower =
        segment_start_position_ns.saturating_sub(NATIVE_VULKAN_AUDIO_POSITION_EARLY_TOLERANCE_NS);
    let upper = segment_start_position_ns
        .saturating_add(segment_elapsed_ns)
        .saturating_add(NATIVE_VULKAN_AUDIO_POSITION_LATE_TOLERANCE_NS);
    position_ns >= lower && position_ns <= upper
}

fn native_vulkan_audio_duration_ms(value: Duration) -> u64 {
    value.as_millis().min(u128::from(u64::MAX)) as u64
}

fn native_vulkan_audio_duration_ns(value: Duration) -> u64 {
    value.as_nanos().min(u128::from(u64::MAX)) as u64
}

fn native_vulkan_audio_update_min(slot: &mut Option<u64>, value: u64) {
    *slot = Some(slot.map_or(value, |current| current.min(value)));
}

fn native_vulkan_audio_update_max(slot: &mut Option<u64>, value: u64) {
    *slot = Some(slot.map_or(value, |current| current.max(value)));
}

fn native_vulkan_audio_update_min_i64(slot: &mut Option<i64>, value: i64) {
    *slot = Some(slot.map_or(value, |current| current.min(value)));
}

fn native_vulkan_audio_update_max_i64(slot: &mut Option<i64>, value: i64) {
    *slot = Some(slot.map_or(value, |current| current.max(value)));
}

fn native_vulkan_audio_i128_to_i64(value: i128) -> Option<i64> {
    i64::try_from(value).ok()
}

fn native_vulkan_audio_decoder_elements(element: &gst::Element) -> Vec<String> {
    native_vulkan_audio_child_factory_names(element)
        .into_iter()
        .filter(|name| {
            matches!(
                name.as_str(),
                "avdec_aac" | "faad" | "fdkaacdec" | "avdec_mp3" | "avdec_opus" | "opusdec"
            )
        })
        .collect()
}

fn native_vulkan_video_decoder_elements(element: &gst::Element) -> Vec<String> {
    native_vulkan_audio_child_factory_names(element)
        .into_iter()
        .filter(|name| {
            matches!(
                name.as_str(),
                "avdec_h264"
                    | "openh264dec"
                    | "vah264dec"
                    | "vaapih264dec"
                    | "nvh264dec"
                    | "vdph264dec"
                    | "avdec_h265"
                    | "avdec_hevc"
                    | "vah265dec"
                    | "vaapih265dec"
                    | "nvh265dec"
                    | "dav1ddec"
                    | "avdec_av1"
                    | "av1dec"
                    | "vaav1dec"
                    | "vaapiav1dec"
                    | "nvav1dec"
            )
        })
        .collect()
}

fn native_vulkan_audio_child_factory_names(element: &gst::Element) -> Vec<String> {
    let Ok(bin) = element.clone().downcast::<gst::Bin>() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut iterator = bin.iterate_recurse();
    loop {
        match iterator.next() {
            Ok(Some(child)) => {
                if let Some(factory) = child.factory() {
                    names.push(factory.name().to_string());
                }
            }
            Ok(None) => break,
            Err(gst::IteratorError::Resync) => iterator.resync(),
            Err(_) => break,
        }
    }
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_window_rejects_old_audio_after_nonzero_seek() {
        let segment_start_ns = 5_000_000_000;
        let elapsed_ns = 100_000_000;

        assert!(!native_vulkan_audio_position_belongs_to_segment(
            0,
            segment_start_ns,
            elapsed_ns
        ));
        assert!(!native_vulkan_audio_position_belongs_to_segment(
            4_700_000_000,
            segment_start_ns,
            elapsed_ns
        ));
    }

    #[test]
    fn segment_window_accepts_seek_target_and_elapsed_audio() {
        let segment_start_ns = 5_000_000_000;
        let elapsed_ns = 100_000_000;

        assert!(native_vulkan_audio_position_belongs_to_segment(
            4_750_000_000,
            segment_start_ns,
            elapsed_ns
        ));
        assert!(native_vulkan_audio_position_belongs_to_segment(
            5_600_000_000,
            segment_start_ns,
            elapsed_ns
        ));
        assert!(!native_vulkan_audio_position_belongs_to_segment(
            5_700_000_000,
            segment_start_ns,
            elapsed_ns
        ));
    }
}
