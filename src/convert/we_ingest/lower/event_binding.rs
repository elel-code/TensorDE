//! Lower authored audio and pointer bindings into typed static records.

use crate::engine::scene::{
    SceneAudioBandMaterialBindingRecord, SceneCameraParallaxRecord, SceneObjectHandle,
    SceneObjectParallaxDepthRecord,
};

use super::super::ir::WeSceneIr;

pub(super) struct LoweredSceneEventBindings {
    pub(super) audio: Vec<SceneAudioBandMaterialBindingRecord>,
    pub(super) camera_parallax: SceneCameraParallaxRecord,
    pub(super) object_parallax_depths: Vec<SceneObjectParallaxDepthRecord>,
}

pub(super) fn lower_event_bindings(ir: &WeSceneIr) -> LoweredSceneEventBindings {
    let audio = ir
        .audio_band_material_bindings
        .iter()
        .map(|binding| SceneAudioBandMaterialBindingRecord {
            object: SceneObjectHandle(binding.object),
            target: binding.target,
            spectrum_resolution: binding.spectrum_resolution,
            band_index: binding.band_index,
            smoothing: binding.smoothing,
            minimum_multiplier: binding.minimum_multiplier,
            maximum_multiplier: binding.maximum_multiplier,
            initial_value: binding.initial_value,
        })
        .collect();
    let object_parallax_depths = ir
        .objects
        .iter()
        .filter(|object| object.parallax_depth != [0.0; 2])
        .map(|object| SceneObjectParallaxDepthRecord {
            object: SceneObjectHandle(object.handle),
            depth: object.parallax_depth,
        })
        .collect();
    LoweredSceneEventBindings {
        audio,
        camera_parallax: SceneCameraParallaxRecord {
            enabled: ir.scene.camera_parallax_enabled,
            amount: ir.scene.camera_parallax_amount,
            delay: ir.scene.camera_parallax_delay,
            mouse_influence: ir.scene.camera_parallax_mouse_influence,
        },
        object_parallax_depths,
    }
}
