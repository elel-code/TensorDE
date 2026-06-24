use std::ptr;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::video_profile_gate::query_disabled_reason;
use super::video_profile_info::{
    with_vulkanalia_av1_video_profile_info, with_vulkanalia_h264_video_profile_info,
    with_vulkanalia_h265_video_profile_info,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoFormatProbeSnapshot {
    pub decode_output_sampled_formats: Vec<NativeVulkanVulkanaliaVideoFormatQuerySnapshot>,
    pub dpb_formats: Vec<NativeVulkanVulkanaliaVideoFormatQuerySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoFormatQuerySnapshot {
    pub codec: &'static str,
    pub profile: &'static str,
    pub image_usage: &'static str,
    pub image_usage_bits: u32,
    pub supported_format_count: usize,
    pub formats: Vec<NativeVulkanVulkanaliaVideoFormatPropertySnapshot>,
    pub query_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoFormatPropertySnapshot {
    pub format: String,
    pub image_type: String,
    pub image_tiling: String,
    pub image_create_flags: Vec<&'static str>,
    pub image_create_flag_bits: u32,
    pub image_usage_flags: Vec<&'static str>,
    pub image_usage_flag_bits: u32,
    pub component_mapping: String,
}

pub(super) fn native_vulkan_vulkanalia_video_format_probe(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    has_video_decode_queue_family: bool,
) -> NativeVulkanVulkanaliaVideoFormatProbeSnapshot {
    let decode_output_sampled_usage =
        vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR | vk::ImageUsageFlags::SAMPLED;
    let dpb_usage = vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR;

    NativeVulkanVulkanaliaVideoFormatProbeSnapshot {
        decode_output_sampled_formats: query_current_profiles_for_usage(
            instance,
            physical_device,
            device_extensions,
            has_video_decode_queue_family,
            "video-decode-dst-sampled",
            decode_output_sampled_usage,
        ),
        dpb_formats: query_current_profiles_for_usage(
            instance,
            physical_device,
            device_extensions,
            has_video_decode_queue_family,
            "video-decode-dpb",
            dpb_usage,
        ),
    }
}

fn query_current_profiles_for_usage(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    has_video_decode_queue_family: bool,
    image_usage_label: &'static str,
    image_usage: vk::ImageUsageFlags,
) -> Vec<NativeVulkanVulkanaliaVideoFormatQuerySnapshot> {
    let mut snapshots = Vec::with_capacity(7);
    snapshots.extend(h264_format_queries(
        instance,
        physical_device,
        device_extensions,
        has_video_decode_queue_family,
        image_usage_label,
        image_usage,
    ));
    snapshots.extend(h265_format_queries(
        instance,
        physical_device,
        device_extensions,
        has_video_decode_queue_family,
        image_usage_label,
        image_usage,
    ));
    snapshots.extend(av1_format_queries(
        instance,
        physical_device,
        device_extensions,
        has_video_decode_queue_family,
        image_usage_label,
        image_usage,
    ));
    snapshots
}

fn h264_format_queries(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    has_video_decode_queue_family: bool,
    image_usage_label: &'static str,
    image_usage: vk::ImageUsageFlags,
) -> Vec<NativeVulkanVulkanaliaVideoFormatQuerySnapshot> {
    [
        ("baseline", vk::video::STD_VIDEO_H264_PROFILE_IDC_BASELINE),
        ("main", vk::video::STD_VIDEO_H264_PROFILE_IDC_MAIN),
        ("high", vk::video::STD_VIDEO_H264_PROFILE_IDC_HIGH),
    ]
    .into_iter()
    .map(|(profile, std_profile_idc)| {
        if let Some(error) = query_disabled_reason(
            device_extensions,
            has_video_decode_queue_family,
            "VK_KHR_video_decode_h264",
        ) {
            return unsupported_format_query(
                "h264",
                profile,
                image_usage_label,
                image_usage,
                error,
            );
        }

        with_vulkanalia_h264_video_profile_info(
            std_profile_idc,
            vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE,
            |profile_info, _| {
                query_format_properties_for_profile(
                    instance,
                    physical_device,
                    "h264",
                    profile,
                    image_usage_label,
                    image_usage,
                    profile_info,
                )
            },
        )
    })
    .collect()
}

fn h265_format_queries(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    has_video_decode_queue_family: bool,
    image_usage_label: &'static str,
    image_usage: vk::ImageUsageFlags,
) -> Vec<NativeVulkanVulkanaliaVideoFormatQuerySnapshot> {
    [
        (
            "main-8",
            vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN,
            vk::VideoComponentBitDepthFlagsKHR::_8,
        ),
        (
            "main-10",
            vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN_10,
            vk::VideoComponentBitDepthFlagsKHR::_10,
        ),
    ]
    .into_iter()
    .map(|(profile, std_profile_idc, bit_depth)| {
        if let Some(error) = query_disabled_reason(
            device_extensions,
            has_video_decode_queue_family,
            "VK_KHR_video_decode_h265",
        ) {
            return unsupported_format_query(
                "h265",
                profile,
                image_usage_label,
                image_usage,
                error,
            );
        }

        with_vulkanalia_h265_video_profile_info(std_profile_idc, bit_depth, |profile_info, _| {
            query_format_properties_for_profile(
                instance,
                physical_device,
                "h265",
                profile,
                image_usage_label,
                image_usage,
                profile_info,
            )
        })
    })
    .collect()
}

fn av1_format_queries(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
    has_video_decode_queue_family: bool,
    image_usage_label: &'static str,
    image_usage: vk::ImageUsageFlags,
) -> Vec<NativeVulkanVulkanaliaVideoFormatQuerySnapshot> {
    [
        ("main-8", vk::VideoComponentBitDepthFlagsKHR::_8),
        ("main-10", vk::VideoComponentBitDepthFlagsKHR::_10),
    ]
    .into_iter()
    .map(|(profile, bit_depth)| {
        if let Some(error) = query_disabled_reason(
            device_extensions,
            has_video_decode_queue_family,
            "VK_KHR_video_decode_av1",
        ) {
            return unsupported_format_query("av1", profile, image_usage_label, image_usage, error);
        }

        with_vulkanalia_av1_video_profile_info(bit_depth, false, |profile_info, _| {
            query_format_properties_for_profile(
                instance,
                physical_device,
                "av1",
                profile,
                image_usage_label,
                image_usage,
                profile_info,
            )
        })
    })
    .collect()
}

fn query_format_properties_for_profile(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    codec: &'static str,
    profile: &'static str,
    image_usage_label: &'static str,
    image_usage: vk::ImageUsageFlags,
    profile_info: &vk::VideoProfileInfoKHR,
) -> NativeVulkanVulkanaliaVideoFormatQuerySnapshot {
    match native_vulkan_vulkanalia_video_format_properties_for_profile(
        instance,
        physical_device,
        profile_info,
        image_usage,
    ) {
        Ok(properties) => {
            let formats = properties
                .into_iter()
                .map(video_format_property_snapshot)
                .collect::<Vec<_>>();
            NativeVulkanVulkanaliaVideoFormatQuerySnapshot {
                codec,
                profile,
                image_usage: image_usage_label,
                image_usage_bits: image_usage.bits(),
                supported_format_count: formats.len(),
                formats,
                query_error: None,
            }
        }
        Err(error) => {
            unsupported_format_query(codec, profile, image_usage_label, image_usage, error)
        }
    }
}

pub(super) fn native_vulkan_vulkanalia_video_format_properties_for_profile(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR,
    image_usage: vk::ImageUsageFlags,
) -> Result<Vec<vk::VideoFormatPropertiesKHR>, String> {
    let profiles = [*profile_info];
    let mut profile_list_info = vk::VideoProfileListInfoKHR::builder()
        .profiles(&profiles)
        .build();
    let video_format_info = vk::PhysicalDeviceVideoFormatInfoKHR::builder()
        .image_usage(image_usage)
        .push_next(&mut profile_list_info)
        .build();

    let mut property_count = 0;
    let result = unsafe {
        (instance
            .commands()
            .get_physical_device_video_format_properties_khr)(
            physical_device,
            &video_format_info,
            &mut property_count,
            ptr::null_mut(),
        )
    };
    if result != vk::Result::SUCCESS {
        return Err(format!(
            "vkGetPhysicalDeviceVideoFormatPropertiesKHR(count): {result:?}"
        ));
    }
    if property_count == 0 {
        return Ok(Vec::new());
    }

    let mut properties = vec![vk::VideoFormatPropertiesKHR::default(); property_count as usize];
    let result = unsafe {
        (instance
            .commands()
            .get_physical_device_video_format_properties_khr)(
            physical_device,
            &video_format_info,
            &mut property_count,
            properties.as_mut_ptr(),
        )
    };
    if result != vk::Result::SUCCESS {
        return Err(format!(
            "vkGetPhysicalDeviceVideoFormatPropertiesKHR(values): {result:?}"
        ));
    }
    properties.truncate(property_count as usize);
    Ok(properties)
}

fn video_format_property_snapshot(
    property: vk::VideoFormatPropertiesKHR,
) -> NativeVulkanVulkanaliaVideoFormatPropertySnapshot {
    NativeVulkanVulkanaliaVideoFormatPropertySnapshot {
        format: format!("{:?}", property.format),
        image_type: format!("{:?}", property.image_type),
        image_tiling: format!("{:?}", property.image_tiling),
        image_create_flags: image_create_flag_labels(property.image_create_flags),
        image_create_flag_bits: property.image_create_flags.bits(),
        image_usage_flags: image_usage_flag_labels(property.image_usage_flags),
        image_usage_flag_bits: property.image_usage_flags.bits(),
        component_mapping: format!("{:?}", property.component_mapping),
    }
}

fn unsupported_format_query(
    codec: &'static str,
    profile: &'static str,
    image_usage_label: &'static str,
    image_usage: vk::ImageUsageFlags,
    query_error: String,
) -> NativeVulkanVulkanaliaVideoFormatQuerySnapshot {
    NativeVulkanVulkanaliaVideoFormatQuerySnapshot {
        codec,
        profile,
        image_usage: image_usage_label,
        image_usage_bits: image_usage.bits(),
        supported_format_count: 0,
        formats: Vec::new(),
        query_error: Some(query_error),
    }
}

fn image_usage_flag_labels(flags: vk::ImageUsageFlags) -> Vec<&'static str> {
    [
        (vk::ImageUsageFlags::TRANSFER_SRC, "transfer-src"),
        (vk::ImageUsageFlags::TRANSFER_DST, "transfer-dst"),
        (vk::ImageUsageFlags::SAMPLED, "sampled"),
        (vk::ImageUsageFlags::STORAGE, "storage"),
        (vk::ImageUsageFlags::COLOR_ATTACHMENT, "color-attachment"),
        (
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            "depth-stencil-attachment",
        ),
        (
            vk::ImageUsageFlags::TRANSIENT_ATTACHMENT,
            "transient-attachment",
        ),
        (vk::ImageUsageFlags::INPUT_ATTACHMENT, "input-attachment"),
        (
            vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR,
            "video-decode-dst",
        ),
        (
            vk::ImageUsageFlags::VIDEO_DECODE_SRC_KHR,
            "video-decode-src",
        ),
        (
            vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR,
            "video-decode-dpb",
        ),
    ]
    .into_iter()
    .filter_map(|(flag, label)| flags.contains(flag).then_some(label))
    .collect()
}

fn image_create_flag_labels(flags: vk::ImageCreateFlags) -> Vec<&'static str> {
    [
        (vk::ImageCreateFlags::SPARSE_BINDING, "sparse-binding"),
        (vk::ImageCreateFlags::SPARSE_RESIDENCY, "sparse-residency"),
        (vk::ImageCreateFlags::SPARSE_ALIASED, "sparse-aliased"),
        (vk::ImageCreateFlags::MUTABLE_FORMAT, "mutable-format"),
        (vk::ImageCreateFlags::CUBE_COMPATIBLE, "cube-compatible"),
        (vk::ImageCreateFlags::ALIAS, "alias"),
        (vk::ImageCreateFlags::DISJOINT, "disjoint"),
    ]
    .into_iter()
    .filter_map(|(flag, label)| flags.contains(flag).then_some(label))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_usage_labels_cover_video_decode_output_and_dpb() {
        let output_labels = image_usage_flag_labels(
            vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR | vk::ImageUsageFlags::SAMPLED,
        );
        assert!(output_labels.contains(&"video-decode-dst"));
        assert!(output_labels.contains(&"sampled"));

        let dpb_labels = image_usage_flag_labels(vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR);
        assert_eq!(dpb_labels, vec!["video-decode-dpb"]);
    }

    #[test]
    fn unsupported_format_query_preserves_codec_profile_and_usage() {
        let query = unsupported_format_query(
            "h265",
            "main-10",
            "video-decode-dst-sampled",
            vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR | vk::ImageUsageFlags::SAMPLED,
            "missing required Vulkan Video decode extensions: VK_KHR_video_decode_h265".to_owned(),
        );

        assert_eq!(query.codec, "h265");
        assert_eq!(query.profile, "main-10");
        assert_eq!(query.image_usage, "video-decode-dst-sampled");
        assert_eq!(query.supported_format_count, 0);
        assert!(
            query
                .query_error
                .unwrap()
                .contains("VK_KHR_video_decode_h265")
        );
    }

    #[test]
    fn format_property_snapshot_keeps_usage_and_create_flags() {
        let property = vk::VideoFormatPropertiesKHR::builder()
            .image_create_flags(vk::ImageCreateFlags::MUTABLE_FORMAT)
            .image_usage_flags(
                vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR | vk::ImageUsageFlags::SAMPLED,
            )
            .build();

        let snapshot = video_format_property_snapshot(property);

        assert!(snapshot.image_create_flags.contains(&"mutable-format"));
        assert!(snapshot.image_usage_flags.contains(&"video-decode-dst"));
        assert!(snapshot.image_usage_flags.contains(&"sampled"));
    }
}
