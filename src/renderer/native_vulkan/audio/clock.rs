#![allow(dead_code)]

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

use serde::Serialize;

use super::super::NativeVulkanError;
use super::policy::NativeVulkanAudioOutputMode;

pub(in crate::renderer::native_vulkan) const NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS: usize = 3;

const FFMPEG_AUDIO_CLOCK_REFERENCE: &str =
    "references/ffmpeg/fftools/ffplay.c:114-123,1375-1483,1629-1740";
const AUDIO_CLOCK_QUEUE_POLICY: &str = "FFmpeg-style PacketQueue serial metadata; clock-only packets are consumed as timestamp metadata and AVPacket payloads are unref'd immediately";
const AUDIO_CLOCK_MODEL: &str = "muted clock-only audio master: packet PTS/duration advances a serial-scoped audio clock; serial changes invalidate stale samples across loop/seek";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioClockProbeOptions {
    pub(in crate::renderer::native_vulkan) source: PathBuf,
    pub(in crate::renderer::native_vulkan) output_mode: NativeVulkanAudioOutputMode,
    pub(in crate::renderer::native_vulkan) queue_capacity: usize,
    pub(in crate::renderer::native_vulkan) packets_to_probe: u32,
    pub(in crate::renderer::native_vulkan) loop_on_eos: bool,
    pub(in crate::renderer::native_vulkan) target_playback_clock_ns: Option<u64>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    pub audio_output_stream_ready: bool,
    pub playback_runtime_model: &'static str,
    pub playback_target_clock_ns: Option<u64>,
    pub playback_covered_clock_ns: Option<u64>,
    pub playback_coverage_percent: u32,
    pub playback_target_reached: bool,
    pub decoded_frames: u32,
    pub decoded_samples: u64,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioClockPacket {
    pub(in crate::renderer::native_vulkan) serial: u32,
    pub(in crate::renderer::native_vulkan) pts_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan) duration_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan) payload_bytes: u32,
    pub(in crate::renderer::native_vulkan) decoded_frames: u32,
    pub(in crate::renderer::native_vulkan) decoded_samples: u32,
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
        }
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
                NativeVulkanAudioOutputMode::Auto => "pipewire-s16le",
                NativeVulkanAudioOutputMode::ClockOnly => "none",
            },
            audio_output_sample_format: match self.output_mode {
                NativeVulkanAudioOutputMode::Auto => "s16le-interleaved",
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
            audio_output_stream_ready: self.audio_output_stream_ready,
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
            .with_source(options.source.clone());
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
            .with_source(options.source);
    runtime.set_playback_target_clock_ns(options.target_playback_clock_ns);
    runtime.set_audio_stream_error(
        "native-vulkan-video feature is required for FFmpeg audio clock probing".to_owned(),
    );
    Ok(runtime.snapshot())
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVFormatContext {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVPacket {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVCodec {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVCodecContext {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVFrame {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct GilderAudioOutput {
    _private: [u8; 0],
}

#[cfg(feature = "native-vulkan-video")]
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVRational {
    num: c_int,
    den: c_int,
}

#[cfg(feature = "native-vulkan-video")]
unsafe extern "C" {
    fn gilder_av_error_again() -> c_int;
    fn gilder_av_error_eof() -> c_int;
    fn gilder_av_nopts_value() -> i64;
    fn gilder_av_strerror(errnum: c_int, errbuf: *mut c_char, errbuf_size: usize) -> c_int;
    fn gilder_avformat_open_input(ctx: *mut *mut AVFormatContext, url: *const c_char) -> c_int;
    fn gilder_avformat_close_input(ctx: *mut *mut AVFormatContext);
    fn gilder_av_find_audio_stream(ctx: *mut AVFormatContext) -> c_int;
    fn gilder_av_packet_alloc() -> *mut AVPacket;
    fn gilder_av_packet_free(packet: *mut *mut AVPacket);
    fn gilder_av_packet_unref(packet: *mut AVPacket);
    fn gilder_av_read_frame(ctx: *mut AVFormatContext, packet: *mut AVPacket) -> c_int;
    fn gilder_av_packet_stream_index(packet: *const AVPacket) -> c_int;
    fn gilder_av_packet_size(packet: *const AVPacket) -> c_int;
    fn gilder_av_packet_pts(packet: *const AVPacket) -> i64;
    fn gilder_av_packet_duration(packet: *const AVPacket) -> i64;
    fn gilder_av_stream_time_base(ctx: *mut AVFormatContext, stream_index: c_int) -> AVRational;
    fn gilder_av_seek_stream_start(ctx: *mut AVFormatContext, stream_index: c_int) -> c_int;
    fn gilder_av_stream_decoder(ctx: *mut AVFormatContext, stream_index: c_int) -> *const AVCodec;
    fn gilder_avcodec_alloc_context3(codec: *const AVCodec) -> *mut AVCodecContext;
    fn gilder_avcodec_free_context(ctx: *mut *mut AVCodecContext);
    fn gilder_avcodec_parameters_to_context_for_stream(
        codec_ctx: *mut AVCodecContext,
        format_ctx: *mut AVFormatContext,
        stream_index: c_int,
    ) -> c_int;
    fn gilder_avcodec_open2(ctx: *mut AVCodecContext, codec: *const AVCodec) -> c_int;
    fn gilder_avcodec_send_packet(ctx: *mut AVCodecContext, packet: *const AVPacket) -> c_int;
    fn gilder_avcodec_receive_frame(ctx: *mut AVCodecContext, frame: *mut AVFrame) -> c_int;
    fn gilder_avcodec_context_sample_rate(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_channels(ctx: *const AVCodecContext) -> c_int;
    fn gilder_av_frame_alloc() -> *mut AVFrame;
    fn gilder_av_frame_free(frame: *mut *mut AVFrame);
    fn gilder_av_frame_unref(frame: *mut AVFrame);
    fn gilder_av_frame_nb_samples(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_sample_rate(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_channels(frame: *const AVFrame) -> c_int;
    fn gilder_audio_output_alloc() -> *mut GilderAudioOutput;
    fn gilder_audio_output_free(output: *mut *mut GilderAudioOutput);
    fn gilder_audio_output_write_frame(
        output: *mut GilderAudioOutput,
        codec_ctx: *mut AVCodecContext,
        frame: *const AVFrame,
        samples_written: *mut c_longlong,
        bytes_written: *mut c_longlong,
        sample_rate: *mut c_int,
        channels: *mut c_int,
        write_calls: *mut c_longlong,
        write_waits: *mut c_longlong,
        process_callbacks: *mut c_longlong,
        buffer_errors: *mut c_longlong,
        timeout_errors: *mut c_longlong,
        stream_ready: *mut c_int,
    ) -> c_int;
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegAudioClockReader {
    format: NativeVulkanFfmpegAudioFormatContext,
    input_packet: NativeVulkanFfmpegAudioReusablePacket,
    decoder: NativeVulkanFfmpegAudioDecoder,
    stream_index: c_int,
    time_base: AVRational,
    eos_count: u32,
    loop_count: u32,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegAudioClockReader {
    fn open(source: &PathBuf, output_mode: NativeVulkanAudioOutputMode) -> Result<Self, String> {
        let format = NativeVulkanFfmpegAudioFormatContext::open(source)?;
        let stream_index = unsafe { gilder_av_find_audio_stream(format.ptr.as_ptr()) };
        if stream_index < 0 {
            return Err(native_vulkan_audio_ffmpeg_error(
                stream_index,
                "av_find_best_stream/select audio stream",
            ));
        }
        let time_base = unsafe { gilder_av_stream_time_base(format.ptr.as_ptr(), stream_index) };
        let input_packet = NativeVulkanFfmpegAudioReusablePacket::new()?;
        let decoder = NativeVulkanFfmpegAudioDecoder::open(&format, stream_index, output_mode)?;
        Ok(Self {
            format,
            input_packet,
            decoder,
            stream_index,
            time_base,
            eos_count: 0,
            loop_count: 0,
        })
    }

    fn read_next_packet(
        &mut self,
        loop_on_eos: bool,
    ) -> Result<Option<NativeVulkanAudioClockPacket>, NativeVulkanError> {
        loop {
            let input = self.input_packet.as_mut_ptr();
            let read_ret = unsafe { gilder_av_read_frame(self.format.ptr.as_ptr(), input) };
            if read_ret == 0 {
                let packet_stream_index = unsafe { gilder_av_packet_stream_index(input) };
                if packet_stream_index != self.stream_index {
                    self.input_packet.unref();
                    continue;
                }
                let decoded = self.decoder.decode_packet(input)?;
                let packet_duration_ns = native_vulkan_audio_ffmpeg_duration_ns(
                    unsafe { gilder_av_packet_duration(input) },
                    self.time_base,
                );
                let packet = NativeVulkanAudioClockPacket {
                    serial: self.loop_count,
                    pts_ns: native_vulkan_audio_ffmpeg_timestamp_ns(
                        unsafe { gilder_av_packet_pts(input) },
                        self.time_base,
                    ),
                    duration_ns: packet_duration_ns.or_else(|| {
                        native_vulkan_audio_decoded_duration_ns(
                            decoded.decoded_samples,
                            decoded.sample_rate_hz,
                        )
                    }),
                    payload_bytes: native_vulkan_audio_ffmpeg_packet_size(input),
                    decoded_frames: decoded.decoded_frames,
                    decoded_samples: decoded.decoded_samples,
                    sample_rate_hz: decoded.sample_rate_hz.or(self.decoder.sample_rate_hz()),
                    channel_count: decoded.channel_count.or(self.decoder.channel_count()),
                    output_frames: decoded.output_frames,
                    output_samples: decoded.output_samples,
                    output_bytes: decoded.output_bytes,
                    output_sample_rate_hz: decoded.output_sample_rate_hz,
                    output_channel_count: decoded.output_channel_count,
                    output_write_calls: decoded.output_write_calls,
                    output_write_waits: decoded.output_write_waits,
                    output_process_callbacks: decoded.output_process_callbacks,
                    output_buffer_errors: decoded.output_buffer_errors,
                    output_timeout_errors: decoded.output_timeout_errors,
                    output_stream_ready: decoded.output_stream_ready,
                };
                self.input_packet.unref();
                return Ok(Some(packet));
            }
            self.input_packet.unref();

            if read_ret == unsafe { gilder_av_error_eof() } {
                self.eos_count = self.eos_count.saturating_add(1);
                if !loop_on_eos {
                    return Ok(None);
                }
                let ret = unsafe {
                    gilder_av_seek_stream_start(self.format.ptr.as_ptr(), self.stream_index)
                };
                if ret < 0 {
                    return Err(NativeVulkanError::Video(native_vulkan_audio_ffmpeg_error(
                        ret,
                        "av_seek_frame audio stream start",
                    )));
                }
                self.loop_count = self.loop_count.saturating_add(1);
                continue;
            }

            return Err(NativeVulkanError::Video(native_vulkan_audio_ffmpeg_error(
                read_ret,
                "av_read_frame audio clock",
            )));
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
#[derive(Debug, Clone, Copy, Default)]
struct NativeVulkanFfmpegAudioDecodedPacket {
    decoded_frames: u32,
    decoded_samples: u32,
    sample_rate_hz: Option<u32>,
    channel_count: Option<u32>,
    output_frames: u32,
    output_samples: u32,
    output_bytes: u64,
    output_sample_rate_hz: Option<u32>,
    output_channel_count: Option<u32>,
    output_write_calls: u64,
    output_write_waits: u64,
    output_process_callbacks: u64,
    output_buffer_errors: u64,
    output_timeout_errors: u64,
    output_stream_ready: bool,
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegAudioDecoder {
    context: NonNull<AVCodecContext>,
    frame: NonNull<AVFrame>,
    output: Option<NonNull<GilderAudioOutput>>,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegAudioDecoder {
    fn open(
        format: &NativeVulkanFfmpegAudioFormatContext,
        stream_index: c_int,
        output_mode: NativeVulkanAudioOutputMode,
    ) -> Result<Self, String> {
        let codec = unsafe { gilder_av_stream_decoder(format.ptr.as_ptr(), stream_index) };
        if codec.is_null() {
            return Err("FFmpeg audio decoder was not found for selected stream".to_owned());
        }

        let context = unsafe { gilder_avcodec_alloc_context3(codec) };
        let context = NonNull::new(context)
            .ok_or_else(|| "FFmpeg avcodec_alloc_context3 failed".to_owned())?;
        let result =
            (|| -> Result<(NonNull<AVFrame>, Option<NonNull<GilderAudioOutput>>), String> {
                let ret = unsafe {
                    gilder_avcodec_parameters_to_context_for_stream(
                        context.as_ptr(),
                        format.ptr.as_ptr(),
                        stream_index,
                    )
                };
                if ret < 0 {
                    return Err(native_vulkan_audio_ffmpeg_error(
                        ret,
                        "avcodec_parameters_to_context audio stream",
                    ));
                }
                let ret = unsafe { gilder_avcodec_open2(context.as_ptr(), codec) };
                if ret < 0 {
                    return Err(native_vulkan_audio_ffmpeg_error(
                        ret,
                        "avcodec_open2 audio stream",
                    ));
                }
                let frame = unsafe { gilder_av_frame_alloc() };
                let frame =
                    NonNull::new(frame).ok_or_else(|| "FFmpeg av_frame_alloc failed".to_owned())?;
                let output = if output_mode == NativeVulkanAudioOutputMode::Auto {
                    let output = unsafe { gilder_audio_output_alloc() };
                    match NonNull::new(output) {
                        Some(output) => Some(output),
                        None => {
                            let mut frame = frame.as_ptr();
                            unsafe {
                                gilder_av_frame_free(&mut frame);
                            }
                            return Err("PipeWire audio output allocation failed".to_owned());
                        }
                    }
                } else {
                    None
                };
                Ok((frame, output))
            })();

        match result {
            Ok((frame, output)) => Ok(Self {
                context,
                frame,
                output,
            }),
            Err(err) => {
                let mut ptr = context.as_ptr();
                unsafe {
                    gilder_avcodec_free_context(&mut ptr);
                }
                Err(err)
            }
        }
    }

    fn sample_rate_hz(&self) -> Option<u32> {
        native_vulkan_audio_positive_c_int(unsafe {
            gilder_avcodec_context_sample_rate(self.context.as_ptr())
        })
    }

    fn channel_count(&self) -> Option<u32> {
        native_vulkan_audio_positive_c_int(unsafe {
            gilder_avcodec_context_channels(self.context.as_ptr())
        })
    }

    fn decode_packet(
        &mut self,
        packet: *const AVPacket,
    ) -> Result<NativeVulkanFfmpegAudioDecodedPacket, NativeVulkanError> {
        let send_ret = unsafe { gilder_avcodec_send_packet(self.context.as_ptr(), packet) };
        if send_ret < 0 {
            return Err(NativeVulkanError::Video(native_vulkan_audio_ffmpeg_error(
                send_ret,
                "avcodec_send_packet audio stream",
            )));
        }
        self.receive_available_frames()
    }

    fn receive_available_frames(
        &mut self,
    ) -> Result<NativeVulkanFfmpegAudioDecodedPacket, NativeVulkanError> {
        let mut decoded = NativeVulkanFfmpegAudioDecodedPacket::default();
        loop {
            let receive_ret =
                unsafe { gilder_avcodec_receive_frame(self.context.as_ptr(), self.frame.as_ptr()) };
            if receive_ret == 0 {
                decoded.decoded_frames = decoded.decoded_frames.saturating_add(1);
                let samples = native_vulkan_audio_positive_c_int(unsafe {
                    gilder_av_frame_nb_samples(self.frame.as_ptr())
                })
                .unwrap_or(0);
                decoded.decoded_samples = decoded.decoded_samples.saturating_add(samples);
                if decoded.sample_rate_hz.is_none() {
                    decoded.sample_rate_hz = native_vulkan_audio_positive_c_int(unsafe {
                        gilder_av_frame_sample_rate(self.frame.as_ptr())
                    })
                    .or_else(|| self.sample_rate_hz());
                }
                if decoded.channel_count.is_none() {
                    decoded.channel_count = native_vulkan_audio_positive_c_int(unsafe {
                        gilder_av_frame_channels(self.frame.as_ptr())
                    })
                    .or_else(|| self.channel_count());
                }
                let output_result = self.write_output_frame(&mut decoded);
                unsafe {
                    gilder_av_frame_unref(self.frame.as_ptr());
                }
                output_result?;
                continue;
            }
            unsafe {
                gilder_av_frame_unref(self.frame.as_ptr());
            }
            if receive_ret == unsafe { gilder_av_error_again() }
                || receive_ret == unsafe { gilder_av_error_eof() }
            {
                return Ok(decoded);
            }
            return Err(NativeVulkanError::Video(native_vulkan_audio_ffmpeg_error(
                receive_ret,
                "avcodec_receive_frame audio stream",
            )));
        }
    }

    fn write_output_frame(
        &mut self,
        decoded: &mut NativeVulkanFfmpegAudioDecodedPacket,
    ) -> Result<(), NativeVulkanError> {
        let Some(output) = self.output else {
            return Ok(());
        };
        let mut samples_written: c_longlong = 0;
        let mut bytes_written: c_longlong = 0;
        let mut sample_rate: c_int = 0;
        let mut channels: c_int = 0;
        let mut write_calls: c_longlong = 0;
        let mut write_waits: c_longlong = 0;
        let mut process_callbacks: c_longlong = 0;
        let mut buffer_errors: c_longlong = 0;
        let mut timeout_errors: c_longlong = 0;
        let mut stream_ready: c_int = 0;
        let ret = unsafe {
            gilder_audio_output_write_frame(
                output.as_ptr(),
                self.context.as_ptr(),
                self.frame.as_ptr(),
                &mut samples_written,
                &mut bytes_written,
                &mut sample_rate,
                &mut channels,
                &mut write_calls,
                &mut write_waits,
                &mut process_callbacks,
                &mut buffer_errors,
                &mut timeout_errors,
                &mut stream_ready,
            )
        };
        if ret < 0 {
            return Err(NativeVulkanError::Video(native_vulkan_audio_ffmpeg_error(
                ret,
                "PipeWire audio output write frame",
            )));
        }

        let output_bytes = native_vulkan_audio_positive_c_longlong_u64(bytes_written);
        if output_bytes > 0 {
            decoded.output_frames = decoded.output_frames.saturating_add(1);
            decoded.output_samples = decoded
                .output_samples
                .saturating_add(native_vulkan_audio_positive_c_longlong_u32(samples_written));
            decoded.output_bytes = decoded.output_bytes.saturating_add(output_bytes);
        }
        if decoded.output_sample_rate_hz.is_none() {
            decoded.output_sample_rate_hz = native_vulkan_audio_positive_c_int(sample_rate);
        }
        if decoded.output_channel_count.is_none() {
            decoded.output_channel_count = native_vulkan_audio_positive_c_int(channels);
        }
        decoded.output_write_calls = decoded
            .output_write_calls
            .max(native_vulkan_audio_positive_c_longlong_u64(write_calls));
        decoded.output_write_waits = decoded
            .output_write_waits
            .max(native_vulkan_audio_positive_c_longlong_u64(write_waits));
        decoded.output_process_callbacks =
            decoded
                .output_process_callbacks
                .max(native_vulkan_audio_positive_c_longlong_u64(
                    process_callbacks,
                ));
        decoded.output_buffer_errors = decoded
            .output_buffer_errors
            .max(native_vulkan_audio_positive_c_longlong_u64(buffer_errors));
        decoded.output_timeout_errors = decoded
            .output_timeout_errors
            .max(native_vulkan_audio_positive_c_longlong_u64(timeout_errors));
        decoded.output_stream_ready |= stream_ready != 0;
        Ok(())
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegAudioDecoder {
    fn drop(&mut self) {
        if let Some(output) = self.output {
            let mut output = output.as_ptr();
            unsafe {
                gilder_audio_output_free(&mut output);
            }
        }
        let mut frame = self.frame.as_ptr();
        let mut context = self.context.as_ptr();
        unsafe {
            gilder_av_frame_free(&mut frame);
            gilder_avcodec_free_context(&mut context);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegAudioFormatContext {
    ptr: NonNull<AVFormatContext>,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegAudioFormatContext {
    fn open(source: &PathBuf) -> Result<Self, String> {
        let source = CString::new(source.as_os_str().as_bytes())
            .map_err(|_| "FFmpeg audio source path contains an interior NUL".to_owned())?;
        let mut ctx = ptr::null_mut();
        let ret = unsafe { gilder_avformat_open_input(&mut ctx, source.as_ptr()) };
        if ret < 0 {
            return Err(native_vulkan_audio_ffmpeg_error(ret, "avformat_open_input"));
        }
        let ptr = NonNull::new(ctx)
            .ok_or_else(|| "FFmpeg avformat_open_input returned null".to_owned())?;
        Ok(Self { ptr })
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegAudioFormatContext {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        unsafe {
            gilder_avformat_close_input(&mut ptr);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegAudioReusablePacket {
    packet: NonNull<AVPacket>,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegAudioReusablePacket {
    fn new() -> Result<Self, String> {
        let packet = unsafe { gilder_av_packet_alloc() };
        let packet =
            NonNull::new(packet).ok_or_else(|| "FFmpeg av_packet_alloc failed".to_owned())?;
        Ok(Self { packet })
    }

    fn as_mut_ptr(&mut self) -> *mut AVPacket {
        self.packet.as_ptr()
    }

    fn unref(&mut self) {
        unsafe {
            gilder_av_packet_unref(self.packet.as_ptr());
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
impl Drop for NativeVulkanFfmpegAudioReusablePacket {
    fn drop(&mut self) {
        let mut packet = self.packet.as_ptr();
        unsafe {
            gilder_av_packet_free(&mut packet);
        }
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_ffmpeg_packet_size(packet: *const AVPacket) -> u32 {
    let size = unsafe { gilder_av_packet_size(packet) };
    if size <= 0 { 0 } else { size as u32 }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_positive_c_int(value: c_int) -> Option<u32> {
    if value > 0 { Some(value as u32) } else { None }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_positive_c_longlong_u32(value: c_longlong) -> u32 {
    if value <= 0 {
        0
    } else {
        (value as u64).min(u64::from(u32::MAX)) as u32
    }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_positive_c_longlong_u64(value: c_longlong) -> u64 {
    if value <= 0 { 0 } else { value as u64 }
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_decoded_duration_ns(
    decoded_samples: u32,
    sample_rate_hz: Option<u32>,
) -> Option<u64> {
    let sample_rate_hz = u64::from(sample_rate_hz?);
    if decoded_samples == 0 || sample_rate_hz == 0 {
        return None;
    }
    Some(u64::from(decoded_samples).saturating_mul(1_000_000_000) / sample_rate_hz)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_ffmpeg_timestamp_ns(value: i64, time_base: AVRational) -> Option<u64> {
    if value == unsafe { gilder_av_nopts_value() } || value < 0 {
        return None;
    }
    native_vulkan_audio_ffmpeg_rescale_to_ns(value, time_base)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_ffmpeg_duration_ns(value: i64, time_base: AVRational) -> Option<u64> {
    if value <= 0 {
        return None;
    }
    native_vulkan_audio_ffmpeg_rescale_to_ns(value, time_base)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_ffmpeg_rescale_to_ns(value: i64, time_base: AVRational) -> Option<u64> {
    let den = NonZeroI32::new(time_base.den)?;
    let scaled =
        i128::from(value) * i128::from(time_base.num) * 1_000_000_000i128 / i128::from(den.get());
    if scaled < 0 {
        return None;
    }
    Some(scaled.min(i128::from(u64::MAX)) as u64)
}

#[cfg(feature = "native-vulkan-video")]
fn native_vulkan_audio_ffmpeg_error(ret: c_int, action: &str) -> String {
    let mut buffer = [0 as c_char; 256];
    unsafe {
        let _ = gilder_av_strerror(ret, buffer.as_mut_ptr(), buffer.len());
    }
    let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    format!("FFmpeg audio clock {action} failed: {message} ({ret})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_clock_advances_from_pts_and_duration() {
        let mut clock = NativeVulkanAudioClock::new();

        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(1_000_000_000),
                duration_ns: Some(20_000_000),
                payload_bytes: 128,
                decoded_frames: 0,
                decoded_samples: 0,
                sample_rate_hz: None,
                channel_count: None,
                ..NativeVulkanAudioClockPacket::default()
            }),
            Some(20_000_000)
        );
        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(1_020_000_000),
                duration_ns: Some(20_000_000),
                payload_bytes: 128,
                decoded_frames: 0,
                decoded_samples: 0,
                sample_rate_hz: None,
                channel_count: None,
                ..NativeVulkanAudioClockPacket::default()
            }),
            Some(40_000_000)
        );
    }

    #[test]
    fn audio_clock_serial_reset_rebases_loop_without_reusing_stale_packets() {
        let mut clock = NativeVulkanAudioClock::new();
        clock.advance(NativeVulkanAudioClockPacket {
            serial: 0,
            pts_ns: Some(5_000_000_000),
            duration_ns: Some(10_000_000),
            payload_bytes: 64,
            decoded_frames: 0,
            decoded_samples: 0,
            sample_rate_hz: None,
            channel_count: None,
            ..NativeVulkanAudioClockPacket::default()
        });

        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 1,
                pts_ns: Some(5_000_000_000),
                duration_ns: Some(10_000_000),
                payload_bytes: 64,
                decoded_frames: 0,
                decoded_samples: 0,
                sample_rate_hz: None,
                channel_count: None,
                ..NativeVulkanAudioClockPacket::default()
            }),
            Some(20_000_000)
        );
        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(5_010_000_000),
                duration_ns: Some(10_000_000),
                payload_bytes: 64,
                decoded_frames: 0,
                decoded_samples: 0,
                sample_rate_hz: None,
                channel_count: None,
                ..NativeVulkanAudioClockPacket::default()
            }),
            Some(20_000_000)
        );
        assert_eq!(clock.stale_dropped_packets, 1);
    }

    #[test]
    fn audio_packet_queue_is_bounded_and_retains_no_payload_bytes() {
        let mut queue = NativeVulkanAudioClockPacketQueue::new(1);
        queue.push(NativeVulkanAudioClockPacket {
            serial: 0,
            pts_ns: Some(0),
            duration_ns: Some(1_000_000),
            payload_bytes: 128,
            decoded_frames: 0,
            decoded_samples: 0,
            sample_rate_hz: None,
            channel_count: None,
            ..NativeVulkanAudioClockPacket::default()
        });
        queue.push(NativeVulkanAudioClockPacket {
            serial: 0,
            pts_ns: Some(1_000_000),
            duration_ns: Some(1_000_000),
            payload_bytes: 256,
            decoded_frames: 0,
            decoded_samples: 0,
            sample_rate_hz: None,
            channel_count: None,
            ..NativeVulkanAudioClockPacket::default()
        });

        assert_eq!(queue.queued_packets(), 1);
        assert_eq!(queue.overflow_dropped_packets, 1);
        assert_eq!(queue.retained_payload_bytes(), 0);
        assert_eq!(queue.max_payload_bytes, 256);
    }

    #[test]
    fn audio_runtime_snapshot_reports_clock_only_boundary() {
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::ClockOnly,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        );
        runtime.set_audio_stream(2);
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(42_000_000),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.output_mode, "clock-only");
        assert!(snapshot.audio_stream_found);
        assert_eq!(snapshot.audio_stream_index, Some(2));
        assert!(!snapshot.audible_output_started);
        assert_eq!(snapshot.audio_output_backend, "none");
        assert_eq!(snapshot.audio_output_sample_format, "none");
        assert_eq!(snapshot.audio_output_frames, 0);
        assert_eq!(snapshot.audio_output_samples, 0);
        assert_eq!(snapshot.audio_output_bytes, 0);
        assert_eq!(snapshot.audio_output_sample_rate_hz, None);
        assert_eq!(snapshot.audio_output_channel_count, None);
        assert_eq!(snapshot.audio_output_write_calls, 0);
        assert_eq!(snapshot.audio_output_write_waits, 0);
        assert_eq!(snapshot.audio_output_process_callbacks, 0);
        assert_eq!(snapshot.audio_output_buffer_errors, 0);
        assert_eq!(snapshot.audio_output_timeout_errors, 0);
        assert!(!snapshot.audio_output_stream_ready);
        assert_eq!(snapshot.retained_payload_bytes, 0);
        assert_eq!(snapshot.retained_pcm_frame_bytes, 0);
        assert_eq!(snapshot.decoded_frames, 1);
        assert_eq!(snapshot.decoded_samples, 1008);
        assert_eq!(snapshot.audio_sample_rate_hz, Some(48_000));
        assert_eq!(snapshot.audio_channel_count, Some(2));
        assert_eq!(snapshot.clock_ns, Some(21_000_000));
        assert!(snapshot.video_master_clock_ready);
        assert_eq!(snapshot.video_master_start_clock_ns, Some(21_000_000));
        assert_eq!(snapshot.video_master_start_serial, Some(0));
        assert_eq!(snapshot.video_master_start_packet_index, Some(0));
        assert_eq!(snapshot.current_serial_start_clock_ns, Some(21_000_000));
        assert_eq!(snapshot.current_serial_start_serial, Some(0));
        assert_eq!(snapshot.current_serial_start_packet_index, Some(0));
        assert_eq!(snapshot.packets_head.len(), 1);
        assert_eq!(snapshot.packets_head[0].decoded_frames, 1);
        assert_eq!(snapshot.packets_head[0].decoded_samples, 1008);
    }

    #[test]
    fn audio_runtime_snapshot_reports_pipewire_output_boundary() {
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::Auto,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        );
        runtime.set_audio_stream(2);
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(42_000_000),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                output_frames: 1,
                output_samples: 1008,
                output_bytes: 4032,
                output_sample_rate_hz: Some(48_000),
                output_channel_count: Some(2),
                output_write_calls: 1,
                output_write_waits: 1,
                output_process_callbacks: 1,
                output_buffer_errors: 0,
                output_timeout_errors: 0,
                output_stream_ready: true,
            },
        );

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.output_mode, "auto");
        assert!(snapshot.audible_output_started);
        assert_eq!(snapshot.audio_output_backend, "pipewire-s16le");
        assert_eq!(snapshot.audio_output_sample_format, "s16le-interleaved");
        assert_eq!(snapshot.audio_output_frames, 1);
        assert_eq!(snapshot.audio_output_samples, 1008);
        assert_eq!(snapshot.audio_output_bytes, 4032);
        assert_eq!(snapshot.audio_output_sample_rate_hz, Some(48_000));
        assert_eq!(snapshot.audio_output_channel_count, Some(2));
        assert_eq!(snapshot.audio_output_write_calls, 1);
        assert_eq!(snapshot.audio_output_write_waits, 1);
        assert_eq!(snapshot.audio_output_process_callbacks, 1);
        assert_eq!(snapshot.audio_output_buffer_errors, 0);
        assert_eq!(snapshot.audio_output_timeout_errors, 0);
        assert!(snapshot.audio_output_stream_ready);
    }

    #[test]
    fn audio_runtime_reports_playback_target_coverage() {
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::Auto,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        );
        runtime.set_audio_stream(2);
        runtime.set_playback_target_clock_ns(Some(42_000_000));
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(0),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                output_frames: 1,
                output_samples: 1008,
                output_bytes: 4032,
                output_sample_rate_hz: Some(48_000),
                output_channel_count: Some(2),
                output_write_calls: 1,
                output_write_waits: 1,
                output_process_callbacks: 1,
                output_buffer_errors: 0,
                output_timeout_errors: 0,
                output_stream_ready: true,
            },
        );

        let partial = runtime.snapshot();
        assert_eq!(partial.playback_target_clock_ns, Some(42_000_000));
        assert_eq!(partial.playback_covered_clock_ns, Some(21_000_000));
        assert_eq!(partial.playback_coverage_percent, 50);
        assert!(!partial.playback_target_reached);

        runtime.push_and_advance(
            1,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(21_000_000),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                output_frames: 1,
                output_samples: 1008,
                output_bytes: 4032,
                output_sample_rate_hz: Some(48_000),
                output_channel_count: Some(2),
                output_write_calls: 2,
                output_write_waits: 2,
                output_process_callbacks: 2,
                output_buffer_errors: 0,
                output_timeout_errors: 0,
                output_stream_ready: true,
            },
        );

        let covered = runtime.snapshot();
        assert_eq!(covered.playback_covered_clock_ns, Some(42_000_000));
        assert_eq!(covered.playback_coverage_percent, 100);
        assert!(covered.playback_target_reached);
    }

    #[test]
    fn audio_runtime_video_master_start_uses_first_ready_clock_sample() {
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::ClockOnly,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        );
        runtime.set_audio_stream(2);
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: None,
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );
        runtime.push_and_advance(
            1,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(0),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );
        runtime.push_and_advance(
            2,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(21_000_000),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.clock_ns, Some(42_000_000));
        assert_eq!(snapshot.video_master_start_clock_ns, Some(21_000_000));
        assert_eq!(snapshot.video_master_start_serial, Some(0));
        assert_eq!(snapshot.video_master_start_packet_index, Some(1));
        assert_eq!(snapshot.current_serial_start_clock_ns, Some(21_000_000));
        assert_eq!(snapshot.current_serial_start_serial, Some(0));
        assert_eq!(snapshot.current_serial_start_packet_index, Some(1));
    }

    #[test]
    fn audio_runtime_current_serial_start_resets_on_new_serial() {
        let mut runtime = NativeVulkanAudioClockRuntime::new(
            NativeVulkanAudioOutputMode::ClockOnly,
            NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
        );
        runtime.set_audio_stream(2);
        runtime.push_and_advance(
            0,
            NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(0),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );
        runtime.push_and_advance(
            3,
            NativeVulkanAudioClockPacket {
                serial: 1,
                pts_ns: Some(0),
                duration_ns: Some(21_000_000),
                payload_bytes: 512,
                decoded_frames: 1,
                decoded_samples: 1008,
                sample_rate_hz: Some(48_000),
                channel_count: Some(2),
                ..NativeVulkanAudioClockPacket::default()
            },
        );

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.clock_ns, Some(42_000_000));
        assert_eq!(snapshot.video_master_start_clock_ns, Some(21_000_000));
        assert_eq!(snapshot.video_master_start_serial, Some(0));
        assert_eq!(snapshot.video_master_start_packet_index, Some(0));
        assert_eq!(snapshot.current_serial_start_clock_ns, Some(42_000_000));
        assert_eq!(snapshot.current_serial_start_serial, Some(1));
        assert_eq!(snapshot.current_serial_start_packet_index, Some(3));
    }

    #[test]
    fn unattached_audio_clock_is_not_a_video_master() {
        let snapshot =
            native_vulkan_unattached_audio_clock_snapshot(NativeVulkanAudioOutputMode::ClockOnly);

        assert!(!snapshot.audio_stream_found);
        assert!(!snapshot.video_master_clock_ready);
        assert_eq!(snapshot.video_master_start_clock_ns, None);
        assert_eq!(snapshot.video_master_start_serial, None);
        assert_eq!(snapshot.video_master_start_packet_index, None);
        assert_eq!(snapshot.current_serial_start_clock_ns, None);
        assert_eq!(snapshot.current_serial_start_serial, None);
        assert_eq!(snapshot.current_serial_start_packet_index, None);
    }
}
