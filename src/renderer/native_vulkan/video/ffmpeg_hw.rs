//! FFmpeg hardware decode boundary for the native video mainline.
//!
//! The intended main path is FFmpeg's Vulkan hwaccel producing
//! `AVFrame(format=AV_PIX_FMT_VULKAN)`. Gilder owns the Vulkanalia device,
//! descriptor heap, render pass and Wayland present path; FFmpeg borrows that
//! device for codec/session/frame-pool work and hands back `AVVkFrame` images.

#![allow(dead_code)]

use serde::Serialize;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uchar, c_uint};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{self, NonNull};
use vulkanalia::prelude::v1_4::{Device, DeviceV1_0, Instance, InstanceV1_0};
use vulkanalia::vk::{self, Handle};

use super::codec::NativeVulkanVideoSessionCodec;

const FFMPEG_HWCONTEXT_REFERENCE: &str = "references/ffmpeg/libavutil/hwcontext.h";
const FFMPEG_VULKAN_HWCONTEXT_REFERENCE: &str = "references/ffmpeg/libavutil/hwcontext_vulkan.h";
const FFMPEG_VULKAN_DECODE_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_decode.c";
const FFMPEG_VULKAN_H264_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_h264.c";
const FFMPEG_VULKAN_H265_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_hevc.c";
const FFMPEG_VULKAN_AV1_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_av1.c";

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(in crate::renderer::native_vulkan) struct AVFrame {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(in crate::renderer::native_vulkan) struct AVBufferRef {
    _private: [u8; 0],
}

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
struct AVCodec {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVCodecContext {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct GilderFfmpegObjectPool {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AVRational {
    num: c_int,
    den: c_int,
}

unsafe extern "C" {
    fn gilder_configure_process_allocator_for_streaming_video();
    fn gilder_trim_process_heap();
    fn gilder_av_error_again() -> c_int;
    fn gilder_av_error_eof() -> c_int;
    fn gilder_av_nopts_value() -> i64;
    fn gilder_av_codec_id_h264() -> c_int;
    fn gilder_av_codec_id_hevc() -> c_int;
    fn gilder_av_codec_id_av1() -> c_int;
    fn gilder_av_strerror(errnum: c_int, errbuf: *mut c_char, errbuf_size: usize) -> c_int;
    fn gilder_av_hwdevice_type_vulkan() -> c_int;
    fn gilder_av_pix_fmt_none() -> c_int;
    fn gilder_av_pix_fmt_vulkan() -> c_int;
    fn gilder_av_pix_fmt_nv12() -> c_int;
    fn gilder_av_pix_fmt_p010le() -> c_int;
    fn gilder_av_frame_format(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_is_vulkan_hw(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_vulkan_image_count(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_vulkan_timeline_semaphore_count(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_vulkan_image(frame: *const AVFrame, index: c_int) -> u64;
    fn gilder_av_frame_vulkan_layout(frame: *const AVFrame, index: c_int) -> c_int;
    fn gilder_av_frame_vulkan_timeline_semaphore(frame: *const AVFrame, index: c_int) -> u64;
    fn gilder_av_frame_vulkan_timeline_semaphore_value(frame: *const AVFrame, index: c_int) -> u64;
    fn gilder_av_frame_vulkan_queue_family(frame: *const AVFrame, index: c_int) -> c_uint;
    fn gilder_av_frame_hw_sw_format(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_vulkan_nb_layers(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_width(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_height(frame: *const AVFrame) -> c_int;
    fn gilder_av_frame_pts(frame: *const AVFrame) -> i64;
    fn gilder_av_frame_duration(frame: *const AVFrame) -> i64;
    fn gilder_av_frame_unref(frame: *mut AVFrame);
    fn gilder_av_frame_alloc_owned() -> *mut AVFrame;
    fn gilder_av_frame_move_ref(dst: *mut AVFrame, src: *mut AVFrame);
    fn gilder_av_frame_free_owned(frame: *mut *mut AVFrame);
    fn gilder_av_hwdevice_ctx_alloc_vulkan_existing(
        out: *mut *mut AVBufferRef,
        instance_handle: usize,
        physical_device_handle: usize,
        device_handle: usize,
        enabled_inst_extensions: *const *const c_char,
        nb_enabled_inst_extensions: c_int,
        enabled_dev_extensions: *const *const c_char,
        nb_enabled_dev_extensions: c_int,
        video_queue_family_index: c_int,
        video_queue_count: c_int,
        video_queue_flags: c_uint,
        video_codec_operations: c_uint,
        present_queue_family_index: c_int,
        present_queue_count: c_int,
        present_queue_flags: c_uint,
    ) -> c_int;
    fn gilder_av_buffer_unref(ref_: *mut *mut AVBufferRef);
    fn gilder_avformat_open_input(ctx: *mut *mut AVFormatContext, url: *const c_char) -> c_int;
    fn gilder_avformat_close_input(ctx: *mut *mut AVFormatContext);
    fn gilder_av_find_video_stream_for_codec(ctx: *mut AVFormatContext, codec_id: c_int) -> c_int;
    fn gilder_av_stream_time_base(ctx: *mut AVFormatContext, stream_index: c_int) -> AVRational;
    fn gilder_av_seek_stream_start(ctx: *mut AVFormatContext, stream_index: c_int) -> c_int;
    fn gilder_av_stream_decoder(ctx: *mut AVFormatContext, stream_index: c_int) -> *const AVCodec;
    fn gilder_avcodec_name(codec: *const AVCodec) -> *const c_char;
    fn gilder_avcodec_has_vulkan_hw_config(codec: *const AVCodec) -> c_int;
    fn gilder_avcodec_alloc_context3(codec: *const AVCodec) -> *mut AVCodecContext;
    fn gilder_avcodec_free_context(ctx: *mut *mut AVCodecContext);
    fn gilder_avcodec_parameters_to_context_for_stream(
        codec_ctx: *mut AVCodecContext,
        format_ctx: *mut AVFormatContext,
        stream_index: c_int,
    ) -> c_int;
    fn gilder_avcodec_open2_vulkan_hw(
        ctx: *mut AVCodecContext,
        codec: *const AVCodec,
        hw_device_ctx: *mut AVBufferRef,
    ) -> c_int;
    fn gilder_avcodec_context_thread_count(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_thread_type(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_active_thread_type(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_extra_hw_frames(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_flags(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_flags2(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_has_b_frames(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_delay(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_hw_frames_initial_pool_size(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_coded_width(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_coded_height(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_context_h264_enable_er(ctx: *const AVCodecContext) -> c_int;
    fn gilder_avcodec_send_packet(ctx: *mut AVCodecContext, packet: *const AVPacket) -> c_int;
    fn gilder_avcodec_receive_frame(ctx: *mut AVCodecContext, frame: *mut AVFrame) -> c_int;
    fn gilder_avcodec_flush_buffers(ctx: *mut AVCodecContext);
    fn gilder_ffmpeg_pool_alloc() -> *mut GilderFfmpegObjectPool;
    fn gilder_ffmpeg_pool_free(pool: *mut *mut GilderFfmpegObjectPool);
    fn gilder_ffmpeg_pool_get_packet(pool: *mut GilderFfmpegObjectPool) -> *mut AVPacket;
    fn gilder_ffmpeg_pool_put_packet(pool: *mut GilderFfmpegObjectPool, packet: *mut *mut AVPacket);
    fn gilder_ffmpeg_pool_get_frame(pool: *mut GilderFfmpegObjectPool) -> *mut AVFrame;
    fn gilder_ffmpeg_pool_put_frame(pool: *mut GilderFfmpegObjectPool, frame: *mut *mut AVFrame);
    fn gilder_av_read_frame(ctx: *mut AVFormatContext, packet: *mut AVPacket) -> c_int;
    fn gilder_av_packet_stream_index(packet: *const AVPacket) -> c_int;
    fn gilder_av_packet_unref(packet: *mut AVPacket);
    fn gilder_av_packet_pts(packet: *const AVPacket) -> i64;
    fn gilder_av_packet_duration(packet: *const AVPacket) -> i64;
    fn gilder_av_packet_size(packet: *const AVPacket) -> c_int;
    fn gilder_av_packet_data(packet: *const AVPacket) -> *const c_uchar;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanFfmpegHwDecodeDevicePolicy {
    VulkanaliaProvidedDevice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanFfmpegHwDecodeFallbackPolicy {
    RejectSoftwareDecode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanFfmpegVulkanHwFrameContract {
    pub binding: &'static str,
    pub route: &'static str,
    pub ffmpeg_hwdevice_type: &'static str,
    pub required_avframe_format: &'static str,
    pub required_avframe_data0: &'static str,
    pub image_identity: &'static str,
    pub synchronization_identity: &'static str,
    pub queue_family_identity: &'static str,
    pub descriptor_heap_input: &'static str,
    pub release_rule: &'static str,
    pub forbidden_operations: &'static [&'static str],
    pub zero_copy_scope: &'static str,
    pub primary_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanFfmpegHwDecodeBackendContract {
    pub binding: &'static str,
    pub route: &'static str,
    pub mainline: bool,
    pub device_policy: NativeVulkanFfmpegHwDecodeDevicePolicy,
    pub fallback_policy: NativeVulkanFfmpegHwDecodeFallbackPolicy,
    pub decode_owner: &'static str,
    pub vulkan_device_owner: &'static str,
    pub render_owner: &'static str,
    pub output_frame_contract: NativeVulkanFfmpegVulkanHwFrameContract,
    pub codec_hwaccels: Vec<NativeVulkanFfmpegHwDecodeCodecContract>,
    pub required_telemetry: &'static [&'static str],
    pub migration_rule: &'static str,
    pub ffmpeg_reference_files: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanFfmpegHwDecodeCodecContract {
    pub codec: NativeVulkanVideoSessionCodec,
    pub ffmpeg_hwaccel_name: &'static str,
    pub ffmpeg_reference: &'static str,
    pub output_format: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanFfmpegVulkanHwFrameProbe {
    pub frame_present: bool,
    pub frame_format: i32,
    pub expected_vulkan_format: i32,
    pub is_vulkan_hw_frame: bool,
    pub vulkan_image_count: i32,
    pub vulkan_timeline_semaphore_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanFfmpegVulkanHwDeviceBorrowSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub instance_handle_present: bool,
    pub physical_device_handle_present: bool,
    pub device_handle_present: bool,
    pub enabled_instance_extension_count: usize,
    pub enabled_device_extension_count: usize,
    pub enabled_device_extensions: Vec<String>,
    pub video_queue_family_index: u32,
    pub video_queue_count: u32,
    pub present_queue_family_index: u32,
    pub present_queue_count: u32,
    pub present_queue_exposed_to_ffmpeg: bool,
    pub video_codec_operations: Vec<&'static str>,
    pub private_ffmpeg_device: bool,
}

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanFfmpegVulkanHwDeviceBorrow<'a> {
    pub instance: &'a Instance,
    pub physical_device: vk::PhysicalDevice,
    pub device: &'a Device,
    pub enabled_instance_extensions: &'a [&'a str],
    pub enabled_device_extensions: &'a [&'a str],
    pub video_queue_family_index: u32,
    pub video_queue_count: u32,
    pub video_queue_flags: vk::QueueFlags,
    pub video_codec_operations: vk::VideoCodecOperationFlagsKHR,
    pub present_queue_family_index: u32,
    pub present_queue_count: u32,
    pub present_queue_flags: vk::QueueFlags,
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanFfmpegVulkanHwDevice {
    ptr: NonNull<AVBufferRef>,
    snapshot: NativeVulkanFfmpegVulkanHwDeviceBorrowSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanFfmpegDecodedGpuFramePlane {
    pub image: vk::Image,
    pub layout: vk::ImageLayout,
    pub timeline_semaphore: vk::Semaphore,
    pub timeline_value: u64,
    pub queue_family_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanFfmpegDecodedGpuFrameDescriptorSource {
    pub binding: &'static str,
    pub route: &'static str,
    pub picture_format: vk::Format,
    pub sw_format: &'static str,
    pub extent: (u32, u32, u32),
    pub array_layers: u32,
    pub planes: Vec<NativeVulkanFfmpegDecodedGpuFramePlane>,
    pub pts_raw: Option<i64>,
    pub duration_raw: Option<i64>,
    pub zero_copy_scope: &'static str,
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanFfmpegDecodedGpuFrame {
    frame: NonNull<AVFrame>,
}

// AVFrame refs returned by FFmpeg are refcounted GPU-frame handles. Gilder moves
// each cloned ref from the FFmpeg decode worker to the present worker and never
// aliases a mutable AVFrame across threads.
unsafe impl Send for NativeVulkanFfmpegDecodedGpuFrame {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanFfmpegVulkanHwDecoderSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub codec: NativeVulkanVideoSessionCodec,
    pub decoder_name: String,
    pub decoder_has_vulkan_hw_config: bool,
    pub stream_index: i32,
    pub time_base: (i32, i32),
    pub hw_device: NativeVulkanFfmpegVulkanHwDeviceBorrowSnapshot,
    pub software_decode_fallback: bool,
    pub decoded_frame_format: &'static str,
    pub coded_extent: (i32, i32),
    pub thread_count: i32,
    pub thread_type: i32,
    pub active_thread_type: i32,
    pub extra_hw_frames: i32,
    pub codec_flags: i32,
    pub codec_flags2: i32,
    pub low_delay_flag: bool,
    pub fast_flag: bool,
    pub has_b_frames: i32,
    pub codec_delay: i32,
    pub hw_frames_initial_pool_size: i32,
    pub h264_enable_er: i32,
    pub sent_packet_count: u64,
    pub sent_packet_payload_bytes: u64,
    pub max_packet_size_bytes: i32,
    pub bitstream_buffer_model: &'static str,
    pub inferred_min_ffmpeg_slice_buffer_slot_bytes: u64,
    pub codec_host_memory_model: &'static str,
    pub inferred_codec_resolution_scaled_host_bytes: u64,
    pub h264_refstruct_model: &'static str,
    pub inferred_h264_refstruct_bytes_per_picture: u64,
    pub inferred_h264_refstruct_min_three_picture_bytes: u64,
    pub hevc_refstruct_model: &'static str,
    pub inferred_hevc_refstruct_bytes_per_picture: u64,
    pub inferred_hevc_refstruct_min_three_picture_bytes: u64,
    pub inferred_hevc_layer_tables_bytes: u64,
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanFfmpegVulkanHwDecoder {
    codec_context: NativeVulkanFfmpegHwCodecContext,
    packet: NativeVulkanFfmpegHwReusablePacket,
    frame: NativeVulkanFfmpegHwReusableFrame,
    format: NativeVulkanFfmpegHwFormatContext,
    pool: NativeVulkanFfmpegHwObjectPool,
    stream_index: c_int,
    time_base: AVRational,
    codec: NativeVulkanVideoSessionCodec,
    hw_device_snapshot: NativeVulkanFfmpegVulkanHwDeviceBorrowSnapshot,
    eos_count: u32,
    loop_count: u32,
    sent_packet_count: u64,
    sent_packet_payload_bytes: u64,
    max_packet_size_bytes: c_int,
}

// The decoder is owned by exactly one worker thread after construction. It is
// not shared; moving it lets avcodec overlap Vulkan hwdecode with Wayland
// present on devices where video and present queues are distinct.
unsafe impl Send for NativeVulkanFfmpegVulkanHwDecoder {}

struct NativeVulkanFfmpegHwFormatContext {
    ptr: NonNull<AVFormatContext>,
}

struct NativeVulkanFfmpegHwCodecContext {
    ptr: NonNull<AVCodecContext>,
    decoder_name: String,
    decoder_has_vulkan_hw_config: bool,
}

struct NativeVulkanFfmpegHwObjectPool {
    ptr: NonNull<GilderFfmpegObjectPool>,
}

struct NativeVulkanFfmpegHwReusablePacket {
    packet: NonNull<AVPacket>,
    pool: NonNull<GilderFfmpegObjectPool>,
}

struct NativeVulkanFfmpegHwReusableFrame {
    frame: NonNull<AVFrame>,
    pool: NonNull<GilderFfmpegObjectPool>,
}

pub fn native_vulkan_ffmpeg_hw_decode_backend_contract() -> NativeVulkanFfmpegHwDecodeBackendContract
{
    NativeVulkanFfmpegHwDecodeBackendContract {
        binding: "ffmpeg-vulkan-hwdecode",
        route: "mainline-video-decode",
        mainline: true,
        device_policy: NativeVulkanFfmpegHwDecodeDevicePolicy::VulkanaliaProvidedDevice,
        fallback_policy: NativeVulkanFfmpegHwDecodeFallbackPolicy::RejectSoftwareDecode,
        decode_owner: "FFmpeg avcodec Vulkan hwaccel",
        vulkan_device_owner: "Gilder Vulkanalia creates instance/device/queues/features and exports them through AVVulkanDeviceContext",
        render_owner: "Gilder descriptor heap, dynamic rendering and Wayland present",
        output_frame_contract: native_vulkan_ffmpeg_vulkan_hw_frame_contract(),
        codec_hwaccels: native_vulkan_ffmpeg_hw_decode_codec_contracts(),
        required_telemetry: &[
            "decode_backend=ffmpeg-vulkan-hwdecode",
            "ffmpeg_hwdevice_type=AV_HWDEVICE_TYPE_VULKAN",
            "ffmpeg_hw_format=AV_PIX_FMT_VULKAN",
            "software_decode_fallback=false",
            "av_hwframe_transfer_data_calls=0",
            "legacy_bind_groups=0",
            "descriptor_heap_only=true",
            "decoded_image_zero_copy_presented=true",
        ],
        migration_rule: "the old Vulkan Video submit/runtime path is compatibility-only until removed; --run-video must target FFmpeg Vulkan hwaccel and must fail rather than falling back to software decode",
        ffmpeg_reference_files: &[
            FFMPEG_HWCONTEXT_REFERENCE,
            FFMPEG_VULKAN_HWCONTEXT_REFERENCE,
            FFMPEG_VULKAN_DECODE_REFERENCE,
            FFMPEG_VULKAN_H264_REFERENCE,
            FFMPEG_VULKAN_H265_REFERENCE,
            FFMPEG_VULKAN_AV1_REFERENCE,
        ],
    }
}

pub fn native_vulkan_ffmpeg_vulkan_hw_frame_contract() -> NativeVulkanFfmpegVulkanHwFrameContract {
    NativeVulkanFfmpegVulkanHwFrameContract {
        binding: "ffmpeg-avvkframe",
        route: "decoded-gpu-frame-handoff",
        ffmpeg_hwdevice_type: "AV_HWDEVICE_TYPE_VULKAN",
        required_avframe_format: "AV_PIX_FMT_VULKAN",
        required_avframe_data0: "AVVkFrame",
        image_identity: "AVVkFrame.img[] VkImage handles on the Vulkanalia-provided VkDevice",
        synchronization_identity: "AVVkFrame.sem[] timeline semaphores and sem_value[] are waited before descriptor-heap sampling",
        queue_family_identity: "AVVkFrame.queue_family[] drives any video->present queue ownership transfer",
        descriptor_heap_input: "VkImage plane views, current layout and sampler metadata only; no decoded pixel upload",
        release_rule: "retain the AVFrame ref until the present fence releases the sampled VkImage",
        forbidden_operations: &[
            "av_hwframe_transfer_data",
            "software NV12/P010 AVFrame upload",
            "FFmpeg-created private Vulkan device on the mainline",
            "legacy binding fallback",
        ],
        zero_copy_scope: "decoded pixels remain in FFmpeg-produced Vulkan images and are sampled through VK_EXT_descriptor_heap; descriptor writes copy metadata only",
        primary_reference: FFMPEG_VULKAN_HWCONTEXT_REFERENCE,
    }
}

pub fn native_vulkan_ffmpeg_hw_decode_codec_contracts()
-> Vec<NativeVulkanFfmpegHwDecodeCodecContract> {
    vec![
        NativeVulkanFfmpegHwDecodeCodecContract {
            codec: NativeVulkanVideoSessionCodec::H264High8,
            ffmpeg_hwaccel_name: "h264_vulkan",
            ffmpeg_reference: FFMPEG_VULKAN_H264_REFERENCE,
            output_format: "AV_PIX_FMT_VULKAN",
        },
        NativeVulkanFfmpegHwDecodeCodecContract {
            codec: NativeVulkanVideoSessionCodec::H265Main8,
            ffmpeg_hwaccel_name: "hevc_vulkan",
            ffmpeg_reference: FFMPEG_VULKAN_H265_REFERENCE,
            output_format: "AV_PIX_FMT_VULKAN",
        },
        NativeVulkanFfmpegHwDecodeCodecContract {
            codec: NativeVulkanVideoSessionCodec::H265Main10,
            ffmpeg_hwaccel_name: "hevc_vulkan",
            ffmpeg_reference: FFMPEG_VULKAN_H265_REFERENCE,
            output_format: "AV_PIX_FMT_VULKAN",
        },
        NativeVulkanFfmpegHwDecodeCodecContract {
            codec: NativeVulkanVideoSessionCodec::Av1Main8,
            ffmpeg_hwaccel_name: "av1_vulkan",
            ffmpeg_reference: FFMPEG_VULKAN_AV1_REFERENCE,
            output_format: "AV_PIX_FMT_VULKAN",
        },
        NativeVulkanFfmpegHwDecodeCodecContract {
            codec: NativeVulkanVideoSessionCodec::Av1Main10,
            ffmpeg_hwaccel_name: "av1_vulkan",
            ffmpeg_reference: FFMPEG_VULKAN_AV1_REFERENCE,
            output_format: "AV_PIX_FMT_VULKAN",
        },
    ]
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_ffmpeg_vulkan_hw_frame_probe(
    frame: *const AVFrame,
) -> NativeVulkanFfmpegVulkanHwFrameProbe {
    unsafe {
        NativeVulkanFfmpegVulkanHwFrameProbe {
            frame_present: !frame.is_null(),
            frame_format: gilder_av_frame_format(frame),
            expected_vulkan_format: gilder_av_pix_fmt_vulkan(),
            is_vulkan_hw_frame: gilder_av_frame_is_vulkan_hw(frame) != 0,
            vulkan_image_count: gilder_av_frame_vulkan_image_count(frame),
            vulkan_timeline_semaphore_count: gilder_av_frame_vulkan_timeline_semaphore_count(frame),
        }
    }
}

impl<'a> NativeVulkanFfmpegVulkanHwDeviceBorrow<'a> {
    pub(in crate::renderer::native_vulkan) fn snapshot(
        &self,
    ) -> NativeVulkanFfmpegVulkanHwDeviceBorrowSnapshot {
        NativeVulkanFfmpegVulkanHwDeviceBorrowSnapshot {
            binding: "ffmpeg-avhwdevice-vulkan",
            route: "vulkanalia-device-borrow",
            instance_handle_present: self.instance.handle().as_raw() != 0,
            physical_device_handle_present: self.physical_device.as_raw() != 0,
            device_handle_present: self.device.handle().as_raw() != 0,
            enabled_instance_extension_count: self.enabled_instance_extensions.len(),
            enabled_device_extension_count: self.enabled_device_extensions.len(),
            enabled_device_extensions: self
                .enabled_device_extensions
                .iter()
                .map(|extension| (*extension).to_owned())
                .collect(),
            video_queue_family_index: self.video_queue_family_index,
            video_queue_count: self.video_queue_count,
            present_queue_family_index: self.present_queue_family_index,
            present_queue_count: self.present_queue_count,
            present_queue_exposed_to_ffmpeg: true,
            video_codec_operations: native_vulkan_ffmpeg_video_codec_operation_labels(
                self.video_codec_operations,
            ),
            private_ffmpeg_device: false,
        }
    }
}

impl NativeVulkanFfmpegVulkanHwDevice {
    pub(in crate::renderer::native_vulkan) fn borrow_existing(
        borrow: NativeVulkanFfmpegVulkanHwDeviceBorrow<'_>,
    ) -> Result<Self, String> {
        if borrow.video_queue_count == 0 {
            return Err("FFmpeg Vulkan hwdevice borrow requires a video queue".to_owned());
        }
        if borrow.video_codec_operations.is_empty() {
            return Err(
                "FFmpeg Vulkan hwdevice borrow requires at least one decode codec operation"
                    .to_owned(),
            );
        }
        let (instance_extensions, instance_extension_ptrs) =
            native_vulkan_ffmpeg_c_extension_ptrs(borrow.enabled_instance_extensions)?;
        let (device_extensions, device_extension_ptrs) =
            native_vulkan_ffmpeg_c_extension_ptrs(borrow.enabled_device_extensions)?;

        let mut ptr = ptr::null_mut();
        let ret = unsafe {
            gilder_av_hwdevice_ctx_alloc_vulkan_existing(
                &mut ptr,
                borrow.instance.handle().as_raw(),
                borrow.physical_device.as_raw(),
                borrow.device.handle().as_raw(),
                instance_extension_ptrs.as_ptr(),
                instance_extension_ptrs.len() as c_int,
                device_extension_ptrs.as_ptr(),
                device_extension_ptrs.len() as c_int,
                borrow.video_queue_family_index as c_int,
                borrow.video_queue_count as c_int,
                borrow.video_queue_flags.bits(),
                borrow.video_codec_operations.bits(),
                borrow.present_queue_family_index as c_int,
                borrow.present_queue_count as c_int,
                borrow.present_queue_flags.bits(),
            )
        };
        drop(instance_extension_ptrs);
        drop(device_extension_ptrs);
        drop(instance_extensions);
        drop(device_extensions);
        native_vulkan_ffmpeg_hw_ok(ret, "av_hwdevice_ctx_init Vulkan borrow")?;
        let ptr = NonNull::new(ptr).ok_or_else(|| {
            "FFmpeg av_hwdevice_ctx_init Vulkan borrow returned a null AVBufferRef".to_owned()
        })?;
        Ok(Self {
            ptr,
            snapshot: borrow.snapshot(),
        })
    }

    pub(in crate::renderer::native_vulkan) fn as_ptr(&self) -> *mut AVBufferRef {
        self.ptr.as_ptr()
    }

    pub(in crate::renderer::native_vulkan) fn snapshot(
        &self,
    ) -> &NativeVulkanFfmpegVulkanHwDeviceBorrowSnapshot {
        &self.snapshot
    }
}

impl Drop for NativeVulkanFfmpegVulkanHwDevice {
    fn drop(&mut self) {
        let mut ptr = self.ptr.as_ptr();
        unsafe {
            gilder_av_buffer_unref(&mut ptr);
        }
    }
}

impl NativeVulkanFfmpegVulkanHwDecoder {
    pub(in crate::renderer::native_vulkan) fn open(
        source: &Path,
        codec: NativeVulkanVideoSessionCodec,
        hw_device: &NativeVulkanFfmpegVulkanHwDevice,
    ) -> Result<Self, String> {
        unsafe {
            gilder_configure_process_allocator_for_streaming_video();
        }
        let (format, stream_index, time_base) =
            NativeVulkanFfmpegHwFormatContext::open(source, native_vulkan_ffmpeg_codec_id(codec))?;
        let codec_context =
            NativeVulkanFfmpegHwCodecContext::open(&format, stream_index, hw_device)?;
        let pool = NativeVulkanFfmpegHwObjectPool::new()?;
        let packet = NativeVulkanFfmpegHwReusablePacket::new(&pool)?;
        let frame = NativeVulkanFfmpegHwReusableFrame::new(&pool)?;

        Ok(Self {
            codec_context,
            packet,
            frame,
            format,
            pool,
            stream_index,
            time_base,
            codec,
            hw_device_snapshot: hw_device.snapshot().clone(),
            eos_count: 0,
            loop_count: 0,
            sent_packet_count: 0,
            sent_packet_payload_bytes: 0,
            max_packet_size_bytes: 0,
        })
    }

    pub(in crate::renderer::native_vulkan) fn decode_next_frame(
        &mut self,
        loop_on_eos: bool,
    ) -> Result<Option<NativeVulkanFfmpegDecodedGpuFrame>, String> {
        if let Some(frame) = self.receive_next_ready_frame()? {
            return Ok(Some(frame));
        }

        loop {
            let read_ret = unsafe {
                gilder_av_read_frame(self.format.ptr.as_ptr(), self.packet.packet.as_ptr())
            };
            if read_ret == 0 {
                let packet_stream_index =
                    unsafe { gilder_av_packet_stream_index(self.packet.packet.as_ptr()) };
                if packet_stream_index != self.stream_index {
                    self.packet.unref();
                    continue;
                }
                let packet_size = unsafe { gilder_av_packet_size(self.packet.packet.as_ptr()) };
                self.sent_packet_count = self.sent_packet_count.saturating_add(1);
                if packet_size > 0 {
                    self.sent_packet_payload_bytes = self
                        .sent_packet_payload_bytes
                        .saturating_add(packet_size as u64);
                    self.max_packet_size_bytes = self.max_packet_size_bytes.max(packet_size);
                }
                let send_ret = unsafe {
                    gilder_avcodec_send_packet(
                        self.codec_context.ptr.as_ptr(),
                        self.packet.packet.as_ptr(),
                    )
                };
                self.packet.unref();
                if send_ret < 0 && send_ret != native_vulkan_ffmpeg_again() {
                    return Err(native_vulkan_ffmpeg_hw_error(
                        send_ret,
                        "avcodec_send_packet FFmpeg Vulkan hwdecode",
                    ));
                }
                if let Some(frame) = self.receive_next_ready_frame()? {
                    return Ok(Some(frame));
                }
                continue;
            }
            self.packet.unref();

            if read_ret == native_vulkan_ffmpeg_eof() {
                self.eos_count = self.eos_count.saturating_add(1);
                let drain_ret = unsafe {
                    gilder_avcodec_send_packet(self.codec_context.ptr.as_ptr(), ptr::null())
                };
                if drain_ret < 0
                    && drain_ret != native_vulkan_ffmpeg_again()
                    && drain_ret != native_vulkan_ffmpeg_eof()
                {
                    return Err(native_vulkan_ffmpeg_hw_error(
                        drain_ret,
                        "avcodec_send_packet drain FFmpeg Vulkan hwdecode",
                    ));
                }
                if let Some(frame) = self.receive_next_ready_frame()? {
                    return Ok(Some(frame));
                }
                if !loop_on_eos {
                    return Ok(None);
                }
                self.seek_to_start()?;
                continue;
            }

            return Err(native_vulkan_ffmpeg_hw_error(
                read_ret,
                "av_read_frame FFmpeg Vulkan hwdecode",
            ));
        }
    }

    pub(in crate::renderer::native_vulkan) fn snapshot(
        &self,
    ) -> NativeVulkanFfmpegVulkanHwDecoderSnapshot {
        let coded_extent = unsafe {
            (
                gilder_avcodec_context_coded_width(self.codec_context.ptr.as_ptr()),
                gilder_avcodec_context_coded_height(self.codec_context.ptr.as_ptr()),
            )
        };
        let inferred_h264_refstruct_bytes_per_picture =
            native_vulkan_ffmpeg_infer_h264_refstruct_picture_bytes(
                self.codec,
                coded_extent.0,
                coded_extent.1,
            );
        let inferred_h264_refstruct_min_three_picture_bytes =
            inferred_h264_refstruct_bytes_per_picture.saturating_mul(3);
        let inferred_hevc_refstruct_bytes_per_picture =
            native_vulkan_ffmpeg_infer_hevc_refstruct_picture_bytes(
                self.codec,
                coded_extent.0,
                coded_extent.1,
            );
        let inferred_hevc_refstruct_min_three_picture_bytes =
            inferred_hevc_refstruct_bytes_per_picture.saturating_mul(3);
        let inferred_hevc_layer_tables_bytes = native_vulkan_ffmpeg_infer_hevc_layer_table_bytes(
            self.codec,
            coded_extent.0,
            coded_extent.1,
        );
        NativeVulkanFfmpegVulkanHwDecoderSnapshot {
            binding: "ffmpeg-vulkan-hwdecode",
            route: "avcodec-send-receive-avvkframe",
            codec: self.codec,
            decoder_name: self.codec_context.decoder_name.clone(),
            decoder_has_vulkan_hw_config: self.codec_context.decoder_has_vulkan_hw_config,
            stream_index: self.stream_index,
            time_base: (self.time_base.num, self.time_base.den),
            hw_device: self.hw_device_snapshot.clone(),
            software_decode_fallback: false,
            decoded_frame_format: "AV_PIX_FMT_VULKAN",
            coded_extent,
            thread_count: unsafe {
                gilder_avcodec_context_thread_count(self.codec_context.ptr.as_ptr())
            },
            thread_type: unsafe {
                gilder_avcodec_context_thread_type(self.codec_context.ptr.as_ptr())
            },
            active_thread_type: unsafe {
                gilder_avcodec_context_active_thread_type(self.codec_context.ptr.as_ptr())
            },
            extra_hw_frames: unsafe {
                gilder_avcodec_context_extra_hw_frames(self.codec_context.ptr.as_ptr())
            },
            codec_flags: unsafe { gilder_avcodec_context_flags(self.codec_context.ptr.as_ptr()) },
            codec_flags2: unsafe { gilder_avcodec_context_flags2(self.codec_context.ptr.as_ptr()) },
            low_delay_flag: unsafe {
                gilder_avcodec_context_flags(self.codec_context.ptr.as_ptr()) & (1 << 19) != 0
            },
            fast_flag: unsafe {
                gilder_avcodec_context_flags2(self.codec_context.ptr.as_ptr()) & 1 != 0
            },
            has_b_frames: unsafe {
                gilder_avcodec_context_has_b_frames(self.codec_context.ptr.as_ptr())
            },
            codec_delay: unsafe { gilder_avcodec_context_delay(self.codec_context.ptr.as_ptr()) },
            hw_frames_initial_pool_size: unsafe {
                gilder_avcodec_context_hw_frames_initial_pool_size(self.codec_context.ptr.as_ptr())
            },
            h264_enable_er: unsafe {
                gilder_avcodec_context_h264_enable_er(self.codec_context.ptr.as_ptr())
            },
            sent_packet_count: self.sent_packet_count,
            sent_packet_payload_bytes: self.sent_packet_payload_bytes,
            max_packet_size_bytes: self.max_packet_size_bytes,
            bitstream_buffer_model: "ffmpeg-vulkan-per-picture-host-visible-slices-buffer-pool",
            inferred_min_ffmpeg_slice_buffer_slot_bytes:
                native_vulkan_ffmpeg_infer_vulkan_slice_buffer_slot_bytes(
                    self.max_packet_size_bytes,
                ),
            codec_host_memory_model: native_vulkan_ffmpeg_codec_resolution_scaled_host_memory_model(
                self.codec,
            ),
            inferred_codec_resolution_scaled_host_bytes:
                native_vulkan_ffmpeg_infer_codec_resolution_scaled_host_bytes(
                    self.codec,
                    inferred_h264_refstruct_min_three_picture_bytes,
                    inferred_hevc_refstruct_min_three_picture_bytes,
                    inferred_hevc_layer_tables_bytes,
                ),
            h264_refstruct_model: "ffmpeg-h264-hwaccel-still-allocates-per-picture-qscale-mbtype-motionval-refindex",
            inferred_h264_refstruct_bytes_per_picture,
            inferred_h264_refstruct_min_three_picture_bytes,
            hevc_refstruct_model: "ffmpeg-hevc-hwaccel-still-allocates-resolution-scaled-mvfield-refpiclisttab-refstruct-pools-plus-layer-tables",
            inferred_hevc_refstruct_bytes_per_picture,
            inferred_hevc_refstruct_min_three_picture_bytes,
            inferred_hevc_layer_tables_bytes,
        }
    }

    pub(in crate::renderer::native_vulkan) fn eos_count(&self) -> u32 {
        self.eos_count
    }

    pub(in crate::renderer::native_vulkan) fn loop_count(&self) -> u32 {
        self.loop_count
    }

    fn receive_next_ready_frame(
        &mut self,
    ) -> Result<Option<NativeVulkanFfmpegDecodedGpuFrame>, String> {
        loop {
            let receive_ret = unsafe {
                gilder_avcodec_receive_frame(
                    self.codec_context.ptr.as_ptr(),
                    self.frame.frame.as_ptr(),
                )
            };
            if receive_ret == 0 {
                let moved = unsafe {
                    NativeVulkanFfmpegDecodedGpuFrame::move_from_avframe(self.frame.frame.as_ptr())
                };
                if moved.is_err() {
                    self.frame.unref();
                }
                return moved.map(Some);
            }
            self.frame.unref();
            if receive_ret == native_vulkan_ffmpeg_again()
                || receive_ret == native_vulkan_ffmpeg_eof()
            {
                return Ok(None);
            }
            return Err(native_vulkan_ffmpeg_hw_error(
                receive_ret,
                "avcodec_receive_frame FFmpeg Vulkan hwdecode",
            ));
        }
    }

    fn seek_to_start(&mut self) -> Result<(), String> {
        let ret =
            unsafe { gilder_av_seek_stream_start(self.format.ptr.as_ptr(), self.stream_index) };
        native_vulkan_ffmpeg_hw_ok(ret, "av_seek_frame FFmpeg Vulkan hwdecode stream start")?;
        unsafe {
            gilder_avcodec_flush_buffers(self.codec_context.ptr.as_ptr());
        }
        self.loop_count = self.loop_count.saturating_add(1);
        Ok(())
    }
}

include!("ffmpeg_hw/decoder_resources.rs");
