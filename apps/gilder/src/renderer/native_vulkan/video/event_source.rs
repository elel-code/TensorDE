//! Typed media-session adapter shared by video frames and decoded audio analysis.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

use crate::engine::scene::{
    SceneAudioSource, SceneAudioState, SceneEventQueue, SceneFrameEvents,
    SceneMediaClockState, SceneMediaGeneration, SceneMediaPlaybackState, SceneMediaSessionId,
    SceneVideoState, StereoSpectrum64,
};
use crate::renderer::native_vulkan::audio::event_source::{
    NativeVulkanAudioEventChannel, audio_state_summary,
};

static NEXT_MEDIA_SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanVideoEventSample {
    pub generation: u64,
    pub frame_serial: u64,
    pub frame_identity: u64,
    pub presentation_time_ns: u64,
    pub frame_duration_ns: Option<u64>,
    pub media_duration_ns: Option<u64>,
    pub playback: SceneMediaPlaybackState,
    pub rate_milli: i32,
    pub loop_index: u64,
    pub ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanMediaEventSource {
    session: SceneMediaSessionId,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanMediaEventRuntimeSnapshot {
    pub session_identity: u64,
    pub published_frame_count: u64,
    pub first_sequence: u64,
    pub last_sequence: u64,
    pub generation: u64,
    pub frame_serial: u64,
    pub frame_identity: u64,
    pub presentation_time_ns: u64,
    pub frame_duration_ns: Option<u64>,
    pub playback: &'static str,
    pub loop_index: u64,
    pub video_ready: bool,
    pub audio_ready: bool,
    pub audio_peak: f32,
    pub audio_active_band_count: u32,
    pub audio_ready_frame_count: u64,
    pub audio_peak_max: f32,
    pub audio_active_band_count_max: u32,
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanMediaEventRuntime {
    source: NativeVulkanMediaEventSource,
    audio: NativeVulkanAudioEventChannel,
    queue: SceneEventQueue,
    frame: SceneFrameEvents,
    published_frame_count: u64,
    audio_ready_frame_count: u64,
    audio_peak_max: f32,
    audio_active_band_count_max: u32,
}

impl NativeVulkanMediaEventRuntime {
    pub(in crate::renderer::native_vulkan) fn new(audio: NativeVulkanAudioEventChannel) -> Self {
        let session_identity = NEXT_MEDIA_SESSION_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            source: NativeVulkanMediaEventSource::new(session_identity),
            audio,
            queue: SceneEventQueue::default(),
            frame: SceneFrameEvents::default(),
            published_frame_count: 0,
            audio_ready_frame_count: 0,
            audio_peak_max: 0.0,
            audio_active_band_count_max: 0,
        }
    }

    pub(in crate::renderer::native_vulkan) fn publish_presented_frame(
        &mut self,
        sample: NativeVulkanVideoEventSample,
    ) -> &SceneFrameEvents {
        self.source.publish_video(&mut self.queue, sample);
        let audio = self
            .audio
            .capture(sample.generation, sample.presentation_time_ns);
        self.source.publish_audio(
            &mut self.queue,
            sample.generation,
            audio.sample_time_ns,
            audio.spectrum,
            audio.ready,
        );
        self.frame = self.queue.finish_frame();
        self.published_frame_count = self.published_frame_count.saturating_add(1);
        let (audio_peak, audio_active_band_count) = audio_state_summary(&self.frame.audio);
        if self.frame.audio.ready {
            self.audio_ready_frame_count = self.audio_ready_frame_count.saturating_add(1);
        }
        self.audio_peak_max = self.audio_peak_max.max(audio_peak);
        self.audio_active_band_count_max = self
            .audio_active_band_count_max
            .max(audio_active_band_count);
        &self.frame
    }

    pub(in crate::renderer::native_vulkan) fn snapshot(
        &self,
    ) -> NativeVulkanMediaEventRuntimeSnapshot {
        let media = self.frame.media.unwrap_or_default();
        let video = self.frame.video.unwrap_or_default();
        let (audio_peak, audio_active_band_count) = audio_state_summary(&self.frame.audio);
        NativeVulkanMediaEventRuntimeSnapshot {
            session_identity: self.source.session.0,
            published_frame_count: self.published_frame_count,
            first_sequence: self.frame.first_sequence.0,
            last_sequence: self.frame.last_sequence.0,
            generation: media.generation.0,
            frame_serial: video.frame_serial,
            frame_identity: video.frame_identity,
            presentation_time_ns: video.presentation_time_ns,
            frame_duration_ns: video.duration_ns,
            playback: media_playback_label(media.playback),
            loop_index: media.loop_index,
            video_ready: video.ready,
            audio_ready: self.frame.audio.ready,
            audio_peak,
            audio_active_band_count,
            audio_ready_frame_count: self.audio_ready_frame_count,
            audio_peak_max: self.audio_peak_max,
            audio_active_band_count_max: self.audio_active_band_count_max,
        }
    }
}

impl NativeVulkanMediaEventSource {
    pub(in crate::renderer::native_vulkan) fn new(session_identity: u64) -> Self {
        Self {
            session: SceneMediaSessionId(session_identity),
        }
    }

    pub(in crate::renderer::native_vulkan) fn publish_video(
        self,
        queue: &mut SceneEventQueue,
        sample: NativeVulkanVideoEventSample,
    ) {
        let generation = SceneMediaGeneration(sample.generation);
        queue.publish_media(SceneMediaClockState {
            session: self.session,
            generation,
            playback: sample.playback,
            clock_ns: sample.presentation_time_ns,
            duration_ns: sample.media_duration_ns,
            rate_milli: sample.rate_milli,
            loop_index: sample.loop_index,
            ..SceneMediaClockState::default()
        });
        queue.publish_video(SceneVideoState {
            session: self.session,
            generation,
            frame_serial: sample.frame_serial,
            frame_identity: sample.frame_identity,
            presentation_time_ns: sample.presentation_time_ns,
            duration_ns: sample.frame_duration_ns,
            ready: sample.ready,
            ..SceneVideoState::default()
        });
    }

    pub(in crate::renderer::native_vulkan) fn publish_audio(
        self,
        queue: &mut SceneEventQueue,
        generation: u64,
        sample_time_ns: u64,
        spectrum: StereoSpectrum64,
        ready: bool,
    ) {
        queue.publish_audio(SceneAudioState {
            source: SceneAudioSource::MediaSession,
            media_session: Some(self.session),
            media_generation: SceneMediaGeneration(generation),
            sample_time_ns,
            spectrum,
            ready,
            ..SceneAudioState::default()
        });
    }
}

fn media_playback_label(playback: SceneMediaPlaybackState) -> &'static str {
    match playback {
        SceneMediaPlaybackState::Idle => "idle",
        SceneMediaPlaybackState::Buffering => "buffering",
        SceneMediaPlaybackState::Playing => "playing",
        SceneMediaPlaybackState::Paused => "paused",
        SceneMediaPlaybackState::Ended => "ended",
        SceneMediaPlaybackState::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_and_video_share_media_session_and_generation() {
        let source = NativeVulkanMediaEventSource::new(42);
        let mut queue = SceneEventQueue::default();
        source.publish_video(
            &mut queue,
            NativeVulkanVideoEventSample {
                generation: 3,
                frame_serial: 8,
                frame_identity: 88,
                presentation_time_ns: 1_000_000,
                frame_duration_ns: Some(16_666_667),
                media_duration_ns: Some(60_000_000_000),
                playback: SceneMediaPlaybackState::Playing,
                rate_milli: 1_000,
                loop_index: 2,
                ready: true,
            },
        );
        source.publish_audio(
            &mut queue,
            3,
            1_000_000,
            StereoSpectrum64 {
                left: [0.25; 64],
                right: [0.75; 64],
            },
            true,
        );
        let frame = queue.finish_frame();
        assert_eq!(frame.media.unwrap().session, SceneMediaSessionId(42));
        assert_eq!(frame.video.unwrap().generation, SceneMediaGeneration(3));
        assert_eq!(frame.audio.media_session, Some(SceneMediaSessionId(42)));
        assert_eq!(frame.audio.media_generation, SceneMediaGeneration(3));
    }

    #[test]
    fn retained_runtime_publishes_one_coherent_frame_snapshot() {
        let audio = NativeVulkanAudioEventChannel::default();
        audio.publish(
            4,
            1_500_000,
            StereoSpectrum64 {
                left: [0.25; 64],
                right: [0.75; 64],
            },
        );
        let mut runtime = NativeVulkanMediaEventRuntime::new(audio);
        runtime.publish_presented_frame(NativeVulkanVideoEventSample {
            generation: 4,
            frame_serial: 9,
            frame_identity: 99,
            presentation_time_ns: 2_000_000,
            frame_duration_ns: Some(16_666_667),
            media_duration_ns: None,
            playback: SceneMediaPlaybackState::Playing,
            rate_milli: 1_000,
            loop_index: 4,
            ready: true,
        });

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.published_frame_count, 1);
        assert_eq!(snapshot.generation, 4);
        assert_eq!(snapshot.frame_serial, 9);
        assert_eq!(snapshot.frame_identity, 99);
        assert_eq!(snapshot.playback, "playing");
        assert!(snapshot.video_ready);
        assert_eq!(snapshot.audio_ready_frame_count, 1);
        assert!(snapshot.audio_peak_max > 0.0);
        assert_eq!(snapshot.audio_active_band_count_max, 64);
        assert_eq!(snapshot.last_sequence - snapshot.first_sequence, 2);
    }
}
