use std::path::PathBuf;
use std::ptr;

use ash::vk;

use crate::core::FitMode;

use super::{NativeVulkanClearColor, NativeVulkanError, native_vulkan_memory_type_index};

pub(super) struct NativeVulkanStaticImageUpload {
    pub(super) buffer: vk::Buffer,
    pub(super) memory: vk::DeviceMemory,
    pub(super) buffer_image_copy: vk::BufferImageCopy,
    pub(super) size_bytes: vk::DeviceSize,
}

impl NativeVulkanStaticImageUpload {
    pub(super) fn new(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        device: &ash::Device,
        source: &PathBuf,
        fit: FitMode,
        background: Option<&str>,
        swapchain_format: vk::Format,
        extent: vk::Extent2D,
    ) -> Result<Self, NativeVulkanError> {
        let pixels = native_vulkan_static_image_pixels(
            source,
            fit,
            background,
            swapchain_format,
            (extent.width, extent.height),
        )?;
        let size_bytes = pixels.len() as vk::DeviceSize;
        let buffer_create_info = vk::BufferCreateInfo::default()
            .size(size_bytes)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer =
            unsafe { device.create_buffer(&buffer_create_info, None) }.map_err(|result| {
                NativeVulkanError::Vulkan {
                    operation: "vkCreateBuffer(static_image)",
                    result,
                }
            })?;
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let memory_type_index = native_vulkan_memory_type_index(
            &memory_properties,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .ok_or(NativeVulkanError::MissingMemoryType(
            "static image staging buffer",
        ))?;
        let allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type_index);
        let memory = unsafe { device.allocate_memory(&allocate_info, None) }.map_err(|result| {
            unsafe {
                device.destroy_buffer(buffer, None);
            }
            NativeVulkanError::Vulkan {
                operation: "vkAllocateMemory(static_image)",
                result,
            }
        })?;
        if let Err(err) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_buffer(buffer, None);
            }
            return Err(NativeVulkanError::Vulkan {
                operation: "vkBindBufferMemory(static_image)",
                result: err,
            });
        }
        let map = unsafe { device.map_memory(memory, 0, size_bytes, vk::MemoryMapFlags::empty()) }
            .map_err(|result| {
                unsafe {
                    device.free_memory(memory, None);
                    device.destroy_buffer(buffer, None);
                }
                NativeVulkanError::Vulkan {
                    operation: "vkMapMemory(static_image)",
                    result,
                }
            })?;
        unsafe {
            ptr::copy_nonoverlapping(pixels.as_ptr(), map.cast::<u8>(), pixels.len());
            device.unmap_memory(memory);
        }

        Ok(Self {
            buffer,
            memory,
            buffer_image_copy: vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                image_extent: vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                },
            },
            size_bytes,
        })
    }

    pub(super) fn destroy(self, device: &ash::Device) {
        unsafe {
            device.free_memory(self.memory, None);
            device.destroy_buffer(self.buffer, None);
        }
    }
}

pub(super) fn native_vulkan_static_image_pixels(
    source: &PathBuf,
    fit: FitMode,
    background: Option<&str>,
    format: vk::Format,
    target_size: (u32, u32),
) -> Result<Vec<u8>, NativeVulkanError> {
    if target_size.0 == 0 || target_size.1 == 0 {
        return Err(NativeVulkanError::StaticImage(
            "target image size is zero".to_owned(),
        ));
    }
    let image = image::ImageReader::open(source)
        .map_err(|err| NativeVulkanError::StaticImage(format!("open {}: {err}", source.display())))?
        .with_guessed_format()
        .map_err(|err| {
            NativeVulkanError::StaticImage(format!("guess format {}: {err}", source.display()))
        })?
        .decode()
        .map_err(|err| {
            NativeVulkanError::StaticImage(format!("decode {}: {err}", source.display()))
        })?
        .to_rgba8();
    let mut canvas = image::RgbaImage::from_pixel(
        target_size.0,
        target_size.1,
        native_vulkan_parse_background(background),
    );
    native_vulkan_blit_fit(&image, &mut canvas, fit);
    Ok(native_vulkan_encode_swapchain_pixels(&canvas, format))
}

pub(super) fn native_vulkan_parse_background(background: Option<&str>) -> image::Rgba<u8> {
    let Some(value) = background else {
        return image::Rgba([0, 0, 0, 255]);
    };
    let Some(hex) = value.trim().strip_prefix('#') else {
        return image::Rgba([0, 0, 0, 255]);
    };
    if hex.len() != 6 {
        return image::Rgba([0, 0, 0, 255]);
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    image::Rgba([r, g, b, 255])
}

pub(super) fn native_vulkan_static_background_clear_color(
    background: Option<&str>,
) -> NativeVulkanClearColor {
    let rgba = native_vulkan_parse_background(background);
    NativeVulkanClearColor {
        r: rgba[0] as f32 / 255.0,
        g: rgba[1] as f32 / 255.0,
        b: rgba[2] as f32 / 255.0,
        a: rgba[3] as f32 / 255.0,
    }
}

pub(super) fn native_vulkan_blit_fit(
    source: &image::RgbaImage,
    canvas: &mut image::RgbaImage,
    fit: FitMode,
) {
    let source_width = source.width().max(1);
    let source_height = source.height().max(1);
    let target_width = canvas.width().max(1);
    let target_height = canvas.height().max(1);
    match fit {
        FitMode::Stretch => {
            let resized = image::imageops::resize(
                source,
                target_width,
                target_height,
                image::imageops::FilterType::Triangle,
            );
            image::imageops::replace(canvas, &resized, 0, 0);
        }
        FitMode::Center => {
            let x = (target_width as i64 - source_width as i64) / 2;
            let y = (target_height as i64 - source_height as i64) / 2;
            image::imageops::overlay(canvas, source, x, y);
        }
        FitMode::Tile => {
            let mut y = 0;
            while y < target_height {
                let mut x = 0;
                while x < target_width {
                    image::imageops::overlay(canvas, source, x as i64, y as i64);
                    x = x.saturating_add(source_width);
                }
                y = y.saturating_add(source_height);
            }
        }
        FitMode::Contain | FitMode::Cover => {
            let scale_x = target_width as f64 / source_width as f64;
            let scale_y = target_height as f64 / source_height as f64;
            let scale = if fit == FitMode::Cover {
                scale_x.max(scale_y)
            } else {
                scale_x.min(scale_y)
            };
            let scaled_width = ((source_width as f64 * scale).round() as u32).max(1);
            let scaled_height = ((source_height as f64 * scale).round() as u32).max(1);
            let resized = image::imageops::resize(
                source,
                scaled_width,
                scaled_height,
                image::imageops::FilterType::Triangle,
            );
            let x = (target_width as i64 - scaled_width as i64) / 2;
            let y = (target_height as i64 - scaled_height as i64) / 2;
            image::imageops::overlay(canvas, &resized, x, y);
        }
    }
}

pub(super) fn native_vulkan_encode_swapchain_pixels(
    image: &image::RgbaImage,
    format: vk::Format,
) -> Vec<u8> {
    let mut pixels = image.as_raw().clone();
    if matches!(
        format,
        vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB
    ) {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_static_background_hex() {
        assert_eq!(
            native_vulkan_parse_background(Some("#102030")),
            image::Rgba([0x10, 0x20, 0x30, 255])
        );
        assert_eq!(
            native_vulkan_parse_background(Some("bad")),
            image::Rgba([0, 0, 0, 255])
        );
    }

    #[test]
    fn encodes_bgra_swapchain_pixels() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 4]));

        assert_eq!(
            native_vulkan_encode_swapchain_pixels(&image, vk::Format::B8G8R8A8_UNORM),
            vec![3, 2, 1, 4]
        );
        assert_eq!(
            native_vulkan_encode_swapchain_pixels(&image, vk::Format::R8G8B8A8_UNORM),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn contain_fit_preserves_letterbox_background() {
        let source = image::RgbaImage::from_pixel(2, 1, image::Rgba([255, 0, 0, 255]));
        let mut canvas = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));

        native_vulkan_blit_fit(&source, &mut canvas, FitMode::Contain);

        assert_eq!(canvas.get_pixel(0, 0), &image::Rgba([0, 0, 0, 255]));
        assert_eq!(canvas.get_pixel(0, 1), &image::Rgba([255, 0, 0, 255]));
        assert_eq!(canvas.get_pixel(3, 2), &image::Rgba([255, 0, 0, 255]));
        assert_eq!(canvas.get_pixel(0, 3), &image::Rgba([0, 0, 0, 255]));
    }

    #[test]
    fn static_background_clear_color_matches_legacy_background_parse() {
        let color = native_vulkan_static_background_clear_color(Some("#336699"));

        assert_eq!(
            color,
            NativeVulkanClearColor {
                r: 0x33 as f32 / 255.0,
                g: 0x66 as f32 / 255.0,
                b: 0x99 as f32 / 255.0,
                a: 1.0,
            }
        );
        assert_eq!(
            native_vulkan_static_background_clear_color(None),
            NativeVulkanClearColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }
        );
    }
}
