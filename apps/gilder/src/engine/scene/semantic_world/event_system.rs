//! Retained semantic consumers of one immutable scene-event snapshot.

use crate::engine::scene::event::SceneFrameEvents;

use super::{
    ResolvedSemanticFrame, SceneSemanticWorld, pointer_parallax::RetainedPointerParallaxSystem,
};

#[derive(Debug)]
pub(super) struct RetainedSceneEventSystem {
    pointer_parallax: RetainedPointerParallaxSystem,
}

impl RetainedSceneEventSystem {
    pub(super) fn from_world(world: &SceneSemanticWorld<'_>) -> Self {
        Self {
            pointer_parallax: RetainedPointerParallaxSystem::from_world(world),
        }
    }

    pub(super) fn begin_frame(
        &mut self,
        world: &SceneSemanticWorld<'_>,
        frame: &mut ResolvedSemanticFrame,
        frame_delta_seconds: f32,
        events: &SceneFrameEvents,
    ) {
        frame.media_clock = events.media;
        frame.video_frame = coherent_video_frame(events);
        self.pointer_parallax
            .begin_frame(world, frame_delta_seconds, events);
    }

    pub(super) fn finish_frame(&self, frame: &mut ResolvedSemanticFrame) {
        self.pointer_parallax.apply_frame(frame);
    }
}

fn coherent_video_frame(
    events: &SceneFrameEvents,
) -> Option<crate::engine::scene::event::SceneVideoState> {
    events.video.filter(|video| {
        events.media.is_some_and(|media| {
            media.session == video.session && media.generation == video.generation
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::event::{
        SceneMediaClockState, SceneMediaGeneration, SceneMediaSessionId, SceneVideoState,
    };

    #[test]
    fn video_frame_requires_the_current_media_session_generation() {
        let mut events = SceneFrameEvents {
            media: Some(SceneMediaClockState {
                session: SceneMediaSessionId(5),
                generation: SceneMediaGeneration(2),
                ..SceneMediaClockState::default()
            }),
            video: Some(SceneVideoState {
                session: SceneMediaSessionId(5),
                generation: SceneMediaGeneration(1),
                frame_identity: 17,
                ready: true,
                ..SceneVideoState::default()
            }),
            ..SceneFrameEvents::default()
        };

        assert!(coherent_video_frame(&events).is_none());
        events.video.as_mut().unwrap().generation = SceneMediaGeneration(2);
        assert_eq!(coherent_video_frame(&events).unwrap().frame_identity, 17);
    }
}
