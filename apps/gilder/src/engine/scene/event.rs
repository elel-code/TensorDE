//! Typed runtime events consumed by the scene semantic layer.
//!
//! Runtime events are deliberately outside the `.gscene` ABI. The binary stores
//! authored bindings; providers publish process-local values through this layer.

pub mod audio;
pub mod frame;
pub mod local_time;
pub mod media;
pub mod pointer;
pub mod queue;
pub mod video;

pub use audio::{SceneAudioSource, SceneAudioState, StereoSpectrum64};
pub use frame::{SceneFrameEvents, SceneSequencedEvent};
pub use local_time::SceneLocalTime;
pub use media::{
    SceneMediaClockState, SceneMediaGeneration, SceneMediaPlaybackState, SceneMediaSessionId,
};
pub use pointer::{
    ScenePointerEvent, ScenePointerEventKind, ScenePointerSource, ScenePointerState,
};
pub use queue::SceneEventQueue;
pub use video::SceneVideoState;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneEventSequence(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub enum SceneEvent {
    Pointer(ScenePointerEvent),
    Audio(SceneAudioState),
    Media(SceneMediaClockState),
    Video(SceneVideoState),
}
