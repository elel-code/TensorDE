//! One-shot scene swapchain readback and PNG capture.

use std::io::{BufWriter, Write as _};
use std::path::{Path, PathBuf};

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaBuffer, NativeVulkanVulkanaliaBufferMemoryPreference,
    native_vulkan_vulkanalia_create_buffer, native_vulkan_vulkanalia_destroy_buffer,
    native_vulkan_vulkanalia_read_host_buffer,
};

use super::color_subresource_range;

const SCENE_FRAME_CAPTURE_BYTES_PER_PIXEL: u64 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneFrameCaptureSnapshot {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub source_format: String,
    pub output_format: &'static str,
    pub frame_number: u64,
    pub rgba_bytes: u64,
    pub png_bytes: u64,
}

pub(super) struct SceneFrameCapture {
    path: PathBuf,
    extent: vk::Extent2D,
    source_format: vk::Format,
    byte_count: u64,
    readback_buffer: NativeVulkanVulkanaliaBuffer,
    captured_frame: Option<SceneFrameCapturePixels>,
    snapshot: Option<NativeVulkanSceneFrameCaptureSnapshot>,
}

struct SceneFrameCapturePixels {
    frame_number: u64,
    rgba: Vec<u8>,
}

impl SceneFrameCapture {
    pub(super) fn create(
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        path: PathBuf,
        extent: vk::Extent2D,
        source_format: vk::Format,
    ) -> Result<Self, String> {
        scene_frame_capture_channel_order(source_format)?;
        let byte_count = scene_frame_capture_byte_count(extent)?;
        let readback_buffer = native_vulkan_vulkanalia_create_buffer(
            device,
            memory_properties,
            "scene-frame-capture-readback",
            byte_count,
            vk::BufferUsageFlags::TRANSFER_DST,
            NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
            None,
        )?;
        Ok(Self {
            path,
            extent,
            source_format,
            byte_count,
            readback_buffer,
            captured_frame: None,
            snapshot: None,
        })
    }

    pub(super) fn is_pending(&self) -> bool {
        self.captured_frame.is_none() && self.snapshot.is_none()
    }

    pub(super) fn record_swapchain_copy(
        &self,
        device: &Device,
        command_buffer: vk::CommandBuffer,
        swapchain_image: vk::Image,
    ) {
        let to_transfer = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(color_subresource_range())
            .build();
        let to_transfer_barriers = [to_transfer];
        unsafe {
            device.cmd_pipeline_barrier2(
                command_buffer,
                &vk::DependencyInfo::builder()
                    .image_memory_barriers(&to_transfer_barriers)
                    .build(),
            );
        }

        let copy_region = vk::BufferImageCopy::builder()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(
                vk::ImageSubresourceLayers::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(0)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: self.extent.width,
                height: self.extent.height,
                depth: 1,
            })
            .build();
        unsafe {
            device.cmd_copy_image_to_buffer(
                command_buffer,
                swapchain_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.readback_buffer.buffer,
                &[copy_region],
            );
        }

        let to_present = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags2::empty())
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(color_subresource_range())
            .build();
        let readback_to_host = vk::BufferMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_access_mask(vk::AccessFlags2::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(self.readback_buffer.buffer)
            .offset(0)
            .size(self.byte_count)
            .build();
        let image_barriers = [to_present];
        let buffer_barriers = [readback_to_host];
        unsafe {
            device.cmd_pipeline_barrier2(
                command_buffer,
                &vk::DependencyInfo::builder()
                    .image_memory_barriers(&image_barriers)
                    .buffer_memory_barriers(&buffer_barriers)
                    .build(),
            );
        }
    }

    pub(super) fn read_completed_frame(
        &mut self,
        device: &Device,
        frame_number: u64,
    ) -> Result<(), String> {
        if !self.is_pending() {
            return Ok(());
        }
        let pixels = native_vulkan_vulkanalia_read_host_buffer(
            device,
            &self.readback_buffer,
            self.byte_count,
        )?;
        let rgba = scene_frame_capture_rgba(self.source_format, pixels)?;
        self.captured_frame = Some(SceneFrameCapturePixels { frame_number, rgba });
        Ok(())
    }

    pub(super) fn write_png(&mut self) -> Result<(), String> {
        if self.snapshot.is_some() {
            return Ok(());
        }
        let captured_frame = self.captured_frame.as_ref().ok_or_else(|| {
            "scene frame capture has no completed GPU readback to encode".to_owned()
        })?;
        let png_bytes = write_scene_frame_png(
            &self.path,
            self.extent.width,
            self.extent.height,
            &captured_frame.rgba,
        )?;
        self.snapshot = Some(NativeVulkanSceneFrameCaptureSnapshot {
            path: self.path.clone(),
            width: self.extent.width,
            height: self.extent.height,
            source_format: format!("{:?}", self.source_format),
            output_format: "PNG/RGBA8",
            frame_number: captured_frame.frame_number,
            rgba_bytes: self.byte_count,
            png_bytes,
        });
        Ok(())
    }

    pub(super) fn snapshot(&self) -> Option<&NativeVulkanSceneFrameCaptureSnapshot> {
        self.snapshot.as_ref()
    }

    pub(super) fn destroy(self, device: &Device) {
        native_vulkan_vulkanalia_destroy_buffer(device, self.readback_buffer);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneFrameCaptureChannelOrder {
    Rgba,
    Bgra,
}

fn scene_frame_capture_channel_order(
    format: vk::Format,
) -> Result<SceneFrameCaptureChannelOrder, String> {
    match format {
        vk::Format::R8G8B8A8_UNORM | vk::Format::R8G8B8A8_SRGB => {
            Ok(SceneFrameCaptureChannelOrder::Rgba)
        }
        vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB => {
            Ok(SceneFrameCaptureChannelOrder::Bgra)
        }
        _ => Err(format!(
            "scene frame capture does not support swapchain format {format:?}"
        )),
    }
}

fn scene_frame_capture_byte_count(extent: vk::Extent2D) -> Result<u64, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene frame capture requires a non-zero swapchain extent".to_owned());
    }
    u64::from(extent.width)
        .checked_mul(u64::from(extent.height))
        .and_then(|pixels| pixels.checked_mul(SCENE_FRAME_CAPTURE_BYTES_PER_PIXEL))
        .ok_or_else(|| {
            format!(
                "scene frame capture extent {}x{} exceeds addressable byte range",
                extent.width, extent.height
            )
        })
}

fn scene_frame_capture_rgba(format: vk::Format, mut pixels: Vec<u8>) -> Result<Vec<u8>, String> {
    if pixels.len() % SCENE_FRAME_CAPTURE_BYTES_PER_PIXEL as usize != 0 {
        return Err(format!(
            "scene frame capture pixel payload has invalid RGBA8 byte count {}",
            pixels.len()
        ));
    }
    if scene_frame_capture_channel_order(format)? == SceneFrameCaptureChannelOrder::Bgra {
        for pixel in pixels.chunks_exact_mut(SCENE_FRAME_CAPTURE_BYTES_PER_PIXEL as usize) {
            pixel.swap(0, 2);
        }
    }
    Ok(pixels)
}

fn write_scene_frame_png(
    path: &Path,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<u64, String> {
    let expected_bytes = scene_frame_capture_byte_count(vk::Extent2D { width, height })?;
    if rgba.len() as u64 != expected_bytes {
        return Err(format!(
            "scene frame capture PNG payload has {} bytes, expected {expected_bytes} for {width}x{height}",
            rgba.len()
        ));
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "create scene frame capture directory {}: {err}",
                parent.display()
            )
        })?;
    }
    let file = std::fs::File::create(path)
        .map_err(|err| format!("create scene frame capture {}: {err}", path.display()))?;
    let mut output = BufWriter::new(file);
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|err| {
            format!(
                "write scene frame capture PNG header {}: {err}",
                path.display()
            )
        })?;
        writer.write_image_data(rgba).map_err(|err| {
            format!(
                "write scene frame capture PNG pixels {}: {err}",
                path.display()
            )
        })?;
        writer.finish().map_err(|err| {
            format!(
                "finish scene frame capture PNG {}: {err}",
                path.display()
            )
        })?;
    }
    output
        .flush()
        .map_err(|err| format!("flush scene frame capture PNG {}: {err}", path.display()))?;
    drop(output);
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|err| format!("stat scene frame capture PNG {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use image::GenericImageView as _;

    use super::*;

    static NEXT_CAPTURE_TEST_FILE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn bgra_swapchain_pixels_are_lowered_to_rgba() {
        let rgba = scene_frame_capture_rgba(
            vk::Format::B8G8R8A8_SRGB,
            vec![30, 20, 10, 255, 3, 2, 1, 4],
        )
        .unwrap();
        assert_eq!(rgba, vec![10, 20, 30, 255, 1, 2, 3, 4]);
    }

    #[test]
    fn rgba_swapchain_pixels_keep_channel_order() {
        let pixels = vec![10, 20, 30, 255];
        assert_eq!(
            scene_frame_capture_rgba(vk::Format::R8G8B8A8_UNORM, pixels.clone()).unwrap(),
            pixels
        );
    }

    #[test]
    fn unsupported_swapchain_format_is_rejected() {
        let err = scene_frame_capture_rgba(vk::Format::R16G16B16A16_SFLOAT, vec![0; 8])
            .unwrap_err();
        assert!(err.contains("R16G16B16A16_SFLOAT"));
    }

    #[test]
    fn png_writer_preserves_dimensions_and_rgba_pixels() {
        let serial = NEXT_CAPTURE_TEST_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "gilder-scene-frame-capture-{}-{serial}.png",
            std::process::id()
        ));
        let pixels = [255, 0, 0, 255, 0, 128, 255, 64];
        let png_bytes = write_scene_frame_png(&path, 2, 1, &pixels).unwrap();
        assert!(png_bytes > 0);
        let decoded = image::open(&path).unwrap();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.to_rgba8().as_raw(), &pixels);
        std::fs::remove_file(path).unwrap();
    }
}
