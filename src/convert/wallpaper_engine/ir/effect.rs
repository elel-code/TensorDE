use serde_json::{Map, Value, json};

#[derive(Debug, Clone, PartialEq)]
pub(in crate::convert::wallpaper_engine) struct SceneOpacityEffectIr {
    keyframes: Vec<SceneOpacityEffectKeyframeIr>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SceneOpacityEffectKeyframeIr {
    time_ms: u64,
    value: f64,
}

impl SceneOpacityEffectIr {
    pub(in crate::convert::wallpaper_engine) fn from_wallpaper_engine_effect(
        file: &str,
        effect: &Map<String, Value>,
    ) -> Option<Self> {
        if !scene_effect_file_is_opacity(file)
            || scene_effect_bool(effect.get("visible")).is_some_and(|visible| !visible)
            || !scene_opacity_effect_is_alpha_only(effect)
        {
            return None;
        }
        scene_opacity_effect_script_keyframes(effect)
            .or_else(|| scene_opacity_effect_constant_keyframes(effect))
            .map(|keyframes| Self { keyframes })
    }

    pub(in crate::convert::wallpaper_engine) fn timeline_value(
        &self,
        timeline_id: String,
        target_node: &str,
    ) -> Value {
        json!({
            "id": timeline_id,
            "target_node": target_node,
            "channels": [{
                "property": "opacity",
                "loop": false,
                "keyframes": self.keyframes.iter().map(|keyframe| {
                    json!({
                        "time_ms": keyframe.time_ms,
                        "value": keyframe.value,
                        "curve": "linear"
                    })
                }).collect::<Vec<_>>()
            }]
        })
    }
}

fn scene_opacity_effect_script_keyframes(
    effect: &Map<String, Value>,
) -> Option<Vec<SceneOpacityEffectKeyframeIr>> {
    let alpha = scene_effect_alpha_object(effect)?;
    let script = alpha.get("script").and_then(Value::as_str)?;
    let delay_seconds = scene_script_numeric_constant_any(
        script,
        &["delayTime", "startDelay", "delay", "fadeDelay"],
    )
    .unwrap_or(0.0);
    let fade_seconds =
        scene_script_numeric_constant_any(script, &["fadeTime", "fadeDuration", "duration"])?;
    let initial_opacity = scene_script_numeric_constant_any(
        script,
        &["fromAlpha", "startAlpha", "initialAlpha", "sourceAlpha"],
    )
    .or_else(|| alpha.get("value").and_then(scene_effect_number))
    .unwrap_or(1.0)
    .clamp(0.0, 1.0);
    let target_opacity = scene_script_numeric_constant_any(
        script,
        &["toAlpha", "targetAlpha", "endAlpha", "finalAlpha"],
    )
    .unwrap_or(0.0)
    .clamp(0.0, 1.0);

    let delay_ms = scene_seconds_to_ms(delay_seconds);
    let end_ms = delay_ms.saturating_add(scene_seconds_to_ms(fade_seconds));
    let mut keyframes = vec![SceneOpacityEffectKeyframeIr {
        time_ms: 0,
        value: initial_opacity,
    }];
    if delay_ms > 0 {
        keyframes.push(SceneOpacityEffectKeyframeIr {
            time_ms: delay_ms,
            value: initial_opacity,
        });
    }
    keyframes.push(SceneOpacityEffectKeyframeIr {
        time_ms: end_ms,
        value: target_opacity,
    });
    Some(keyframes)
}

fn scene_opacity_effect_constant_keyframes(
    effect: &Map<String, Value>,
) -> Option<Vec<SceneOpacityEffectKeyframeIr>> {
    Some(vec![SceneOpacityEffectKeyframeIr {
        time_ms: 0,
        value: scene_effect_alpha_constant(effect)?.clamp(0.0, 1.0),
    }])
}

fn scene_seconds_to_ms(seconds: f64) -> u64 {
    (seconds.max(0.0) * 1000.0).round() as u64
}

fn scene_opacity_effect_is_alpha_only(effect: &Map<String, Value>) -> bool {
    effect.iter().all(|(key, value)| {
        matches!(
            key.as_str(),
            "file" | "id" | "name" | "visible" | "enabled" | "passes"
        ) && (key != "passes" || scene_opacity_effect_passes_are_alpha_only(value))
    })
}

fn scene_opacity_effect_passes_are_alpha_only(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|passes| passes.iter().all(scene_opacity_effect_pass_is_alpha_only))
}

fn scene_opacity_effect_pass_is_alpha_only(value: &Value) -> bool {
    let Some(pass) = value.as_object() else {
        return false;
    };
    pass.iter().all(|(key, value)| {
        matches!(
            key.as_str(),
            "id" | "name"
                | "visible"
                | "enabled"
                | "constantshadervalues"
                | "constant_shader_values"
        ) && (!matches!(
            key.as_str(),
            "constantshadervalues" | "constant_shader_values"
        ) || value
            .as_object()
            .is_some_and(|values| values.keys().all(|key| key.eq_ignore_ascii_case("alpha"))))
    })
}

fn scene_effect_file_is_opacity(file: &str) -> bool {
    let normalized = file.replace('\\', "/").to_ascii_lowercase();
    normalized.ends_with("/opacity/effect.json") || normalized == "effects/opacity/effect.json"
}

fn scene_effect_alpha_object(effect: &Map<String, Value>) -> Option<&Map<String, Value>> {
    effect
        .get("passes")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|pass| {
            pass.get("constantshadervalues")
                .or_else(|| pass.get("constant_shader_values"))
                .and_then(Value::as_object)
        })
        .filter_map(|values| values.get("alpha").and_then(Value::as_object))
        .next()
}

fn scene_effect_alpha_constant(effect: &Map<String, Value>) -> Option<f64> {
    effect
        .get("passes")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|pass| {
            pass.get("constantshadervalues")
                .or_else(|| pass.get("constant_shader_values"))
                .and_then(Value::as_object)
        })
        .filter_map(|values| values.get("alpha").and_then(scene_effect_number_unwrapped))
        .next()
}

fn scene_script_numeric_constant_any(script: &str, names: &[&str]) -> Option<f64> {
    names
        .iter()
        .filter_map(|name| scene_script_numeric_constant(script, name))
        .next()
}

fn scene_script_numeric_constant(script: &str, name: &str) -> Option<f64> {
    let mut search_start = 0usize;
    while let Some(relative) = script.get(search_start..)?.find(name) {
        let start = search_start + relative + name.len();
        let before = script[..search_start + relative].chars().next_back();
        let after = script[start..].chars().next();
        let before_boundary =
            before.is_none_or(|character| !scene_script_identifier_character(character));
        let after_boundary =
            after.is_none_or(|character| !scene_script_identifier_character(character));
        if !before_boundary || !after_boundary {
            search_start = start;
            continue;
        }
        let after_name = script.get(start..)?.trim_start();
        let after_equals = after_name.strip_prefix('=')?.trim_start();
        let end = after_equals
            .char_indices()
            .find_map(|(index, character)| {
                if character.is_ascii_digit() || matches!(character, '.' | '-' | '+') {
                    None
                } else {
                    Some(index)
                }
            })
            .unwrap_or(after_equals.len());
        if let Ok(value) = after_equals.get(..end)?.parse::<f64>()
            && value.is_finite()
        {
            return Some(value);
        }
        search_start = start;
    }
    None
}

fn scene_script_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '$')
}

fn scene_effect_bool(value: Option<&Value>) -> Option<bool> {
    match value? {
        Value::Bool(value) => Some(*value),
        Value::Number(value) => value.as_f64().map(|value| value != 0.0),
        Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        },
        Value::Object(object) => scene_effect_bool(object.get("value")),
        Value::Array(_) | Value::Null => None,
    }
}

fn scene_effect_number(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Number(value) => value.as_f64()?,
        Value::String(value) => value.parse::<f64>().ok()?,
        Value::Bool(value) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        Value::Object(object) => scene_effect_number(object.get("value")?)?,
        Value::Array(_) | Value::Null => return None,
    };
    number.is_finite().then_some(number)
}

fn scene_effect_number_unwrapped(value: &Value) -> Option<f64> {
    match value {
        Value::Object(object) => scene_effect_number(object.get("value")?),
        _ => scene_effect_number(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_effect_ir_lowers_delay_and_fade_to_alpha_timeline() {
        let effect = json!({
            "passes": [{
                "constantshadervalues": {
                    "alpha": {
                        "script": "const delayTime = 3; const fadeTime = 2;",
                        "value": 1
                    }
                }
            }]
        });
        let ir = SceneOpacityEffectIr::from_wallpaper_engine_effect(
            "effects/opacity/effect.json",
            effect.as_object().unwrap(),
        )
        .unwrap();

        assert_eq!(
            ir.timeline_value("timeline-1".to_owned(), "node-panel"),
            json!({
                "id": "timeline-1",
                "target_node": "node-panel",
                "channels": [{
                    "property": "opacity",
                    "loop": false,
                    "keyframes": [
                        { "time_ms": 0, "value": 1.0, "curve": "linear" },
                        { "time_ms": 3000, "value": 1.0, "curve": "linear" },
                        { "time_ms": 5000, "value": 0.0, "curve": "linear" }
                    ]
                }]
            })
        );
    }

    #[test]
    fn opacity_effect_ir_lowers_explicit_alpha_range() {
        let effect = json!({
            "passes": [{
                "constant_shader_values": {
                    "alpha": {
                        "script": "let startDelay = 1; let fadeDuration = 2; let fromAlpha = 0.25; let targetAlpha = 0.85;",
                        "value": 0
                    }
                }
            }]
        });
        let ir = SceneOpacityEffectIr::from_wallpaper_engine_effect(
            "effects/opacity/effect.json",
            effect.as_object().unwrap(),
        )
        .unwrap();

        assert_eq!(
            ir.timeline_value("timeline-2".to_owned(), "node-panel")["channels"][0]["keyframes"],
            json!([
                { "time_ms": 0, "value": 0.25, "curve": "linear" },
                { "time_ms": 1000, "value": 0.25, "curve": "linear" },
                { "time_ms": 3000, "value": 0.85, "curve": "linear" }
            ])
        );
    }
}
