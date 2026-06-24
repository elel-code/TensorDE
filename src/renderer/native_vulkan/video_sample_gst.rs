//! GStreamer decoded video sample shape extraction.
//!
//! This module turns provider-specific `gst::Sample` caps/meta into the stable
//! NV12/P010 plane shape used by CUDA, VA/DMABuf and system-memory importers.

use ash::vk;
use gstreamer as gst;
use gstreamer_video as gst_video;

use super::{
    CUDA_ARRAY_FORMAT_UNSIGNED_INT8, CUDA_ARRAY_FORMAT_UNSIGNED_INT16, DRM_FORMAT_NV12,
    DRM_FORMAT_P010, NativeVulkanError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeVulkanVideoSampleFormat {
    Nv12,
    P010,
}

impl NativeVulkanVideoSampleFormat {
    pub(super) fn from_caps_format(format: &str) -> Option<Self> {
        match format {
            "NV12" => Some(Self::Nv12),
            "P010_10LE" | "P010" => Some(Self::P010),
            _ => None,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Nv12 => "NV12",
            Self::P010 => "P010_10LE",
        }
    }

    pub(super) fn vulkan_image_format(self) -> vk::Format {
        match self {
            Self::Nv12 => vk::Format::G8_B8R8_2PLANE_420_UNORM,
            Self::P010 => vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
        }
    }

    pub(super) fn y_view_format(self) -> vk::Format {
        match self {
            Self::Nv12 => vk::Format::R8_UNORM,
            Self::P010 => vk::Format::R16_UNORM,
        }
    }

    pub(super) fn uv_view_format(self) -> vk::Format {
        match self {
            Self::Nv12 => vk::Format::R8G8_UNORM,
            Self::P010 => vk::Format::R16G16_UNORM,
        }
    }

    pub(super) fn bytes_per_component(self) -> u32 {
        match self {
            Self::Nv12 => 1,
            Self::P010 => 2,
        }
    }

    pub(super) fn cuda_array_format(self) -> u32 {
        match self {
            Self::Nv12 => CUDA_ARRAY_FORMAT_UNSIGNED_INT8,
            Self::P010 => CUDA_ARRAY_FORMAT_UNSIGNED_INT16,
        }
    }

    pub(super) fn drm_format(self) -> u32 {
        match self {
            Self::Nv12 => DRM_FORMAT_NV12,
            Self::P010 => DRM_FORMAT_P010,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanGstSystemNv12Plane {
    pub(super) offset: usize,
    pub(super) stride: u32,
    pub(super) height: u32,
    pub(super) row_bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanGstSystemNv12Meta {
    pub(super) format: NativeVulkanVideoSampleFormat,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) y: NativeVulkanGstSystemNv12Plane,
    pub(super) uv: NativeVulkanGstSystemNv12Plane,
}

pub(super) fn native_vulkan_gst_system_yuv420_meta(
    sample: &gst::Sample,
    buffer: &gst::BufferRef,
) -> Result<NativeVulkanGstSystemNv12Meta, NativeVulkanError> {
    let meta = match native_vulkan_gst_yuv420_meta_from_video_meta(sample.caps(), buffer) {
        Ok(meta) => meta,
        Err(meta_err) => native_vulkan_gst_yuv420_meta_from_caps(sample)
            .map_err(|caps_err| NativeVulkanError::Video(format!("{meta_err};{caps_err}")))?,
    };
    Ok(meta)
}

fn native_vulkan_gst_yuv420_meta_from_video_meta(
    caps: Option<&gst::CapsRef>,
    buffer: &gst::BufferRef,
) -> Result<NativeVulkanGstSystemNv12Meta, String> {
    let meta = buffer
        .meta::<gst_video::VideoMeta>()
        .ok_or_else(|| "appsink buffer has no GstVideoMeta".to_owned())?;
    let caps_format = caps
        .and_then(|caps| caps.structure(0))
        .and_then(|structure| structure.get::<String>("format").ok())
        .unwrap_or_else(|| meta.format().to_str().to_string());
    let format = NativeVulkanVideoSampleFormat::from_caps_format(&caps_format)
        .ok_or_else(|| format!("expected NV12 or P010 appsink frame, got {caps_format}"))?;
    let width = meta.width();
    let height = meta.height();
    if width == 0 || height == 0 {
        return Err("NV12 frame has zero dimension".to_owned());
    }
    if width % 2 != 0 || height % 2 != 0 {
        return Err(format!(
            "{} frame dimensions must be even, got {width}x{height}",
            format.label()
        ));
    }
    if meta.offset().len() < 2 || meta.stride().len() < 2 {
        return Err(format!(
            "{} frame needs 2 planes, got offsets={} strides={}",
            format.label(),
            meta.offset().len(),
            meta.stride().len()
        ));
    }
    let y_stride =
        native_vulkan_positive_stride(format!("{} y", format.label()), meta.stride()[0])?;
    let uv_stride =
        native_vulkan_positive_stride(format!("{} uv", format.label()), meta.stride()[1])?;
    let row_bytes = width
        .checked_mul(format.bytes_per_component())
        .ok_or_else(|| format!("{} row byte width overflow", format.label()))?;
    if y_stride < row_bytes || uv_stride < row_bytes {
        return Err(format!(
            "{} stride too small: y={y_stride} uv={uv_stride} row_bytes={row_bytes}",
            format.label()
        ));
    }
    Ok(NativeVulkanGstSystemNv12Meta {
        format,
        width,
        height,
        y: NativeVulkanGstSystemNv12Plane {
            offset: meta.offset()[0],
            stride: y_stride,
            height,
            row_bytes,
        },
        uv: NativeVulkanGstSystemNv12Plane {
            offset: meta.offset()[1],
            stride: uv_stride,
            height: height / 2,
            row_bytes,
        },
    })
}

fn native_vulkan_gst_yuv420_meta_from_caps(
    sample: &gst::Sample,
) -> Result<NativeVulkanGstSystemNv12Meta, String> {
    let caps = sample
        .caps()
        .ok_or_else(|| "appsink sample has no caps".to_owned())?;
    let structure = caps
        .structure(0)
        .ok_or_else(|| "appsink caps has no structure".to_owned())?;
    let format = structure
        .get::<String>("format")
        .unwrap_or_else(|_| "unknown".to_owned());
    let format = NativeVulkanVideoSampleFormat::from_caps_format(&format)
        .ok_or_else(|| format!("caps fallback expected NV12 or P010, got {format}"))?;
    let width = structure
        .get::<i32>("width")
        .map_err(|_| "appsink caps missing width".to_owned())
        .and_then(|width| {
            u32::try_from(width)
                .ok()
                .filter(|width| *width > 0)
                .ok_or_else(|| "invalid appsink frame width".to_owned())
        })?;
    let height = structure
        .get::<i32>("height")
        .map_err(|_| "appsink caps missing height".to_owned())
        .and_then(|height| {
            u32::try_from(height)
                .ok()
                .filter(|height| *height > 0)
                .ok_or_else(|| "invalid appsink frame height".to_owned())
        })?;
    if width % 2 != 0 || height % 2 != 0 {
        return Err(format!(
            "{} frame dimensions must be even, got {width}x{height}",
            format.label()
        ));
    }
    let row_bytes = width
        .checked_mul(format.bytes_per_component())
        .ok_or_else(|| format!("{} row byte width overflow", format.label()))?;
    let y_size = usize::try_from(u64::from(row_bytes) * u64::from(height))
        .map_err(|_| format!("{} plane offset overflow", format.label()))?;
    Ok(NativeVulkanGstSystemNv12Meta {
        format,
        width,
        height,
        y: NativeVulkanGstSystemNv12Plane {
            offset: 0,
            stride: row_bytes,
            height,
            row_bytes,
        },
        uv: NativeVulkanGstSystemNv12Plane {
            offset: y_size,
            stride: row_bytes,
            height: height / 2,
            row_bytes,
        },
    })
}

fn native_vulkan_positive_stride(label: String, stride: i32) -> Result<u32, String> {
    u32::try_from(stride)
        .ok()
        .filter(|stride| *stride > 0)
        .ok_or_else(|| format!("{label} stride must be positive, got {stride}"))
}
