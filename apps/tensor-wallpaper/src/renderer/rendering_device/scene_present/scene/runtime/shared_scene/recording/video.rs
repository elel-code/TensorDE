//! Typed decoded-video lifecycle for retained scene descriptors and commands.

use vulkan_renderer::{CommandEncoder, Extent2D, Image, ImageView};

use crate::renderer::rendering_device::RenderingDeviceClearColor;

use super::super::SharedSceneGpuResources;

impl SharedSceneGpuResources {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn record_graphs_to_scene_color_with_video(
        &mut self,
        encoder: &mut CommandEncoder,
        frame_slot: usize,
        scene_color: &Image,
        scene_color_view: &ImageView,
        extent: Extent2D,
        reference_phase: usize,
        scene_color_initialized: bool,
        clear_color: RenderingDeviceClearColor,
        video_media_instances: &[u32],
        video_frames: &[vulkan_renderer::DecodedVideoFrame],
        gpu_timing: Option<super::super::super::gpu_timing::SceneGpuTimingFrame<'_>>,
    ) -> Result<(), String> {
        self.validate_video_frames(video_media_instances, video_frames)?;
        for (&media_instance, frame) in video_media_instances.iter().zip(video_frames) {
            self.frames
                .get_mut(frame_slot)
                .ok_or_else(|| format!("shared scene frame slot {frame_slot} is missing"))?
                .bind_decoded_video_frame(reference_phase, media_instance, frame)?;
            encoder
                .begin_decoded_video_sampling(frame)
                .map_err(|error| {
                    format!(
                        "begin shared scene video media instance {} sampling: {error}",
                        media_instance
                    )
                })?;
        }
        self.record_graphs_to_scene_color(
            encoder,
            frame_slot,
            scene_color,
            scene_color_view,
            extent,
            reference_phase,
            scene_color_initialized,
            clear_color,
            gpu_timing,
        )?;
        for (&media_instance, frame) in video_media_instances.iter().zip(video_frames) {
            encoder.end_decoded_video_sampling(frame).map_err(|error| {
                format!(
                    "end shared scene video media instance {} sampling: {error}",
                    media_instance
                )
            })?;
        }
        Ok(())
    }

    fn validate_video_frames(
        &self,
        video_media_instances: &[u32],
        video_frames: &[vulkan_renderer::DecodedVideoFrame],
    ) -> Result<(), String> {
        if video_media_instances.len() != video_frames.len() {
            return Err(format!(
                "shared scene video source/frame count differs: {} sources, {} frames",
                video_media_instances.len(),
                video_frames.len()
            ));
        }
        for (source_index, (&media_instance, frame)) in
            video_media_instances.iter().zip(video_frames).enumerate()
        {
            if frame.array_layers() != 1 {
                return Err(format!(
                    "shared scene video media instance {} exposes {} array layers without an explicit sampled layer",
                    media_instance,
                    frame.array_layers()
                ));
            }
            if video_media_instances[..source_index].contains(&media_instance) {
                return Err(format!(
                    "shared scene video media instance {} is supplied more than once",
                    media_instance
                ));
            }
            if !self
                .draw_commands
                .iter()
                .any(|draw| draw.video_media_instance == Some(media_instance))
            {
                return Err(format!(
                    "shared scene video media instance {} has no typed graph consumer",
                    media_instance
                ));
            }
        }
        for media_instance in self
            .draw_commands
            .iter()
            .filter_map(|draw| draw.video_media_instance)
        {
            if !video_media_instances.contains(&media_instance) {
                return Err(format!(
                    "shared scene video media instance {media_instance} has no decoded frame"
                ));
            }
        }
        Ok(())
    }
}
