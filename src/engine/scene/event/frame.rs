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
}
