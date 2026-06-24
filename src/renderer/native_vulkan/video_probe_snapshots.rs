//! Snapshot data types extracted from the native Vulkan renderer.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSurfaceProbeSnapshot {
    pub wayland_surface_logical_size: (u32, u32),
    pub wayland_surface_buffer_size: (u32, u32),
    pub dmabuf_main_device: Option<u64>,
    pub physical_device_count: usize,
    pub present_queue_family_count: usize,
    pub selected_physical_device_index: Option<usize>,
    pub selected_physical_device_name: Option<String>,
    pub selected_physical_device_type: Option<&'static str>,
    pub selected_queue_family_index: Option<u32>,
    pub selected_queue_count: Option<u32>,
    pub selected_queue_flags: Vec<&'static str>,
    pub selected_queue_supports_graphics: bool,
    pub selected_queue_supports_video_decode: bool,
    pub selected_queue_supports_h265_decode: bool,
    pub selected_queue_video_codec_operation_bits: u32,
    pub selected_queue_video_codec_operations: Vec<String>,
    pub selected_device_has_video_queue_extension: bool,
    pub selected_device_has_video_decode_queue_extension: bool,
    pub selected_device_has_h265_decode_extension: bool,
    pub selected_device_decode_codec_extensions: Vec<String>,
    pub same_device_h265_decode_queue_family_index: Option<u32>,
    pub same_device_h265_decode_queue_count: Option<u32>,
    pub same_device_h265_decode_queue_flags: Vec<&'static str>,
    pub same_device_h265_decode_queue_video_codec_operations: Vec<String>,
    pub h265_decode_requires_cross_queue_sync: bool,
    pub surface_capabilities: Option<NativeVulkanSurfaceCapabilitiesSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSurfaceCapabilitiesSnapshot {
    pub min_image_count: u32,
    pub max_image_count: u32,
    pub current_extent: Option<(u32, u32)>,
    pub min_image_extent: (u32, u32),
    pub max_image_extent: (u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeProbeSnapshot {
    pub physical_device_count: usize,
    pub devices: Vec<NativeVulkanVideoDecodeDeviceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeDeviceSnapshot {
    pub physical_device_index: usize,
    pub physical_device_name: String,
    pub physical_device_type: &'static str,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: String,
    pub driver_version: u32,
    pub has_video_queue_extension: bool,
    pub has_video_decode_queue_extension: bool,
    pub decode_codec_extensions: Vec<String>,
    pub has_video_decode_queue_family: bool,
    pub video_decode_ready: bool,
    pub h264_direct_decode_ready: bool,
    pub h264_zero_copy_sampled_candidate: bool,
    pub h264_profiles: Vec<NativeVulkanVideoDecodeH264ProfileSnapshot>,
    pub h265_direct_decode_ready: bool,
    pub h265_zero_copy_sampled_candidate: bool,
    pub h265_profiles: Vec<NativeVulkanVideoDecodeH265ProfileSnapshot>,
    pub av1_direct_decode_ready: bool,
    pub av1_zero_copy_sampled_candidate: bool,
    pub av1_profiles: Vec<NativeVulkanVideoDecodeAv1ProfileSnapshot>,
    pub queue_families: Vec<NativeVulkanVideoDecodeQueueFamilySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeQueueFamilySnapshot {
    pub queue_family_index: u32,
    pub queue_count: u32,
    pub queue_flags: Vec<&'static str>,
    pub video_codec_operation_bits: u32,
    pub video_codec_operations: Vec<String>,
    pub query_result_status_support: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeH264ProfileSnapshot {
    pub profile: &'static str,
    pub std_profile_idc: u32,
    pub picture_layout: &'static str,
    pub chroma_subsampling: Vec<&'static str>,
    pub luma_bit_depth: Vec<&'static str>,
    pub chroma_bit_depth: Vec<&'static str>,
    pub supported: bool,
    pub max_level_idc: Option<u32>,
    pub max_level: Option<&'static str>,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: Option<u64>,
    pub min_bitstream_buffer_size_alignment: Option<u64>,
    pub picture_access_granularity: Option<(u32, u32)>,
    pub min_coded_extent: Option<(u32, u32)>,
    pub max_coded_extent: Option<(u32, u32)>,
    pub max_dpb_slots: Option<u32>,
    pub max_active_reference_pictures: Option<u32>,
    pub field_offset_granularity: Option<(i32, i32)>,
    pub dpb_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub sampled_output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub nv12_dpb_supported: bool,
    pub nv12_output_supported: bool,
    pub nv12_sampled_output_supported: bool,
    pub query_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeH265ProfileSnapshot {
    pub profile: &'static str,
    pub std_profile_idc: u32,
    pub chroma_subsampling: Vec<&'static str>,
    pub luma_bit_depth: Vec<&'static str>,
    pub chroma_bit_depth: Vec<&'static str>,
    pub supported: bool,
    pub max_level_idc: Option<u32>,
    pub max_level: Option<&'static str>,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: Option<u64>,
    pub min_bitstream_buffer_size_alignment: Option<u64>,
    pub picture_access_granularity: Option<(u32, u32)>,
    pub min_coded_extent: Option<(u32, u32)>,
    pub max_coded_extent: Option<(u32, u32)>,
    pub max_dpb_slots: Option<u32>,
    pub max_active_reference_pictures: Option<u32>,
    pub dpb_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub sampled_output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub nv12_dpb_supported: bool,
    pub nv12_output_supported: bool,
    pub nv12_sampled_output_supported: bool,
    pub query_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoDecodeAv1ProfileSnapshot {
    pub profile: &'static str,
    pub std_profile: u32,
    pub film_grain_support: bool,
    pub chroma_subsampling: Vec<&'static str>,
    pub luma_bit_depth: Vec<&'static str>,
    pub chroma_bit_depth: Vec<&'static str>,
    pub supported: bool,
    pub max_level: Option<&'static str>,
    pub max_level_raw: Option<u32>,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: Option<u64>,
    pub min_bitstream_buffer_size_alignment: Option<u64>,
    pub picture_access_granularity: Option<(u32, u32)>,
    pub min_coded_extent: Option<(u32, u32)>,
    pub max_coded_extent: Option<(u32, u32)>,
    pub max_dpb_slots: Option<u32>,
    pub max_active_reference_pictures: Option<u32>,
    pub dpb_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub sampled_output_formats: Vec<NativeVulkanVideoFormatPropertiesSnapshot>,
    pub nv12_dpb_supported: bool,
    pub nv12_output_supported: bool,
    pub nv12_sampled_output_supported: bool,
    pub query_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVideoFormatPropertiesSnapshot {
    pub format: &'static str,
    pub format_raw: i32,
    pub image_type: &'static str,
    pub image_tiling: &'static str,
    pub image_usage_flags: Vec<&'static str>,
    pub image_create_flags: Vec<&'static str>,
}
