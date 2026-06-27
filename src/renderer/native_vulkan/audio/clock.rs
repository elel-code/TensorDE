use std::collections::VecDeque;
#[cfg(feature = "native-vulkan-video")]
use std::ffi::{CStr, CString};
use std::path::PathBuf;
#[cfg(feature = "native-vulkan-video")]
use std::ptr::{self, NonNull};

#[cfg(feature = "native-vulkan-video")]
use std::num::NonZeroI32;
#[cfg(feature = "native-vulkan-video")]
use std::os::raw::{c_char, c_int};
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
}

impl NativeVulkanAudioClockProbeOptions {
    pub(in crate::renderer::native_vulkan) fn clock_only(source: PathBuf) -> Self {
        Self {
            source,
            output_mode: NativeVulkanAudioOutputMode::ClockOnly,
            queue_capacity: NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS,
            packets_to_probe: NATIVE_VULKAN_AUDIO_CLOCK_QUEUE_PACKETS as u32,
            loop_on_eos: false,
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
    pub clock_ns: Option<u64>,
    pub clock_ms: Option<u64>,
    pub last_packet_pts_ns: Option<u64>,
    pub last_packet_duration_ns: Option<u64>,
    pub retained_payload_bytes: u64,
    pub max_payload_bytes: u64,
    pub packets_head: Vec<NativeVulkanAudioClockPacketSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanAudioClockPacket {
    pub(in crate::renderer::native_vulkan) serial: u32,
    pub(in crate::renderer::native_vulkan) pts_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan) duration_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan) payload_bytes: u32,
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
    eos_count: u32,
    loop_count: u32,
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
            eos_count: 0,
            loop_count: 0,
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

    pub(in crate::renderer::native_vulkan) fn push_and_advance(
        &mut self,
        packet_index: u32,
        packet: NativeVulkanAudioClockPacket,
    ) {
        self.queue.push(packet);
        while let Some(packet) = self.queue.pop() {
            self.clock.advance(packet);
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
                    });
            }
        }
    }

    pub(in crate::renderer::native_vulkan) fn snapshot(
        &self,
    ) -> NativeVulkanAudioClockRuntimeSnapshot {
        NativeVulkanAudioClockRuntimeSnapshot {
            route: "native-vulkan-audio-clock-only",
            boundary: "FFmpeg audio demux metadata -> serial-scoped muted audio clock -> video pacing master input",
            output_mode: self.output_mode.as_str(),
            source: self.source.clone(),
            audio_stream_found: self.audio_stream_found,
            audio_stream_index: self.audio_stream_index,
            audio_stream_error: self.audio_stream_error.clone(),
            ffmpeg_reference: FFMPEG_AUDIO_CLOCK_REFERENCE,
            queue_policy: AUDIO_CLOCK_QUEUE_POLICY,
            clock_model: AUDIO_CLOCK_MODEL,
            audible_output_started: self.audible_output_started,
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
            clock_ns: self.clock.clock_ns,
            clock_ms: self.clock.clock_ns.map(|clock| clock / 1_000_000),
            last_packet_pts_ns: self.clock.last_packet_pts_ns,
            last_packet_duration_ns: self.clock.last_packet_duration_ns,
            retained_payload_bytes: self.queue.retained_payload_bytes(),
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
    let mut reader = match NativeVulkanFfmpegAudioClockReader::open(&options.source) {
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
struct AVRational {
    num: c_int,
    den: c_int,
}

#[cfg(feature = "native-vulkan-video")]
unsafe extern "C" {
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
}

#[cfg(feature = "native-vulkan-video")]
struct NativeVulkanFfmpegAudioClockReader {
    format: NativeVulkanFfmpegAudioFormatContext,
    input_packet: NativeVulkanFfmpegAudioReusablePacket,
    stream_index: c_int,
    time_base: AVRational,
    eos_count: u32,
    loop_count: u32,
}

#[cfg(feature = "native-vulkan-video")]
impl NativeVulkanFfmpegAudioClockReader {
    fn open(source: &PathBuf) -> Result<Self, String> {
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
        Ok(Self {
            format,
            input_packet,
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
                let packet = NativeVulkanAudioClockPacket {
                    serial: self.loop_count,
                    pts_ns: native_vulkan_audio_ffmpeg_timestamp_ns(
                        unsafe { gilder_av_packet_pts(input) },
                        self.time_base,
                    ),
                    duration_ns: native_vulkan_audio_ffmpeg_duration_ns(
                        unsafe { gilder_av_packet_duration(input) },
                        self.time_base,
                    ),
                    payload_bytes: native_vulkan_audio_ffmpeg_packet_size(input),
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
            }),
            Some(20_000_000)
        );
        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(1_020_000_000),
                duration_ns: Some(20_000_000),
                payload_bytes: 128,
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
        });

        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 1,
                pts_ns: Some(5_000_000_000),
                duration_ns: Some(10_000_000),
                payload_bytes: 64,
            }),
            Some(20_000_000)
        );
        assert_eq!(
            clock.advance(NativeVulkanAudioClockPacket {
                serial: 0,
                pts_ns: Some(5_010_000_000),
                duration_ns: Some(10_000_000),
                payload_bytes: 64,
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
        });
        queue.push(NativeVulkanAudioClockPacket {
            serial: 0,
            pts_ns: Some(1_000_000),
            duration_ns: Some(1_000_000),
            payload_bytes: 256,
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
            },
        );

        let snapshot = runtime.snapshot();

        assert_eq!(snapshot.output_mode, "clock-only");
        assert!(snapshot.audio_stream_found);
        assert_eq!(snapshot.audio_stream_index, Some(2));
        assert!(!snapshot.audible_output_started);
        assert_eq!(snapshot.retained_payload_bytes, 0);
        assert_eq!(snapshot.clock_ns, Some(21_000_000));
        assert_eq!(snapshot.packets_head.len(), 1);
    }
}
