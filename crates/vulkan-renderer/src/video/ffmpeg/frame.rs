use std::fmt;
use std::sync::Arc;

use vulkanalia::{
    prelude::v1_4::*,
    vk::{self, Handle},
};

use super::FfmpegTimeBase;
use super::ffi::{self, AVFrame, FrameSnapshot};
use super::resources::{ReusableFrame, ffmpeg_ok};
use crate::external_image::retain_external_image_for_owner;
use crate::sync::retain_external_timeline_semaphore_for_owner;
use crate::video::VideoDecodeDevice;
use crate::{
    CommandBuffer, CommandEncoder, Error, Extent2D, Extent3D, ExternalImageViewDescriptor,
    ExternalTimelineSemaphoreDescriptor, Result, RetainedExternalImage,
    RetainedExternalTimelineSemaphore, SampleCount, SemaphoreWait, SubmissionLease,
    SubmissionResource, TextureFormat, TextureUsages,
};

/// Software color model and exact multiplane Vulkan picture format.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DecodedVideoFormat {
    Nv12,
    P010,
}

impl DecodedVideoFormat {
    pub const fn picture_format(self) -> TextureFormat {
        match self {
            Self::Nv12 => TextureFormat::G8B8R8TwoPlane420Unorm,
            Self::P010 => TextureFormat::G10X6B10X6R10X6TwoPlane420Unorm3Pack16,
        }
    }

    const fn plane_formats(self) -> [TextureFormat; 2] {
        match self {
            Self::Nv12 => [TextureFormat::R8Unorm, TextureFormat::Rg8Unorm],
            Self::P010 => [TextureFormat::R16Unorm, TextureFormat::Rg16Unorm],
        }
    }
}

/// The two sampled plane-array views retained by one decoded frame.
#[derive(Clone, Copy, Debug)]
pub struct DecodedVideoPlanes<'a> {
    pub y: &'a RetainedExternalImage,
    pub uv: &'a RetainedExternalImage,
}

/// Immutable, cloneable decoded-frame lease.
///
/// The underlying AVFrame, VkImage and timeline semaphore remain alive through
/// every clone. No raw FFmpeg or Vulkan handle crosses the public API.
#[derive(Clone)]
pub struct DecodedVideoFrame {
    format: DecodedVideoFormat,
    extent: Extent2D,
    array_layers: u32,
    y_plane: RetainedExternalImage,
    uv_plane: RetainedExternalImage,
    ready: RetainedExternalTimelineSemaphore,
    ready_value: u64,
    source_queue_family: u32,
    current_layout: vk::ImageLayout,
    pts_raw: Option<i64>,
    duration_raw: Option<i64>,
    time_base: FfmpegTimeBase,
}

impl fmt::Debug for DecodedVideoFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DecodedVideoFrame")
            .field("format", &self.format)
            .field("extent", &self.extent)
            .field("array_layers", &self.array_layers)
            .field("ready_value", &self.ready_value)
            .field("pts_raw", &self.pts_raw)
            .field("duration_raw", &self.duration_raw)
            .field("time_base", &self.time_base)
            .finish_non_exhaustive()
    }
}

impl DecodedVideoFrame {
    pub const fn format(&self) -> DecodedVideoFormat {
        self.format
    }

    pub const fn extent(&self) -> Extent2D {
        self.extent
    }

    pub const fn array_layers(&self) -> u32 {
        self.array_layers
    }

    pub const fn planes(&self) -> DecodedVideoPlanes<'_> {
        DecodedVideoPlanes {
            y: &self.y_plane,
            uv: &self.uv_plane,
        }
    }

    pub const fn pts_raw(&self) -> Option<i64> {
        self.pts_raw
    }

    pub const fn duration_raw(&self) -> Option<i64> {
        self.duration_raw
    }

    pub const fn time_base(&self) -> FfmpegTimeBase {
        self.time_base
    }

    pub fn pts_ns(&self) -> Option<u64> {
        self.time_base.timestamp_ns(self.pts_raw)
    }

    pub fn duration_ns(&self) -> Option<u64> {
        self.time_base.timestamp_ns(self.duration_raw)
    }

    pub const fn ready_value(&self) -> u64 {
        self.ready_value
    }
}

impl CommandEncoder {
    /// Acquires a decoded multiplane image for fragment sampling.
    ///
    /// Pair this with [`CommandEncoder::end_decoded_video_sampling`] after the
    /// final sampling draw and submit through
    /// [`crate::Queue::submit_with_decoded_video_frames`].
    pub fn begin_decoded_video_sampling(&mut self, frame: &DecodedVideoFrame) -> Result<()> {
        frame.validate_owner(self.owner())?;
        frame.record_sampling_barrier(self, true);
        self.retain_resource(&frame.y_plane);
        self.retain_resource(&frame.uv_plane);
        self.retain_resource(&frame.ready);
        Ok(())
    }

    /// Restores FFmpeg's exact image layout and video-queue ownership after the
    /// final sampling draw so decoder-pool reuse observes the original state.
    pub fn end_decoded_video_sampling(&mut self, frame: &DecodedVideoFrame) -> Result<()> {
        frame.validate_owner(self.owner())?;
        frame.record_sampling_barrier(self, false);
        self.retain_resource(&frame.y_plane);
        self.retain_resource(&frame.uv_plane);
        self.retain_resource(&frame.ready);
        Ok(())
    }
}

impl crate::Queue {
    /// Submits commands which sample decoded frames, waits their decode
    /// timelines and retains every AVFrame until the GPU timeline retires.
    pub fn submit_with_decoded_video_frames<I>(
        &self,
        command_buffers: I,
        frames: &[DecodedVideoFrame],
    ) -> Result<crate::FrameToken>
    where
        I: IntoIterator<Item = CommandBuffer>,
    {
        let (waits, leases) = decoded_video_submission_parts(&self.owner, frames)?;
        self.submit_retained(command_buffers, &waits, leases)
    }
}

pub(crate) fn decoded_video_submission_parts(
    owner: &Arc<crate::backend::DeviceOwner>,
    frames: &[DecodedVideoFrame],
) -> Result<(Vec<SemaphoreWait>, Vec<SubmissionLease>)> {
    let waits = frames
        .iter()
        .map(|frame| {
            frame.validate_owner(owner)?;
            frame
                .ready
                .wait(frame.ready_value, crate::PipelineStages::FRAGMENT_SHADER)
        })
        .collect::<Result<Vec<_>>>()?;
    let leases = frames
        .iter()
        .flat_map(|frame| {
            [
                frame.y_plane.submission_lease(),
                frame.uv_plane.submission_lease(),
                frame.ready.submission_lease(),
            ]
        })
        .collect();
    Ok((waits, leases))
}

impl DecodedVideoFrame {
    fn validate_owner(&self, owner: &Arc<crate::backend::DeviceOwner>) -> Result<()> {
        if !Arc::ptr_eq(self.y_plane.owner(), owner) || !Arc::ptr_eq(self.uv_plane.owner(), owner) {
            return Err(Error::Validation(
                "decoded video frame belongs to a different Device".into(),
            ));
        }
        Ok(())
    }

    fn record_sampling_barrier(&self, encoder: &mut CommandEncoder, acquire: bool) {
        let graphics_family = encoder.owner().graphics_queue_family();
        let (source_queue, destination_queue) = if graphics_family == self.source_queue_family {
            (vk::QUEUE_FAMILY_IGNORED, vk::QUEUE_FAMILY_IGNORED)
        } else if acquire {
            (self.source_queue_family, graphics_family)
        } else {
            (graphics_family, self.source_queue_family)
        };
        let (
            source_stages,
            source_access,
            destination_stages,
            destination_access,
            old_layout,
            new_layout,
        ) = if acquire {
            (
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                self.current_layout,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            )
        } else {
            (
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::PipelineStageFlags2::NONE,
                vk::AccessFlags2::NONE,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                self.current_layout,
            )
        };
        let barrier = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(source_stages)
            .src_access_mask(source_access)
            .dst_stage_mask(destination_stages)
            .dst_access_mask(destination_access)
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(source_queue)
            .dst_queue_family_index(destination_queue)
            .image(self.y_plane.raw_image())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: self.array_layers,
            })
            .build();
        unsafe { encoder.external_image_barrier(barrier) };
    }
}

pub(super) fn move_decoded_frame(
    reusable: &mut ReusableFrame,
    device: &VideoDecodeDevice,
    time_base: FfmpegTimeBase,
) -> Result<DecodedVideoFrame> {
    let raw = unsafe { ffi::vr_ffmpeg_frame_alloc() };
    if raw.is_null() {
        return Err(Error::VideoDecode(
            "allocate retained FFmpeg decoded frame failed".into(),
        ));
    }
    unsafe { ffi::vr_ffmpeg_frame_move(raw, reusable.raw()) };
    let lease = Arc::new(DecodedFrameLease {
        raw,
        device: device.clone(),
    });
    let c_snapshot_size = unsafe { ffi::vr_ffmpeg_frame_snapshot_size() };
    if c_snapshot_size != std::mem::size_of::<FrameSnapshot>() {
        return Err(Error::VideoDecode(format!(
            "FFmpeg frame snapshot ABI size mismatch: C={c_snapshot_size}, Rust={}",
            std::mem::size_of::<FrameSnapshot>()
        )));
    }
    let mut snapshot = FrameSnapshot::default();
    ffmpeg_ok(
        unsafe { ffi::vr_ffmpeg_frame_snapshot(lease.raw, &mut snapshot) },
        "snapshot FFmpeg AVVkFrame",
    )?;
    build_frame(lease, snapshot, time_base)
}

fn build_frame(
    lease: Arc<DecodedFrameLease>,
    snapshot: FrameSnapshot,
    time_base: FfmpegTimeBase,
) -> Result<DecodedVideoFrame> {
    let device = &lease.device;
    let format = validate_snapshot(&snapshot, device.queue_family)?;
    let extent = Extent2D::new(snapshot.width as u32, snapshot.height as u32);
    let array_layers = snapshot.array_layers as u32;
    let image = vk::Image::from_raw(snapshot.image);
    let [y_format, uv_format] = format.plane_formats();
    let plane = |label: &'static str, plane_format, aspect_mask| ExternalImageViewDescriptor {
        label: Some(label.into()),
        image,
        view_type: vk::ImageViewType::_2D_ARRAY,
        format: plane_format,
        extent: Extent3D::new(extent.width, extent.height, 1),
        mip_levels: 1,
        array_layers,
        samples: SampleCount::One,
        // AVVulkanFramesContext.usage is only the caller's extra usage. FFmpeg
        // independently guarantees SAMPLED when the picture format supports it;
        // do not claim the field contains the decoder's full image-create usage.
        usage: TextureUsages::SAMPLED,
        view_usage: Some(TextureUsages::SAMPLED),
        components: vk::ComponentMapping::default(),
        subresource_range: vk::ImageSubresourceRange {
            aspect_mask,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: array_layers,
        },
    };
    let owner = Arc::clone(&device.owner);
    let y_plane = retain_external_image_for_owner(
        Arc::clone(&owner),
        &plane(
            "ffmpeg-decoded-y-plane",
            y_format,
            vk::ImageAspectFlags::PLANE_0,
        ),
        Arc::clone(&lease),
    )?;
    let uv_plane = retain_external_image_for_owner(
        Arc::clone(&owner),
        &plane(
            "ffmpeg-decoded-uv-plane",
            uv_format,
            vk::ImageAspectFlags::PLANE_1,
        ),
        Arc::clone(&lease),
    )?;
    let ready = retain_external_timeline_semaphore_for_owner(
        owner,
        &ExternalTimelineSemaphoreDescriptor {
            label: Some("ffmpeg-decoded-frame-ready".into()),
            semaphore: vk::Semaphore::from_raw(snapshot.semaphore),
        },
        lease,
    )?;
    let no_timestamp = unsafe { ffi::vr_ffmpeg_nopts_value() };
    Ok(DecodedVideoFrame {
        format,
        extent,
        array_layers,
        y_plane,
        uv_plane,
        ready,
        ready_value: snapshot.semaphore_value,
        source_queue_family: snapshot.queue_family,
        current_layout: vk::ImageLayout::from_raw(snapshot.layout),
        pts_raw: (snapshot.pts != no_timestamp).then_some(snapshot.pts),
        duration_raw: (snapshot.duration > 0).then_some(snapshot.duration),
        time_base,
    })
}

fn validate_snapshot(
    snapshot: &FrameSnapshot,
    video_queue_family: u32,
) -> Result<DecodedVideoFormat> {
    if snapshot.frame_format != unsafe { ffi::vr_ffmpeg_pixel_vulkan() } {
        return invalid(format!(
            "decoded frame is not AV_PIX_FMT_VULKAN: {}",
            snapshot.frame_format
        ));
    }
    if snapshot.image_count != 1 || snapshot.semaphore_count != 1 {
        return invalid(format!(
            "decoded frame requires one multiplane image and one timeline semaphore, got images={} semaphores={}",
            snapshot.image_count, snapshot.semaphore_count
        ));
    }
    if snapshot.image == 0 || snapshot.semaphore == 0 {
        return invalid("decoded image and timeline semaphore must not be null".into());
    }
    if snapshot.semaphore_value == 0 {
        return invalid("decoded timeline semaphore value must be positive".into());
    }
    if snapshot.width <= 0 || snapshot.height <= 0 || snapshot.array_layers <= 0 {
        return invalid(format!(
            "decoded extent/layers are invalid: {}x{}, layers={}",
            snapshot.width, snapshot.height, snapshot.array_layers
        ));
    }
    if snapshot.queue_family != video_queue_family {
        return invalid(format!(
            "decoded queue family {} differs from renderer-owned video queue {}",
            snapshot.queue_family, video_queue_family
        ));
    }
    let layout = vk::ImageLayout::from_raw(snapshot.layout);
    if !matches!(
        layout,
        vk::ImageLayout::GENERAL
            | vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
            | vk::ImageLayout::VIDEO_DECODE_DST_KHR
            | vk::ImageLayout::VIDEO_DECODE_DPB_KHR
    ) {
        return invalid(format!("unsupported decoded image layout {layout:?}"));
    }
    let flags = vk::ImageCreateFlags::from_bits(snapshot.image_flags).ok_or_else(|| {
        Error::VideoDecode(format!(
            "decoded image has unknown create flags 0x{:x}",
            snapshot.image_flags
        ))
    })?;
    if !flags.contains(vk::ImageCreateFlags::MUTABLE_FORMAT) {
        return invalid("decoded multiplane image is missing MUTABLE_FORMAT".into());
    }
    let software_format = snapshot.software_format;
    let (format, expected_picture) = if software_format == unsafe { ffi::vr_ffmpeg_pixel_nv12() } {
        (
            DecodedVideoFormat::Nv12,
            vk::Format::G8_B8R8_2PLANE_420_UNORM,
        )
    } else if software_format == unsafe { ffi::vr_ffmpeg_pixel_p010() } {
        (
            DecodedVideoFormat::P010,
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
        )
    } else {
        return invalid(format!(
            "unsupported FFmpeg decoded software format {software_format}"
        ));
    };
    let picture = vk::Format::from_raw(snapshot.picture_format);
    if picture != expected_picture {
        return invalid(format!(
            "decoded picture format {picture:?} does not match {format:?} ({expected_picture:?})"
        ));
    }
    Ok(format)
}

fn invalid<T>(message: String) -> Result<T> {
    Err(Error::VideoDecode(message))
}

struct DecodedFrameLease {
    raw: *mut AVFrame,
    // Keeps the exact logical device alive until FFmpeg releases AVVkFrame.
    device: VideoDecodeDevice,
}

impl Drop for DecodedFrameLease {
    fn drop(&mut self) {
        unsafe { ffi::vr_ffmpeg_frame_free(&mut self.raw) };
    }
}

// Safety: after av_frame_move_ref and snapshot, this AVFrame is immutable. No
// method exposes its pointer or mutates it; its only later operation is the
// refcounted av_frame_free destructor, which FFmpeg permits on any thread.
unsafe impl Send for DecodedFrameLease {}
unsafe impl Sync for DecodedFrameLease {}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_snapshot() -> FrameSnapshot {
        FrameSnapshot {
            image: 7,
            semaphore: 9,
            semaphore_value: 11,
            queue_family: 3,
            image_flags: vk::ImageCreateFlags::MUTABLE_FORMAT.bits(),
            layout: vk::ImageLayout::VIDEO_DECODE_DPB_KHR.as_raw(),
            frame_format: unsafe { ffi::vr_ffmpeg_pixel_vulkan() },
            software_format: unsafe { ffi::vr_ffmpeg_pixel_nv12() },
            picture_format: vk::Format::G8_B8R8_2PLANE_420_UNORM.as_raw(),
            width: 1920,
            height: 1080,
            array_layers: 1,
            image_count: 1,
            semaphore_count: 1,
            ..FrameSnapshot::default()
        }
    }

    #[test]
    fn rust_snapshot_matches_renderer_owned_c_abi_size() {
        assert_eq!(std::mem::size_of::<FrameSnapshot>(), unsafe {
            ffi::vr_ffmpeg_frame_snapshot_size()
        });
    }

    #[test]
    fn decoded_snapshot_requires_exact_multiplane_format_and_queue() {
        let mut snapshot = valid_snapshot();
        assert_eq!(
            validate_snapshot(&snapshot, 3).unwrap(),
            DecodedVideoFormat::Nv12
        );
        snapshot.picture_format = vk::Format::G8_B8_R8_3PLANE_420_UNORM.as_raw();
        assert!(validate_snapshot(&snapshot, 3).is_err());
        snapshot = valid_snapshot();
        assert!(validate_snapshot(&snapshot, 4).is_err());
    }

    #[test]
    fn decoded_snapshot_rejects_non_mutable_or_unready_images() {
        let mut snapshot = valid_snapshot();
        snapshot.image_flags = vk::ImageCreateFlags::ALIAS.bits();
        assert!(validate_snapshot(&snapshot, 3).is_err());
        snapshot = valid_snapshot();
        snapshot.semaphore_value = 0;
        assert!(validate_snapshot(&snapshot, 3).is_err());
    }
}
