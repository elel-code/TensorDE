use std::ffi::CStr;

use ash::vk;

pub(super) const NATIVE_VULKAN_VIDEO_CODEC_OPERATION_DECODE_VP9: u32 = 0x0000_0008;

pub(super) fn native_vulkan_queue_flag_labels(flags: vk::QueueFlags) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::QueueFlags::GRAPHICS) {
        labels.push("graphics");
    }
    if flags.contains(vk::QueueFlags::COMPUTE) {
        labels.push("compute");
    }
    if flags.contains(vk::QueueFlags::TRANSFER) {
        labels.push("transfer");
    }
    if flags.contains(vk::QueueFlags::SPARSE_BINDING) {
        labels.push("sparse-binding");
    }
    if flags.contains(vk::QueueFlags::VIDEO_DECODE_KHR) {
        labels.push("video-decode");
    }
    if flags.contains(vk::QueueFlags::VIDEO_ENCODE_KHR) {
        labels.push("video-encode");
    }
    labels
}

pub(super) fn native_vulkan_video_codec_operation_labels(
    operations: vk::VideoCodecOperationFlagsKHR,
) -> Vec<String> {
    let raw = operations.as_raw();
    let known = [
        (
            vk::VideoCodecOperationFlagsKHR::DECODE_H264.as_raw(),
            "decode-h264",
        ),
        (
            vk::VideoCodecOperationFlagsKHR::DECODE_H265.as_raw(),
            "decode-h265",
        ),
        (
            vk::VideoCodecOperationFlagsKHR::DECODE_AV1.as_raw(),
            "decode-av1",
        ),
        (NATIVE_VULKAN_VIDEO_CODEC_OPERATION_DECODE_VP9, "decode-vp9"),
    ];
    let known_bits = known.iter().fold(0u32, |bits, (bit, _)| bits | bit);
    let mut labels = known
        .into_iter()
        .filter_map(|(bit, label)| ((raw & bit) != 0).then(|| label.to_owned()))
        .collect::<Vec<_>>();
    let unknown = raw & !known_bits;
    if unknown != 0 {
        labels.push(format!("unknown-0x{unknown:x}"));
    }
    labels
}

pub(super) fn native_vulkan_h264_picture_layout_label(
    layout: vk::VideoDecodeH264PictureLayoutFlagsKHR,
) -> &'static str {
    if layout.contains(vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_INTERLEAVED_LINES) {
        "interlaced-interleaved-lines"
    } else if layout.contains(vk::VideoDecodeH264PictureLayoutFlagsKHR::INTERLACED_SEPARATE_PLANES)
    {
        "interlaced-separate-planes"
    } else if layout.as_raw() == vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE.as_raw() {
        "progressive"
    } else {
        "unknown"
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_image_layout_label(layout: vk::ImageLayout) -> &'static str {
    match layout {
        vk::ImageLayout::UNDEFINED => "undefined",
        vk::ImageLayout::GENERAL => "general",
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL => "color-attachment-optimal",
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL => "transfer-src-optimal",
        vk::ImageLayout::TRANSFER_DST_OPTIMAL => "transfer-dst-optimal",
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL => "shader-read-only-optimal",
        vk::ImageLayout::PRESENT_SRC_KHR => "present-src",
        vk::ImageLayout::VIDEO_DECODE_DPB_KHR => "video-decode-dpb",
        _ => "other",
    }
}

pub(super) fn native_vulkan_video_chroma_subsampling_labels(
    flags: vk::VideoChromaSubsamplingFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::MONOCHROME) {
        labels.push("monochrome");
    }
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420) {
        labels.push("420");
    }
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::TYPE_422) {
        labels.push("422");
    }
    if flags.contains(vk::VideoChromaSubsamplingFlagsKHR::TYPE_444) {
        labels.push("444");
    }
    labels
}

pub(super) fn native_vulkan_video_component_bit_depth_labels(
    flags: vk::VideoComponentBitDepthFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoComponentBitDepthFlagsKHR::TYPE_8) {
        labels.push("8-bit");
    }
    if flags.contains(vk::VideoComponentBitDepthFlagsKHR::TYPE_10) {
        labels.push("10-bit");
    }
    if flags.contains(vk::VideoComponentBitDepthFlagsKHR::TYPE_12) {
        labels.push("12-bit");
    }
    labels
}

pub(super) fn native_vulkan_video_capability_flag_labels(
    flags: vk::VideoCapabilityFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoCapabilityFlagsKHR::PROTECTED_CONTENT) {
        labels.push("protected-content");
    }
    if flags.contains(vk::VideoCapabilityFlagsKHR::SEPARATE_REFERENCE_IMAGES) {
        labels.push("separate-reference-images");
    }
    labels
}

pub(super) fn native_vulkan_video_decode_capability_flag_labels(
    flags: vk::VideoDecodeCapabilityFlagsKHR,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE) {
        labels.push("dpb-and-output-coincide");
    }
    if flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_DISTINCT) {
        labels.push("dpb-and-output-distinct");
    }
    labels
}

pub(super) fn native_vulkan_h264_level_label(
    level: vk::native::StdVideoH264LevelIdc,
) -> Option<&'static str> {
    match level {
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_0 => Some("1.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_1 => Some("1.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_2 => Some("1.2"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_1_3 => Some("1.3"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_0 => Some("2.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_1 => Some("2.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_2_2 => Some("2.2"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_0 => Some("3.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_1 => Some("3.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_3_2 => Some("3.2"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_0 => Some("4.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_1 => Some("4.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_4_2 => Some("4.2"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_0 => Some("5.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_1 => Some("5.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_5_2 => Some("5.2"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_0 => Some("6.0"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_1 => Some("6.1"),
        vk::native::StdVideoH264LevelIdc_STD_VIDEO_H264_LEVEL_IDC_6_2 => Some("6.2"),
        _ => None,
    }
}

pub(super) fn native_vulkan_h265_level_label(
    level: vk::native::StdVideoH265LevelIdc,
) -> Option<&'static str> {
    match level {
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_1_0 => Some("1.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_0 => Some("2.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_2_1 => Some("2.1"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_0 => Some("3.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_3_1 => Some("3.1"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_0 => Some("4.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_4_1 => Some("4.1"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_0 => Some("5.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_1 => Some("5.1"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_5_2 => Some("5.2"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_0 => Some("6.0"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_1 => Some("6.1"),
        vk::native::StdVideoH265LevelIdc_STD_VIDEO_H265_LEVEL_IDC_6_2 => Some("6.2"),
        _ => None,
    }
}

pub(super) fn native_vulkan_av1_level_label(
    level: vk::native::StdVideoAV1Level,
) -> Option<&'static str> {
    match level {
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_2_0 => Some("2.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_2_1 => Some("2.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_2_2 => Some("2.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_2_3 => Some("2.3"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_3_0 => Some("3.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_3_1 => Some("3.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_3_2 => Some("3.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_3_3 => Some("3.3"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_4_0 => Some("4.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_4_1 => Some("4.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_4_2 => Some("4.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_4_3 => Some("4.3"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_5_0 => Some("5.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_5_1 => Some("5.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_5_2 => Some("5.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_5_3 => Some("5.3"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_6_0 => Some("6.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_6_1 => Some("6.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_6_2 => Some("6.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_6_3 => Some("6.3"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_7_0 => Some("7.0"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_7_1 => Some("7.1"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_7_2 => Some("7.2"),
        vk::native::StdVideoAV1Level_STD_VIDEO_AV1_LEVEL_7_3 => Some("7.3"),
        _ => None,
    }
}

pub(super) fn native_vulkan_format_label(format: vk::Format) -> &'static str {
    match format {
        vk::Format::G8_B8R8_2PLANE_420_UNORM => "G8_B8R8_2PLANE_420_UNORM",
        vk::Format::G8_B8_R8_3PLANE_420_UNORM => "G8_B8_R8_3PLANE_420_UNORM",
        vk::Format::G8_B8R8_2PLANE_422_UNORM => "G8_B8R8_2PLANE_422_UNORM",
        vk::Format::G8_B8_R8_3PLANE_422_UNORM => "G8_B8_R8_3PLANE_422_UNORM",
        vk::Format::G8_B8R8_2PLANE_444_UNORM => "G8_B8R8_2PLANE_444_UNORM",
        vk::Format::G8_B8_R8_3PLANE_444_UNORM => "G8_B8_R8_3PLANE_444_UNORM",
        vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => {
            "G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16"
        }
        vk::Format::G10X6_B10X6_R10X6_3PLANE_420_UNORM_3PACK16 => {
            "G10X6_B10X6_R10X6_3PLANE_420_UNORM_3PACK16"
        }
        vk::Format::G16_B16R16_2PLANE_420_UNORM => "G16_B16R16_2PLANE_420_UNORM",
        vk::Format::R8G8B8A8_UNORM => "R8G8B8A8_UNORM",
        vk::Format::B8G8R8A8_UNORM => "B8G8R8A8_UNORM",
        vk::Format::R8_UNORM => "R8_UNORM",
        vk::Format::R8G8_UNORM => "R8G8_UNORM",
        vk::Format::R16_UNORM => "R16_UNORM",
        vk::Format::R16G16_UNORM => "R16G16_UNORM",
        _ => "unknown",
    }
}

pub(super) fn native_vulkan_image_type_label(image_type: vk::ImageType) -> &'static str {
    match image_type {
        vk::ImageType::TYPE_1D => "1d",
        vk::ImageType::TYPE_2D => "2d",
        vk::ImageType::TYPE_3D => "3d",
        _ => "unknown",
    }
}

pub(super) fn native_vulkan_image_tiling_label(image_tiling: vk::ImageTiling) -> &'static str {
    match image_tiling {
        vk::ImageTiling::OPTIMAL => "optimal",
        vk::ImageTiling::LINEAR => "linear",
        vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT => "drm-format-modifier",
        _ => "unknown",
    }
}

pub(super) fn native_vulkan_image_usage_flag_labels(
    flags: vk::ImageUsageFlags,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::ImageUsageFlags::TRANSFER_SRC) {
        labels.push("transfer-src");
    }
    if flags.contains(vk::ImageUsageFlags::TRANSFER_DST) {
        labels.push("transfer-dst");
    }
    if flags.contains(vk::ImageUsageFlags::SAMPLED) {
        labels.push("sampled");
    }
    if flags.contains(vk::ImageUsageFlags::STORAGE) {
        labels.push("storage");
    }
    if flags.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT) {
        labels.push("color-attachment");
    }
    if flags.contains(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT) {
        labels.push("depth-stencil-attachment");
    }
    if flags.contains(vk::ImageUsageFlags::TRANSIENT_ATTACHMENT) {
        labels.push("transient-attachment");
    }
    if flags.contains(vk::ImageUsageFlags::INPUT_ATTACHMENT) {
        labels.push("input-attachment");
    }
    if flags.contains(vk::ImageUsageFlags::VIDEO_DECODE_SRC_KHR) {
        labels.push("video-decode-src");
    }
    if flags.contains(vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR) {
        labels.push("video-decode-dst");
    }
    if flags.contains(vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR) {
        labels.push("video-decode-dpb");
    }
    labels
}

pub(super) fn native_vulkan_buffer_usage_flag_labels(
    flags: vk::BufferUsageFlags,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::BufferUsageFlags::TRANSFER_SRC) {
        labels.push("transfer-src");
    }
    if flags.contains(vk::BufferUsageFlags::TRANSFER_DST) {
        labels.push("transfer-dst");
    }
    if flags.contains(vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER) {
        labels.push("uniform-texel-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::STORAGE_TEXEL_BUFFER) {
        labels.push("storage-texel-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::UNIFORM_BUFFER) {
        labels.push("uniform-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::STORAGE_BUFFER) {
        labels.push("storage-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::INDEX_BUFFER) {
        labels.push("index-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::VERTEX_BUFFER) {
        labels.push("vertex-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::INDIRECT_BUFFER) {
        labels.push("indirect-buffer");
    }
    if flags.contains(vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR) {
        labels.push("video-decode-src");
    }
    labels
}

pub(super) fn native_vulkan_image_create_flag_labels(
    flags: vk::ImageCreateFlags,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::ImageCreateFlags::SPARSE_BINDING) {
        labels.push("sparse-binding");
    }
    if flags.contains(vk::ImageCreateFlags::SPARSE_RESIDENCY) {
        labels.push("sparse-residency");
    }
    if flags.contains(vk::ImageCreateFlags::SPARSE_ALIASED) {
        labels.push("sparse-aliased");
    }
    if flags.contains(vk::ImageCreateFlags::MUTABLE_FORMAT) {
        labels.push("mutable-format");
    }
    if flags.contains(vk::ImageCreateFlags::CUBE_COMPATIBLE) {
        labels.push("cube-compatible");
    }
    if flags.contains(vk::ImageCreateFlags::DISJOINT) {
        labels.push("disjoint");
    }
    labels
}

pub(super) fn native_vulkan_memory_property_flag_labels(
    flags: vk::MemoryPropertyFlags,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL) {
        labels.push("device-local");
    }
    if flags.contains(vk::MemoryPropertyFlags::HOST_VISIBLE) {
        labels.push("host-visible");
    }
    if flags.contains(vk::MemoryPropertyFlags::HOST_COHERENT) {
        labels.push("host-coherent");
    }
    if flags.contains(vk::MemoryPropertyFlags::HOST_CACHED) {
        labels.push("host-cached");
    }
    if flags.contains(vk::MemoryPropertyFlags::LAZILY_ALLOCATED) {
        labels.push("lazily-allocated");
    }
    if flags.contains(vk::MemoryPropertyFlags::PROTECTED) {
        labels.push("protected");
    }
    labels
}

pub(super) fn native_vulkan_extension_properties_name(
    properties: &vk::ExtensionProperties,
) -> String {
    unsafe { CStr::from_ptr(properties.extension_name.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

pub(super) fn native_vulkan_api_version_label(version: u32) -> String {
    format!(
        "{}.{}.{}",
        vk::api_version_major(version),
        vk::api_version_minor(version),
        vk::api_version_patch(version)
    )
}
