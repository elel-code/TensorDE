//! FFmpeg packet frontend for the native Vulkan demux boundary.
//!
//! This follows the local FFmpeg ownership shape:
//! `references/ffmpeg/fftools/ffplay.c:420-456` moves AVPacket ownership into a
//! bounded PacketQueue. The FFmpeg read worker uses a codec-limited handoff
//! here: it can overlap demux/BSF work with decode like ffplay's read thread,
//! but it cannot retain a second unbounded payload queue outside the renderer's
//! bounded packet queue. `references/ffmpeg/fftools/ffplay.c:3132-3141` blocks
//! the read thread when queues are full, `references/ffmpeg/fftools/ffplay.c:3154-3215`
//! filters the target stream in the read loop, and
//! `references/ffmpeg/libavcodec/bsf.h:162-222` defines the
//! send-packet/drain-packet BSF contract used for H.264/H.265. AV1 follows
//! `references/ffmpeg/libavcodec/av1dec.c:1456-1474`: container packets are read
//! directly as OBU packets instead of being forced through the raw AV1 demuxer's
//! `av1_frame_merge` Temporal Delimiter contract.

use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroI32;
use std::os::raw::{c_char, c_int, c_uchar};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::{self, JoinHandle};

use super::NativeVulkanError;
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
struct AVBSFContext {
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
    fn gilder_av_error_eagain() -> c_int;
    fn gilder_av_error_eof() -> c_int;
    fn gilder_av_nopts_value() -> i64;
    fn gilder_av_codec_id_h264() -> c_int;
    fn gilder_av_codec_id_hevc() -> c_int;
    fn gilder_av_codec_id_av1() -> c_int;
    fn gilder_av_strerror(errnum: c_int, errbuf: *mut c_char, errbuf_size: usize) -> c_int;
    fn gilder_avformat_open_input(ctx: *mut *mut AVFormatContext, url: *const c_char) -> c_int;
    fn gilder_avformat_find_stream_info(ctx: *mut AVFormatContext) -> c_int;
    fn gilder_avformat_close_input(ctx: *mut *mut AVFormatContext);
    fn gilder_av_find_video_stream_for_codec(ctx: *mut AVFormatContext, codec_id: c_int) -> c_int;
    fn gilder_av_stream_width(ctx: *mut AVFormatContext, stream_index: c_int) -> c_int;
    fn gilder_av_stream_height(ctx: *mut AVFormatContext, stream_index: c_int) -> c_int;
    fn gilder_av_stream_avg_frame_rate(
        ctx: *mut AVFormatContext,
        stream_index: c_int,
    ) -> AVRational;
    fn gilder_av_packet_alloc() -> *mut AVPacket;
    fn gilder_av_packet_free(packet: *mut *mut AVPacket);
    fn gilder_av_read_frame(ctx: *mut AVFormatContext, packet: *mut AVPacket) -> c_int;
    fn gilder_av_packet_stream_index(packet: *const AVPacket) -> c_int;
    fn gilder_av_packet_data(packet: *const AVPacket) -> *const c_uchar;
    fn gilder_av_packet_size(packet: *const AVPacket) -> c_int;
    fn gilder_av_packet_pts(packet: *const AVPacket) -> i64;
    fn gilder_av_packet_duration(packet: *const AVPacket) -> i64;
    fn gilder_av_bsf_alloc_name(name: *const c_char, ctx: *mut *mut AVBSFContext) -> c_int;
    fn gilder_av_bsf_copy_stream_params(
        bsf: *mut AVBSFContext,
        fmt: *mut AVFormatContext,
        stream_index: c_int,
    ) -> c_int;
    fn gilder_av_bsf_init(ctx: *mut AVBSFContext) -> c_int;
    fn gilder_av_bsf_free(ctx: *mut *mut AVBSFContext);
    fn gilder_av_bsf_flush(ctx: *mut AVBSFContext);
    fn gilder_av_bsf_send_packet(ctx: *mut AVBSFContext, packet: *mut AVPacket) -> c_int;
    fn gilder_av_bsf_receive_packet(ctx: *mut AVBSFContext, packet: *mut AVPacket) -> c_int;
    fn gilder_av_bsf_time_base_out(ctx: *mut AVBSFContext) -> AVRational;
    fn gilder_av_seek_stream_start(ctx: *mut AVFormatContext, stream_index: c_int) -> c_int;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeVulkanFfmpegCodec {
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

    fn bsf_name(self) -> &'static CStr {
        match self {
            Self::H264 => c"h264_mp4toannexb",
            Self::H265 => c"hevc_mp4toannexb",
            Self::Av1 => c"",
        }
    }

    fn stream_format(self) -> &'static str {
        match self {
            Self::H264 | Self::H265 => "byte-stream",
            Self::Av1 => "obu-stream",
        }
    }

    fn alignment(self) -> &'static str {
        match self {
            Self::H264 | Self::H265 => "au",
            Self::Av1 => "frame",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct NativeVulkanFfmpegPacketMetadata {
    pub(super) pts_ns: Option<u64>,
    pub(super) duration_ns: Option<u64>,
    pub(super) pts_ms: Option<u64>,
    pub(super) duration_ms: Option<u64>,
    pub(super) caps: Option<String>,
    pub(super) stream_format: Option<String>,
    pub(super) alignment: Option<String>,
    pub(super) width: Option<u32>,
    pub(super) height: Option<u32>,
    pub(super) framerate: Option<String>,
}

pub(super) trait NativeVulkanFfmpegStreamingAccessUnit:
    NativeVulkanStreamingAccessUnit
{
    const FFMPEG_CODEC: NativeVulkanFfmpegCodec;
    const FFMPEG_READ_THREAD_HANDOFF_PACKETS: usize = 1;

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

pub(super) fn native_vulkan_start_ffmpeg_streaming_packet_queue<
    A: NativeVulkanFfmpegStreamingAccessUnit + Send + 'static,
>(
    source: &Path,
    capacity: usize,
) -> Result<NativeVulkanStreamingPacketQueue<A>, NativeVulkanError> {
    super::native_vulkan_configure_process_allocator_for_streaming_video();
    let frontend = NativeVulkanFfmpegStreamingPacketFrontend::<A>::new(source, capacity)?;
    native_vulkan_start_streaming_packet_queue_from_frontend(Box::new(frontend), capacity)
}

pub struct NativeVulkanFfmpegPacketPayload {
    packet: NonNull<AVPacket>,
}

unsafe impl Send for NativeVulkanFfmpegPacketPayload {}

impl NativeVulkanFfmpegPacketPayload {
    fn from_raw(packet: *mut AVPacket) -> Result<Self, NativeVulkanError> {
        let packet = NonNull::new(packet).ok_or_else(|| {
            NativeVulkanError::Video("FFmpeg produced a null AVPacket".to_owned())
        })?;
        Ok(Self { packet })
    }

    pub(super) fn bytes(&self) -> &[u8] {
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

    pub(super) fn len(&self) -> usize {
        self.bytes().len()
    }
}

impl Drop for NativeVulkanFfmpegPacketPayload {
    fn drop(&mut self) {
        let mut packet = self.packet.as_ptr();
        unsafe {
            gilder_av_packet_free(&mut packet);
        }
    }
}

impl fmt::Debug for NativeVulkanFfmpegPacketPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeVulkanFfmpegPacketPayload")
            .field("model", &"avpacket-owned")
            .field("bytes", &self.len())
            .finish()
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
        let ret = unsafe { gilder_avformat_find_stream_info(format.ptr.as_ptr()) };
        native_vulkan_ffmpeg_ok(ret, "avformat_find_stream_info")?;
        let stream_index =
            unsafe { gilder_av_find_video_stream_for_codec(format.ptr.as_ptr(), codec.codec_id()) };
        native_vulkan_ffmpeg_ok(
            stream_index,
            &format!("av_find_best_stream/select {:?} video stream", codec),
        )?;
        Ok((format, stream_index))
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

struct NativeVulkanFfmpegBsfContext {
    ptr: NonNull<AVBSFContext>,
}

unsafe impl Send for NativeVulkanFfmpegBsfContext {}

impl NativeVulkanFfmpegBsfContext {
    fn new(
        codec: NativeVulkanFfmpegCodec,
        format: &NativeVulkanFfmpegFormatContext,
        stream_index: c_int,
    ) -> Result<Self, NativeVulkanError> {
        let mut ctx = ptr::null_mut();
        let ret = unsafe { gilder_av_bsf_alloc_name(codec.bsf_name().as_ptr(), &mut ctx) };
        native_vulkan_ffmpeg_ok(ret, codec.bsf_name().to_string_lossy().as_ref())?;
        let ptr = NonNull::new(ctx).ok_or_else(|| {
            NativeVulkanError::Video(format!(
                "FFmpeg {} BSF allocation returned null",
                codec.bsf_name().to_string_lossy()
            ))
        })?;
        let bsf = Self { ptr };
        let ret = unsafe {
            gilder_av_bsf_copy_stream_params(bsf.ptr.as_ptr(), format.ptr.as_ptr(), stream_index)
        };
        native_vulkan_ffmpeg_ok(ret, "avcodec_parameters_copy to AVBSFContext")?;
        let ret = unsafe { gilder_av_bsf_init(bsf.ptr.as_ptr()) };
        native_vulkan_ffmpeg_ok(ret, "av_bsf_init")?;
        Ok(bsf)
    }
}

impl Drop for NativeVulkanFfmpegBsfContext {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        unsafe {
            gilder_av_bsf_free(&mut ptr);
        }
    }
}

#[derive(Clone, Debug)]
struct NativeVulkanFfmpegStaticMetadata {
    width: Option<u32>,
    height: Option<u32>,
    framerate: Option<String>,
    stream_format: Option<String>,
    alignment: Option<String>,
}

struct NativeVulkanFfmpegStreamingPacketWorker<A: NativeVulkanFfmpegStreamingAccessUnit> {
    format: NativeVulkanFfmpegFormatContext,
    bsf: NativeVulkanFfmpegBsfContext,
    stream_index: c_int,
    static_metadata: NativeVulkanFfmpegStaticMetadata,
    eos_count: u32,
    loop_count: u32,
    eof_sent_to_bsf: bool,
    pending_access_units: VecDeque<A>,
    _access_unit: PhantomData<A>,
}

impl<A: NativeVulkanFfmpegStreamingAccessUnit> NativeVulkanFfmpegStreamingPacketWorker<A> {
    fn new(source: &Path) -> Result<Self, NativeVulkanError> {
        let (format, stream_index) =
            NativeVulkanFfmpegFormatContext::open(source, A::FFMPEG_CODEC)?;
        let bsf = NativeVulkanFfmpegBsfContext::new(A::FFMPEG_CODEC, &format, stream_index)?;
        let width = unsafe { gilder_av_stream_width(format.ptr.as_ptr(), stream_index) };
        let height = unsafe { gilder_av_stream_height(format.ptr.as_ptr(), stream_index) };
        let framerate = unsafe {
            native_vulkan_ffmpeg_rational_string(gilder_av_stream_avg_frame_rate(
                format.ptr.as_ptr(),
                stream_index,
            ))
        };
        Ok(Self {
            format,
            bsf,
            stream_index,
            static_metadata: NativeVulkanFfmpegStaticMetadata {
                width: u32::try_from(width).ok().filter(|value| *value > 0),
                height: u32::try_from(height).ok().filter(|value| *value > 0),
                framerate,
                stream_format: Some(A::FFMPEG_CODEC.stream_format().to_owned()),
                alignment: Some(A::FFMPEG_CODEC.alignment().to_owned()),
            },
            eos_count: 0,
            loop_count: 0,
            eof_sent_to_bsf: false,
            pending_access_units: VecDeque::new(),
            _access_unit: PhantomData,
        })
    }

    fn pull_next(&mut self, loop_on_eos: bool) -> Result<Option<A>, NativeVulkanError> {
        if let Some(access_unit) = self.pending_access_units.pop_front() {
            return Ok(Some(access_unit));
        }
        loop {
            let output = native_vulkan_ffmpeg_alloc_packet()?;
            let receive_ret =
                unsafe { gilder_av_bsf_receive_packet(self.bsf.ptr.as_ptr(), output) };
            if receive_ret == 0 {
                let time_base = unsafe { gilder_av_bsf_time_base_out(self.bsf.ptr.as_ptr()) };
                let metadata = self.metadata_for_packet(output, time_base);
                let payload = NativeVulkanFfmpegPacketPayload::from_raw(output)?;
                let mut access_units = A::from_ffmpeg_packet_many(payload, metadata)?;
                if access_units.is_empty() {
                    continue;
                }
                let first = access_units.remove(0);
                self.pending_access_units.extend(access_units);
                return Ok(Some(first));
            }
            native_vulkan_ffmpeg_free_packet(output);

            if receive_ret == native_vulkan_ffmpeg_eagain() {
                if self.eof_sent_to_bsf {
                    return Ok(None);
                }
                self.read_and_send_next_input_packet()?;
                continue;
            }

            if receive_ret == native_vulkan_ffmpeg_eof() {
                if !loop_on_eos {
                    return Ok(None);
                }
                self.seek_to_start()?;
                continue;
            }

            return Err(native_vulkan_ffmpeg_error(
                receive_ret,
                "av_bsf_receive_packet",
            ));
        }
    }

    fn read_and_send_next_input_packet(&mut self) -> Result<(), NativeVulkanError> {
        loop {
            let input = native_vulkan_ffmpeg_alloc_packet()?;
            let read_ret = unsafe { gilder_av_read_frame(self.format.ptr.as_ptr(), input) };
            if read_ret == 0 {
                let packet_stream_index = unsafe { gilder_av_packet_stream_index(input) };
                if packet_stream_index != self.stream_index {
                    native_vulkan_ffmpeg_free_packet(input);
                    continue;
                }
                let send_ret = unsafe { gilder_av_bsf_send_packet(self.bsf.ptr.as_ptr(), input) };
                native_vulkan_ffmpeg_free_packet(input);
                native_vulkan_ffmpeg_ok(send_ret, "av_bsf_send_packet")?;
                return Ok(());
            }
            native_vulkan_ffmpeg_free_packet(input);

            if read_ret == native_vulkan_ffmpeg_eof() {
                self.eos_count = self.eos_count.saturating_add(1);
                let send_ret =
                    unsafe { gilder_av_bsf_send_packet(self.bsf.ptr.as_ptr(), ptr::null_mut()) };
                native_vulkan_ffmpeg_ok(send_ret, "av_bsf_send_packet(NULL)")?;
                self.eof_sent_to_bsf = true;
                return Ok(());
            }

            return Err(native_vulkan_ffmpeg_error(read_ret, "av_read_frame"));
        }
    }

    fn seek_to_start(&mut self) -> Result<(), NativeVulkanError> {
        let ret =
            unsafe { gilder_av_seek_stream_start(self.format.ptr.as_ptr(), self.stream_index) };
        native_vulkan_ffmpeg_ok(ret, "av_seek_frame stream start")?;
        unsafe {
            gilder_av_bsf_flush(self.bsf.ptr.as_ptr());
        }
        self.eof_sent_to_bsf = false;
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
            caps: None,
            stream_format: self.static_metadata.stream_format.clone(),
            alignment: self.static_metadata.alignment.clone(),
            width: self.static_metadata.width,
            height: self.static_metadata.height,
            framerate: self.static_metadata.framerate.clone(),
        }
    }
}

struct NativeVulkanFfmpegStreamingPacketFrontend<A: NativeVulkanFfmpegStreamingAccessUnit> {
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
    sender: SyncSender<NativeVulkanFfmpegStreamingPacketFrontendMessage<A>>,
) {
    let mut worker = match NativeVulkanFfmpegStreamingPacketWorker::<A>::new(source) {
        Ok(worker) => worker,
        Err(err) => {
            let _ = sender.send(NativeVulkanFfmpegStreamingPacketFrontendMessage {
                result: Err(err),
                eos_count: 0,
                loop_count: 0,
            });
            return;
        }
    };

    loop {
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

fn native_vulkan_ffmpeg_free_packet(mut packet: *mut AVPacket) {
    if !packet.is_null() {
        unsafe {
            gilder_av_packet_free(&mut packet);
        }
    }
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

fn native_vulkan_ffmpeg_rational_string(value: AVRational) -> Option<String> {
    if value.num <= 0 || value.den <= 0 {
        return None;
    }
    Some(format!("{}/{}", value.num, value.den))
}

fn native_vulkan_ffmpeg_eagain() -> c_int {
    unsafe { gilder_av_error_eagain() }
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
