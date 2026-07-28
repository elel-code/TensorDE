use std::ffi::c_void;

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_bind_buffer_memory2(
    device: &Device,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    offset: vk::DeviceSize,
    label: &str,
) -> Result<(), String> {
    let bind_info = vk::BindBufferMemoryInfo::builder()
        .buffer(buffer)
        .memory(memory)
        .memory_offset(offset)
        .build();
    unsafe { device.bind_buffer_memory2(&[bind_info]) }
        .map_err(|err| format!("vkBindBufferMemory2(vulkanalia {label}): {err:?}"))
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_bind_image_memory2(
    device: &Device,
    image: vk::Image,
    memory: vk::DeviceMemory,
    offset: vk::DeviceSize,
    label: &str,
) -> Result<(), String> {
    let bind_info = vk::BindImageMemoryInfo::builder()
        .image(image)
        .memory(memory)
        .memory_offset(offset)
        .build();
    unsafe { device.bind_image_memory2(&[bind_info]) }
        .map_err(|err| format!("vkBindImageMemory2(vulkanalia {label}): {err:?}"))
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_map_memory2(
    device: &Device,
    memory: vk::DeviceMemory,
    offset: vk::DeviceSize,
    size: vk::DeviceSize,
    flags: vk::MemoryMapFlags,
    label: &str,
) -> Result<*mut c_void, String> {
    let map_info = vk::MemoryMapInfo::builder()
        .memory(memory)
        .offset(offset)
        .size(size)
        .flags(flags)
        .build();
    unsafe { device.map_memory2(&map_info) }
        .map_err(|err| format!("vkMapMemory2(vulkanalia {label}): {err:?}"))
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_unmap_memory2(
    device: &Device,
    memory: vk::DeviceMemory,
    label: &str,
) -> Result<(), String> {
    let unmap_info = vk::MemoryUnmapInfo::builder()
        .memory(memory)
        .flags(vk::MemoryUnmapFlags::empty())
        .build();
    unsafe { device.unmap_memory2(&unmap_info) }
        .map_err(|err| format!("vkUnmapMemory2(vulkanalia {label}): {err:?}"))
}
