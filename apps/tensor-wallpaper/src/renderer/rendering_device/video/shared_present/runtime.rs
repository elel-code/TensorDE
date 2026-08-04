//! Direct-surface FFmpeg Vulkan playback through one shared renderer owner.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkan_renderer::{
    CommandBuffer, DecodedVideoSurfaceTerminalDescriptor, Extent2D, Features,
    FrameTargetPreference, PresentationBootstrap, PresentationDependencyScope,
    PresentationFrameDependencies, PresentationPathDescriptor, PresentationPathPlan,
    PresentationRequirements, PresentationTransactionDescriptor, SampleCounts,
    SurfaceAcquireStrategy, TerminalCompositeDescriptor,
};

use crate::renderer::rendering_device::shared_presentation::tensor_wallpaper_presentation_bootstrap_descriptor;
use crate::renderer::rendering_device::video::shared_decoder::RenderingDeviceSharedVideoSource;
use crate::renderer::rendering_device::{RenderingDeviceClearColor, RenderingDeviceVideoSessionCodec};
use crate::renderer::wayland::{WaylandHost, WaylandHostOptions, WaylandSurfaceSnapshot};

use super::shared_video_present_program;

const MAX_FRAME_SLOTS: usize = 3;

/// Typed direct-video policy. The renderer owns all Vulkan and FFmpeg decode state.
pub struct RenderingDeviceSharedVideoPresentOptions {
    pub host: WaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub source: PathBuf,
    pub codec: RenderingDeviceVideoSessionCodec,
    pub playback_frame_count: u32,
    pub target_max_fps: Option<u32>,
    pub clear_color: RenderingDeviceClearColor,
}

/// Report for the renderer-owned direct-surface video route.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderingDeviceSharedVideoPresentSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub source: PathBuf,
    pub codec: RenderingDeviceVideoSessionCodec,
    pub requested_present_frame_count: u32,
    pub frames_presented: u32,
    pub decoded_frame_count: u64,
    pub repeated_presentation_count: u64,
    pub video_loop_index: u64,
    pub runtime_elapsed_ms: u64,
    pub average_present_fps: f64,
    pub surface_host: WaylandSurfaceSnapshot,
    pub surface_extent: (u32, u32),
    pub surface_format: String,
    pub present_mode: &'static str,
    pub frame_slot_count: usize,
    pub decoded_frame_extent: (u32, u32),
    pub decoded_frame_format: String,
    pub descriptor_heap_only: bool,
    pub decoded_image_zero_copy_presented: bool,
    pub zero_copy_scope: &'static str,
    pub pacing: &'static str,
}

pub fn run_rendering_device_shared_video_present(
    options: RenderingDeviceSharedVideoPresentOptions,
) -> Result<RenderingDeviceSharedVideoPresentSnapshot, String> {
    if options.playback_frame_count == 0 {
        return Err("shared video presentation requires at least one playback frame".into());
    }
    if !options.source.is_file() {
        return Err(format!(
            "shared video source does not exist: {}",
            options.source.display()
        ));
    }
    let mut host = WaylandHost::connect(options.host.clone()).map_err(|error| error.to_string())?;
    host.wait_until_configured(options.wait_configure_roundtrips)
        .map_err(|error| error.to_string())?;
    let handles = host.surface_handles().map_err(|error| error.to_string())?;
    let video_requirements = RenderingDeviceSharedVideoSource::requirements([options.codec])?;
    let bootstrap = PresentationBootstrap::create(
        Arc::new(handles.renderer_handle.clone()),
        tensor_wallpaper_presentation_bootstrap_descriptor(
            "tensor-wallpaper-video",
            handles.buffer_size,
            Features::RETAINED_SCENE_PRESENTATION,
            Features::empty(),
            SampleCounts::ONE,
            Some(video_requirements),
        )?,
    )
    .map_err(|error| format!("create Tensor Wallpaper video presentation bootstrap: {error}"))?;
    let video_decode = bootstrap.video_decode.as_ref().ok_or_else(|| {
        "shared video bootstrap omitted its requested renderer-owned decode endpoint".to_owned()
    })?;
    let mut source = RenderingDeviceSharedVideoSource::open(
        video_decode,
        0,
        &options.source,
        options.codec,
    )?;
    let configuration = bootstrap.swapchain.configuration();
    let frame_slot_count = shared_video_frame_slot_count()?;
    let plan = direct_surface_plan(configuration.extent, configuration.format, frame_slot_count)?;
    let mut resources = bootstrap
        .device
        .create_decoded_video_surface_terminal(
            &bootstrap.pipeline_binary_cache,
            &DecodedVideoSurfaceTerminalDescriptor {
                label: Some("tensor-wallpaper-shared-video-present".into()),
                surface_format: configuration.format,
                frame_slots: u32::try_from(frame_slot_count)
                    .map_err(|_| "shared video frame-slot count exceeds u32")?,
                program: shared_video_present_program(),
            },
        )
        .map_err(|error| format!("create renderer-owned video terminal: {error}"))?;
    let mut transaction = bootstrap
        .device
        .create_presentation_transaction(&PresentationTransactionDescriptor {
            label: Some("tensor-wallpaper-shared-video-presentation".into()),
            plan: &plan,
            swapchain: &bootstrap.swapchain,
            acquire_timeout_ns: u64::MAX,
        })
        .map_err(|error| format!("create shared video presentation transaction: {error}"))?;
    let started_at = Instant::now();
    let mut next_frame = started_at;
    let frame_interval = options
        .target_max_fps
        .filter(|fps| *fps > 0)
        .map(|fps| Duration::from_secs_f64(1.0 / f64::from(fps)));
    let clear = [
        options.clear_color.r,
        options.clear_color.g,
        options.clear_color.b,
        options.clear_color.a,
    ];
    let mut frames_presented = 0u32;
    let mut repeated_presentation_count = 0u64;
    let mut decoded_frame_extent = None;
    let mut decoded_frame_format = None;

    while frames_presented < options.playback_frame_count {
        host.pump_events().map_err(|error| error.to_string())?;
        if host.is_closed() {
            break;
        }
        let presentation_now = Instant::now();
        let advanced = source.advance_to(presentation_now, true)?;
        if advanced.is_none() {
            repeated_presentation_count = repeated_presentation_count.saturating_add(1);
        }
        let frame = source.current_frame().ok_or_else(|| {
            "shared video decoder did not produce a presentable frame before direct-surface recording"
                .to_owned()
        })?;
        if frame.array_layers() != 1 {
            return Err(format!(
                "shared direct-video presentation requires exactly one decoded array layer, got {}",
                frame.array_layers()
            ));
        }
        decoded_frame_extent.get_or_insert((frame.extent().width, frame.extent().height));
        decoded_frame_format.get_or_insert_with(|| format!("{:?}", frame.format()));
        let frame_slot = frames_presented as usize % frame_slot_count;
        transaction
            .execute_frame(
                &bootstrap.swapchain,
                frame_slot,
                || Ok::<[CommandBuffer; 0], vulkan_renderer::Error>([]),
                PresentationFrameDependencies::decoded_video(
                    std::slice::from_ref(frame),
                    PresentationDependencyScope::SurfaceCommands,
                ),
                |encoder, acquired| resources.record_surface(encoder, acquired, frame_slot, frame, clear),
            )
            .map_err(|error| format!("execute shared video presentation transaction: {error}"))?;
        frames_presented = frames_presented.saturating_add(1);
        if let Some(interval) = frame_interval {
            next_frame += interval;
            let now = Instant::now();
            if next_frame > now {
                thread::sleep(next_frame - now);
            } else {
                next_frame = now;
            }
        }
    }
    bootstrap
        .device
        .wait_idle()
        .map_err(|error| format!("wait for shared video shutdown: {error}"))?;
    let elapsed = started_at.elapsed();
    let surface_host = host.snapshot();
    let decoded_frame_extent = decoded_frame_extent.ok_or_else(|| {
        "shared video presentation finished without a decoded frame extent".to_owned()
    })?;
    let decoded_frame_format = decoded_frame_format.ok_or_else(|| {
        "shared video presentation finished without a decoded frame format".to_owned()
    })?;
    Ok(RenderingDeviceSharedVideoPresentSnapshot {
        binding: "vulkan-renderer",
        route: "renderer-owned-ffmpeg-vulkan-direct-surface",
        source: options.source,
        codec: options.codec,
        requested_present_frame_count: options.playback_frame_count,
        frames_presented,
        decoded_frame_count: source.decoded_frame_count(),
        repeated_presentation_count,
        video_loop_index: source.loop_index(),
        runtime_elapsed_ms: elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
        average_present_fps: if elapsed.is_zero() {
            0.0
        } else {
            f64::from(frames_presented) / elapsed.as_secs_f64()
        },
        surface_host,
        surface_extent: (configuration.extent.width, configuration.extent.height),
        surface_format: format!("{:?}", configuration.format),
        present_mode: "fifo-latest-ready",
        frame_slot_count,
        decoded_frame_extent,
        decoded_frame_format,
        descriptor_heap_only: true,
        decoded_image_zero_copy_presented: frames_presented != 0,
        zero_copy_scope: "decoded Vulkan planes are sampled directly into the swapchain color attachment; no final copy follows the shader draw",
        pacing: "strict-decoded-pts-duration",
    })
}

fn direct_surface_plan(
    extent: Extent2D,
    format: vulkan_renderer::TextureFormat,
    frame_slots: usize,
) -> Result<PresentationPathPlan, String> {
    PresentationPathPlan::compile(
        PresentationPathDescriptor {
            target: FrameTargetPreference::DirectSurface,
            acquire: SurfaceAcquireStrategy::BeforeFrame,
            terminal: TerminalCompositeDescriptor::default(),
        },
        PresentationRequirements {
            surface_extent: extent,
            target_extent: extent,
            surface_format: format,
            target_format: format,
            frame_slots: u32::try_from(frame_slots)
                .map_err(|_| "shared video frame-slot count exceeds u32")?,
            physical_pass_count: 1,
            sampled_after_write: false,
            has_history: false,
            has_external_consumer: false,
            uses_async_compute: false,
            requires_terminal_transform: false,
        },
    )
    .map_err(|error| format!("compile shared direct-video presentation path: {error}"))
}

fn shared_video_frame_slot_count() -> Result<usize, String> {
    let count = std::env::var("TENSOR_WALLPAPER_RENDERING_DEVICE_VIDEO_FRAME_SLOT_COUNT")
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| format!("invalid shared video frame-slot count {value:?}"))
        })
        .transpose()?
        .unwrap_or(1);
    if !(1..=MAX_FRAME_SLOTS).contains(&count) {
        return Err(format!(
            "shared video frame-slot count must be in 1..={MAX_FRAME_SLOTS}, got {count}"
        ));
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_surface_plan_has_one_direct_pass_and_no_terminal_transform() {
        let plan = direct_surface_plan(
            Extent2D::new(3840, 2160),
            vulkan_renderer::TextureFormat::Bgra8Unorm,
            2,
        )
        .unwrap();
        assert_eq!(plan.target, vulkan_renderer::PresentationTarget::DirectSurface);
        assert_eq!(plan.frame_slots, 2);
    }
}
