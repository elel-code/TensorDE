//! Typed lowering for recognized audio-responsive SceneScript property bindings.

use serde_json::Value;

use super::WeIrAudioBandMaterialBinding;
use super::json_value::{bound_bool, bound_string, value_f32, value_u32};
use crate::engine::scene::SceneAudioBandMaterialTarget;

const TECH_CIRCLE_SECTOR_WIDTH: &str = "ui_editor_properties_5_sector_1_width";

pub(super) fn ingest_audio_material_bindings(
    object: u32,
    object_json: &Value,
    output: &mut Vec<WeIrAudioBandMaterialBinding>,
) {
    for effect in object_json
        .get("effects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if !bound_bool(effect.get("visible")).unwrap_or(true) {
            continue;
        }
        let Some(file) = bound_string(effect.get("file")) else {
            continue;
        };
        if !file.to_ascii_lowercase().contains("/tech_circle/") {
            continue;
        }
        for constants in effect
            .get("passes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|pass| pass.get("constantshadervalues"))
        {
            let Some(binding) = constants.get(TECH_CIRCLE_SECTOR_WIDTH) else {
                continue;
            };
            if let Some(binding) = parse_audio_scale_binding(object, binding) {
                output.push(binding);
            }
        }
    }
}

fn parse_audio_scale_binding(object: u32, binding: &Value) -> Option<WeIrAudioBandMaterialBinding> {
    let script = binding.get("script")?.as_str()?;
    let resolution = [16, 32, 64]
        .into_iter()
        .find(|resolution| script.contains(&format!("AUDIO_RESOLUTION_{resolution}")))?;
    if !script.contains("audioBuffer.average[scriptProperties.frequency]")
        || !script.contains("smoothValue +=")
        || !script.contains("initialValue *")
    {
        return None;
    }
    let properties = binding.get("scriptproperties")?;
    let band_index = value_u32(properties.get("frequency"))?;
    if band_index >= resolution {
        return None;
    }
    let smoothing = value_f32(properties.get("smoothing"))?;
    let minimum_multiplier = value_f32(properties.get("minvalue"))?;
    let maximum_multiplier = value_f32(properties.get("maxvalue"))?;
    let initial_value = value_f32(Some(binding))?;
    if ![
        smoothing,
        minimum_multiplier,
        maximum_multiplier,
        initial_value,
    ]
    .into_iter()
    .all(f32::is_finite)
        || smoothing < 0.0
    {
        return None;
    }
    Some(WeIrAudioBandMaterialBinding {
        object,
        target: SceneAudioBandMaterialTarget::TechCircleSectorWidth,
        spectrum_resolution: resolution,
        band_index,
        smoothing,
        minimum_multiplier,
        maximum_multiplier,
        initial_value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_known_audio_average_scale_script_to_typed_binding() {
        let object = serde_json::json!({
            "effects": [{
                "file": "effects/workshop/2123274886/tech_circle/effect.json",
                "passes": [{"constantshadervalues": {
                    "ui_editor_properties_5_sector_1_width": {
                        "script": "const audioBuffer = engine.registerAudioBuffers(engine.AUDIO_RESOLUTION_16); audioBuffer.average[scriptProperties.frequency]; smoothValue += 1; initialValue * 1;",
                        "scriptproperties": {"frequency": 0, "smoothing": 15, "minvalue": 1, "maxvalue": 2},
                        "value": 0.3
                    }
                }}]
            }]
        });
        let mut bindings = Vec::new();
        ingest_audio_material_bindings(7, &object, &mut bindings);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].object, 7);
        assert_eq!(bindings[0].spectrum_resolution, 16);
        assert_eq!(bindings[0].initial_value, 0.3);
    }
}
