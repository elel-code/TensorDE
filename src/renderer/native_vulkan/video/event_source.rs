//! Typed media-session adapter shared by video frames and decoded audio analysis.

use crate::engine::scene::{
    SceneAudioSource, SceneAudioState, SceneEvent, SceneEventQueue, SceneMediaClockState,
    SceneMediaGeneration, SceneMediaPlaybackState, SceneMediaSessionId, SceneVideoState,
};

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
        queue.publish(SceneEvent::Media(SceneMediaClockState {
            session: self.session,
            generation,
            playback: sample.playback,
            clock_ns: sample.presentation_time_ns,
            duration_ns: sample.media_duration_ns,
            rate_milli: sample.rate_milli,
            loop_index: sample.loop_index,
            ..SceneMediaClockState::default()
        }));
        queue.publish(SceneEvent::Video(SceneVideoState {
            session: self.session,
            generation,
            frame_serial: sample.frame_serial,
            frame_identity: sample.frame_identity,
            presentation_time_ns: sample.presentation_time_ns,
            duration_ns: sample.frame_duration_ns,
            ready: sample.ready,
            ..SceneVideoState::default()
        }));
    }

    pub(in crate::renderer::native_vulkan) fn publish_audio(
        self,
        queue: &mut SceneEventQueue,
        generation: u64,
        sample_time_ns: u64,
        spectrum32: [f32; 32],
        ready: bool,
    ) {
        queue.publish(SceneEvent::Audio(SceneAudioState {
            source: SceneAudioSource::MediaSession,
            media_session: Some(self.session),
            media_generation: SceneMediaGeneration(generation),
            sample_time_ns,
            spectrum32,
            ready,
            ..SceneAudioState::default()
        }));
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
        source.publish_audio(&mut queue, 3, 1_000_000, [0.5; 32], true);
        let frame = queue.finish_frame();
        assert_eq!(frame.media.unwrap().session, SceneMediaSessionId(42));
        assert_eq!(frame.video.unwrap().generation, SceneMediaGeneration(3));
        assert_eq!(frame.audio.media_session, Some(SceneMediaSessionId(42)));
        assert_eq!(frame.audio.media_generation, SceneMediaGeneration(3));
    }
}
