//! Retained runtime state for typed audio-band material bindings.

use super::{ResolvedAudioBandMaterialValue, SceneSemanticWorld};
use crate::engine::scene::abi::SceneAudioBandMaterialBindingRecord;

#[derive(Debug)]
pub(super) struct RetainedAudioBandMaterialBindings {
    smoothed_values: Vec<f32>,
    previous_time_seconds: f32,
}

impl RetainedAudioBandMaterialBindings {
    pub(super) fn from_world(world: &SceneSemanticWorld<'_>) -> Self {
        Self {
            smoothed_values: vec![0.0; world.storage.audio_band_material_bindings().len()],
            previous_time_seconds: 0.0,
        }
    }

    pub(super) fn initialize_frame(
        &self,
        world: &SceneSemanticWorld<'_>,
        values: &mut Vec<ResolvedAudioBandMaterialValue>,
    ) {
        values.clear();
        values.extend(
            world
                .storage
                .audio_band_material_bindings()
                .iter()
                .map(|binding| resolved_value(binding, 0.0)),
        );
    }

    pub(super) fn update_frame(
        &mut self,
        world: &SceneSemanticWorld<'_>,
        values: &mut [ResolvedAudioBandMaterialValue],
        scene_time_seconds: f32,
        spectrum32: &[f32; 32],
    ) {
        let delta_seconds = if scene_time_seconds >= self.previous_time_seconds {
            scene_time_seconds - self.previous_time_seconds
        } else {
            self.smoothed_values.fill(0.0);
            0.0
        };
        self.previous_time_seconds = scene_time_seconds;
        for ((binding, smoothed), output) in world
            .storage
            .audio_band_material_bindings()
            .iter()
            .zip(&mut self.smoothed_values)
            .zip(values)
        {
            let target = sampled_spectrum_value(binding, spectrum32);
            let response = (delta_seconds * binding.smoothing).clamp(0.0, 1.0);
            *smoothed += (target - *smoothed) * response;
            *smoothed = smoothed.clamp(0.0, 1.0);
            *output = resolved_value(binding, *smoothed);
        }
    }
}

fn sampled_spectrum_value(
    binding: &SceneAudioBandMaterialBindingRecord,
    spectrum32: &[f32; 32],
) -> f32 {
    match binding.spectrum_resolution {
        16 => {
            let start = (binding.band_index as usize).min(15) * 2;
            spectrum32[start].max(spectrum32[start + 1])
        }
        32 => spectrum32[(binding.band_index as usize).min(31)],
        64 => {
            let position = binding.band_index.min(63) as f32 * 31.0 / 63.0;
            let lower = position.floor() as usize;
            let upper = (lower + 1).min(31);
            spectrum32[lower] + (spectrum32[upper] - spectrum32[lower]) * position.fract()
        }
        _ => 0.0,
    }
}

fn resolved_value(
    binding: &SceneAudioBandMaterialBindingRecord,
    smoothed: f32,
) -> ResolvedAudioBandMaterialValue {
    let multiplier = binding.minimum_multiplier
        + smoothed * (binding.maximum_multiplier - binding.minimum_multiplier);
    ResolvedAudioBandMaterialValue {
        object: binding.object,
        target: binding.target,
        value: binding.initial_value * multiplier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{SceneAudioBandMaterialTarget, SceneObjectHandle};

    #[test]
    fn resolution_16_binding_uses_the_stronger_pair_from_spectrum32() {
        let binding = SceneAudioBandMaterialBindingRecord {
            object: SceneObjectHandle(0),
            target: SceneAudioBandMaterialTarget::TechCircleSectorWidth,
            spectrum_resolution: 16,
            band_index: 3,
            smoothing: 15.0,
            minimum_multiplier: 1.0,
            maximum_multiplier: 2.0,
            initial_value: 0.3,
        };
        let mut spectrum = [0.0; 32];
        spectrum[6] = 0.25;
        spectrum[7] = 0.75;
        assert_eq!(sampled_spectrum_value(&binding, &spectrum), 0.75);
        assert!((resolved_value(&binding, 0.5).value - 0.45).abs() < 1.0e-6);
    }
}
