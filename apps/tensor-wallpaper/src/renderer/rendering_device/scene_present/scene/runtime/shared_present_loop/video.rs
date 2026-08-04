//! Typed scene-media frontend over renderer-owned decode and submission.

use std::time::Instant;

use vulkan_renderer::{DecodedVideoFrame, VideoDecodeDevice, VideoDecodeRequirements};

use crate::engine::scene::SceneRenderBindingKind;
use crate::renderer::rendering_device::SceneExecutionPlan;
use crate::renderer::rendering_device::scene::RenderingDeviceSceneVideoSource;
use crate::renderer::rendering_device::video::shared_decoder::RenderingDeviceSharedVideoSource;

use super::super::frame_events::SceneRuntimeEventSources;

pub(super) struct SharedSceneVideoRuntime {
    sources: Vec<RenderingDeviceSharedVideoSource>,
    media_instances: Vec<u32>,
    loop_playback: Vec<bool>,
    decoded_frames: Vec<DecodedVideoFrame>,
}

impl SharedSceneVideoRuntime {
    pub(super) fn requirements(
        plan: &SceneExecutionPlan,
        sources: &[RenderingDeviceSceneVideoSource],
    ) -> Result<Option<VideoDecodeRequirements>, String> {
        let required = required_media_instances(plan);
        validate_sources(&required, sources)?;
        if sources.is_empty() {
            return Ok(None);
        }
        RenderingDeviceSharedVideoSource::requirements(sources.iter().map(|source| source.codec))
            .map(Some)
    }

    pub(super) fn open(
        device: Option<&VideoDecodeDevice>,
        plan: &SceneExecutionPlan,
        source_options: Vec<RenderingDeviceSceneVideoSource>,
    ) -> Result<Self, String> {
        let required = required_media_instances(plan);
        validate_sources(&required, &source_options)?;
        let device = match (source_options.is_empty(), device) {
            (true, _) => None,
            (false, Some(device)) => Some(device),
            (false, None) => {
                return Err(
                    "scene video sources require the requested renderer-owned decode endpoint"
                        .into(),
                );
            }
        };
        let mut sources = Vec::with_capacity(source_options.len());
        let mut media_instances = Vec::with_capacity(source_options.len());
        let mut loop_playback = Vec::with_capacity(source_options.len());
        for source in source_options {
            media_instances.push(source.media_instance);
            loop_playback.push(source.loop_playback);
            sources.push(RenderingDeviceSharedVideoSource::open(
                device.expect("non-empty source list has a decode endpoint"),
                source.media_instance,
                &source.source,
                source.codec,
            )?);
        }
        Ok(Self {
            decoded_frames: Vec::with_capacity(sources.len()),
            sources,
            media_instances,
            loop_playback,
        })
    }

    pub(super) fn advance_to(
        &mut self,
        presentation_now: Instant,
        event_sources: &mut SceneRuntimeEventSources,
    ) -> Result<(), String> {
        self.decoded_frames.clear();
        let mut primary_sample = None;
        for (source, loop_playback) in self.sources.iter_mut().zip(&self.loop_playback) {
            if let Some(sample) = source.advance_to(presentation_now, *loop_playback)?
                && primary_sample.is_none()
            {
                primary_sample = Some(sample);
            }
            self.decoded_frames.push(
                source
                    .current_frame()
                    .ok_or_else(|| {
                        format!(
                            "scene video media instance {} produced no presentable frame",
                            source.media_instance()
                        )
                    })?
                    .clone(),
            );
        }
        if let Some(sample) = primary_sample {
            event_sources.publish_video(sample);
        }
        Ok(())
    }

    pub(super) fn media_instances(&self) -> &[u32] {
        &self.media_instances
    }

    pub(super) fn decoded_frames(&self) -> &[DecodedVideoFrame] {
        &self.decoded_frames
    }
}

fn required_media_instances(plan: &SceneExecutionPlan) -> Vec<u32> {
    let mut required = plan
        .rendering_device_graph
        .sampled_bindings
        .iter()
        .filter(|binding| binding.kind == SceneRenderBindingKind::VideoFrame)
        .map(|binding| binding.slot)
        .collect::<Vec<_>>();
    required.sort_unstable();
    required.dedup();
    required
}

fn validate_sources(
    required: &[u32],
    sources: &[RenderingDeviceSceneVideoSource],
) -> Result<(), String> {
    let mut supplied = sources
        .iter()
        .map(|source| source.media_instance)
        .collect::<Vec<_>>();
    supplied.sort_unstable();
    if supplied.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(format!(
            "scene video media instances must be unique, got {supplied:?}"
        ));
    }
    if supplied != required {
        return Err(format!(
            "scene typed VideoFrame media instances {required:?} require exactly matching --scene-video inputs, got {supplied:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::renderer::rendering_device::RenderingDeviceVideoSessionCodec;

    fn source(media_instance: u32) -> RenderingDeviceSceneVideoSource {
        RenderingDeviceSceneVideoSource {
            media_instance,
            source: PathBuf::from(format!("/media/{media_instance}.mkv")),
            codec: RenderingDeviceVideoSessionCodec::H265Main10,
            loop_playback: true,
        }
    }

    #[test]
    fn typed_video_inputs_must_exactly_cover_graph_media_instances() {
        assert!(validate_sources(&[3, 7], &[source(7), source(3)]).is_ok());
        assert!(validate_sources(&[3, 7], &[source(3)]).is_err());
        assert!(validate_sources(&[3, 7], &[source(3), source(7), source(9)]).is_err());
        assert!(validate_sources(&[3, 7], &[source(3), source(3)]).is_err());
    }
}
