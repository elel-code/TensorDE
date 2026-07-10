//! Wallpaper Engine object animation-layer script semantics.

use serde_json::Value;

use super::value_f32;

pub(super) fn animation_layer_initial_progress(layer: &Value) -> f32 {
    let Some(binding) = layer.get("visible").and_then(Value::as_object) else {
        return 0.0;
    };
    let set_frame_script = binding
        .get("script")
        .and_then(Value::as_str)
        .is_some_and(|script| script.contains("setFrame") && script.contains("frameCount"));
    if !set_frame_script {
        return 0.0;
    }
    binding
        .get("scriptproperties")
        .and_then(|properties| value_f32(properties.get("percentage")))
        .filter(|progress| progress.is_finite())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_progress_requires_set_frame_script_semantics() {
        let layer = serde_json::json!({
            "visible": {
                "value": true,
                "script": "animation.setFrame(animation.frameCount * scriptProperties.percentage)",
                "scriptproperties": { "percentage": 0.94 }
            }
        });
        assert_eq!(animation_layer_initial_progress(&layer), 0.94);

        let unrelated = serde_json::json!({
            "visible": {
                "value": true,
                "scriptproperties": { "percentage": 0.5 }
            }
        });
        assert_eq!(animation_layer_initial_progress(&unrelated), 0.0);
    }
}
