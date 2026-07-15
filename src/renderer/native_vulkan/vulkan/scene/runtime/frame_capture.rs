//! Bounded scene swapchain readback and PNG frame-sequence capture.

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
    pub source_width: u32,
    pub source_height: u32,
    pub region_x: u32,
    pub region_y: u32,
    pub region_width: u32,
    pub region_height: u32,
    pub width: u32,
    pub height: u32,
    pub source_format: String,
    pub output_format: &'static str,
    pub frame_number: u64,
    pub last_frame_number: u64,
    pub frame_count: u64,
    pub frame_step: u64,
    pub downscale: u32,
    pub rgba_bytes: u64,
    pub png_bytes: u64,
}

pub(super) struct SceneFrameCapture {
    path: PathBuf,
    source_extent: vk::Extent2D,
    image_offset: vk::Offset3D,
    extent: vk::Extent2D,
    source_format: vk::Format,
    target_frame_number: u64,
    target_frame_count: u64,
    target_frame_step: u64,
    downscale: u32,
    output_extent: vk::Extent2D,
    byte_count: u64,
    output_byte_count: u64,
    readback_buffer: NativeVulkanVulkanaliaBuffer,
    captured_frames: Vec<SceneFrameCapturePixels>,
    pending_frame_number: Option<u64>,
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
        target_frame_number: u64,
        target_frame_count: u64,
        target_frame_step: u64,
        downscale: u32,
        region: Option<(u32, u32, u32, u32)>,
    ) -> Result<Self, String> {
        if target_frame_number == 0 {
            return Err("scene frame capture number must be at least 1".to_owned());
        }
        if target_frame_count == 0 {
            return Err("scene frame capture count must be at least 1".to_owned());
        }
        if target_frame_step == 0 {
            return Err("scene frame capture step must be at least 1".to_owned());
        }
        if downscale == 0 {
            return Err("scene frame capture downscale must be at least 1".to_owned());
        }
        scene_frame_capture_channel_order(source_format)?;
        let (image_offset, capture_extent) = scene_frame_capture_region(extent, region)?;
        let source_extent = extent;
        let extent = capture_extent;
        let byte_count = scene_frame_capture_byte_count(extent)?;
        let output_extent = scene_frame_capture_output_extent(extent, downscale);
        let output_byte_count = scene_frame_capture_byte_count(output_extent)?;
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
            source_extent,
            image_offset,
            extent,
            source_format,
            target_frame_number,
            target_frame_count,
            target_frame_step,
            downscale,
            output_extent,
            byte_count,
            output_byte_count,
            readback_buffer,
            captured_frames: Vec::with_capacity(
                target_frame_count.min(usize::MAX as u64) as usize,
            ),
            pending_frame_number: None,
            snapshot: None,
        })
    }

    pub(super) fn is_pending(&self) -> bool {
        (self.captured_frames.len() as u64) < self.target_frame_count && self.snapshot.is_none()
    }

    pub(super) fn should_capture(&self, frame_number: u64) -> bool {
        self.is_pending()
            && self.pending_frame_number.is_none()
            && frame_number
                == self
                    .target_frame_number
                    .saturating_add(
                        (self.captured_frames.len() as u64)
                            .saturating_mul(self.target_frame_step),
                    )
    }

    pub(super) fn mark_submitted(&mut self, frame_number: u64) {
        self.pending_frame_number = Some(frame_number);
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
            .image_offset(self.image_offset)
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
    ) -> Result<(), String> {
        let Some(frame_number) = self.pending_frame_number else {
            return Ok(());
        };
        let pixels = native_vulkan_vulkanalia_read_host_buffer(
            device,
            &self.readback_buffer,
            self.byte_count,
        )?;
        let rgba = scene_frame_capture_rgba(self.source_format, pixels)?;
        let rgba = scene_frame_capture_downscale_rgba(
            rgba,
            self.extent,
            self.output_extent,
            self.downscale,
        );
        self.captured_frames
            .push(SceneFrameCapturePixels { frame_number, rgba });
        self.pending_frame_number = None;
        Ok(())
    }

    pub(super) fn write_png(&mut self) -> Result<(), String> {
        if self.snapshot.is_some() {
            return Ok(());
        }
        let captured_frames = std::mem::take(&mut self.captured_frames);
        let first_frame = captured_frames.first().ok_or_else(|| {
            "scene frame capture has no completed GPU readback to encode".to_owned()
        })?;
        let frame_number = first_frame.frame_number;
        let last_frame_number = captured_frames
            .last()
            .map_or(frame_number, |frame| frame.frame_number);
        let frame_count = captured_frames.len() as u64;
        let multiple_frames = self.target_frame_count > 1;
        let mut png_bytes = 0u64;
        for captured_frame in captured_frames {
            let path = scene_frame_capture_output_path(
                &self.path,
                captured_frame.frame_number,
                multiple_frames,
            );
            png_bytes = png_bytes.saturating_add(write_scene_frame_png(
                &path,
                self.output_extent.width,
                self.output_extent.height,
                &captured_frame.rgba,
            )?);
        }
        self.snapshot = Some(NativeVulkanSceneFrameCaptureSnapshot {
            path: self.path.clone(),
            source_width: self.source_extent.width,
            source_height: self.source_extent.height,
            region_x: self.image_offset.x as u32,
            region_y: self.image_offset.y as u32,
            region_width: self.extent.width,
            region_height: self.extent.height,
            width: self.output_extent.width,
            height: self.output_extent.height,
            source_format: format!("{:?}", self.source_format),
            output_format: "PNG/RGBA8",
            frame_number,
            last_frame_number,
            frame_count,
            frame_step: self.target_frame_step,
            downscale: self.downscale,
            rgba_bytes: self.output_byte_count.saturating_mul(frame_count),
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

fn scene_frame_capture_region(
    source_extent: vk::Extent2D,
    region: Option<(u32, u32, u32, u32)>,
) -> Result<(vk::Offset3D, vk::Extent2D), String> {
    let (x, y, width, height) = region.unwrap_or((
        0,
        0,
        source_extent.width,
        source_extent.height,
    ));
    let end_x = x
        .checked_add(width)
        .ok_or_else(|| "scene frame capture region x range overflows".to_owned())?;
    let end_y = y
        .checked_add(height)
        .ok_or_else(|| "scene frame capture region y range overflows".to_owned())?;
    if width == 0
        || height == 0
        || end_x > source_extent.width
        || end_y > source_extent.height
    {
        return Err(format!(
            "scene frame capture region {x},{y},{width},{height} exceeds {}x{} swapchain",
            source_extent.width, source_extent.height
        ));
    }
    Ok((
        vk::Offset3D {
            x: x as i32,
            y: y as i32,
            z: 0,
        },
        vk::Extent2D { width, height },
    ))
}

fn scene_frame_capture_output_path(
    path: &Path,
    frame_number: u64,
    multiple_frames: bool,
) -> PathBuf {
    if !multiple_frames {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("frame");
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("png");
    path.with_file_name(format!("{stem}-{frame_number:06}.{extension}"))
}

fn scene_frame_capture_output_extent(extent: vk::Extent2D, downscale: u32) -> vk::Extent2D {
    vk::Extent2D {
        width: extent.width.div_ceil(downscale).max(1),
        height: extent.height.div_ceil(downscale).max(1),
    }
}

fn scene_frame_capture_downscale_rgba(
    rgba: Vec<u8>,
    source_extent: vk::Extent2D,
    output_extent: vk::Extent2D,
    downscale: u32,
) -> Vec<u8> {
    if downscale == 1 {
        return rgba;
    }
    let mut output = vec![0; output_extent.width as usize * output_extent.height as usize * 4];
    let sample_offset = downscale / 2;
    for output_y in 0..output_extent.height {
        let source_y = output_y
            .saturating_mul(downscale)
            .saturating_add(sample_offset)
            .min(source_extent.height - 1);
        for output_x in 0..output_extent.width {
            let source_x = output_x
                .saturating_mul(downscale)
                .saturating_add(sample_offset)
                .min(source_extent.width - 1);
            let source = (source_y as usize * source_extent.width as usize + source_x as usize) * 4;
            let destination =
                (output_y as usize * output_extent.width as usize + output_x as usize) * 4;
            output[destination..destination + 4].copy_from_slice(&rgba[source..source + 4]);
        }
    }
    output
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

fn write_scene_frame_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<u64, String> {
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
        writer
            .finish()
            .map_err(|err| format!("finish scene frame capture PNG {}: {err}", path.display()))?;
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
        let rgba =
            scene_frame_capture_rgba(vk::Format::B8G8R8A8_SRGB, vec![30, 20, 10, 255, 3, 2, 1, 4])
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
        let err =
            scene_frame_capture_rgba(vk::Format::R16G16B16A16_SFLOAT, vec![0; 8]).unwrap_err();
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

    #[test]
    fn sequence_capture_paths_preserve_parent_stem_and_extension() {
        assert_eq!(
            scene_frame_capture_output_path(Path::new("/tmp/timeline/frame.png"), 42, true),
            PathBuf::from("/tmp/timeline/frame-000042.png")
        );
        assert_eq!(
            scene_frame_capture_output_path(Path::new("/tmp/frame.png"), 42, false),
            PathBuf::from("/tmp/frame.png")
        );
    }

    #[test]
    fn sequence_downscale_keeps_full_render_extent_and_samples_pixel_centers() {
        let source_extent = vk::Extent2D {
            width: 4,
            height: 2,
        };
        let output_extent = scene_frame_capture_output_extent(source_extent, 2);
        assert_eq!(output_extent.width, 2);
        assert_eq!(output_extent.height, 1);
        let rgba = (0u8..32).collect::<Vec<_>>();
        assert_eq!(
            scene_frame_capture_downscale_rgba(rgba, source_extent, output_extent, 2),
            vec![20, 21, 22, 23, 28, 29, 30, 31]
        );
    }

    #[test]
    fn capture_region_keeps_source_coordinates_and_rejects_overflow() {
        let source = vk::Extent2D {
            width: 3840,
            height: 2160,
        };
        assert_eq!(
            scene_frame_capture_region(source, Some((1200, 800, 640, 360))),
            Ok((
                vk::Offset3D {
                    x: 1200,
                    y: 800,
                    z: 0,
                },
                vk::Extent2D {
                    width: 640,
                    height: 360,
                },
            ))
        );
        assert!(scene_frame_capture_region(source, Some((3800, 0, 100, 100))).is_err());
    }
}
