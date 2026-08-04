//! Cold Tensor Wallpaper media frontend over renderer-owned FFmpeg Vulkan decode.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vulkan_renderer::{
    DecodedVideoFrame, FfmpegVulkanDecoder, VideoDecodeCodecs, VideoDecodeDevice,
    VideoDecodeRequirements,
};

use crate::engine::scene::SceneMediaPlaybackState;

use super::codec::RenderingDeviceVideoSessionCodec;
use super::event_source::RenderingDeviceVideoEventSample;

pub(in crate::renderer::rendering_device) struct RenderingDeviceSharedVideoSource {
    media_instance: u32,
    source: PathBuf,
    codec: RenderingDeviceVideoSessionCodec,
    decoder: FfmpegVulkanDecoder,
    current: Option<DecodedVideoFrame>,
    generation: u64,
    frame_serial: u64,
    ended: bool,
    pacing: RenderingDeviceVideoPacing,
}

impl std::fmt::Debug for RenderingDeviceSharedVideoSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RenderingDeviceSharedVideoSource")
            .field("media_instance", &self.media_instance)
            .field("source", &self.source)
            .field("codec", &self.codec)
            .field("decoder", &self.decoder)
            .field("generation", &self.generation)
            .field("frame_serial", &self.frame_serial)
            .field("ended", &self.ended)
            .field("pacing", &self.pacing)
            .finish_non_exhaustive()
    }
}

impl RenderingDeviceSharedVideoSource {
    pub(in crate::renderer::rendering_device) fn requirements(
        codecs: impl IntoIterator<Item = RenderingDeviceVideoSessionCodec>,
    ) -> Result<VideoDecodeRequirements, String> {
        let profiles = codecs
            .into_iter()
            .fold(VideoDecodeCodecs::empty(), |profiles, codec| {
                profiles | codec.renderer_requirement()
            });
        VideoDecodeRequirements::new(profiles)
            .map_err(|error| format!("build shared video decode requirements: {error}"))
    }

    pub(in crate::renderer::rendering_device) fn open(
        device: &VideoDecodeDevice,
        media_instance: u32,
        source: impl AsRef<Path>,
        codec: RenderingDeviceVideoSessionCodec,
    ) -> Result<Self, String> {
        let source = source.as_ref().to_owned();
        if !source.is_file() {
            return Err(format!(
                "shared video media instance {media_instance} source does not exist: {}",
                source.display()
            ));
        }
        let decoder = device
            .open_ffmpeg_decoder(&source, codec.renderer_codec())
            .map_err(|error| {
                format!(
                    "open shared video media instance {media_instance} decoder: {error}"
                )
            })?;
        Ok(Self {
            media_instance,
            source,
            codec,
            decoder,
            current: None,
            generation: 0,
            frame_serial: 0,
            ended: false,
            pacing: RenderingDeviceVideoPacing::default(),
        })
    }

    pub(in crate::renderer::rendering_device) const fn media_instance(&self) -> u32 {
        self.media_instance
    }

    pub(in crate::renderer::rendering_device) fn current_frame(
        &self,
    ) -> Option<&DecodedVideoFrame> {
        self.current.as_ref()
    }

    /// Advances only when the current decoded frame's exact PTS/duration
    /// interval has elapsed. High refresh-rate presentation therefore repeats
    /// a retained frame instead of decoding a new one on every vblank.
    pub(in crate::renderer::rendering_device) fn advance_to(
        &mut self,
        presentation_now: Instant,
        loop_on_eos: bool,
    ) -> Result<Option<RenderingDeviceVideoEventSample>, String> {
        if self.current.is_some() && (!self.pacing.due(presentation_now) || self.ended) {
            return Ok(None);
        }
        let mut latest_sample = None;
        for _ in 0..MAX_CATCH_UP_DECODE_FRAMES {
            let previous_loop = self.decoder.loop_count();
            let Some(frame) = self
                .decoder
                .decode_next_frame(loop_on_eos)
                .map_err(|error| {
                    format!(
                        "decode shared video media instance {} frame: {error}",
                        self.media_instance
                    )
                })?
            else {
                if self.ended || self.current.is_none() {
                    return Ok(latest_sample);
                }
                self.ended = true;
                let frame = self.current.as_ref().expect("current frame was checked");
                return Ok(Some(RenderingDeviceVideoEventSample {
                    generation: self.generation,
                    frame_serial: self.frame_serial,
                    frame_identity: self.frame_serial,
                    presentation_time_ns: frame.pts_ns().ok_or_else(|| {
                        "shared video final frame is missing a non-negative PTS".to_owned()
                    })?,
                    frame_duration_ns: Some(self.pacing.current_duration_ns()),
                    media_duration_ns: None,
                    playback: SceneMediaPlaybackState::Ended,
                    rate_milli: 1_000,
                    loop_index: u64::from(self.decoder.loop_count()),
                    ready: true,
                }));
            };
            let loop_count = self.decoder.loop_count();
            if loop_count < previous_loop {
                return Err("shared video decoder loop counter regressed".into());
            }
            self.generation = self
                .generation
                .saturating_add(u64::from(loop_count - previous_loop));
            let presentation_time_ns = frame.pts_ns().ok_or_else(|| {
                format!(
                    "shared video media instance {} frame {} is missing a non-negative PTS",
                    self.media_instance,
                    self.frame_serial.saturating_add(1),
                )
            })?;
            let duration_ns = frame.duration_ns().filter(|duration| *duration > 0).ok_or_else(|| {
                format!(
                    "shared video media instance {} frame {} is missing a positive PTS duration",
                    self.media_instance,
                    self.frame_serial.saturating_add(1),
                )
            })?;
            self.pacing.schedule(
                presentation_now,
                loop_count,
                presentation_time_ns,
                duration_ns,
            )?;
            self.ended = false;
            self.frame_serial = self.frame_serial.saturating_add(1);
            self.current = Some(frame);
            latest_sample = Some(RenderingDeviceVideoEventSample {
                generation: self.generation,
                frame_serial: self.frame_serial,
                frame_identity: self.frame_serial,
                presentation_time_ns,
                frame_duration_ns: Some(duration_ns),
                media_duration_ns: None,
                playback: SceneMediaPlaybackState::Playing,
                rate_milli: 1_000,
                loop_index: u64::from(loop_count),
                ready: true,
            });
            if !self.pacing.due(presentation_now) {
                return Ok(latest_sample);
            }
        }
        Err(format!(
            "shared video media instance {} required more than {MAX_CATCH_UP_DECODE_FRAMES} PTS catch-up decodes in one presentation interval",
            self.media_instance
        ))
    }

    pub(in crate::renderer::rendering_device) const fn decoded_frame_count(&self) -> u64 {
        self.frame_serial
    }

    pub(in crate::renderer::rendering_device) fn loop_index(&self) -> u64 {
        self.decoder.loop_count() as u64
    }
}

const MAX_CATCH_UP_DECODE_FRAMES: usize = 4_096;

#[derive(Debug, Default)]
struct RenderingDeviceVideoPacing {
    cycle_started_at: Option<Instant>,
    cycle_first_pts_ns: Option<u64>,
    loop_count: Option<u32>,
    current_pts_ns: Option<u64>,
    current_duration_ns: Option<u64>,
    next_frame_due_at: Option<Instant>,
}

impl RenderingDeviceVideoPacing {
    fn due(&self, now: Instant) -> bool {
        self.next_frame_due_at.is_none_or(|due_at| now >= due_at)
    }

    fn current_duration_ns(&self) -> u64 {
        self.current_duration_ns
            .expect("a current decoded frame always has a PTS duration")
    }

    fn schedule(
        &mut self,
        now: Instant,
        loop_count: u32,
        pts_ns: u64,
        duration_ns: u64,
    ) -> Result<(), String> {
        let cycle_changed = match self.loop_count {
            Some(previous) if loop_count < previous => {
                return Err("shared video decoder loop counter regressed".into());
            }
            Some(previous) => loop_count > previous,
            None => true,
        };
        if cycle_changed {
            self.cycle_started_at = Some(self.next_frame_due_at.unwrap_or(now));
            self.cycle_first_pts_ns = Some(pts_ns);
        } else if let Some(previous_pts_ns) = self.current_pts_ns
            && pts_ns < previous_pts_ns
        {
            return Err(format!(
                "shared video PTS regressed within loop {loop_count}: {pts_ns} < {previous_pts_ns}"
            ));
        }
        let cycle_started_at = self
            .cycle_started_at
            .ok_or_else(|| "shared video pacing lost its cycle start".to_owned())?;
        let cycle_first_pts_ns = self
            .cycle_first_pts_ns
            .ok_or_else(|| "shared video pacing lost its cycle PTS origin".to_owned())?;
        let elapsed_ns = pts_ns.checked_sub(cycle_first_pts_ns).ok_or_else(|| {
            format!(
                "shared video PTS {pts_ns} precedes cycle {loop_count} origin {cycle_first_pts_ns}"
            )
        })?;
        let presentation_at = cycle_started_at
            .checked_add(Duration::from_nanos(elapsed_ns))
            .ok_or_else(|| "shared video PTS presentation instant overflowed".to_owned())?;
        let next_frame_due_at = presentation_at
            .checked_add(Duration::from_nanos(duration_ns))
            .ok_or_else(|| "shared video PTS duration instant overflowed".to_owned())?;
        self.loop_count = Some(loop_count);
        self.current_pts_ns = Some(pts_ns);
        self.current_duration_ns = Some(duration_ns);
        self.next_frame_due_at = Some(next_frame_due_at);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_union_keeps_every_exact_profile() {
        let requirements = RenderingDeviceSharedVideoSource::requirements([
            RenderingDeviceVideoSessionCodec::H264High8,
            RenderingDeviceVideoSessionCodec::H265Main10,
        ])
        .unwrap();
        assert_eq!(
            requirements.codecs(),
            VideoDecodeCodecs::H264_HIGH_8 | VideoDecodeCodecs::H265_MAIN_10
        );
    }

    #[test]
    fn pacing_holds_a_decoded_frame_until_its_pts_duration_expires() {
        let started = Instant::now();
        let mut pacing = RenderingDeviceVideoPacing::default();
        pacing.schedule(started, 0, 2_000_000_000, 20_000_000).unwrap();
        assert!(!pacing.due(started + Duration::from_millis(19)));
        assert!(pacing.due(started + Duration::from_millis(20)));
    }

    #[test]
    fn pacing_uses_pts_to_preserve_a_gap_between_decoded_frames() {
        let started = Instant::now();
        let mut pacing = RenderingDeviceVideoPacing::default();
        pacing.schedule(started, 0, 5, 10).unwrap();
        pacing
            .schedule(started + Duration::from_nanos(10), 0, 35, 10)
            .unwrap();
        assert!(!pacing.due(started + Duration::from_nanos(39)));
        assert!(pacing.due(started + Duration::from_nanos(40)));
    }

    #[test]
    fn pacing_restarts_the_pts_origin_at_a_decoder_loop_boundary() {
        let started = Instant::now();
        let mut pacing = RenderingDeviceVideoPacing::default();
        pacing.schedule(started, 0, 100, 20).unwrap();
        pacing
            .schedule(started + Duration::from_nanos(20), 1, 0, 20)
            .unwrap();
        assert!(!pacing.due(started + Duration::from_nanos(39)));
        assert!(pacing.due(started + Duration::from_nanos(40)));
    }
}
