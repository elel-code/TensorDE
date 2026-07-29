use std::collections::VecDeque;
#[cfg(feature = "native-vulkan-video")]
use std::ffi::{CStr, CString};
use std::path::PathBuf;
#[cfg(feature = "native-vulkan-video")]
use std::ptr::{self, NonNull};

#[cfg(feature = "native-vulkan-video")]
use std::num::NonZeroI32;
#[cfg(feature = "native-vulkan-video")]
use std::os::raw::{c_char, c_int, c_longlong};
#[cfg(feature = "native-vulkan-video")]
use std::os::unix::ffi::OsStrExt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
#[cfg(feature = "native-vulkan-video")]
use std::sync::{Arc, Once};

use serde::Serialize;

use super::super::NativeVulkanError;
use super::event_source::NativeVulkanAudioEventChannel;
use super::policy::NativeVulkanAudioOutputMode;
use crate::engine::scene::StereoSpectrum64;

pub(in crate::renderer::native_vulkan) const NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS: usize = 3;

const FFMPEG_AUDIO_CLOCK_REFERENCE: &str =
    "references/gilder/ffmpeg/fftools/ffplay.c:114-123,1375-1483,1629-1740";
const AUDIO_CLOCK_QUEUE_POLICY: &str = "FFmpeg-style PacketQueue serial metadata; clock-only packets are consumed as timestamp metadata and AVPacket payloads are unref'd immediately";
const AUDIO_CLOCK_MODEL: &str = "muted clock-only audio master: packet PTS/duration advances a serial-scoped audio clock; serial changes invalidate stale samples across loop/seek";
const NATIVE_VULKAN_AUDIO_SIGNAL_SCALE: f32 = 1_000_000.0;
static NATIVE_VULKAN_AUDIO_SIGNAL_READY: AtomicBool = AtomicBool::new(false);
static NATIVE_VULKAN_AUDIO_SIGNAL_LEVEL_MICROS: AtomicU32 = AtomicU32::new(0);
static NATIVE_VULKAN_AUDIO_SPECTRUM64_READY: AtomicBool = AtomicBool::new(false);
static NATIVE_VULKAN_AUDIO_SPECTRUM64: Mutex<StereoSpectrum64> = Mutex::new(StereoSpectrum64::ZERO);

pub(in crate::renderer::native_vulkan) fn native_vulkan_audio_signal_level() -> Option<f32> {
    NATIVE_VULKAN_AUDIO_SIGNAL_READY
        .load(Ordering::Relaxed)
        .then(|| {
            NATIVE_VULKAN_AUDIO_SIGNAL_LEVEL_MICROS.load(Ordering::Relaxed) as f32
                / NATIVE_VULKAN_AUDIO_SIGNAL_SCALE
        })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_audio_spectrum64()
-> Option<StereoSpectrum64> {
    if !NATIVE_VULKAN_AUDIO_SPECTRUM64_READY.load(Ordering::Acquire) {
        return None;
    }
    NATIVE_VULKAN_AUDIO_SPECTRUM64
        .lock()
        .ok()
        .map(|spectrum| *spectrum)
}

fn native_vulkan_audio_publish_signal_level(level_micros: u32) {
    NATIVE_VULKAN_AUDIO_SIGNAL_LEVEL_MICROS.store(level_micros.min(1_000_000), Ordering::Relaxed);
    NATIVE_VULKAN_AUDIO_SIGNAL_READY.store(true, Ordering::Relaxed);
}

pub(super) fn native_vulkan_audio_publish_spectrum64(spectrum64: StereoSpectrum64) {
    if let Ok(mut spectrum) = NATIVE_VULKAN_AUDIO_SPECTRUM64.lock() {
        *spectrum = spectrum64;
        NATIVE_VULKAN_AUDIO_SPECTRUM64_READY.store(true, Ordering::Release);
    }
}

pub(super) fn native_vulkan_audio_clear_spectrum64() {
    NATIVE_VULKAN_AUDIO_SPECTRUM64_READY.store(false, Ordering::Release);
    if let Ok(mut spectrum) = NATIVE_VULKAN_AUDIO_SPECTRUM64.lock() {
        *spectrum = StereoSpectrum64::ZERO;
    }
}

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioClockProbeOptions {
    pub(in crate::renderer::native_vulkan) source: PathBuf,
    pub(in crate::renderer::native_vulkan) output_mode: NativeVulkanAudioOutputMode,
    pub(in crate::renderer::native_vulkan) queue_capacity: usize,
    pub(in crate::renderer::native_vulkan) packets_to_probe: u32,
    pub(in crate::renderer::native_vulkan) loop_on_eos: bool,
    pub(in crate::renderer::native_vulkan) target_playback_clock_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan) event_channel: Option<NativeVulkanAudioEventChannel>,
}

impl NativeVulkanAudioClockProbeOptions {
    pub(in crate::renderer::native_vulkan) fn clock_only(source: PathBuf) -> Self {
        Self {
            source,
            output_mode: NativeVulkanAudioOutputMode::ClockOnly,
            queue_capacity: NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
            packets_to_probe: NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS as u32,
            loop_on_eos: false,
            target_playback_clock_ns: None,
            event_channel: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanAudioClockPacketSnapshot {
    pub packet_index: u32,
    pub serial: u32,
    pub pts_ns: Option<u64>,
    pub duration_ns: Option<u64>,
    pub pts_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub payload_bytes: u32,
    pub decoded_frames: u32,
    pub decoded_samples: u32,
    pub sample_rate_hz: Option<u32>,
    pub channel_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanAudioClockRuntimeSnapshot {
    pub route: &'static str,
    pub boundary: &'static str,
    pub output_mode: &'static str,
    pub source: Option<PathBuf>,
    pub audio_stream_found: bool,
    pub audio_stream_index: Option<i32>,
    pub audio_stream_error: Option<String>,
    pub ffmpeg_reference: &'static str,
    pub queue_policy: &'static str,
    pub clock_model: &'static str,
    pub audible_output_started: bool,
    pub audio_output_backend: &'static str,
    pub audio_output_sample_format: &'static str,
    pub audio_output_frames: u32,
    pub audio_output_samples: u64,
    pub audio_output_bytes: u64,
    pub audio_output_sample_rate_hz: Option<u32>,
    pub audio_output_channel_count: Option<u32>,
    pub audio_output_write_calls: u64,
    pub audio_output_write_waits: u64,
    pub audio_output_process_callbacks: u64,
    pub audio_output_buffer_errors: u64,
    pub audio_output_timeout_errors: u64,
    pub audio_output_xrun_count: u64,
    pub audio_output_state_changes: u64,
    pub audio_output_ready_state_changes: u64,
    pub audio_output_stream_state: &'static str,
    pub audio_output_stream_ready: bool,
    pub audio_output_lifecycle_model: &'static str,
    pub audio_output_latency_policy: &'static str,
    pub playback_runtime_model: &'static str,
    pub playback_target_clock_ns: Option<u64>,
    pub playback_covered_clock_ns: Option<u64>,
    pub playback_coverage_percent: u32,
    pub playback_target_reached: bool,
    pub decoded_frames: u32,
    pub decoded_samples: u64,
    pub audio_signal_level_micros: u32,
    pub audio_signal_model: &'static str,
    pub audio_spectrum: StereoSpectrum64,
    pub audio_spectrum_model: &'static str,
    pub audio_sample_rate_hz: Option<u32>,
    pub audio_channel_count: Option<u32>,
    pub capacity: u32,
    pub queued_packets: u32,
    pub pushed_packets: u32,
    pub consumed_packets: u32,
    pub overflow_dropped_packets: u32,
    pub stale_dropped_packets: u32,
    pub current_serial: u32,
    pub serial_resets: u32,
    pub eos_count: u32,
    pub loop_count: u32,
    pub video_master_clock_ready: bool,
    pub video_master_start_clock_ns: Option<u64>,
    pub video_master_start_serial: Option<u32>,
    pub video_master_start_packet_index: Option<u32>,
    pub current_serial_start_clock_ns: Option<u64>,
    pub current_serial_start_serial: Option<u32>,
    pub current_serial_start_packet_index: Option<u32>,
    pub clock_ns: Option<u64>,
    pub clock_ms: Option<u64>,
    pub last_packet_pts_ns: Option<u64>,
    pub last_packet_duration_ns: Option<u64>,
    pub retained_payload_bytes: u64,
    pub retained_pcm_frame_bytes: u64,
    pub max_payload_bytes: u64,
    pub packets_head: Vec<NativeVulkanAudioClockPacketSnapshot>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioClockPacket {
    pub(in crate::renderer::native_vulkan) serial: u32,
    pub(in crate::renderer::native_vulkan) pts_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan) duration_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan) payload_bytes: u32,
    pub(in crate::renderer::native_vulkan) decoded_frames: u32,
    pub(in crate::renderer::native_vulkan) decoded_samples: u32,
    pub(in crate::renderer::native_vulkan) audio_signal_level_micros: u32,
    pub(in crate::renderer::native_vulkan) audio_spectrum: Option<StereoSpectrum64>,
    pub(in crate::renderer::native_vulkan) sample_rate_hz: Option<u32>,
    pub(in crate::renderer::native_vulkan) channel_count: Option<u32>,
    pub(in crate::renderer::native_vulkan) output_frames: u32,
    pub(in crate::renderer::native_vulkan) output_samples: u32,
    pub(in crate::renderer::native_vulkan) output_bytes: u64,
    pub(in crate::renderer::native_vulkan) output_sample_rate_hz: Option<u32>,
    pub(in crate::renderer::native_vulkan) output_channel_count: Option<u32>,
    pub(in crate::renderer::native_vulkan) output_write_calls: u64,
    pub(in crate::renderer::native_vulkan) output_write_waits: u64,
    pub(in crate::renderer::native_vulkan) output_process_callbacks: u64,
    pub(in crate::renderer::native_vulkan) output_buffer_errors: u64,
    pub(in crate::renderer::native_vulkan) output_timeout_errors: u64,
    pub(in crate::renderer::native_vulkan) output_state_changes: u64,
    pub(in crate::renderer::native_vulkan) output_ready_state_changes: u64,
    pub(in crate::renderer::native_vulkan) output_stream_state: i32,
    pub(in crate::renderer::native_vulkan) output_stream_ready: bool,
}

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioClockPacketQueue {
    capacity: usize,
    queued: VecDeque<NativeVulkanAudioClockPacket>,
    current_serial: u32,
    pushed_packets: u32,
    consumed_packets: u32,
    overflow_dropped_packets: u32,
    stale_dropped_packets: u32,
    max_payload_bytes: u64,
}

impl NativeVulkanAudioClockPacketQueue {
    pub(in crate::renderer::native_vulkan) fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            queued: VecDeque::with_capacity(capacity),
            current_serial: 0,
            pushed_packets: 0,
            consumed_packets: 0,
            overflow_dropped_packets: 0,
            stale_dropped_packets: 0,
            max_payload_bytes: 0,
        }
    }

    pub(in crate::renderer::native_vulkan) fn push(
        &mut self,
        packet: NativeVulkanAudioClockPacket,
    ) {
        if packet.serial < self.current_serial {
            self.stale_dropped_packets = self.stale_dropped_packets.saturating_add(1);
            return;
        }
        if packet.serial > self.current_serial {
            self.start_serial(packet.serial);
        }
        if self.queued.len() >= self.capacity {
            let _ = self.queued.pop_front();
            self.overflow_dropped_packets = self.overflow_dropped_packets.saturating_add(1);
        }
        self.max_payload_bytes = self.max_payload_bytes.max(u64::from(packet.payload_bytes));
        self.pushed_packets = self.pushed_packets.saturating_add(1);
        self.queued.push_back(packet);
    }

    pub(in crate::renderer::native_vulkan) fn pop(
        &mut self,
    ) -> Option<NativeVulkanAudioClockPacket> {
        let packet = self.queued.pop_front()?;
        self.consumed_packets = self.consumed_packets.saturating_add(1);
        Some(packet)
    }

    pub(in crate::renderer::native_vulkan) fn start_serial(&mut self, serial: u32) {
        self.current_serial = serial;
        self.queued.clear();
    }

    fn queued_packets(&self) -> u32 {
        self.queued.len().min(u32::MAX as usize) as u32
    }

    fn retained_payload_bytes(&self) -> u64 {
        0
    }
}

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioClock {
    current_serial: u32,
    serial_resets: u32,
    pts_offset_ns: u64,
    loop_base_source_pts_ns: Option<u64>,
    clock_ns: Option<u64>,
    last_packet_pts_ns: Option<u64>,
    last_packet_duration_ns: Option<u64>,
    stale_dropped_packets: u32,
}

impl NativeVulkanAudioClock {
    pub(in crate::renderer::native_vulkan) fn new() -> Self {
        Self {
            current_serial: 0,
            serial_resets: 0,
            pts_offset_ns: 0,
            loop_base_source_pts_ns: None,
            clock_ns: None,
            last_packet_pts_ns: None,
            last_packet_duration_ns: None,
            stale_dropped_packets: 0,
        }
    }

    pub(in crate::renderer::native_vulkan) fn advance(
        &mut self,
        packet: NativeVulkanAudioClockPacket,
    ) -> Option<u64> {
        if packet.serial < self.current_serial {
            self.stale_dropped_packets = self.stale_dropped_packets.saturating_add(1);
            return self.clock_ns;
        }
        if packet.serial > self.current_serial {
            self.reset_for_serial(packet.serial);
        }

        let packet_start_ns = packet.pts_ns.map(|pts| {
            let base = *self.loop_base_source_pts_ns.get_or_insert(pts);
            pts.saturating_sub(base).saturating_add(self.pts_offset_ns)
        });
        let clock_ns = match (packet_start_ns, packet.duration_ns) {
            (Some(start), Some(duration)) => Some(start.saturating_add(duration)),
            (Some(start), None) => Some(start),
            (None, Some(duration)) => self.clock_ns.map(|clock| clock.saturating_add(duration)),
            (None, None) => self.clock_ns,
        };
        if let Some(clock_ns) = clock_ns {
            self.clock_ns = Some(clock_ns);
        }
        self.last_packet_pts_ns = packet.pts_ns;
        if packet.duration_ns.is_some() {
            self.last_packet_duration_ns = packet.duration_ns;
        }
        self.clock_ns
    }

    pub(in crate::renderer::native_vulkan) fn reset_for_serial(&mut self, serial: u32) {
        if serial == self.current_serial {
            return;
        }
        self.pts_offset_ns = self.clock_ns.unwrap_or(self.pts_offset_ns);
        self.loop_base_source_pts_ns = None;
        self.current_serial = serial;
        self.serial_resets = self.serial_resets.saturating_add(1);
    }
}

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioClockRuntime {
    output_mode: NativeVulkanAudioOutputMode,
    source: Option<PathBuf>,
    audio_stream_found: bool,
    audio_stream_index: Option<i32>,
    audio_stream_error: Option<String>,
    audible_output_started: bool,
    queue: NativeVulkanAudioClockPacketQueue,
    clock: NativeVulkanAudioClock,
    decoded_frames: u32,
    decoded_samples: u64,
    audio_signal_level_micros: u32,
    audio_spectrum: StereoSpectrum64,
    audio_sample_rate_hz: Option<u32>,
    audio_channel_count: Option<u32>,
    audio_output_frames: u32,
    audio_output_samples: u64,
    audio_output_bytes: u64,
    audio_output_sample_rate_hz: Option<u32>,
    audio_output_channel_count: Option<u32>,
    audio_output_write_calls: u64,
    audio_output_write_waits: u64,
    audio_output_process_callbacks: u64,
    audio_output_buffer_errors: u64,
    audio_output_timeout_errors: u64,
    audio_output_state_changes: u64,
    audio_output_ready_state_changes: u64,
    audio_output_stream_state: i32,
    audio_output_stream_ready: bool,
    playback_target_clock_ns: Option<u64>,
    eos_count: u32,
    loop_count: u32,
    video_master_start_clock_ns: Option<u64>,
    video_master_start_serial: Option<u32>,
    video_master_start_packet_index: Option<u32>,
    current_serial_start_clock_ns: Option<u64>,
    current_serial_start_serial: Option<u32>,
    current_serial_start_packet_index: Option<u32>,
    packets_head: Vec<NativeVulkanAudioClockPacketSnapshot>,
    event_channel: Option<NativeVulkanAudioEventChannel>,
}

impl NativeVulkanAudioClockRuntime {
    pub(in crate::renderer::native_vulkan) fn new(
        output_mode: NativeVulkanAudioOutputMode,
        queue_capacity: usize,
    ) -> Self {
        Self {
            output_mode,
            source: None,
            audio_stream_found: false,
            audio_stream_index: None,
            audio_stream_error: None,
            audible_output_started: false,
            queue: NativeVulkanAudioClockPacketQueue::new(queue_capacity),
            clock: NativeVulkanAudioClock::new(),
            decoded_frames: 0,
            decoded_samples: 0,
            audio_signal_level_micros: 0,
            audio_spectrum: StereoSpectrum64::ZERO,
            audio_sample_rate_hz: None,
            audio_channel_count: None,
            audio_output_frames: 0,
            audio_output_samples: 0,
            audio_output_bytes: 0,
            audio_output_sample_rate_hz: None,
            audio_output_channel_count: None,
            audio_output_write_calls: 0,
            audio_output_write_waits: 0,
            audio_output_process_callbacks: 0,
            audio_output_buffer_errors: 0,
            audio_output_timeout_errors: 0,
            audio_output_state_changes: 0,
            audio_output_ready_state_changes: 0,
            audio_output_stream_state: 0,
            audio_output_stream_ready: false,
            playback_target_clock_ns: None,
            eos_count: 0,
            loop_count: 0,
            video_master_start_clock_ns: None,
            video_master_start_serial: None,
            video_master_start_packet_index: None,
            current_serial_start_clock_ns: None,
            current_serial_start_serial: None,
            current_serial_start_packet_index: None,
            packets_head: Vec::new(),
            event_channel: None,
        }
    }

    pub(in crate::renderer::native_vulkan) fn with_event_channel(
        mut self,
        event_channel: Option<NativeVulkanAudioEventChannel>,
    ) -> Self {
        self.event_channel = event_channel;
        self
    }

    pub(in crate::renderer::native_vulkan) fn with_source(mut self, source: PathBuf) -> Self {
        self.source = Some(source);
        self
    }

    pub(in crate::renderer::native_vulkan) fn set_audio_stream(&mut self, stream_index: i32) {
        self.audio_stream_found = true;
        self.audio_stream_index = Some(stream_index);
        self.audio_stream_error = None;
    }

    pub(in crate::renderer::native_vulkan) fn set_audio_stream_error(&mut self, error: String) {
        self.audio_stream_found = false;
        self.audio_stream_index = None;
        self.audio_stream_error = Some(error);
    }

    pub(in crate::renderer::native_vulkan) fn set_eos_counts(
        &mut self,
        eos_count: u32,
        loop_count: u32,
    ) {
        self.eos_count = eos_count;
        self.loop_count = loop_count;
    }

    pub(in crate::renderer::native_vulkan) fn set_playback_target_clock_ns(
        &mut self,
        target_clock_ns: Option<u64>,
    ) {
        self.playback_target_clock_ns = target_clock_ns.filter(|target| *target > 0);
    }

    pub(in crate::renderer::native_vulkan) fn push_and_advance(
        &mut self,
        packet_index: u32,
        packet: NativeVulkanAudioClockPacket,
    ) {
        self.decoded_frames = self.decoded_frames.saturating_add(packet.decoded_frames);
        self.decoded_samples = self
            .decoded_samples
            .saturating_add(u64::from(packet.decoded_samples));
        self.audio_signal_level_micros = self
            .audio_signal_level_micros
            .max(packet.audio_signal_level_micros);
        let has_output_pcm = packet.output_bytes > 0 && packet.output_samples > 0;
        if has_output_pcm {
            if let Some(spectrum) = packet.audio_spectrum {
                self.audio_spectrum = spectrum;
                if let Some(event_channel) = &self.event_channel {
                    event_channel.publish(
                        u64::from(packet.serial),
                        packet.pts_ns.unwrap_or_default(),
                        spectrum,
                    );
                }
                #[cfg(not(test))]
                if self.event_channel.is_none() {
                    native_vulkan_audio_publish_spectrum64(spectrum);
                }
            }
            #[cfg(not(test))]
            if self.event_channel.is_none() {
                native_vulkan_audio_publish_signal_level(packet.audio_signal_level_micros);
            }
        }
        if self.audio_sample_rate_hz.is_none() {
            self.audio_sample_rate_hz = packet.sample_rate_hz;
        }
        if self.audio_channel_count.is_none() {
            self.audio_channel_count = packet.channel_count;
        }
        self.audio_output_frames = self
            .audio_output_frames
            .saturating_add(packet.output_frames);
        self.audio_output_samples = self
            .audio_output_samples
            .saturating_add(u64::from(packet.output_samples));
        self.audio_output_bytes = self.audio_output_bytes.saturating_add(packet.output_bytes);
        if packet.output_bytes > 0 {
            self.audible_output_started = true;
        }
        if self.audio_output_sample_rate_hz.is_none() {
            self.audio_output_sample_rate_hz = packet.output_sample_rate_hz;
        }
        if self.audio_output_channel_count.is_none() {
            self.audio_output_channel_count = packet.output_channel_count;
        }
        self.audio_output_write_calls =
            self.audio_output_write_calls.max(packet.output_write_calls);
        self.audio_output_write_waits =
            self.audio_output_write_waits.max(packet.output_write_waits);
        self.audio_output_process_callbacks = self
            .audio_output_process_callbacks
            .max(packet.output_process_callbacks);
        self.audio_output_buffer_errors = self
            .audio_output_buffer_errors
            .max(packet.output_buffer_errors);
        self.audio_output_timeout_errors = self
            .audio_output_timeout_errors
            .max(packet.output_timeout_errors);
        self.audio_output_state_changes = self
            .audio_output_state_changes
            .max(packet.output_state_changes);
        self.audio_output_ready_state_changes = self
            .audio_output_ready_state_changes
            .max(packet.output_ready_state_changes);
        if packet.output_stream_state != 0 {
            self.audio_output_stream_state = packet.output_stream_state;
        }
        self.audio_output_stream_ready |= packet.output_stream_ready;
        if packet.serial > self.clock.current_serial {
            self.current_serial_start_clock_ns = None;
            self.current_serial_start_serial = None;
            self.current_serial_start_packet_index = None;
        }
        self.queue.push(packet);
        while let Some(packet) = self.queue.pop() {
            if let Some(clock_ns) = self.clock.advance(packet) {
                if self.video_master_start_clock_ns.is_none() {
                    self.video_master_start_clock_ns = Some(clock_ns);
                    self.video_master_start_serial = Some(packet.serial);
                    self.video_master_start_packet_index = Some(packet_index);
                }
                if self.current_serial_start_clock_ns.is_none() {
                    self.current_serial_start_clock_ns = Some(clock_ns);
                    self.current_serial_start_serial = Some(packet.serial);
                    self.current_serial_start_packet_index = Some(packet_index);
                }
            }
            if self.packets_head.len() < NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS {
                self.packets_head
                    .push(NativeVulkanAudioClockPacketSnapshot {
                        packet_index,
                        serial: packet.serial,
                        pts_ns: packet.pts_ns,
                        duration_ns: packet.duration_ns,
                        pts_ms: packet.pts_ns.map(|pts| pts / 1_000_000),
                        duration_ms: packet.duration_ns.map(|duration| duration / 1_000_000),
                        payload_bytes: packet.payload_bytes,
                        decoded_frames: packet.decoded_frames,
                        decoded_samples: packet.decoded_samples,
                        sample_rate_hz: packet.sample_rate_hz,
                        channel_count: packet.channel_count,
                    });
            }
        }
    }

    pub(in crate::renderer::native_vulkan) fn playback_target_reached(&self) -> bool {
        match (self.playback_target_clock_ns, self.clock.clock_ns) {
            (Some(target), Some(covered)) => covered >= target,
            _ => false,
        }
    }

    pub(in crate::renderer::native_vulkan) fn snapshot(
        &self,
    ) -> NativeVulkanAudioClockRuntimeSnapshot {
        let playback_covered_clock_ns = self.clock.clock_ns;
        let playback_target_reached =
            match (self.playback_target_clock_ns, playback_covered_clock_ns) {
                (Some(target), Some(covered)) => covered >= target,
                (None, _) => false,
                _ => false,
            };
        let playback_coverage_percent =
            match (self.playback_target_clock_ns, playback_covered_clock_ns) {
                (Some(target), Some(covered)) if target > 0 => {
                    let percent = u128::from(covered)
                        .saturating_mul(100)
                        .checked_div(u128::from(target))
                        .unwrap_or(0);
                    percent.min(u128::from(u32::MAX)) as u32
                }
                _ => 0,
            };
        NativeVulkanAudioClockRuntimeSnapshot {
            route: "native-vulkan-audio-runtime",
            boundary: "FFmpeg audio decode -> serial-scoped audio clock -> PipeWire-only output/runtime telemetry -> video pacing master input",
            output_mode: self.output_mode.as_str(),
            source: self.source.clone(),
            audio_stream_found: self.audio_stream_found,
            audio_stream_index: self.audio_stream_index,
            audio_stream_error: self.audio_stream_error.clone(),
            ffmpeg_reference: FFMPEG_AUDIO_CLOCK_REFERENCE,
            queue_policy: AUDIO_CLOCK_QUEUE_POLICY,
            clock_model: AUDIO_CLOCK_MODEL,
            audible_output_started: self.audible_output_started,
            audio_output_backend: match self.output_mode {
                NativeVulkanAudioOutputMode::Auto => "pipewire-f32le",
                NativeVulkanAudioOutputMode::ClockOnly => "none",
            },
            audio_output_sample_format: match self.output_mode {
                NativeVulkanAudioOutputMode::Auto => "f32le-interleaved",
                NativeVulkanAudioOutputMode::ClockOnly => "none",
            },
            audio_output_frames: self.audio_output_frames,
            audio_output_samples: self.audio_output_samples,
            audio_output_bytes: self.audio_output_bytes,
            audio_output_sample_rate_hz: self.audio_output_sample_rate_hz,
            audio_output_channel_count: self.audio_output_channel_count,
            audio_output_write_calls: self.audio_output_write_calls,
            audio_output_write_waits: self.audio_output_write_waits,
            audio_output_process_callbacks: self.audio_output_process_callbacks,
            audio_output_buffer_errors: self.audio_output_buffer_errors,
            audio_output_timeout_errors: self.audio_output_timeout_errors,
            audio_output_xrun_count: self
                .audio_output_buffer_errors
                .saturating_add(self.audio_output_timeout_errors),
            audio_output_state_changes: self.audio_output_state_changes,
            audio_output_ready_state_changes: self.audio_output_ready_state_changes,
            audio_output_stream_state: native_vulkan_pipewire_stream_state_label(
                self.audio_output_stream_state,
            ),
            audio_output_stream_ready: self.audio_output_stream_ready,
            audio_output_lifecycle_model: match self.output_mode {
                NativeVulkanAudioOutputMode::Auto => {
                    "pipewire-thread-loop-stream-state-owned-by-audio-runtime"
                }
                NativeVulkanAudioOutputMode::ClockOnly => "clock-only-no-output-stream-lifecycle",
            },
            audio_output_latency_policy: match self.output_mode {
                NativeVulkanAudioOutputMode::Auto => {
                    "bounded-pipewire-write-wait-with-zero-buffer-timeout-error-gate"
                }
                NativeVulkanAudioOutputMode::ClockOnly => "clock-only-no-output-latency",
            },
            playback_runtime_model: match self.output_mode {
                NativeVulkanAudioOutputMode::Auto => "pipewire-duration-covered-runtime",
                NativeVulkanAudioOutputMode::ClockOnly => "clock-only-duration-covered-runtime",
            },
            playback_target_clock_ns: self.playback_target_clock_ns,
            playback_covered_clock_ns,
            playback_coverage_percent,
            playback_target_reached,
            decoded_frames: self.decoded_frames,
            decoded_samples: self.decoded_samples,
            audio_signal_level_micros: self.audio_signal_level_micros,
            audio_signal_model: "decoded-f32le-frame-rms",
            audio_spectrum: self.audio_spectrum,
            audio_spectrum_model: "decoded-f32-canonical-stereo64",
            audio_sample_rate_hz: self.audio_sample_rate_hz,
            audio_channel_count: self.audio_channel_count,
            capacity: self.queue.capacity.min(u32::MAX as usize) as u32,
            queued_packets: self.queue.queued_packets(),
            pushed_packets: self.queue.pushed_packets,
            consumed_packets: self.queue.consumed_packets,
            overflow_dropped_packets: self.queue.overflow_dropped_packets,
            stale_dropped_packets: self
                .queue
                .stale_dropped_packets
                .saturating_add(self.clock.stale_dropped_packets),
            current_serial: self.clock.current_serial,
            serial_resets: self.clock.serial_resets,
            eos_count: self.eos_count,
            loop_count: self.loop_count,
            video_master_clock_ready: self.audio_stream_found && self.clock.clock_ns.is_some(),
            video_master_start_clock_ns: if self.audio_stream_found {
                self.video_master_start_clock_ns
            } else {
                None
            },
            video_master_start_serial: if self.audio_stream_found {
                self.video_master_start_serial
            } else {
                None
            },
            video_master_start_packet_index: if self.audio_stream_found {
                self.video_master_start_packet_index
            } else {
                None
            },
            current_serial_start_clock_ns: if self.audio_stream_found {
                self.current_serial_start_clock_ns
            } else {
                None
            },
            current_serial_start_serial: if self.audio_stream_found {
                self.current_serial_start_serial
            } else {
                None
            },
            current_serial_start_packet_index: if self.audio_stream_found {
                self.current_serial_start_packet_index
            } else {
                None
            },
            clock_ns: self.clock.clock_ns,
            clock_ms: self.clock.clock_ns.map(|clock| clock / 1_000_000),
            last_packet_pts_ns: self.clock.last_packet_pts_ns,
            last_packet_duration_ns: self.clock.last_packet_duration_ns,
            retained_payload_bytes: self.queue.retained_payload_bytes(),
            retained_pcm_frame_bytes: 0,
            max_payload_bytes: self.queue.max_payload_bytes,
            packets_head: self.packets_head.clone(),
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_unattached_audio_clock_snapshot(
    output_mode: NativeVulkanAudioOutputMode,
) -> NativeVulkanAudioClockRuntimeSnapshot {
    let mut runtime =
        NativeVulkanAudioClockRuntime::new(output_mode, NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS);
    runtime.set_audio_stream_error(
        "audio clock probe was not requested; no FFmpeg audio stream is attached yet".to_owned(),
    );
    runtime.snapshot()
}

#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan) fn native_vulkan_probe_ffmpeg_audio_clock(
    options: NativeVulkanAudioClockProbeOptions,
) -> Result<NativeVulkanAudioClockRuntimeSnapshot, NativeVulkanError> {
    let mut runtime =
        NativeVulkanAudioClockRuntime::new(options.output_mode, options.queue_capacity)
            .with_source(options.source.clone())
            .with_event_channel(options.event_channel.clone());
    runtime.set_playback_target_clock_ns(options.target_playback_clock_ns);
    let mut reader =
        match NativeVulkanFfmpegAudioClockReader::open(&options.source, options.output_mode) {
            Ok(reader) => reader,
            Err(err) => {
                runtime.set_audio_stream_error(err);
                return Ok(runtime.snapshot());
            }
        };
    runtime.set_audio_stream(reader.stream_index);

    for packet_index in 0..options.packets_to_probe {
        let Some(packet) = reader.read_next_packet(options.loop_on_eos)? else {
            break;
        };
        runtime.push_and_advance(packet_index, packet);
        if runtime.playback_target_reached() {
            break;
        }
        if options.output_mode == NativeVulkanAudioOutputMode::ClockOnly
            && packet_index == 0
            && let (Some(target_clock_ns), Some(clock_ns)) =
                (options.target_playback_clock_ns, runtime.clock.clock_ns)
            && reader.can_fast_forward_clock_only(target_clock_ns, clock_ns)
        {
            let fast_forward =
                reader.metadata_only_fast_forward_packet(target_clock_ns.saturating_sub(clock_ns));
            runtime.push_and_advance(packet_index.saturating_add(1), fast_forward);
            break;
        }
    }
    runtime.set_eos_counts(reader.eos_count, reader.loop_count);
    Ok(runtime.snapshot())
}

#[cfg(not(feature = "native-vulkan-video"))]
pub(in crate::renderer::native_vulkan) fn native_vulkan_probe_ffmpeg_audio_clock(
    options: NativeVulkanAudioClockProbeOptions,
) -> Result<NativeVulkanAudioClockRuntimeSnapshot, NativeVulkanError> {
    let mut runtime =
        NativeVulkanAudioClockRuntime::new(options.output_mode, options.queue_capacity)
            .with_source(options.source)
            .with_event_channel(options.event_channel);
    runtime.set_playback_target_clock_ns(options.target_playback_clock_ns);
    runtime.set_audio_stream_error(
        "native-vulkan-video feature is required for FFmpeg audio clock probing".to_owned(),
    );
    Ok(runtime.snapshot())
}
include!("runtime_ffmpeg/decoder_backend.rs");
