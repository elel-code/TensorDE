//! Stable video-frame identity published without decoder or Vulkan ownership.

use super::{SceneEventSequence, SceneMediaGeneration, SceneMediaSessionId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneVideoState {
    pub sequence: SceneEventSequence,
    pub session: SceneMediaSessionId,
    pub generation: SceneMediaGeneration,
    pub frame_serial: u64,
    pub frame_identity: u64,
    pub presentation_time_ns: u64,
    pub duration_ns: Option<u64>,
    pub ready: bool,
}
