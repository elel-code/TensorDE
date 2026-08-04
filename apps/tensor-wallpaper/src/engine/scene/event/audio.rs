//! Frame-stable audio analysis input.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

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
pub struct StereoSpectrum64 {
    pub left: [f32; 64],
    pub right: [f32; 64],
}

impl Serialize for StereoSpectrum64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("StereoSpectrum64", 2)?;
        state.serialize_field("left", self.left.as_slice())?;
        state.serialize_field("right", self.right.as_slice())?;
        state.end()
    }
}

impl StereoSpectrum64 {
    pub const ZERO: Self = Self {
        left: [0.0; 64],
        right: [0.0; 64],
    };

    pub fn average64(&self) -> [f32; 64] {
        std::array::from_fn(|band| 0.5 * (self.left[band] + self.right[band]))
    }

    pub fn average32(&self) -> [f32; 32] {
        Self::max_pool_32(&self.average64())
    }

    pub fn max_pool_32(channel: &[f32; 64]) -> [f32; 32] {
        std::array::from_fn(|band| channel[2 * band].max(channel[2 * band + 1]))
    }

    pub fn max_pool_16(channel: &[f32; 32]) -> [f32; 16] {
        std::array::from_fn(|band| channel[2 * band].max(channel[2 * band + 1]))
    }
}

impl Default for StereoSpectrum64 {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneAudioState {
    pub sequence: SceneEventSequence,
    pub source: SceneAudioSource,
    pub media_session: Option<SceneMediaSessionId>,
    pub media_generation: SceneMediaGeneration,
    pub sample_time_ns: u64,
    pub spectrum: StereoSpectrum64,
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
            spectrum: StereoSpectrum64::ZERO,
            ready: false,
        }
    }
}

impl SceneAudioState {
    pub fn spectrum(&self) -> Option<&StereoSpectrum64> {
        self.ready.then_some(&self.spectrum)
    }
}
