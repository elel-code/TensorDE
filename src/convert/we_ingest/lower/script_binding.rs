use crate::engine::scene::{SceneAudioBandMaterialBindingRecord, SceneObjectHandle};

use super::super::ir::WeSceneIr;

pub(super) fn lower_audio_band_material_bindings(
    ir: &WeSceneIr,
) -> Vec<SceneAudioBandMaterialBindingRecord> {
    ir.audio_band_material_bindings
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
        .collect()
}
