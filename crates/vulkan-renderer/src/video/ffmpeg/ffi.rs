use std::ffi::{c_char, c_int, c_uint, c_void};

#[repr(C)]
pub(super) struct AVBufferRef {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct AVCodec {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct AVCodecContext {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct AVFormatContext {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct AVFrame {
    _private: [u8; 0],
}

#[repr(C)]
pub(super) struct AVPacket {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub(super) struct AVRational {
    pub(super) numerator: c_int,
    pub(super) denominator: c_int,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub(super) struct FrameSnapshot {
    pub(super) image: u64,
    pub(super) semaphore: u64,
    pub(super) semaphore_value: u64,
    pub(super) queue_family: c_uint,
    pub(super) extra_image_usage: c_uint,
    pub(super) image_flags: c_uint,
    pub(super) layout: c_int,
    pub(super) frame_format: c_int,
    pub(super) software_format: c_int,
    pub(super) picture_format: c_int,
    pub(super) width: c_int,
    pub(super) height: c_int,
    pub(super) array_layers: c_int,
    pub(super) image_count: c_int,
    pub(super) semaphore_count: c_int,
    pub(super) pts: i64,
    pub(super) duration: i64,
}

unsafe extern "C" {
    pub(super) fn vr_ffmpeg_create_vulkan_device(
        output: *mut *mut AVBufferRef,
        instance: usize,
        physical_device: usize,
        device: usize,
        instance_extensions: *const *const c_char,
        instance_extension_count: c_int,
        device_extensions: *const *const c_char,
        device_extension_count: c_int,
        video_queue_family: c_int,
        video_queue_count: c_int,
        video_queue_flags: c_uint,
        video_codec_operations: c_uint,
    ) -> c_int;
    pub(super) fn vr_ffmpeg_buffer_unref(reference: *mut *mut AVBufferRef);

    pub(super) fn vr_ffmpeg_error_again() -> c_int;
    pub(super) fn vr_ffmpeg_error_eof() -> c_int;
    pub(super) fn vr_ffmpeg_codec_h264() -> c_int;
    pub(super) fn vr_ffmpeg_codec_hevc() -> c_int;
    pub(super) fn vr_ffmpeg_codec_av1() -> c_int;
    pub(super) fn vr_ffmpeg_pixel_vulkan() -> c_int;
    pub(super) fn vr_ffmpeg_pixel_nv12() -> c_int;
    pub(super) fn vr_ffmpeg_pixel_p010() -> c_int;
    pub(super) fn vr_ffmpeg_nopts_value() -> i64;
    pub(super) fn vr_ffmpeg_error_string(error: c_int, buffer: *mut c_char, size: usize) -> c_int;

    pub(super) fn vr_ffmpeg_open_input(
        context: *mut *mut AVFormatContext,
        path: *const c_char,
    ) -> c_int;
    pub(super) fn vr_ffmpeg_close_input(context: *mut *mut AVFormatContext);
    pub(super) fn vr_ffmpeg_find_video_stream(
        context: *mut AVFormatContext,
        codec_id: c_int,
    ) -> c_int;
    pub(super) fn vr_ffmpeg_stream_time_base(
        context: *mut AVFormatContext,
        stream: c_int,
    ) -> AVRational;
    pub(super) fn vr_ffmpeg_seek_start(context: *mut AVFormatContext, stream: c_int) -> c_int;
    pub(super) fn vr_ffmpeg_read(context: *mut AVFormatContext, packet: *mut AVPacket) -> c_int;

    pub(super) fn vr_ffmpeg_stream_decoder(
        context: *mut AVFormatContext,
        stream: c_int,
    ) -> *const AVCodec;
    pub(super) fn vr_ffmpeg_codec_name(codec: *const AVCodec) -> *const c_char;
    pub(super) fn vr_ffmpeg_codec_has_vulkan(codec: *const AVCodec) -> c_int;
    pub(super) fn vr_ffmpeg_codec_alloc(codec: *const AVCodec) -> *mut AVCodecContext;
    pub(super) fn vr_ffmpeg_codec_free(context: *mut *mut AVCodecContext);
    pub(super) fn vr_ffmpeg_codec_copy_parameters(
        codec: *mut AVCodecContext,
        format: *mut AVFormatContext,
        stream: c_int,
    ) -> c_int;
    pub(super) fn vr_ffmpeg_codec_open(
        context: *mut AVCodecContext,
        codec: *const AVCodec,
        device: *mut AVBufferRef,
    ) -> c_int;
    pub(super) fn vr_ffmpeg_codec_send(
        context: *mut AVCodecContext,
        packet: *const AVPacket,
    ) -> c_int;
    pub(super) fn vr_ffmpeg_codec_receive(
        context: *mut AVCodecContext,
        frame: *mut AVFrame,
    ) -> c_int;
    pub(super) fn vr_ffmpeg_codec_flush(context: *mut AVCodecContext);
    pub(super) fn vr_ffmpeg_codec_width(context: *const AVCodecContext) -> c_int;
    pub(super) fn vr_ffmpeg_codec_height(context: *const AVCodecContext) -> c_int;

    pub(super) fn vr_ffmpeg_packet_alloc() -> *mut AVPacket;
    pub(super) fn vr_ffmpeg_packet_free(packet: *mut *mut AVPacket);
    pub(super) fn vr_ffmpeg_packet_unref(packet: *mut AVPacket);
    pub(super) fn vr_ffmpeg_packet_stream(packet: *const AVPacket) -> c_int;
    pub(super) fn vr_ffmpeg_packet_size(packet: *const AVPacket) -> c_int;

    pub(super) fn vr_ffmpeg_frame_alloc() -> *mut AVFrame;
    pub(super) fn vr_ffmpeg_frame_free(frame: *mut *mut AVFrame);
    pub(super) fn vr_ffmpeg_frame_unref(frame: *mut AVFrame);
    pub(super) fn vr_ffmpeg_frame_move(destination: *mut AVFrame, source: *mut AVFrame);
    pub(super) fn vr_ffmpeg_frame_snapshot(
        frame: *const AVFrame,
        snapshot: *mut FrameSnapshot,
    ) -> c_int;
    pub(super) fn vr_ffmpeg_frame_snapshot_size() -> usize;
}

pub(super) fn null_packet() -> *const AVPacket {
    std::ptr::null::<c_void>().cast()
}
