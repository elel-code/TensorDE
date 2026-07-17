//! Frame-stable audio analysis input.

use super::{SceneEventSequence, SceneMediaGeneration, SceneMediaSessionId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SceneAudioSource {
    #[default]
    None,
    SystemOutput,
    MediaSession,
    Diagnostic,
    Replay,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneAudioState {
    pub sequence: SceneEventSequence,
    pub source: SceneAudioSource,
    pub media_session: Option<SceneMediaSessionId>,
    pub media_generation: SceneMediaGeneration,
    pub sample_time_ns: u64,
    pub spectrum32: [f32; 32],
    pub ready: bool,
}

impl Default for SceneAudioState {
    fn default() -> Self {
        Self {
            sequence: SceneEventSequence::default(),
            source: SceneAudioSource::None,
            media_session: None,
            media_generation: SceneMediaGeneration::default(),
            sample_time_ns: 0,
            spectrum32: [0.0; 32],
            ready: false,
        }
    }
}

impl SceneAudioState {
    pub fn spectrum(&self) -> Option<&[f32; 32]> {
        self.ready.then_some(&self.spectrum32)
    }
}
