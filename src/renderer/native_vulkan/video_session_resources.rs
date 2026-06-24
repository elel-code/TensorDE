//! Vulkan Video session-owned buffers and readback targets.

use std::ffi::c_void;
use std::ptr;

use ash::vk;

use super::labels::{
    native_vulkan_buffer_usage_flag_labels, native_vulkan_format_label,
    native_vulkan_image_create_flag_labels, native_vulkan_image_tiling_label,
    native_vulkan_image_type_label, native_vulkan_image_usage_flag_labels,
    native_vulkan_memory_property_flag_labels,
};
use super::video_session_snapshots::{
    NativeVulkanVideoSessionBitstreamBufferSnapshot, NativeVulkanVideoSessionResourceImageSnapshot,
};
use super::{
    NativeVulkanError, native_vulkan_align_up, native_vulkan_memory_type_index,
    native_vulkan_stable_byte_hash, native_vulkan_video_format_properties_raw,
};
#[cfg(feature = "native-vulkan-gst-video")]
use super::{
    native_vulkan_memory_type_index_prefer, present::native_vulkan_color_subresource_range,
};

pub(super) struct NativeVulkanVideoResourceImage {
    pub(super) image: vk::Image,
    pub(super) memory: vk::DeviceMemory,
    pub(super) view: vk::ImageView,
    #[cfg_attr(not(feature = "native-vulkan-gst-video"), allow(dead_code))]
    pub(super) format: vk::Format,
    pub(super) layer_views: Vec<vk::ImageView>,
    pub(super) separate_slots: Vec<NativeVulkanVideoResourceImageSlot>,
    pub(super) snapshot: NativeVulkanVideoSessionResourceImageSnapshot,
}

pub(super) struct NativeVulkanVideoResourceImageSlot {
    pub(super) image: vk::Image,
    pub(super) memory: vk::DeviceMemory,
    pub(super) view: vk::ImageView,
}

impl NativeVulkanVideoResourceImage {
    pub(super) fn uses_separate_slots(&self) -> bool {
        !self.separate_slots.is_empty()
    }

    pub(super) fn slot_image(&self, slot: u32) -> Result<vk::Image, NativeVulkanError> {
        if self.uses_separate_slots() {
            self.separate_slots
                .get(slot as usize)
                .map(|slot| slot.image)
                .ok_or_else(|| {
                    NativeVulkanError::Video(format!(
                        "video resource separate slot {slot} exceeds {} slots",
                        self.separate_slots.len()
                    ))
                })
        } else {
            Ok(self.image)
        }
    }

    pub(super) fn slot_base_array_layer(&self, slot: u32) -> u32 {
        if self.uses_separate_slots() { 0 } else { slot }
    }

    pub(super) fn slot_view(&self, slot: u32) -> Result<vk::ImageView, NativeVulkanError> {
        if self.uses_separate_slots() {
            self.separate_slots
                .get(slot as usize)
                .map(|slot| slot.view)
                .ok_or_else(|| {
                    NativeVulkanError::Video(format!(
                        "video resource separate slot {slot} exceeds {} slots",
                        self.separate_slots.len()
                    ))
                })
        } else {
            Ok(self.view)
        }
    }

    #[cfg(feature = "native-vulkan-gst-video")]
    pub(super) fn layer_view(&self, slot: u32) -> Result<vk::ImageView, NativeVulkanError> {
        if self.uses_separate_slots() {
            self.slot_view(slot)
        } else {
            self.layer_views.get(slot as usize).copied().ok_or_else(|| {
                NativeVulkanError::Video(format!(
                    "video resource layer view {slot} exceeds {} layers",
                    self.layer_views.len()
                ))
            })
        }
    }
}

pub(super) fn native_vulkan_create_video_session_resource_image(
    video_queue_loader: &ash::khr::video_queue::Instance,
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    extent: vk::Extent2D,
    array_layers: u32,
    picture_format: vk::Format,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    additional_usage: vk::ImageUsageFlags,
    queue_family_indices: &[u32],
    separate_images: bool,
) -> Result<NativeVulkanVideoResourceImage, NativeVulkanError> {
    if !decode_capability_flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE)
    {
        return Err(NativeVulkanError::Video(
            "video resource smoke currently requires DPB/output coincide".to_owned(),
        ));
    }
    let image_usage = vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
        | vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
        | vk::ImageUsageFlags::SAMPLED
        | additional_usage;
    let format = native_vulkan_video_format_properties_raw(
        video_queue_loader,
        physical_device,
        profile_info,
        image_usage,
    )?
    .into_iter()
    .find(|format| {
        format.format == picture_format && format.image_usage_flags.contains(image_usage)
    })
    .ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "{} video decode dst+dpb+sampled image format is unavailable",
            native_vulkan_format_label(picture_format)
        ))
    })?;
    if separate_images {
        return native_vulkan_create_video_session_separate_resource_images(
            device,
            memory_properties,
            profile_info,
            extent,
            array_layers,
            image_usage,
            queue_family_indices,
            &format,
        );
    }

    let mut profile_list_info =
        vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(profile_info));
    let image_extent = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let image_create_info = vk::ImageCreateInfo::default()
        .flags(format.image_create_flags)
        .image_type(format.image_type)
        .format(format.format)
        .extent(image_extent)
        .mip_levels(1)
        .array_layers(array_layers)
        .samples(vk::SampleCountFlags::TYPE_1)
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
    };
    let image = unsafe { device.create_image(&image_create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateImage(video session resource)",
            result,
        }
    })?;

    let mut image_destroyed = false;
    let result = (|| -> Result<NativeVulkanVideoResourceImage, NativeVulkanError> {
        let memory_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type_index = native_vulkan_memory_type_index(
            memory_properties,
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .or_else(|| {
            native_vulkan_memory_type_index(
                memory_properties,
                memory_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::empty(),
            )
        })
        .ok_or(NativeVulkanError::MissingMemoryType(
            "video session resource image",
        ))?;
        let allocation_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory =
            unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(video session resource image)",
                    result,
                }
            })?;

        let bind_result = unsafe { device.bind_image_memory(image, memory, 0) };
        if let Err(result) = bind_result {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindImageMemory(video session resource)",
                result,
            });
        }

        let view = match native_vulkan_create_video_session_resource_image_view(
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
        let layer_views = match native_vulkan_create_video_session_resource_layer_image_views(
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

        let memory_type = memory_properties.memory_types[memory_type_index as usize];
        Ok(NativeVulkanVideoResourceImage {
            image,
            memory,
            view,
            format: format.format,
            layer_views,
            separate_slots: Vec::new(),
            snapshot: NativeVulkanVideoSessionResourceImageSnapshot {
                role: "coincident-dpb-output-sampled-video",
                format: native_vulkan_format_label(format.format),
                image_type: native_vulkan_image_type_label(format.image_type),
                image_tiling: native_vulkan_image_tiling_label(format.image_tiling),
                image_usage_flags: native_vulkan_image_usage_flag_labels(image_usage),
                image_create_flags: native_vulkan_image_create_flag_labels(
                    format.image_create_flags,
                ),
                extent: (image_extent.width, image_extent.height, image_extent.depth),
                array_layers,
                image_view_type: "2d-array",
                image_view_created: true,
                memory_size: memory_requirements.size,
                memory_alignment: memory_requirements.alignment,
                memory_type_bits: memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type_index,
                selected_memory_property_flags: native_vulkan_memory_property_flag_labels(
                    memory_type.property_flags,
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

pub(super) fn native_vulkan_create_video_session_output_image(
    video_queue_loader: &ash::khr::video_queue::Instance,
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    extent: vk::Extent2D,
    array_layers: u32,
    picture_format: vk::Format,
    additional_usage: vk::ImageUsageFlags,
    queue_family_indices: &[u32],
) -> Result<NativeVulkanVideoResourceImage, NativeVulkanError> {
    if array_layers == 0 {
        return Err(NativeVulkanError::Video(
            "video session output image requires at least one layer".to_owned(),
        ));
    }
    let image_usage =
        vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR | vk::ImageUsageFlags::SAMPLED | additional_usage;
    let format = native_vulkan_video_format_properties_raw(
        video_queue_loader,
        physical_device,
        profile_info,
        image_usage,
    )?
    .into_iter()
    .find(|format| {
        format.format == picture_format && format.image_usage_flags.contains(image_usage)
    })
    .ok_or_else(|| {
        NativeVulkanError::Video(format!(
            "{} video decode dst+sampled output image format is unavailable",
            native_vulkan_format_label(picture_format)
        ))
    })?;

    let mut profile_list_info =
        vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(profile_info));
    let image_extent = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let image_create_info = vk::ImageCreateInfo::default()
        .flags(format.image_create_flags)
        .image_type(format.image_type)
        .format(format.format)
        .extent(image_extent)
        .mip_levels(1)
        .array_layers(array_layers)
        .samples(vk::SampleCountFlags::TYPE_1)
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
    };
    let image = unsafe { device.create_image(&image_create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateImage(video session output)",
            result,
        }
    })?;

    let mut image_destroyed = false;
    let result = (|| -> Result<NativeVulkanVideoResourceImage, NativeVulkanError> {
        let memory_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type_index = native_vulkan_memory_type_index(
            memory_properties,
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .or_else(|| {
            native_vulkan_memory_type_index(
                memory_properties,
                memory_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::empty(),
            )
        })
        .ok_or(NativeVulkanError::MissingMemoryType(
            "video session output image",
        ))?;
        let allocation_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory =
            unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(video session output image)",
                    result,
                }
            })?;

        if let Err(result) = unsafe { device.bind_image_memory(image, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindImageMemory(video session output)",
                result,
            });
        }

        let view = match native_vulkan_create_video_session_resource_image_view(
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
        let layer_views = match native_vulkan_create_video_session_resource_layer_image_views(
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

        let memory_type = memory_properties.memory_types[memory_type_index as usize];
        Ok(NativeVulkanVideoResourceImage {
            image,
            memory,
            view,
            format: format.format,
            layer_views,
            separate_slots: Vec::new(),
            snapshot: NativeVulkanVideoSessionResourceImageSnapshot {
                role: "distinct-output-sampled-video",
                format: native_vulkan_format_label(format.format),
                image_type: native_vulkan_image_type_label(format.image_type),
                image_tiling: native_vulkan_image_tiling_label(format.image_tiling),
                image_usage_flags: native_vulkan_image_usage_flag_labels(image_usage),
                image_create_flags: native_vulkan_image_create_flag_labels(
                    format.image_create_flags,
                ),
                extent: (image_extent.width, image_extent.height, image_extent.depth),
                array_layers,
                image_view_type: "2d-array",
                image_view_created: true,
                memory_size: memory_requirements.size,
                memory_alignment: memory_requirements.alignment,
                memory_type_bits: memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type_index,
                selected_memory_property_flags: native_vulkan_memory_property_flag_labels(
                    memory_type.property_flags,
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

fn native_vulkan_create_video_session_separate_resource_images(
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    extent: vk::Extent2D,
    slot_count: u32,
    image_usage: vk::ImageUsageFlags,
    queue_family_indices: &[u32],
    format: &vk::VideoFormatPropertiesKHR<'_>,
) -> Result<NativeVulkanVideoResourceImage, NativeVulkanError> {
    if slot_count == 0 {
        return Err(NativeVulkanError::Video(
            "separate video resource images require at least one slot".to_owned(),
        ));
    }

    let image_extent = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let mut slots = Vec::<NativeVulkanVideoResourceImageSlot>::with_capacity(slot_count as usize);
    let mut total_memory_size = 0u64;
    let mut max_memory_alignment = 0u64;
    let mut memory_type_bits = 0u32;
    let mut selected_memory_type_index = 0u32;
    let mut selected_memory_property_flags = Vec::<&'static str>::new();

    let result = (|| -> Result<NativeVulkanVideoResourceImage, NativeVulkanError> {
        for _ in 0..slot_count {
            let mut profile_list_info =
                vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(profile_info));
            let image_create_info = vk::ImageCreateInfo::default()
                .flags(format.image_create_flags)
                .image_type(format.image_type)
                .format(format.format)
                .extent(image_extent)
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
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
            };
            let image =
                unsafe { device.create_image(&image_create_info, None) }.map_err(|result| {
                    NativeVulkanError::Vulkan {
                        operation: "vkCreateImage(separate video session resource)",
                        result,
                    }
                })?;

            let slot_result =
                (|| -> Result<NativeVulkanVideoResourceImageSlot, NativeVulkanError> {
                    let memory_requirements =
                        unsafe { device.get_image_memory_requirements(image) };
                    let memory_type_index = native_vulkan_memory_type_index(
                        memory_properties,
                        memory_requirements.memory_type_bits,
                        vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    )
                    .or_else(|| {
                        native_vulkan_memory_type_index(
                            memory_properties,
                            memory_requirements.memory_type_bits,
                            vk::MemoryPropertyFlags::empty(),
                        )
                    })
                    .ok_or(NativeVulkanError::MissingMemoryType(
                        "separate video session resource image",
                    ))?;
                    let allocation_info = vk::MemoryAllocateInfo::default()
                        .allocation_size(memory_requirements.size)
                        .memory_type_index(memory_type_index);
                    let memory = unsafe { device.allocate_memory(&allocation_info, None) }
                        .map_err(|result| NativeVulkanError::Vulkan {
                            operation: "vkAllocateMemory(separate video session resource image)",
                            result,
                        })?;
                    if let Err(result) = unsafe { device.bind_image_memory(image, memory, 0) } {
                        unsafe {
                            device.free_memory(memory, None);
                        }
                        return Err(NativeVulkanError::Vulkan {
                            operation: "vkBindImageMemory(separate video session resource)",
                            result,
                        });
                    }
                    let layer_views =
                        native_vulkan_create_video_session_resource_layer_image_views(
                            device,
                            image,
                            format.format,
                            image_usage,
                            1,
                        )?;
                    let view = layer_views[0];
                    let memory_type = memory_properties.memory_types[memory_type_index as usize];
                    total_memory_size = total_memory_size.saturating_add(memory_requirements.size);
                    max_memory_alignment = max_memory_alignment.max(memory_requirements.alignment);
                    memory_type_bits |= memory_requirements.memory_type_bits;
                    selected_memory_type_index = memory_type_index;
                    selected_memory_property_flags =
                        native_vulkan_memory_property_flag_labels(memory_type.property_flags);
                    Ok(NativeVulkanVideoResourceImageSlot {
                        image,
                        memory,
                        view,
                    })
                })();

            match slot_result {
                Ok(slot) => slots.push(slot),
                Err(err) => {
                    unsafe {
                        device.destroy_image(image, None);
                    }
                    return Err(err);
                }
            }
        }

        let first = slots.first().ok_or_else(|| {
            NativeVulkanError::Video("separate video resource image slots are empty".to_owned())
        })?;
        let first_image = first.image;
        let first_memory = first.memory;
        let first_view = first.view;
        Ok(NativeVulkanVideoResourceImage {
            image: first_image,
            memory: first_memory,
            view: first_view,
            format: format.format,
            layer_views: Vec::new(),
            separate_slots: std::mem::take(&mut slots),
            snapshot: NativeVulkanVideoSessionResourceImageSnapshot {
                role: "separate-coincident-dpb-output-sampled-video",
                format: native_vulkan_format_label(format.format),
                image_type: native_vulkan_image_type_label(format.image_type),
                image_tiling: native_vulkan_image_tiling_label(format.image_tiling),
                image_usage_flags: native_vulkan_image_usage_flag_labels(image_usage),
                image_create_flags: native_vulkan_image_create_flag_labels(
                    format.image_create_flags,
                ),
                extent: (image_extent.width, image_extent.height, image_extent.depth),
                array_layers: slot_count,
                image_view_type: "separate-2d",
                image_view_created: true,
                memory_size: total_memory_size,
                memory_alignment: max_memory_alignment,
                memory_type_bits,
                selected_memory_type_index,
                selected_memory_property_flags,
            },
        })
    })();

    if result.is_err() {
        for slot in slots {
            unsafe {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
                device.free_memory(slot.memory, None);
            }
        }
    }

    result
}

pub(super) fn native_vulkan_destroy_video_session_resource_image(
    device: &ash::Device,
    image: NativeVulkanVideoResourceImage,
) {
    unsafe {
        if image.uses_separate_slots() {
            for slot in image.separate_slots {
                device.destroy_image_view(slot.view, None);
                device.destroy_image(slot.image, None);
                device.free_memory(slot.memory, None);
            }
        } else {
            for view in image.layer_views {
                device.destroy_image_view(view, None);
            }
            device.destroy_image_view(image.view, None);
            device.destroy_image(image.image, None);
            device.free_memory(image.memory, None);
        }
    }
}

fn native_vulkan_create_video_session_resource_image_view(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    image_usage: vk::ImageUsageFlags,
) -> Result<vk::ImageView, NativeVulkanError> {
    let mut view_usage_info = vk::ImageViewUsageCreateInfo::default().usage(image_usage);
    let subresource_range = vk::ImageSubresourceRange {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        base_mip_level: 0,
        level_count: 1,
        base_array_layer: 0,
        layer_count: vk::REMAINING_ARRAY_LAYERS,
    };
    let create_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D_ARRAY)
        .format(format)
        .subresource_range(subresource_range)
        .push_next(&mut view_usage_info);
    unsafe { device.create_image_view(&create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateImageView(video session resource)",
            result,
        }
    })
}

fn native_vulkan_create_video_session_resource_layer_image_views(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    image_usage: vk::ImageUsageFlags,
    array_layers: u32,
) -> Result<Vec<vk::ImageView>, NativeVulkanError> {
    let mut views = Vec::with_capacity(array_layers as usize);
    for layer in 0..array_layers {
        let mut view_usage_info = vk::ImageViewUsageCreateInfo::default().usage(image_usage);
        let subresource_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: layer,
            layer_count: 1,
        };
        let create_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(subresource_range)
            .push_next(&mut view_usage_info);
        match unsafe { device.create_image_view(&create_info, None) } {
            Ok(view) => views.push(view),
            Err(result) => {
                for view in views {
                    unsafe {
                        device.destroy_image_view(view, None);
                    }
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkCreateImageView(video session resource layer)",
                    result,
                });
            }
        }
    }
    Ok(views)
}
pub(super) struct NativeVulkanVideoBitstreamBuffer {
    pub(super) buffer: vk::Buffer,
    pub(super) memory: vk::DeviceMemory,
    pub(super) mapped_ptr: Option<*mut c_void>,
    #[cfg_attr(not(feature = "native-vulkan-gst-video"), allow(dead_code))]
    pub(super) memory_property_flags: vk::MemoryPropertyFlags,
    pub(super) snapshot: NativeVulkanVideoSessionBitstreamBufferSnapshot,
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) struct NativeVulkanVideoDecodeReadbackBuffer {
    pub(super) buffer: vk::Buffer,
    pub(super) memory: vk::DeviceMemory,
    pub(super) format: &'static str,
    pub(super) memory_size: u64,
    pub(super) size: u64,
    pub(super) y_plane_bytes: u64,
    pub(super) uv_plane_bytes: u64,
    pub(super) memory_property_flags: vk::MemoryPropertyFlags,
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) struct NativeVulkanDecodedSamplingTarget {
    pub(super) image: vk::Image,
    pub(super) memory: vk::DeviceMemory,
    pub(super) view: vk::ImageView,
    pub(super) readback_buffer: vk::Buffer,
    pub(super) readback_memory: vk::DeviceMemory,
    pub(super) extent: vk::Extent2D,
    pub(super) format: vk::Format,
    pub(super) total_bytes: u64,
    pub(super) color_memory_size: u64,
    pub(super) readback_memory_size: u64,
    pub(super) readback_memory_property_flags: vk::MemoryPropertyFlags,
}

#[cfg_attr(not(feature = "native-vulkan-gst-video"), allow(dead_code))]
pub(super) fn native_vulkan_create_video_session_bitstream_buffer(
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    requested_size: u64,
    min_size_alignment: u64,
    write_payload: Option<&[u8]>,
) -> Result<NativeVulkanVideoBitstreamBuffer, NativeVulkanError> {
    native_vulkan_create_video_session_bitstream_buffer_with_mapping(
        device,
        memory_properties,
        profile_info,
        requested_size,
        min_size_alignment,
        write_payload,
        false,
    )
}

pub(super) fn native_vulkan_create_video_session_bitstream_buffer_with_mapping(
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    profile_info: &vk::VideoProfileInfoKHR<'_>,
    requested_size: u64,
    min_size_alignment: u64,
    write_payload: Option<&[u8]>,
    keep_mapped: bool,
) -> Result<NativeVulkanVideoBitstreamBuffer, NativeVulkanError> {
    let size = native_vulkan_align_up(requested_size.max(1), min_size_alignment.max(1));
    let usage = vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR;
    let mut profile_list_info =
        vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(profile_info));
    let buffer_create_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .push_next(&mut profile_list_info);
    let buffer = unsafe { device.create_buffer(&buffer_create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateBuffer(video bitstream)",
            result,
        }
    })?;

    let mut buffer_destroyed = false;
    let result = (|| -> Result<NativeVulkanVideoBitstreamBuffer, NativeVulkanError> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_index = native_vulkan_memory_type_index(
            memory_properties,
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .or_else(|| {
            native_vulkan_memory_type_index(
                memory_properties,
                memory_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
            )
        })
        .ok_or(NativeVulkanError::MissingMemoryType(
            "video bitstream host-visible buffer",
        ))?;
        let memory_type = memory_properties.memory_types[memory_type_index as usize];
        let allocation_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory =
            unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(video bitstream)",
                    result,
                }
            })?;

        if let Err(result) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.destroy_buffer(buffer, None);
                buffer_destroyed = true;
                device.free_memory(memory, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindBufferMemory(video bitstream)",
                result,
            });
        }

        let mapped_write_bytes = if keep_mapped {
            memory_requirements.size
        } else {
            write_payload
                .map(|payload| payload.len() as u64)
                .unwrap_or_else(|| size.min(256))
        };
        let map_result = unsafe {
            device.map_memory(memory, 0, mapped_write_bytes, vk::MemoryMapFlags::empty())
        };
        let map = match map_result {
            Ok(map) => map,
            Err(result) => {
                unsafe {
                    device.destroy_buffer(buffer, None);
                    buffer_destroyed = true;
                    device.free_memory(memory, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkMapMemory(video bitstream)",
                    result,
                });
            }
        };
        if let Some(payload) = write_payload {
            unsafe {
                ptr::copy_nonoverlapping(payload.as_ptr(), map.cast::<u8>(), payload.len());
            }
        } else {
            unsafe {
                ptr::write_bytes(map.cast::<u8>(), 0, mapped_write_bytes as usize);
            }
        }
        if !memory_type
            .property_flags
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
        {
            let range = vk::MappedMemoryRange::default()
                .memory(memory)
                .offset(0)
                .size(mapped_write_bytes);
            if let Err(result) = unsafe { device.flush_mapped_memory_ranges(&[range]) } {
                unsafe {
                    device.unmap_memory(memory);
                    device.destroy_buffer(buffer, None);
                    buffer_destroyed = true;
                    device.free_memory(memory, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkFlushMappedMemoryRanges(video bitstream)",
                    result,
                });
            }
        }
        let mapped_ptr = if keep_mapped {
            Some(map)
        } else {
            unsafe {
                device.unmap_memory(memory);
            }
            None
        };

        Ok(NativeVulkanVideoBitstreamBuffer {
            buffer,
            memory,
            mapped_ptr,
            memory_property_flags: memory_type.property_flags,
            snapshot: NativeVulkanVideoSessionBitstreamBufferSnapshot {
                requested_size,
                size,
                min_size_alignment,
                usage_flags: native_vulkan_buffer_usage_flag_labels(usage),
                memory_size: memory_requirements.size,
                memory_alignment: memory_requirements.alignment,
                memory_type_bits: memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type_index,
                selected_memory_property_flags: native_vulkan_memory_property_flag_labels(
                    memory_type.property_flags,
                ),
                mapped_write_bytes,
                mapped_write_source: if keep_mapped {
                    "persistent-mapped-reusable-slot"
                } else if write_payload.is_some() {
                    "extracted-encoded-video-unit"
                } else {
                    "zero-fill-smoke-pattern"
                },
                mapped_write_hash: write_payload.map(native_vulkan_stable_byte_hash),
                host_visible: memory_type
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_VISIBLE),
                host_coherent: memory_type
                    .property_flags
                    .contains(vk::MemoryPropertyFlags::HOST_COHERENT),
            },
        })
    })();

    if result.is_err() && !buffer_destroyed {
        unsafe {
            device.destroy_buffer(buffer, None);
        }
    }
    result
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_write_video_session_bitstream_buffer(
    device: &ash::Device,
    buffer: &NativeVulkanVideoBitstreamBuffer,
    offset: u64,
    range: u64,
    payload: &[u8],
) -> Result<(), NativeVulkanError> {
    let mapped_ptr = buffer.mapped_ptr.ok_or_else(|| {
        NativeVulkanError::Video("video bitstream buffer is not persistently mapped".to_owned())
    })?;
    if payload.len() as u64 > range {
        return Err(NativeVulkanError::Video(format!(
            "video bitstream payload has {} bytes but decode range is {range}",
            payload.len()
        )));
    }
    let end = offset.checked_add(range).ok_or_else(|| {
        NativeVulkanError::Video("video bitstream upload range overflow".to_owned())
    })?;
    if end > buffer.snapshot.size {
        return Err(NativeVulkanError::Video(format!(
            "video bitstream upload range {offset}..{end} exceeds buffer size {}",
            buffer.snapshot.size
        )));
    }
    let offset_usize = usize::try_from(offset).map_err(|_| {
        NativeVulkanError::Video(format!(
            "video bitstream offset {offset} does not fit usize"
        ))
    })?;
    let range_usize = usize::try_from(range).map_err(|_| {
        NativeVulkanError::Video(format!("video bitstream range {range} does not fit usize"))
    })?;
    unsafe {
        let dst = mapped_ptr.cast::<u8>().add(offset_usize);
        ptr::copy_nonoverlapping(payload.as_ptr(), dst, payload.len());
        let padding_bytes = range_usize.saturating_sub(payload.len());
        if padding_bytes > 0 {
            ptr::write_bytes(dst.add(payload.len()), 0, padding_bytes);
        }
    }
    if !buffer
        .memory_property_flags
        .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
    {
        let flush_range = vk::MappedMemoryRange::default()
            .memory(buffer.memory)
            .offset(0)
            .size(vk::WHOLE_SIZE);
        unsafe { device.flush_mapped_memory_ranges(&[flush_range]) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkFlushMappedMemoryRanges(video bitstream reusable slot)",
                result,
            }
        })?;
    }
    Ok(())
}

pub(super) unsafe fn native_vulkan_destroy_video_session_bitstream_buffer(
    device: &ash::Device,
    buffer: NativeVulkanVideoBitstreamBuffer,
) {
    if buffer.mapped_ptr.is_some() {
        unsafe {
            device.unmap_memory(buffer.memory);
        }
    }
    unsafe {
        device.destroy_buffer(buffer.buffer, None);
        device.free_memory(buffer.memory, None);
    }
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) struct NativeVulkanVideoDecodeReadbackLayout {
    pub(super) format: &'static str,
    pub(super) size: u64,
    pub(super) y_plane_bytes: u64,
    pub(super) uv_plane_bytes: u64,
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_video_decode_readback_layout(
    format: vk::Format,
    extent: vk::Extent2D,
) -> Result<NativeVulkanVideoDecodeReadbackLayout, NativeVulkanError> {
    let format_label = native_vulkan_format_label(format);
    if extent.width == 0
        || extent.height == 0
        || !extent.width.is_multiple_of(2)
        || !extent.height.is_multiple_of(2)
    {
        return Err(NativeVulkanError::Video(format!(
            "{format_label} readback requires non-zero even extent, got {}x{}",
            extent.width, extent.height
        )));
    }
    let pixel_count = u64::from(extent.width)
        .checked_mul(u64::from(extent.height))
        .ok_or_else(|| {
            NativeVulkanError::Video(format!("{format_label} readback pixel count overflow"))
        })?;
    let (y_plane_bytes, uv_plane_bytes) = match format {
        vk::Format::G8_B8R8_2PLANE_420_UNORM => (pixel_count, pixel_count / 2),
        vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => (
            pixel_count.checked_mul(2).ok_or_else(|| {
                NativeVulkanError::Video(format!("{format_label} readback Y plane size overflow"))
            })?,
            pixel_count,
        ),
        _ => {
            return Err(NativeVulkanError::Video(format!(
                "{format_label} decode readback is not implemented"
            )));
        }
    };
    let size = y_plane_bytes.checked_add(uv_plane_bytes).ok_or_else(|| {
        NativeVulkanError::Video(format!("{format_label} readback size overflow"))
    })?;
    Ok(NativeVulkanVideoDecodeReadbackLayout {
        format: format_label,
        size,
        y_plane_bytes,
        uv_plane_bytes,
    })
}

#[cfg(feature = "native-vulkan-gst-video")]
pub(super) fn native_vulkan_create_video_decode_readback_buffer(
    device: &ash::Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    format: vk::Format,
    extent: vk::Extent2D,
) -> Result<NativeVulkanVideoDecodeReadbackBuffer, NativeVulkanError> {
    let layout = native_vulkan_video_decode_readback_layout(format, extent)?;
    let buffer_create_info = vk::BufferCreateInfo::default()
        .size(layout.size)
        .usage(vk::BufferUsageFlags::TRANSFER_DST)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&buffer_create_info, None) }.map_err(|result| {
        NativeVulkanError::Vulkan {
            operation: "vkCreateBuffer(video decode readback)",
            result,
        }
    })?;

    let mut buffer_destroyed = false;
    let result = (|| -> Result<NativeVulkanVideoDecodeReadbackBuffer, NativeVulkanError> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_index = native_vulkan_memory_type_index_prefer(
            memory_properties,
            memory_requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_CACHED,
            vk::MemoryPropertyFlags::HOST_VISIBLE,
        )
        .ok_or(NativeVulkanError::MissingMemoryType(
            "video decode readback buffer",
        ))?;
        let memory_type = memory_properties.memory_types[memory_type_index as usize];
        let allocation_info = vk::MemoryAllocateInfo::default()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type_index);
        let memory =
            unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(video decode readback)",
                    result,
                }
            })?;

        if let Err(result) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.destroy_buffer(buffer, None);
                buffer_destroyed = true;
                device.free_memory(memory, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindBufferMemory(video decode readback)",
                result,
            });
        }

        Ok(NativeVulkanVideoDecodeReadbackBuffer {
            buffer,
            memory,
            format: layout.format,
            memory_size: memory_requirements.size,
            size: layout.size,
            y_plane_bytes: layout.y_plane_bytes,
            uv_plane_bytes: layout.uv_plane_bytes,
            memory_property_flags: memory_type.property_flags,
        })
    })();

    if result.is_err() && !buffer_destroyed {
        unsafe {
            device.destroy_buffer(buffer, None);
        }
    }
    result
}

#[cfg(feature = "native-vulkan-gst-video")]
impl NativeVulkanDecodedSamplingTarget {
    pub(super) fn new(
        device: &ash::Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        extent: vk::Extent2D,
    ) -> Result<Self, NativeVulkanError> {
        if extent.width == 0 || extent.height == 0 {
            return Err(NativeVulkanError::Video(format!(
                "decoded sampling target requires non-zero extent, got {}x{}",
                extent.width, extent.height
            )));
        }
        let format = vk::Format::R8G8B8A8_UNORM;
        let total_bytes = u64::from(extent.width)
            .checked_mul(u64::from(extent.height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                NativeVulkanError::Video("RGBA sampling readback size overflow".to_owned())
            })?;
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let image = unsafe { device.create_image(&image_info, None) }.map_err(|result| {
            NativeVulkanError::Vulkan {
                operation: "vkCreateImage(decoded sampling color target)",
                result,
            }
        })?;

        let result = (|| -> Result<Self, NativeVulkanError> {
            let image_requirements = unsafe { device.get_image_memory_requirements(image) };
            let image_memory_type_index = native_vulkan_memory_type_index_prefer(
                memory_properties,
                image_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
                vk::MemoryPropertyFlags::empty(),
            )
            .ok_or(NativeVulkanError::MissingMemoryType(
                "decoded sampling color target",
            ))?;
            let image_allocate_info = vk::MemoryAllocateInfo::default()
                .allocation_size(image_requirements.size)
                .memory_type_index(image_memory_type_index);
            let memory = unsafe { device.allocate_memory(&image_allocate_info, None) }.map_err(
                |result| NativeVulkanError::Vulkan {
                    operation: "vkAllocateMemory(decoded sampling color target)",
                    result,
                },
            )?;
            if let Err(result) = unsafe { device.bind_image_memory(image, memory, 0) } {
                unsafe {
                    device.free_memory(memory, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkBindImageMemory(decoded sampling color target)",
                    result,
                });
            }

            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(native_vulkan_color_subresource_range());
            let view = match unsafe { device.create_image_view(&view_info, None) } {
                Ok(view) => view,
                Err(result) => {
                    unsafe {
                        device.free_memory(memory, None);
                    }
                    return Err(NativeVulkanError::Vulkan {
                        operation: "vkCreateImageView(decoded sampling color target)",
                        result,
                    });
                }
            };

            let readback_info = vk::BufferCreateInfo::default()
                .size(total_bytes)
                .usage(vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let readback_buffer = match unsafe { device.create_buffer(&readback_info, None) } {
                Ok(buffer) => buffer,
                Err(result) => {
                    unsafe {
                        device.destroy_image_view(view, None);
                        device.free_memory(memory, None);
                    }
                    return Err(NativeVulkanError::Vulkan {
                        operation: "vkCreateBuffer(decoded sampling readback)",
                        result,
                    });
                }
            };

            let readback_requirements =
                unsafe { device.get_buffer_memory_requirements(readback_buffer) };
            let readback_memory_type_index = match native_vulkan_memory_type_index_prefer(
                memory_properties,
                readback_requirements.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_CACHED,
                vk::MemoryPropertyFlags::HOST_VISIBLE,
            ) {
                Some(index) => index,
                None => {
                    unsafe {
                        device.destroy_buffer(readback_buffer, None);
                        device.destroy_image_view(view, None);
                        device.free_memory(memory, None);
                    }
                    return Err(NativeVulkanError::MissingMemoryType(
                        "decoded sampling readback buffer",
                    ));
                }
            };
            let readback_memory_type =
                memory_properties.memory_types[readback_memory_type_index as usize];
            let readback_allocate_info = vk::MemoryAllocateInfo::default()
                .allocation_size(readback_requirements.size)
                .memory_type_index(readback_memory_type_index);
            let readback_memory =
                match unsafe { device.allocate_memory(&readback_allocate_info, None) } {
                    Ok(memory) => memory,
                    Err(result) => {
                        unsafe {
                            device.destroy_buffer(readback_buffer, None);
                            device.destroy_image_view(view, None);
                            device.free_memory(memory, None);
                        }
                        return Err(NativeVulkanError::Vulkan {
                            operation: "vkAllocateMemory(decoded sampling readback)",
                            result,
                        });
                    }
                };
            if let Err(result) =
                unsafe { device.bind_buffer_memory(readback_buffer, readback_memory, 0) }
            {
                unsafe {
                    device.free_memory(readback_memory, None);
                    device.destroy_buffer(readback_buffer, None);
                    device.destroy_image_view(view, None);
                    device.free_memory(memory, None);
                }
                return Err(NativeVulkanError::Vulkan {
                    operation: "vkBindBufferMemory(decoded sampling readback)",
                    result,
                });
            }

            Ok(Self {
                image,
                memory,
                view,
                readback_buffer,
                readback_memory,
                extent,
                format,
                total_bytes,
                color_memory_size: image_requirements.size,
                readback_memory_size: readback_requirements.size,
                readback_memory_property_flags: readback_memory_type.property_flags,
            })
        })();

        if result.is_err() {
            unsafe {
                device.destroy_image(image, None);
            }
        }
        result
    }

    pub(super) fn destroy(self, device: &ash::Device) {
        unsafe {
            device.destroy_buffer(self.readback_buffer, None);
            device.free_memory(self.readback_memory, None);
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            device.free_memory(self.memory, None);
        }
    }
}
