//! Shared media-session identity and clock state for synchronized audio/video.

use super::SceneEventSequence;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneMediaSessionId(pub u64);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneMediaGeneration(pub u64);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SceneMediaPlaybackState {
    #[default]
    Idle,
    Buffering,
    Playing,
    Paused,
    Ended,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneMediaClockState {
    pub sequence: SceneEventSequence,
    pub session: SceneMediaSessionId,
    pub generation: SceneMediaGeneration,
    pub playback: SceneMediaPlaybackState,
    pub clock_ns: u64,
    pub duration_ns: Option<u64>,
    pub rate_milli: i32,
    pub loop_index: u64,
}
