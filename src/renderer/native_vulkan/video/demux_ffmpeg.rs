//! FFmpeg packet frontend for the native Vulkan demux boundary.
//!
//! This follows the local FFmpeg ownership shape:
//! `references/ffmpeg/fftools/ffplay.c:420-456` moves AVPacket ownership into a
//! bounded PacketQueue. The FFmpeg read worker hands packets directly to the
//! renderer-owned packet queue instead of adding a second compressed-payload
//! FIFO; the renderer's main packet/frame windows stay capped at
//! `VIDEO_PICTURE_QUEUE_SIZE=3`. The read worker is request-driven with a
//! zero-capacity request/response handoff, so it never keeps a hidden
//! next-packet payload while the renderer is waiting on present pacing. This preserves
//! `references/ffmpeg/fftools/ffplay.c:3132-3141` read-thread backpressure,
//! `references/ffmpeg/fftools/ffplay.c:3154-3215` target-stream filtering, and
//! `references/ffmpeg/fftools/ffplay.c:534-642` has the decoder move one packet
//! out of PacketQueue before send/unref.
//!
//! The frontend also skips `avformat_find_stream_info()`: `ffplay` exposes that
//! probe behind the `find_stream_info` option (`references/ffmpeg/fftools/ffplay.c:2938-2954`)
//! and then selects streams from `codecpar` (`references/ffmpeg/fftools/ffplay.c:3001-3044`).
//! Our MP4/native-video path needs codecpar/extradata for packet normalization
//! and then reads packets directly, so retaining probe/decoder scratch is not
//! part of the hot renderer boundary.
//!
//! H.264 and H.265 MP4 packets are normalized locally from length-prefixed
//! AVCC/HVCC into Annex-B using the same source rules as FFmpeg's
//! `references/ffmpeg/libavcodec/bsf/h264_mp4toannexb.c` and
//! `references/ffmpeg/libavcodec/bsf/hevc_mp4toannexb.c`, but without keeping
//! FFmpeg BSF output AVPackets alive. AV1 follows
//! `references/ffmpeg/libavcodec/av1dec.c:1456-1474`: container packets are read
//! directly as OBU packets instead of being forced through the raw AV1 demuxer's
//! `av1_frame_merge` Temporal Delimiter contract.

use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroI32;
use std::ops::Range;
use std::os::raw::{c_char, c_int, c_uchar};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use super::super::NativeVulkanError;
use super::demux::{
    NativeVulkanStreamingAccessUnit, NativeVulkanStreamingPacketFrontend,
    NativeVulkanStreamingPacketQueue, native_vulkan_start_streaming_packet_queue_from_frontend,
};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVFormatContext {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVPacket {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVRational {
    num: c_int,
    den: c_int,
}

unsafe extern "C" {
    fn gilder_av_error_eof() -> c_int;
    fn gilder_av_nopts_value() -> i64;
    fn gilder_av_codec_id_h264() -> c_int;
    fn gilder_av_codec_id_hevc() -> c_int;
    fn gilder_av_codec_id_av1() -> c_int;
    fn gilder_av_strerror(errnum: c_int, errbuf: *mut c_char, errbuf_size: usize) -> c_int;
    fn gilder_avformat_open_input(ctx: *mut *mut AVFormatContext, url: *const c_char) -> c_int;
    fn gilder_avformat_close_input(ctx: *mut *mut AVFormatContext);
    fn gilder_av_find_video_stream_for_codec(ctx: *mut AVFormatContext, codec_id: c_int) -> c_int;
    fn gilder_av_packet_alloc() -> *mut AVPacket;
    fn gilder_av_packet_free(packet: *mut *mut AVPacket);
    fn gilder_av_packet_unref(packet: *mut AVPacket);
    fn gilder_av_read_frame(ctx: *mut AVFormatContext, packet: *mut AVPacket) -> c_int;
    fn gilder_av_packet_stream_index(packet: *const AVPacket) -> c_int;
    fn gilder_av_packet_data(packet: *const AVPacket) -> *const c_uchar;
    fn gilder_av_packet_size(packet: *const AVPacket) -> c_int;
    fn gilder_av_packet_pts(packet: *const AVPacket) -> i64;
    fn gilder_av_packet_duration(packet: *const AVPacket) -> i64;
    fn gilder_av_stream_extradata(ctx: *mut AVFormatContext, stream_index: c_int)
    -> *const c_uchar;
    fn gilder_av_stream_extradata_size(ctx: *mut AVFormatContext, stream_index: c_int) -> c_int;
    fn gilder_av_stream_time_base(ctx: *mut AVFormatContext, stream_index: c_int) -> AVRational;
    fn gilder_av_seek_stream_start(ctx: *mut AVFormatContext, stream_index: c_int) -> c_int;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanFfmpegCodec {
    H264,
    H265,
    Av1,
}

impl NativeVulkanFfmpegCodec {
    fn codec_id(self) -> c_int {
        unsafe {
            match self {
                Self::H264 => gilder_av_codec_id_h264(),
                Self::H265 => gilder_av_codec_id_hevc(),
                Self::Av1 => gilder_av_codec_id_av1(),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanFfmpegPacketMetadata {
    pub(in crate::renderer::native_vulkan) pts_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan) duration_ns: Option<u64>,
    pub(in crate::renderer::native_vulkan) pts_ms: Option<u64>,
    pub(in crate::renderer::native_vulkan) duration_ms: Option<u64>,
}

pub(in crate::renderer::native_vulkan) trait NativeVulkanFfmpegStreamingAccessUnit:
    NativeVulkanStreamingAccessUnit
{
    const FFMPEG_CODEC: NativeVulkanFfmpegCodec;
    const FFMPEG_READ_THREAD_HANDOFF_PACKETS: usize = 0;
    const FFMPEG_PACKET_SPLITS_ACCESS_UNITS: bool = false;

    fn from_ffmpeg_packet(
        payload: NativeVulkanFfmpegPacketPayload,
        metadata: NativeVulkanFfmpegPacketMetadata,
    ) -> Result<Self, NativeVulkanError>;

    fn from_ffmpeg_packet_many(
        payload: NativeVulkanFfmpegPacketPayload,
        metadata: NativeVulkanFfmpegPacketMetadata,
    ) -> Result<Vec<Self>, NativeVulkanError> {
        Ok(vec![Self::from_ffmpeg_packet(payload, metadata)?])
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_start_ffmpeg_streaming_packet_queue<
    A: NativeVulkanFfmpegStreamingAccessUnit + Send + 'static,
>(
    source: &Path,
    capacity: usize,
) -> Result<NativeVulkanStreamingPacketQueue<A>, NativeVulkanError> {
    let frontend = NativeVulkanFfmpegStreamingPacketFrontend::<A>::new(source, capacity)?;
    native_vulkan_start_streaming_packet_queue_from_frontend(Box::new(frontend), capacity)
}

// The FFmpeg PacketQueue depth remains three, but free converted Annex-B scratch
// follows av_packet_unref lifetime: keep one reusable buffer and release extras.
const NATIVE_VULKAN_FFMPEG_POOLED_PAYLOAD_BUFFERS: usize = 1;
const NATIVE_VULKAN_FFMPEG_MAX_RETAINED_PAYLOAD_CAPACITY: usize = 160 * 1024;
const NATIVE_VULKAN_ANNEXB_START_CODE: [u8; 4] = [0, 0, 0, 1];

pub struct NativeVulkanFfmpegPacketPayload {
    storage: NativeVulkanFfmpegPacketPayloadStorage,
}

enum NativeVulkanFfmpegPacketPayloadStorage {
    AvPacket(NonNull<AVPacket>),
    Pooled(NativeVulkanFfmpegPooledPacketPayload),
    SharedSlice(NativeVulkanFfmpegSharedPacketPayload),
}

struct NativeVulkanFfmpegPooledPacketPayload {
    bytes: Vec<u8>,
    pool: Arc<NativeVulkanFfmpegPacketPayloadPool>,
}

struct NativeVulkanFfmpegSharedPacketPayload {
    backing: Arc<NativeVulkanFfmpegPacketPayload>,
    range: Range<usize>,
}

#[derive(Default)]
struct NativeVulkanFfmpegPacketPayloadPool {
    buffers: Mutex<Vec<Vec<u8>>>,
}

impl NativeVulkanFfmpegPacketPayloadPool {
    fn take(&self, capacity: usize) -> Vec<u8> {
        let mut buffers = self.buffers.lock().unwrap_or_else(|err| err.into_inner());
        let buffer = buffers
            .iter()
            .position(|buffer| buffer.capacity() >= capacity)
            .map(|index| buffers.swap_remove(index));
        let mut buffer = buffer.unwrap_or_else(|| Vec::with_capacity(capacity));
        buffer.clear();
        buffer
    }

    fn recycle(&self, mut bytes: Vec<u8>) {
        if bytes.capacity() > NATIVE_VULKAN_FFMPEG_MAX_RETAINED_PAYLOAD_CAPACITY {
            return;
        }
        bytes.clear();
        let mut buffers = self.buffers.lock().unwrap_or_else(|err| err.into_inner());
        if buffers.len() < NATIVE_VULKAN_FFMPEG_POOLED_PAYLOAD_BUFFERS {
            buffers.push(bytes);
        }
    }
}

impl Drop for NativeVulkanFfmpegPooledPacketPayload {
    fn drop(&mut self) {
        let bytes = std::mem::take(&mut self.bytes);
        self.pool.recycle(bytes);
    }
}

unsafe impl Send for NativeVulkanFfmpegPacketPayload {}

impl NativeVulkanFfmpegPacketPayload {
    fn from_raw(packet: *mut AVPacket) -> Result<Self, NativeVulkanError> {
        let packet = NonNull::new(packet).ok_or_else(|| {
            NativeVulkanError::Video("FFmpeg produced a null AVPacket".to_owned())
        })?;
        Ok(Self {
            storage: NativeVulkanFfmpegPacketPayloadStorage::AvPacket(packet),
        })
    }

    fn from_pooled(bytes: Vec<u8>, pool: Arc<NativeVulkanFfmpegPacketPayloadPool>) -> Self {
        Self {
            storage: NativeVulkanFfmpegPacketPayloadStorage::Pooled(
                NativeVulkanFfmpegPooledPacketPayload { bytes, pool },
            ),
        }
    }

    pub(in crate::renderer::native_vulkan) fn bytes(&self) -> &[u8] {
        match &self.storage {
            NativeVulkanFfmpegPacketPayloadStorage::AvPacket(packet) => {
                let size = unsafe { gilder_av_packet_size(packet.as_ptr()) };
                if size <= 0 {
                    return &[];
                }
                let data = unsafe { gilder_av_packet_data(packet.as_ptr()) };
                if data.is_null() {
                    return &[];
                }
                unsafe { slice::from_raw_parts(data.cast::<u8>(), size as usize) }
            }
            NativeVulkanFfmpegPacketPayloadStorage::Pooled(payload) => &payload.bytes,
            NativeVulkanFfmpegPacketPayloadStorage::SharedSlice(payload) => payload
                .backing
                .bytes()
                .get(payload.range.clone())
                .unwrap_or(&[]),
        }
    }

    pub(in crate::renderer::native_vulkan) fn len(&self) -> usize {
        self.bytes().len()
    }

    pub(in crate::renderer::native_vulkan) fn split_into_ranges(
        self,
        ranges: Vec<Range<usize>>,
        label: &str,
    ) -> Result<Vec<Self>, NativeVulkanError> {
        if ranges.is_empty() {
            return Ok(Vec::new());
        }
        let len = self.len();
        for range in &ranges {
            if range.start > range.end || range.end > len {
                return Err(NativeVulkanError::Video(format!(
                    "{label} FFmpeg packet split range {}..{} exceeds packet length {len}",
                    range.start, range.end
                )));
            }
        }
        if ranges.len() == 1 && ranges[0].start == 0 && ranges[0].end == len {
            return Ok(vec![self]);
        }

        let backing = Arc::new(self);
        Ok(ranges
            .into_iter()
            .map(|range| Self {
                storage: NativeVulkanFfmpegPacketPayloadStorage::SharedSlice(
                    NativeVulkanFfmpegSharedPacketPayload {
                        backing: Arc::clone(&backing),
                        range,
                    },
                ),
            })
            .collect())
    }
}

impl Drop for NativeVulkanFfmpegPacketPayload {
    fn drop(&mut self) {
        if let NativeVulkanFfmpegPacketPayloadStorage::AvPacket(packet) = &mut self.storage {
            let mut packet = packet.as_ptr();
            unsafe {
                gilder_av_packet_free(&mut packet);
            }
        }
    }
}

impl fmt::Debug for NativeVulkanFfmpegPacketPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeVulkanFfmpegPacketPayload")
            .field(
                "model",
                &match &self.storage {
                    NativeVulkanFfmpegPacketPayloadStorage::AvPacket(_) => "avpacket-owned",
                    NativeVulkanFfmpegPacketPayloadStorage::Pooled(_) => {
                        "ffmpeg-source-pooled-annexb"
                    }
                    NativeVulkanFfmpegPacketPayloadStorage::SharedSlice(_) => {
                        "ffmpeg-shared-packet-slice"
                    }
                },
            )
            .field("bytes", &self.len())
            .finish()
    }
}

struct NativeVulkanFfmpegReusablePacket {
    packet: NonNull<AVPacket>,
}

impl NativeVulkanFfmpegReusablePacket {
    fn new() -> Result<Self, NativeVulkanError> {
        let packet = native_vulkan_ffmpeg_alloc_packet()?;
        let packet = NonNull::new(packet).ok_or_else(|| {
            NativeVulkanError::Video("FFmpeg av_packet_alloc returned null".to_owned())
        })?;
        Ok(Self { packet })
    }

    fn as_mut_ptr(&mut self) -> *mut AVPacket {
        self.packet.as_ptr()
    }

    fn bytes(&self) -> &[u8] {
        let size = unsafe { gilder_av_packet_size(self.packet.as_ptr()) };
        if size <= 0 {
            return &[];
        }
        let data = unsafe { gilder_av_packet_data(self.packet.as_ptr()) };
        if data.is_null() {
            return &[];
        }
        unsafe { slice::from_raw_parts(data.cast::<u8>(), size as usize) }
    }

    fn unref(&mut self) {
        unsafe {
            gilder_av_packet_unref(self.packet.as_ptr());
        }
    }

    fn replace_with_empty_and_take(&mut self) -> Result<*mut AVPacket, NativeVulkanError> {
        let replacement = native_vulkan_ffmpeg_alloc_packet()?;
        let replacement = NonNull::new(replacement).ok_or_else(|| {
            NativeVulkanError::Video("FFmpeg av_packet_alloc returned null".to_owned())
        })?;
        let packet = std::mem::replace(&mut self.packet, replacement);
        Ok(packet.as_ptr())
    }
}

impl Drop for NativeVulkanFfmpegReusablePacket {
    fn drop(&mut self) {
        let mut packet = self.packet.as_ptr();
        unsafe {
            gilder_av_packet_free(&mut packet);
        }
    }
}

struct NativeVulkanFfmpegFormatContext {
    ptr: NonNull<AVFormatContext>,
}

unsafe impl Send for NativeVulkanFfmpegFormatContext {}

impl NativeVulkanFfmpegFormatContext {
    fn open(
        source: &Path,
        codec: NativeVulkanFfmpegCodec,
    ) -> Result<(Self, c_int), NativeVulkanError> {
        let source = CString::new(source.as_os_str().as_bytes()).map_err(|_| {
            NativeVulkanError::Video("FFmpeg source path contains an interior NUL".to_owned())
        })?;
        let mut ctx = ptr::null_mut();
        let ret = unsafe { gilder_avformat_open_input(&mut ctx, source.as_ptr()) };
        native_vulkan_ffmpeg_ok(ret, "avformat_open_input")?;
        let ptr = NonNull::new(ctx).ok_or_else(|| {
            NativeVulkanError::Video("FFmpeg avformat_open_input returned null".to_owned())
        })?;
        let format = Self { ptr };
        let stream_index =
            unsafe { gilder_av_find_video_stream_for_codec(format.ptr.as_ptr(), codec.codec_id()) };
        native_vulkan_ffmpeg_ok(
            stream_index,
            &format!("av_find_best_stream/select {:?} video stream", codec),
        )?;
        Ok((format, stream_index))
    }

    fn stream_extradata(&self, stream_index: c_int) -> &[u8] {
        let size = unsafe { gilder_av_stream_extradata_size(self.ptr.as_ptr(), stream_index) };
        if size <= 0 {
            return &[];
        }
        let data = unsafe { gilder_av_stream_extradata(self.ptr.as_ptr(), stream_index) };
        if data.is_null() {
            return &[];
        }
        unsafe { slice::from_raw_parts(data.cast::<u8>(), size as usize) }
    }

    fn stream_time_base(&self, stream_index: c_int) -> AVRational {
        unsafe { gilder_av_stream_time_base(self.ptr.as_ptr(), stream_index) }
    }
}

impl Drop for NativeVulkanFfmpegFormatContext {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        unsafe {
            gilder_avformat_close_input(&mut ptr);
        }
    }
}

enum NativeVulkanFfmpegPacketNormalizer {
    H264(NativeVulkanFfmpegH264AnnexB),
    H265(NativeVulkanFfmpegH265AnnexB),
    Av1,
}

impl NativeVulkanFfmpegPacketNormalizer {
    fn new(codec: NativeVulkanFfmpegCodec, extradata: &[u8]) -> Result<Self, NativeVulkanError> {
        match codec {
            NativeVulkanFfmpegCodec::H264 => Ok(Self::H264(
                NativeVulkanFfmpegH264AnnexB::from_extradata(extradata)
                    .map_err(NativeVulkanError::Video)?,
            )),
            NativeVulkanFfmpegCodec::H265 => Ok(Self::H265(
                NativeVulkanFfmpegH265AnnexB::from_extradata(extradata)
                    .map_err(NativeVulkanError::Video)?,
            )),
            NativeVulkanFfmpegCodec::Av1 => Ok(Self::Av1),
        }
    }

    fn normalize(
        &mut self,
        packet: &mut NativeVulkanFfmpegReusablePacket,
        pool: &Arc<NativeVulkanFfmpegPacketPayloadPool>,
    ) -> Result<NativeVulkanFfmpegPacketPayload, NativeVulkanError> {
        match self {
            Self::H264(converter) => {
                let result = converter.convert_packet(packet.bytes(), pool);
                packet.unref();
                result.map_err(NativeVulkanError::Video)
            }
            Self::H265(converter) => {
                let result = converter.convert_packet(packet.bytes(), pool);
                packet.unref();
                result.map_err(NativeVulkanError::Video)
            }
            Self::Av1 => {
                let packet = packet.replace_with_empty_and_take()?;
                NativeVulkanFfmpegPacketPayload::from_raw(packet)
            }
        }
    }
}

struct NativeVulkanFfmpegH264AnnexB {
    length_size: usize,
    sps: Vec<u8>,
    pps: Vec<u8>,
    passthrough_annexb: bool,
}

impl NativeVulkanFfmpegH264AnnexB {
    fn from_extradata(extradata: &[u8]) -> Result<Self, String> {
        if native_vulkan_ffmpeg_starts_with_annexb_start_code(extradata) {
            return Ok(Self {
                length_size: 0,
                sps: Vec::new(),
                pps: Vec::new(),
                passthrough_annexb: true,
            });
        }
        if extradata.len() < 7 || extradata[0] != 1 {
            return Err(format!(
                "H.264 AVCC extradata is invalid or missing ({} bytes)",
                extradata.len()
            ));
        }

        let length_size = usize::from(extradata[4] & 0x03) + 1;
        let mut offset = 5usize;
        let sps_count = *extradata
            .get(offset)
            .ok_or_else(|| "H.264 AVCC extradata is truncated before SPS count".to_owned())?
            & 0x1f;
        offset += 1;

        let mut sps = Vec::new();
        for _ in 0..sps_count {
            let nal = native_vulkan_ffmpeg_take_be16_unit(extradata, &mut offset, "H.264 SPS")?;
            native_vulkan_ffmpeg_append_annexb_unit(&mut sps, nal);
        }

        let pps_count = *extradata
            .get(offset)
            .ok_or_else(|| "H.264 AVCC extradata is truncated before PPS count".to_owned())?;
        offset += 1;

        let mut pps = Vec::new();
        for _ in 0..pps_count {
            let nal = native_vulkan_ffmpeg_take_be16_unit(extradata, &mut offset, "H.264 PPS")?;
            native_vulkan_ffmpeg_append_annexb_unit(&mut pps, nal);
        }

        Ok(Self {
            length_size,
            sps,
            pps,
            passthrough_annexb: false,
        })
    }

    fn convert_packet(
        &self,
        bytes: &[u8],
        pool: &Arc<NativeVulkanFfmpegPacketPayloadPool>,
    ) -> Result<NativeVulkanFfmpegPacketPayload, String> {
        if self.passthrough_annexb {
            return Ok(native_vulkan_ffmpeg_pooled_payload_copy(bytes, pool));
        }

        let mut output_size = 0usize;
        let mut has_idr = false;
        let mut offset = 0usize;
        while offset < bytes.len() {
            let nal_size = native_vulkan_ffmpeg_take_length_prefixed_size(
                bytes,
                &mut offset,
                self.length_size,
                "H.264 packet",
            )?;
            if nal_size == 0 {
                continue;
            }
            let nal = native_vulkan_ffmpeg_take_payload(bytes, &mut offset, nal_size, "H.264 NAL")?;
            has_idr |= (nal[0] & 0x1f) == 5;
            output_size = output_size.saturating_add(NATIVE_VULKAN_ANNEXB_START_CODE.len());
            output_size = output_size.saturating_add(nal.len());
        }
        if has_idr {
            output_size = output_size
                .saturating_add(self.sps.len())
                .saturating_add(self.pps.len());
        }

        let mut output = pool.take(output_size);
        let mut offset = 0usize;
        let mut sps_seen_before_idr = false;
        let mut pps_seen_before_idr = false;
        let mut parameter_sets_inserted = false;
        while offset < bytes.len() {
            let nal_size = native_vulkan_ffmpeg_take_length_prefixed_size(
                bytes,
                &mut offset,
                self.length_size,
                "H.264 packet",
            )?;
            if nal_size == 0 {
                continue;
            }
            let nal = native_vulkan_ffmpeg_take_payload(bytes, &mut offset, nal_size, "H.264 NAL")?;
            let nal_type = nal[0] & 0x1f;
            if nal_type == 7 {
                sps_seen_before_idr = true;
            } else if nal_type == 8 {
                pps_seen_before_idr = true;
            } else if nal_type == 5 && !parameter_sets_inserted {
                if !sps_seen_before_idr && !pps_seen_before_idr {
                    output.extend_from_slice(&self.sps);
                    output.extend_from_slice(&self.pps);
                } else if sps_seen_before_idr && !pps_seen_before_idr {
                    output.extend_from_slice(&self.pps);
                }
                parameter_sets_inserted = true;
            }
            native_vulkan_ffmpeg_append_annexb_unit(&mut output, nal);
        }

        Ok(NativeVulkanFfmpegPacketPayload::from_pooled(
            output,
            Arc::clone(pool),
        ))
    }
}

struct NativeVulkanFfmpegH265AnnexB {
    length_size: usize,
    parameter_sets: Vec<u8>,
    passthrough_annexb: bool,
}

impl NativeVulkanFfmpegH265AnnexB {
    fn from_extradata(extradata: &[u8]) -> Result<Self, String> {
        if extradata.len() < 23 || native_vulkan_ffmpeg_starts_with_annexb_start_code(extradata) {
            return Ok(Self {
                length_size: 0,
                parameter_sets: Vec::new(),
                passthrough_annexb: true,
            });
        }

        let length_size = usize::from(extradata[21] & 0x03) + 1;
        let array_count = extradata[22] as usize;
        let mut offset = 23usize;
        let mut parameter_sets = Vec::new();
        for _ in 0..array_count {
            let nal_type = *extradata
                .get(offset)
                .ok_or_else(|| "H.265 HVCC extradata is truncated before array type".to_owned())?
                & 0x3f;
            offset += 1;
            if !matches!(nal_type, 32 | 33 | 34 | 39 | 40) {
                return Err(format!(
                    "H.265 HVCC extradata has invalid NAL type {nal_type}"
                ));
            }
            let unit_count =
                native_vulkan_ffmpeg_take_be16(extradata, &mut offset, "H.265 array count")?;
            for _ in 0..unit_count {
                let nal = native_vulkan_ffmpeg_take_be16_unit(
                    extradata,
                    &mut offset,
                    "H.265 parameter set",
                )?;
                native_vulkan_ffmpeg_append_annexb_unit(&mut parameter_sets, nal);
            }
        }

        Ok(Self {
            length_size,
            parameter_sets,
            passthrough_annexb: false,
        })
    }

    fn convert_packet(
        &self,
        bytes: &[u8],
        pool: &Arc<NativeVulkanFfmpegPacketPayloadPool>,
    ) -> Result<NativeVulkanFfmpegPacketPayload, String> {
        if self.passthrough_annexb {
            return Ok(native_vulkan_ffmpeg_pooled_payload_copy(bytes, pool));
        }

        let mut output_size = 0usize;
        let mut got_irap = false;
        let mut got_ps = false;
        let mut offset = 0usize;
        while offset < bytes.len() {
            let nal_size = native_vulkan_ffmpeg_take_length_prefixed_size(
                bytes,
                &mut offset,
                self.length_size,
                "H.265 packet",
            )?;
            if nal_size < 2 {
                return Err("H.265 packet contains a NAL shorter than two bytes".to_owned());
            }
            let nal = native_vulkan_ffmpeg_take_payload(bytes, &mut offset, nal_size, "H.265 NAL")?;
            let nal_type = (nal[0] >> 1) & 0x3f;
            got_irap |= (16..=23).contains(&nal_type);
            got_ps |= (32..=34).contains(&nal_type);
            output_size = output_size.saturating_add(NATIVE_VULKAN_ANNEXB_START_CODE.len());
            output_size = output_size.saturating_add(nal.len());
        }
        if got_irap || got_ps {
            output_size = output_size.saturating_add(self.parameter_sets.len());
        }

        let seen_irap_ps = got_irap && got_ps;
        let mut output = pool.take(output_size);
        let mut got_irap = false;
        let mut got_ps = false;
        let mut offset = 0usize;
        while offset < bytes.len() {
            let nal_size = native_vulkan_ffmpeg_take_length_prefixed_size(
                bytes,
                &mut offset,
                self.length_size,
                "H.265 packet",
            )?;
            if nal_size < 2 {
                return Err("H.265 packet contains a NAL shorter than two bytes".to_owned());
            }
            let nal = native_vulkan_ffmpeg_take_payload(bytes, &mut offset, nal_size, "H.265 NAL")?;
            let nal_type = (nal[0] >> 1) & 0x3f;
            let is_irap = (16..=23).contains(&nal_type);
            let is_ps = (32..=34).contains(&nal_type) && seen_irap_ps;
            if (is_ps || is_irap) && !got_ps && !got_irap {
                output.extend_from_slice(&self.parameter_sets);
            }
            got_irap |= is_irap;
            got_ps |= is_ps;
            native_vulkan_ffmpeg_append_annexb_unit(&mut output, nal);
        }

        Ok(NativeVulkanFfmpegPacketPayload::from_pooled(
            output,
            Arc::clone(pool),
        ))
    }
}

fn native_vulkan_ffmpeg_pooled_payload_copy(
    bytes: &[u8],
    pool: &Arc<NativeVulkanFfmpegPacketPayloadPool>,
) -> NativeVulkanFfmpegPacketPayload {
    let mut output = pool.take(bytes.len());
    output.extend_from_slice(bytes);
    NativeVulkanFfmpegPacketPayload::from_pooled(output, Arc::clone(pool))
}

fn native_vulkan_ffmpeg_append_annexb_unit(output: &mut Vec<u8>, nal: &[u8]) {
    output.extend_from_slice(&NATIVE_VULKAN_ANNEXB_START_CODE);
    output.extend_from_slice(nal);
}

fn native_vulkan_ffmpeg_starts_with_annexb_start_code(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0, 0, 1]) || bytes.starts_with(&NATIVE_VULKAN_ANNEXB_START_CODE)
}

fn native_vulkan_ffmpeg_take_be16(
    bytes: &[u8],
    offset: &mut usize,
    label: &str,
) -> Result<usize, String> {
    let end = offset.saturating_add(2);
    let value = bytes
        .get(*offset..end)
        .ok_or_else(|| format!("{label} is truncated before a 16-bit length"))?;
    *offset = end;
    Ok((usize::from(value[0]) << 8) | usize::from(value[1]))
}

fn native_vulkan_ffmpeg_take_be16_unit<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    label: &str,
) -> Result<&'a [u8], String> {
    let size = native_vulkan_ffmpeg_take_be16(bytes, offset, label)?;
    native_vulkan_ffmpeg_take_payload(bytes, offset, size, label)
}

fn native_vulkan_ffmpeg_take_length_prefixed_size(
    bytes: &[u8],
    offset: &mut usize,
    length_size: usize,
    label: &str,
) -> Result<usize, String> {
    if !(1..=4).contains(&length_size) {
        return Err(format!("{label} has invalid NAL length size {length_size}"));
    }
    let end = offset.saturating_add(length_size);
    let prefix = bytes
        .get(*offset..end)
        .ok_or_else(|| format!("{label} is truncated before a NAL length"))?;
    *offset = end;
    let mut size = 0usize;
    for byte in prefix {
        size = (size << 8) | usize::from(*byte);
    }
    Ok(size)
}

fn native_vulkan_ffmpeg_take_payload<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    size: usize,
    label: &str,
) -> Result<&'a [u8], String> {
    let end = offset.saturating_add(size);
    let payload = bytes
        .get(*offset..end)
        .ok_or_else(|| format!("{label} payload is truncated"))?;
    *offset = end;
    Ok(payload)
}

struct NativeVulkanFfmpegStreamingPacketWorker<A: NativeVulkanFfmpegStreamingAccessUnit> {
    format: NativeVulkanFfmpegFormatContext,
    normalizer: NativeVulkanFfmpegPacketNormalizer,
    payload_pool: Arc<NativeVulkanFfmpegPacketPayloadPool>,
    input_packet: NativeVulkanFfmpegReusablePacket,
    stream_index: c_int,
    stream_time_base: AVRational,
    eos_count: u32,
    loop_count: u32,
    pending_access_units: VecDeque<A>,
    _access_unit: PhantomData<A>,
}

impl<A: NativeVulkanFfmpegStreamingAccessUnit> NativeVulkanFfmpegStreamingPacketWorker<A> {
    fn new(source: &Path) -> Result<Self, NativeVulkanError> {
        let (format, stream_index) =
            NativeVulkanFfmpegFormatContext::open(source, A::FFMPEG_CODEC)?;
        let normalizer = NativeVulkanFfmpegPacketNormalizer::new(
            A::FFMPEG_CODEC,
            format.stream_extradata(stream_index),
        )?;
        let stream_time_base = format.stream_time_base(stream_index);
        let payload_pool = Arc::new(NativeVulkanFfmpegPacketPayloadPool::default());
        let input_packet = NativeVulkanFfmpegReusablePacket::new()?;
        Ok(Self {
            format,
            normalizer,
            payload_pool,
            input_packet,
            stream_index,
            stream_time_base,
            eos_count: 0,
            loop_count: 0,
            pending_access_units: VecDeque::new(),
            _access_unit: PhantomData,
        })
    }

    fn pull_next(&mut self, loop_on_eos: bool) -> Result<Option<A>, NativeVulkanError> {
        if let Some(access_unit) = self.pending_access_units.pop_front() {
            return Ok(Some(access_unit));
        }
        loop {
            let Some((payload, metadata)) = self.read_next_packet(loop_on_eos)? else {
                return Ok(None);
            };
            if !A::FFMPEG_PACKET_SPLITS_ACCESS_UNITS {
                return A::from_ffmpeg_packet(payload, metadata).map(Some);
            }
            let access_units = A::from_ffmpeg_packet_many(payload, metadata)?;
            if access_units.is_empty() {
                continue;
            }
            let mut access_units = access_units.into_iter();
            let first = access_units.next().expect("access_units is not empty");
            self.pending_access_units.extend(access_units);
            return Ok(Some(first));
        }
    }

    fn read_next_packet(
        &mut self,
        loop_on_eos: bool,
    ) -> Result<
        Option<(
            NativeVulkanFfmpegPacketPayload,
            NativeVulkanFfmpegPacketMetadata,
        )>,
        NativeVulkanError,
    > {
        loop {
            let input = self.input_packet.as_mut_ptr();
            let read_ret = unsafe { gilder_av_read_frame(self.format.ptr.as_ptr(), input) };
            if read_ret == 0 {
                let packet_stream_index = unsafe { gilder_av_packet_stream_index(input) };
                if packet_stream_index != self.stream_index {
                    self.input_packet.unref();
                    continue;
                }
                let metadata = self.metadata_for_packet(input, self.stream_time_base);
                let payload = self
                    .normalizer
                    .normalize(&mut self.input_packet, &self.payload_pool)?;
                return Ok(Some((payload, metadata)));
            }
            self.input_packet.unref();

            if read_ret == native_vulkan_ffmpeg_eof() {
                self.eos_count = self.eos_count.saturating_add(1);
                if !loop_on_eos {
                    return Ok(None);
                }
                self.seek_to_start()?;
                continue;
            }

            return Err(native_vulkan_ffmpeg_error(read_ret, "av_read_frame"));
        }
    }

    fn seek_to_start(&mut self) -> Result<(), NativeVulkanError> {
        let ret =
            unsafe { gilder_av_seek_stream_start(self.format.ptr.as_ptr(), self.stream_index) };
        native_vulkan_ffmpeg_ok(ret, "av_seek_frame stream start")?;
        self.pending_access_units.clear();
        self.loop_count = self.loop_count.saturating_add(1);
        Ok(())
    }

    fn metadata_for_packet(
        &self,
        packet: *const AVPacket,
        time_base: AVRational,
    ) -> NativeVulkanFfmpegPacketMetadata {
        let pts_ns =
            native_vulkan_ffmpeg_timestamp_ns(unsafe { gilder_av_packet_pts(packet) }, time_base);
        let duration_ns = native_vulkan_ffmpeg_duration_ns(
            unsafe { gilder_av_packet_duration(packet) },
            time_base,
        );
        NativeVulkanFfmpegPacketMetadata {
            pts_ns,
            duration_ns,
            pts_ms: pts_ns.map(|value| value / 1_000_000),
            duration_ms: duration_ns.map(|value| value / 1_000_000),
        }
    }
}

struct NativeVulkanFfmpegStreamingPacketFrontend<A: NativeVulkanFfmpegStreamingAccessUnit> {
    request_sender: Option<SyncSender<()>>,
    receiver: Option<Receiver<NativeVulkanFfmpegStreamingPacketFrontendMessage<A>>>,
    loop_on_eos: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    eos_count: u32,
    loop_count: u32,
    _access_unit: PhantomData<A>,
}

struct NativeVulkanFfmpegStreamingPacketFrontendMessage<A> {
    result: Result<Option<A>, NativeVulkanError>,
    eos_count: u32,
    loop_count: u32,
}

impl<A: NativeVulkanFfmpegStreamingAccessUnit + Send + 'static>
    NativeVulkanFfmpegStreamingPacketFrontend<A>
{
    fn new(source: &Path, _capacity: usize) -> Result<Self, NativeVulkanError> {
        let (request_sender, request_receiver) = sync_channel(0);
        let (sender, receiver) = sync_channel(A::FFMPEG_READ_THREAD_HANDOFF_PACKETS);
        let loop_on_eos = Arc::new(AtomicBool::new(false));
        let worker_loop_on_eos = Arc::clone(&loop_on_eos);
        let source = source.to_path_buf();
        let worker = thread::Builder::new()
            .name(format!("gilder-ffmpeg-{}-read-thread", A::CODEC_LABEL))
            .spawn(move || {
                native_vulkan_ffmpeg_streaming_packet_worker::<A>(
                    source.as_path(),
                    worker_loop_on_eos,
                    request_receiver,
                    sender,
                );
            })
            .map_err(|err| {
                NativeVulkanError::Video(format!(
                    "spawn {} FFmpeg packet read thread: {err}",
                    A::CODEC_LABEL
                ))
            })?;

        Ok(Self {
            request_sender: Some(request_sender),
            receiver: Some(receiver),
            loop_on_eos,
            worker: Some(worker),
            eos_count: 0,
            loop_count: 0,
            _access_unit: PhantomData,
        })
    }
}

impl<A: NativeVulkanFfmpegStreamingAccessUnit> Drop
    for NativeVulkanFfmpegStreamingPacketFrontend<A>
{
    fn drop(&mut self) {
        let _ = self.request_sender.take();
        let _ = self.receiver.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl<A: NativeVulkanFfmpegStreamingAccessUnit + Send + 'static>
    NativeVulkanStreamingPacketFrontend<A> for NativeVulkanFfmpegStreamingPacketFrontend<A>
{
    fn pull_next_access_unit(&mut self, loop_on_eos: bool) -> Result<Option<A>, NativeVulkanError> {
        if loop_on_eos {
            self.loop_on_eos.store(true, Ordering::Release);
        }
        let request_sender = self.request_sender.as_ref().ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "{} FFmpeg packet read thread request channel is closed",
                A::CODEC_LABEL
            ))
        })?;
        request_sender.send(()).map_err(|err| {
            NativeVulkanError::Video(format!(
                "{} FFmpeg packet read thread stopped before accepting a pull request: {err}",
                A::CODEC_LABEL
            ))
        })?;
        let receiver = self.receiver.as_ref().ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "{} FFmpeg packet read thread is closed",
                A::CODEC_LABEL
            ))
        })?;
        let message = receiver.recv().map_err(|err| {
            NativeVulkanError::Video(format!(
                "{} FFmpeg packet read thread stopped before producing an AU: {err}",
                A::CODEC_LABEL
            ))
        })?;
        self.eos_count = message.eos_count;
        self.loop_count = message.loop_count;
        message.result
    }

    fn eos_count(&self) -> u32 {
        self.eos_count
    }

    fn loop_count(&self) -> u32 {
        self.loop_count
    }
}

fn native_vulkan_ffmpeg_streaming_packet_worker<A: NativeVulkanFfmpegStreamingAccessUnit>(
    source: &Path,
    loop_on_eos: Arc<AtomicBool>,
    request_receiver: Receiver<()>,
    sender: SyncSender<NativeVulkanFfmpegStreamingPacketFrontendMessage<A>>,
) {
    let mut worker = match NativeVulkanFfmpegStreamingPacketWorker::<A>::new(source) {
        Ok(worker) => worker,
        Err(err) => {
            if request_receiver.recv().is_ok() {
                let _ = sender.send(NativeVulkanFfmpegStreamingPacketFrontendMessage {
                    result: Err(err),
                    eos_count: 0,
                    loop_count: 0,
                });
            }
            return;
        }
    };

    loop {
        if request_receiver.recv().is_err() {
            break;
        }
        let result = worker.pull_next(loop_on_eos.load(Ordering::Acquire));
        let stop_after_send = result.as_ref().map_or(true, Option::is_none);
        let message = NativeVulkanFfmpegStreamingPacketFrontendMessage {
            result,
            eos_count: worker.eos_count,
            loop_count: worker.loop_count,
        };
        if sender.send(message).is_err() || stop_after_send {
            break;
        }
    }
}

fn native_vulkan_ffmpeg_alloc_packet() -> Result<*mut AVPacket, NativeVulkanError> {
    let packet = unsafe { gilder_av_packet_alloc() };
    if packet.is_null() {
        return Err(NativeVulkanError::Video(
            "FFmpeg av_packet_alloc failed".to_owned(),
        ));
    }
    Ok(packet)
}

fn native_vulkan_ffmpeg_timestamp_ns(value: i64, time_base: AVRational) -> Option<u64> {
    if value == unsafe { gilder_av_nopts_value() } || value < 0 {
        return None;
    }
    native_vulkan_ffmpeg_rescale_to_ns(value, time_base)
}

fn native_vulkan_ffmpeg_duration_ns(value: i64, time_base: AVRational) -> Option<u64> {
    if value <= 0 {
        return None;
    }
    native_vulkan_ffmpeg_rescale_to_ns(value, time_base)
}

fn native_vulkan_ffmpeg_rescale_to_ns(value: i64, time_base: AVRational) -> Option<u64> {
    let den = NonZeroI32::new(time_base.den)?;
    let scaled =
        i128::from(value) * i128::from(time_base.num) * 1_000_000_000i128 / i128::from(den.get());
    if scaled < 0 {
        return None;
    }
    Some(scaled.min(i128::from(u64::MAX)) as u64)
}

fn native_vulkan_ffmpeg_eof() -> c_int {
    unsafe { gilder_av_error_eof() }
}

fn native_vulkan_ffmpeg_ok(ret: c_int, action: &str) -> Result<(), NativeVulkanError> {
    if ret >= 0 {
        Ok(())
    } else {
        Err(native_vulkan_ffmpeg_error(ret, action))
    }
}

fn native_vulkan_ffmpeg_error(ret: c_int, action: &str) -> NativeVulkanError {
    let mut buffer = [0 as c_char; 256];
    unsafe {
        let _ = gilder_av_strerror(ret, buffer.as_mut_ptr(), buffer.len());
    }
    let message = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    NativeVulkanError::Video(format!("FFmpeg {action} failed: {message} ({ret})"))
}
