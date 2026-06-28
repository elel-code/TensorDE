use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::memory::native_vulkan_vulkanalia_bind_image_memory2;
use super::video_format_probe::native_vulkan_vulkanalia_video_format_properties_for_profile;
use super::video_session::{
    NativeVulkanVulkanaliaMemoryTypeCandidate, native_vulkan_vulkanalia_memory_type_candidates,
};

const DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();
const HOST_VISIBLE_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::HOST_VISIBLE.bits();

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot {
    pub image_created: bool,
    pub memory_bound: bool,
    pub image_view_created: bool,
    pub layer_view_count: usize,
    pub resource_image: NativeVulkanVulkanaliaVideoSessionResourceImageSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionResourceImageSnapshot {
    pub role: &'static str,
    pub format: String,
    pub image_type: String,
    pub image_tiling: String,
    pub image_usage_flags: Vec<&'static str>,
    pub image_create_flags: Vec<&'static str>,
    pub extent: (u32, u32, u32),
    pub array_layers: u32,
    pub image_view_type: &'static str,
    pub image_view_created: bool,
    pub layer_view_count: usize,
    pub memory_size: u64,
    pub memory_alignment: u64,
    pub memory_type_bits: u32,
    pub selected_memory_type_index: u32,
    pub selected_memory_property_flags: Vec<&'static str>,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaVideoSessionResourceImage {
    pub(in crate::renderer::native_vulkan::vulkan) image: vk::Image,
    pub(in crate::renderer::native_vulkan::vulkan) memory: vk::DeviceMemory,
    pub(in crate::renderer::native_vulkan::vulkan) view: vk::ImageView,
    pub(in crate::renderer::native_vulkan::vulkan) layer_views: Vec<vk::ImageView>,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaVideoSessionResourceImageSnapshot,
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_smoke_create_video_session_resource_image(
    instance: &Instance,
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    array_layers: u32,
    picture_format: vk::Format,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    queue_family_indices: &[u32],
) -> Result<NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot, String> {
    let image = native_vulkan_vulkanalia_create_video_session_resource_image(
        instance,
        device,
        memory_properties,
        physical_device,
        profile_info,
        extent,
        array_layers,
        picture_format,
        decode_capability_flags,
        queue_family_indices,
    )?;
    let snapshot = NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot {
        image_created: true,
        memory_bound: true,
        image_view_created: image.view != vk::ImageView::default(),
        layer_view_count: image.layer_views.len(),
        resource_image: image.snapshot.clone(),
    };
    native_vulkan_vulkanalia_destroy_video_session_resource_image(device, image);
    Ok(snapshot)
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_video_session_resource_image(
    instance: &Instance,
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    array_layers: u32,
    picture_format: vk::Format,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    queue_family_indices: &[u32],
) -> Result<VulkanaliaVideoSessionResourceImage, String> {
    if array_layers == 0 {
        return Err("video session resource image requires at least one layer".to_owned());
    }
    if !decode_capability_flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE)
    {
        return Err(
            "Vulkanalia video resource smoke currently requires DPB/output coincide".to_owned(),
        );
    }

    let image_usage = vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
        | vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
        | vk::ImageUsageFlags::SAMPLED;
    let format = native_vulkan_vulkanalia_video_format_properties_for_profile(
        instance,
        physical_device,
        profile_info,
        image_usage,
    )?
    .into_iter()
    .find(|format| {
        format.format == picture_format && format.image_usage_flags.contains(image_usage)
    })
    .ok_or_else(|| {
        format!("{picture_format:?} video decode dst+dpb+sampled image format is unavailable")
    })?;

    let profiles = [*profile_info];
    let mut profile_list_info = vk::VideoProfileListInfoKHR::builder()
        .profiles(&profiles)
        .build();
    let image_extent = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let image_create_info = vk::ImageCreateInfo::builder()
        .flags(format.image_create_flags)
        .image_type(format.image_type)
        .format(format.format)
        .extent(image_extent)
        .mip_levels(1)
        .array_layers(array_layers)
        .samples(vk::SampleCountFlags::_1)
        .tiling(format.image_tiling)
        .usage(image_usage)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut profile_list_info);
    let image_create_info = if queue_family_indices.len() > 1 {
        image_create_info
            .sharing_mode(vk::SharingMode::CONCURRENT)
            .queue_family_indices(queue_family_indices)
    } else {
        image_create_info.sharing_mode(vk::SharingMode::EXCLUSIVE)
    }
    .build();
    let image = unsafe { device.create_image(&image_create_info, None) }
        .map_err(|err| format!("vkCreateImage(vulkanalia video session resource): {err:?}"))?;

    let mut image_destroyed = false;
    let result = (|| -> Result<VulkanaliaVideoSessionResourceImage, String> {
        let memory_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = native_vulkan_vulkanalia_image_memory_type_index_excluding(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            DEVICE_LOCAL_MEMORY_FLAG_BITS,
            HOST_VISIBLE_MEMORY_FLAG_BITS,
        )
        .ok_or_else(|| {
            format!(
                "video session resource image requires device-local non-host-visible memory for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index);
        let memory = unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|err| {
            format!("vkAllocateMemory(vulkanalia video session resource image): {err:?}")
        })?;

        if let Err(err) = native_vulkan_vulkanalia_bind_image_memory2(
            device,
            image,
            memory,
            0,
            "video session resource image",
        ) {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(err);
        }

        let view = match native_vulkan_vulkanalia_create_video_session_resource_image_view(
            device,
            image,
            format.format,
            image_usage,
        ) {
            Ok(view) => view,
            Err(err) => {
                unsafe {
                    device.destroy_image(image, None);
                    image_destroyed = true;
                    device.free_memory(memory, None);
                }
                return Err(err);
            }
        };
        let layer_views = match native_vulkan_vulkanalia_create_video_session_resource_layer_views(
            device,
            image,
            format.format,
            image_usage,
            array_layers,
        ) {
            Ok(layer_views) => layer_views,
            Err(err) => {
                unsafe {
                    device.destroy_image_view(view, None);
                    device.destroy_image(image, None);
                    image_destroyed = true;
                    device.free_memory(memory, None);
                }
                return Err(err);
            }
        };

        Ok(VulkanaliaVideoSessionResourceImage {
            image,
            memory,
            view,
            layer_views,
            snapshot: NativeVulkanVulkanaliaVideoSessionResourceImageSnapshot {
                role: "coincident-dpb-output-sampled-video",
                format: format!("{:?}", format.format),
                image_type: format!("{:?}", format.image_type),
                image_tiling: format!("{:?}", format.image_tiling),
                image_usage_flags: image_usage_flag_labels(image_usage),
                image_create_flags: image_create_flag_labels(format.image_create_flags),
                extent: (image_extent.width, image_extent.height, image_extent.depth),
                array_layers,
                image_view_type: "2d-array",
                image_view_created: true,
                layer_view_count: array_layers as usize,
                memory_size: memory_requirements.size,
                memory_alignment: memory_requirements.alignment,
                memory_type_bits: memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type.index,
                selected_memory_property_flags: memory_property_flag_labels(
                    memory_type.property_flags_bits,
                ),
            },
        })
    })();

    if result.is_err() && !image_destroyed {
        unsafe {
            device.destroy_image(image, None);
        }
    }
    result
}

fn native_vulkan_vulkanalia_create_video_session_resource_image_view(
    device: &Device,
    image: vk::Image,
    format: vk::Format,
    image_usage: vk::ImageUsageFlags,
) -> Result<vk::ImageView, String> {
    // Decode DPB/DST views must drop SAMPLED usage. The image is a multi-planar
    // (YCbCr) format that is also SAMPLED, so any view keeping SAMPLED would require a
    // VkSamplerYcbcrConversion (VUID-VkImageViewCreateInfo-format-06415). The decode
    // picture-resource binding only needs the video-decode usages; the present pass
    // samples through a separate conversion-enabled view.
    let decode_view_usage = image_usage & !vk::ImageUsageFlags::SAMPLED;
    let mut view_usage_info = vk::ImageViewUsageCreateInfo::builder()
        .usage(decode_view_usage)
        .build();
    let subresource_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: vk::REMAINING_ARRAY_LAYERS,
    };
    let create_info = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D_ARRAY)
        .format(format)
        .subresource_range(subresource_range)
        .push_next(&mut view_usage_info);
    unsafe { device.create_image_view(&create_info, None) }
        .map_err(|err| format!("vkCreateImageView(vulkanalia video session resource): {err:?}"))
}

fn native_vulkan_vulkanalia_create_video_session_resource_layer_views(
    device: &Device,
    image: vk::Image,
    format: vk::Format,
    image_usage: vk::ImageUsageFlags,
    array_layers: u32,
) -> Result<Vec<vk::ImageView>, String> {
    let mut views = Vec::with_capacity(array_layers as usize);
    // Same as the array view: drop SAMPLED so multi-planar decode views do not require a
    // YCbCr conversion (VUID-VkImageViewCreateInfo-format-06415).
    let decode_view_usage = image_usage & !vk::ImageUsageFlags::SAMPLED;
    for layer in 0..array_layers {
        let mut view_usage_info = vk::ImageViewUsageCreateInfo::builder()
            .usage(decode_view_usage)
            .build();
        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: layer,
            layer_count: 1,
        };
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::_2D)
            .format(format)
            .subresource_range(subresource_range)
            .push_next(&mut view_usage_info);
        match unsafe { device.create_image_view(&create_info, None) } {
            Ok(view) => views.push(view),
            Err(err) => {
                for view in views {
                    unsafe {
                        device.destroy_image_view(view, None);
                    }
                }
                return Err(format!(
                    "vkCreateImageView(vulkanalia video session resource layer): {err:?}"
                ));
            }
        }
    }
    Ok(views)
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_video_session_resource_image(
    device: &Device,
    image: VulkanaliaVideoSessionResourceImage,
) {
    unsafe {
        for view in image.layer_views {
            device.destroy_image_view(view, None);
        }
        device.destroy_image_view(image.view, None);
        device.destroy_image(image.image, None);
        device.free_memory(image.memory, None);
    }
}

fn native_vulkan_vulkanalia_image_memory_type_index_excluding(
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
    allowed_memory_type_bits: u32,
    required_property_flags_bits: u32,
    excluded_property_flags_bits: u32,
) -> Option<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    memory_types.iter().copied().find(|candidate| {
        let allowed = candidate.index < u32::BITS
            && allowed_memory_type_bits & (1u32 << candidate.index) != 0;
        let properties_match = candidate.property_flags_bits & required_property_flags_bits
            == required_property_flags_bits;
        let excluded_absent = candidate.property_flags_bits & excluded_property_flags_bits == 0;
        allowed && properties_match && excluded_absent
    })
}

fn image_usage_flag_labels(flags: vk::ImageUsageFlags) -> Vec<&'static str> {
    [
        (vk::ImageUsageFlags::TRANSFER_SRC, "transfer-src"),
        (vk::ImageUsageFlags::TRANSFER_DST, "transfer-dst"),
        (vk::ImageUsageFlags::SAMPLED, "sampled"),
        (vk::ImageUsageFlags::STORAGE, "storage"),
        (vk::ImageUsageFlags::COLOR_ATTACHMENT, "color-attachment"),
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

fn memory_property_flag_labels(bits: u32) -> Vec<&'static str> {
    [
        (vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(), "device-local"),
        (vk::MemoryPropertyFlags::HOST_VISIBLE.bits(), "host-visible"),
        (
            vk::MemoryPropertyFlags::HOST_COHERENT.bits(),
            "host-coherent",
        ),
        (vk::MemoryPropertyFlags::HOST_CACHED.bits(), "host-cached"),
        (
            vk::MemoryPropertyFlags::LAZILY_ALLOCATED.bits(),
            "lazily-allocated",
        ),
        (vk::MemoryPropertyFlags::PROTECTED.bits(), "protected"),
    ]
    .into_iter()
    .filter_map(|(flag_bits, label)| (bits & flag_bits == flag_bits).then_some(label))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_usage_labels_cover_coincident_video_resource_usage() {
        let labels = image_usage_flag_labels(
            vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
                | vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
                | vk::ImageUsageFlags::SAMPLED,
        );

        assert!(labels.contains(&"video-decode-dst"));
        assert!(labels.contains(&"video-decode-dpb"));
        assert!(labels.contains(&"sampled"));
    }

    #[test]
    fn image_memory_type_selection_prefers_device_local() {
        let memory_types = vec![
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 0,
                property_flags_bits: vk::MemoryPropertyFlags::HOST_VISIBLE.bits(),
            },
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 1,
                property_flags_bits: vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(),
            },
        ];

        let selected = native_vulkan_vulkanalia_image_memory_type_index_excluding(
            &memory_types,
            0b11,
            DEVICE_LOCAL_MEMORY_FLAG_BITS,
            HOST_VISIBLE_MEMORY_FLAG_BITS,
        )
        .expect("device local memory type");

        assert_eq!(selected.index, 1);
    }

    #[test]
    fn image_memory_type_selection_rejects_host_visible_device_local() {
        let memory_types = vec![
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 0,
                property_flags_bits: (vk::MemoryPropertyFlags::DEVICE_LOCAL
                    | vk::MemoryPropertyFlags::HOST_VISIBLE)
                    .bits(),
            },
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 1,
                property_flags_bits: vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(),
            },
        ];

        let selected = native_vulkan_vulkanalia_image_memory_type_index_excluding(
            &memory_types,
            0b11,
            DEVICE_LOCAL_MEMORY_FLAG_BITS,
            HOST_VISIBLE_MEMORY_FLAG_BITS,
        )
        .expect("non-host-visible device local memory type");

        assert_eq!(selected.index, 1);
        assert!(
            native_vulkan_vulkanalia_image_memory_type_index_excluding(
                &memory_types,
                0b01,
                DEVICE_LOCAL_MEMORY_FLAG_BITS,
                HOST_VISIBLE_MEMORY_FLAG_BITS,
            )
            .is_none()
        );
    }
}
