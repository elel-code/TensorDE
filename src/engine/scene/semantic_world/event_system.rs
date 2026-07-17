//! Retained semantic consumers of one immutable scene-event snapshot.

use crate::engine::scene::event::SceneFrameEvents;

use super::{
    ResolvedSemanticFrame, SceneSemanticWorld, audio_binding::RetainedAudioBandMaterialBindings,
};

#[derive(Debug)]
pub(super) struct RetainedSceneEventSystem {
    audio_bindings: RetainedAudioBandMaterialBindings,
}

impl RetainedSceneEventSystem {
    pub(super) fn from_world(world: &SceneSemanticWorld<'_>) -> Self {
        Self {
            audio_bindings: RetainedAudioBandMaterialBindings::from_world(world),
        }
    }

    pub(super) fn initialize_frame(
        &self,
        world: &SceneSemanticWorld<'_>,
        frame: &mut ResolvedSemanticFrame,
    ) {
        self.audio_bindings
            .initialize_frame(world, &mut frame.audio_band_material_values);
    }

    pub(super) fn update_frame(
        &mut self,
        world: &SceneSemanticWorld<'_>,
        frame: &mut ResolvedSemanticFrame,
        scene_time_seconds: f32,
        events: &SceneFrameEvents,
    ) {
        let spectrum32 = events.audio_spectrum().unwrap_or(&[0.0; 32]);
        self.audio_bindings.update_frame(
            world,
            &mut frame.audio_band_material_values,
            scene_time_seconds,
            spectrum32,
        );
    }
}
