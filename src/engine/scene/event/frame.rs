//! Immutable event snapshot shared by all semantic and rendering consumers.

use super::{
    SceneAudioState, SceneEvent, SceneEventSequence, SceneMediaClockState, ScenePointerState,
    SceneVideoState,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSequencedEvent {
    pub sequence: SceneEventSequence,
    pub event: SceneEvent,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SceneFrameEvents {
    pub first_sequence: SceneEventSequence,
    pub last_sequence: SceneEventSequence,
    pub pointer: ScenePointerState,
    pub audio: SceneAudioState,
    pub media: Option<SceneMediaClockState>,
    pub video: Option<SceneVideoState>,
    pub ordered: Vec<SceneSequencedEvent>,
}

impl SceneFrameEvents {
    pub fn audio_spectrum(&self) -> Option<&[f32; 32]> {
        self.audio.spectrum()
    }

    pub fn coherent_audio_spectrum(&self) -> Option<&[f32; 32]> {
        let spectrum = self.audio.spectrum()?;
        let Some(session) = self.audio.media_session else {
            return Some(spectrum);
        };
        self.media
            .filter(|media| {
                media.session == session && media.generation == self.audio.media_generation
            })
            .map(|_| spectrum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::event::{
        SceneAudioSource, SceneMediaGeneration, SceneMediaSessionId,
    };

    #[test]
    fn media_audio_requires_the_current_session_and_generation() {
        let mut frame = SceneFrameEvents {
            audio: SceneAudioState {
                source: SceneAudioSource::MediaSession,
                media_session: Some(SceneMediaSessionId(7)),
                media_generation: SceneMediaGeneration(2),
                spectrum32: [0.5; 32],
                ready: true,
                ..SceneAudioState::default()
            },
            media: Some(SceneMediaClockState {
                session: SceneMediaSessionId(7),
                generation: SceneMediaGeneration(1),
                ..SceneMediaClockState::default()
            }),
            ..SceneFrameEvents::default()
        };

        assert!(frame.audio_spectrum().is_some());
        assert!(frame.coherent_audio_spectrum().is_none());
        frame.media.as_mut().unwrap().generation = SceneMediaGeneration(2);
        assert_eq!(frame.coherent_audio_spectrum(), Some(&[0.5; 32]));
    }
}
