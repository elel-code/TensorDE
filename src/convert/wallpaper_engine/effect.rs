use serde_json::{Map, Value, json};
use std::fs;
use std::path::Path;

use super::{
    ConversionReport, SceneDocumentBuildContext, WallpaperEngineProject, push_unique,
    scene_copy_resource_as, scene_i64_map_from_value, scene_next_timeline_id,
    scene_push_unsupported, string_field, value_to_bool_unwrapped, value_to_f64,
    value_to_f64_unwrapped, value_to_i64, value_to_string,
};

pub(super) fn scene_effects_from_object(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    object: &Map<String, Value>,
    node_id: &str,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Vec<Value> {
    let Some(effects) = object.get("effects").and_then(Value::as_array) else {
        return Vec::new();
    };
    effects
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|effect| {
            let file = string_field(effect, &["file"])?;
            let mut output = Map::new();
            output.insert("file".to_owned(), Value::String(file.clone()));
            if let Some(id) = effect.get("id").and_then(value_to_i64) {
                output.insert("id".to_owned(), json!(id));
            }
            if let Some(name) = string_field(effect, &["name"]) {
                output.insert("name".to_owned(), Value::String(name));
            }
            if let Some(visible) = effect.get("visible") {
                output.insert("visible".to_owned(), visible.clone());
            }
            let passes = scene_effect_passes_from_object(effect);
            if !passes.is_empty() {
                output.insert("passes".to_owned(), Value::Array(passes));
            }
            let opacity_timeline_lowered =
                scene_lower_opacity_effect_timeline(effect, &file, node_id, report, context);
            let requires_runtime =
                scene_effect_requires_runtime(project, &file, effect, opacity_timeline_lowered);
            if requires_runtime {
                output.insert(
                    "runtime".to_owned(),
                    Value::String("wallpaper-engine-effect".to_owned()),
                );
                if let Some(resource) = scene_copy_resource_as(
                    project,
                    output_dir,
                    &file,
                    "effect",
                    Some("we-effect"),
                    report,
                    context,
                    resources,
                ) {
                    output.insert("resource".to_owned(), Value::String(resource));
                }
                scene_push_unsupported(
                    context,
                    "we-effect-runtime",
                    "Wallpaper Engine effect graph is preserved in gscene but not executed by the native scene runtime yet.",
                    Some(&file),
                );
            } else if opacity_timeline_lowered {
                output.insert(
                    "runtime".to_owned(),
                    Value::String("native-opacity-timeline".to_owned()),
                );
            } else {
                output.insert(
                    "runtime".to_owned(),
                    Value::String("metadata-only".to_owned()),
                );
                push_unique(
                    &mut context.converted_features,
                    "scene-we-noop-effect-preserved",
                );
            }
            Some(Value::Object(output))
        })
        .collect()
}

fn scene_effect_requires_runtime(
    project: &WallpaperEngineProject,
    file: &str,
    effect: &Map<String, Value>,
    opacity_timeline_lowered: bool,
) -> bool {
    if opacity_timeline_lowered {
        return false;
    }
    if effect
        .get("visible")
        .and_then(value_to_bool_unwrapped)
        .is_some_and(|visible| !visible)
    {
        return false;
    }
    if scene_effect_passes_require_runtime(effect.get("passes").and_then(Value::as_array)) {
        return true;
    }
    scene_effect_file_requires_runtime(project, file).unwrap_or(true)
}

fn scene_lower_opacity_effect_timeline(
    effect: &Map<String, Value>,
    file: &str,
    node_id: &str,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
) -> bool {
    if !scene_effect_file_is_opacity(file) {
        return false;
    }
    if effect
        .get("visible")
        .and_then(value_to_bool_unwrapped)
        .is_some_and(|visible| !visible)
    {
        return false;
    }
    if !scene_opacity_effect_is_alpha_only(effect) {
        return false;
    }
    let keyframes = if let Some((delay_seconds, fade_seconds, initial_opacity)) =
        scene_opacity_effect_fade_parameters(effect)
    {
        let delay_ms = (delay_seconds.max(0.0) * 1000.0).round() as u64;
        let fade_ms = (fade_seconds.max(0.0) * 1000.0).round() as u64;
        let end_ms = delay_ms.saturating_add(fade_ms);
        let initial_opacity = initial_opacity.clamp(0.0, 1.0);
        let mut keyframes = vec![json!({
            "time_ms": 0,
            "value": initial_opacity,
            "curve": "linear"
        })];
        if delay_ms > 0 {
            keyframes.push(json!({
                "time_ms": delay_ms,
                "value": initial_opacity,
                "curve": "linear"
            }));
        }
        keyframes.push(json!({
            "time_ms": end_ms,
            "value": 0.0,
            "curve": "linear"
        }));
        keyframes
    } else if let Some(opacity) = scene_opacity_effect_constant_alpha(effect) {
        vec![json!({
            "time_ms": 0,
            "value": opacity,
            "curve": "linear"
        })]
    } else {
        return false;
    };
    let timeline_id = scene_next_timeline_id(context, Some(&format!("{node_id}-opacity-effect")));
    context.timelines.push(json!({
        "id": timeline_id,
        "target_node": node_id,
        "channels": [{
            "property": "opacity",
            "loop": false,
            "keyframes": keyframes
        }]
    }));
    push_unique(
        &mut report.converted_features,
        "scene-we-opacity-effect-timeline",
    );
    true
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

fn scene_opacity_effect_fade_parameters(effect: &Map<String, Value>) -> Option<(f64, f64, f64)> {
    let alpha = scene_effect_alpha_value(effect)?;
    let script = alpha.get("script").and_then(Value::as_str)?;
    let delay = scene_script_numeric_constant(script, "delayTime")?;
    let fade = scene_script_numeric_constant(script, "fadeTime")?;
    let initial = alpha.get("value").and_then(value_to_f64).unwrap_or(1.0);
    Some((delay, fade, initial))
}

fn scene_opacity_effect_constant_alpha(effect: &Map<String, Value>) -> Option<f64> {
    scene_effect_alpha_constant_value(effect).map(|alpha| alpha.clamp(0.0, 1.0))
}

fn scene_effect_alpha_value(effect: &Map<String, Value>) -> Option<&Map<String, Value>> {
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

fn scene_effect_alpha_constant_value(effect: &Map<String, Value>) -> Option<f64> {
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
        .filter_map(|values| values.get("alpha").and_then(value_to_f64_unwrapped))
        .next()
}

fn scene_script_numeric_constant(script: &str, name: &str) -> Option<f64> {
    let mut search_start = 0usize;
    while let Some(relative) = script.get(search_start..)?.find(name) {
        let start = search_start + relative + name.len();
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

fn scene_effect_file_requires_runtime(
    project: &WallpaperEngineProject,
    file: &str,
) -> Option<bool> {
    let source = project.root.join(file);
    let text = fs::read_to_string(source).ok()?;
    let value = serde_json::from_str::<Value>(&text).ok()?;
    let object = value.as_object()?;
    if object
        .get("visible")
        .and_then(value_to_bool_unwrapped)
        .is_some_and(|visible| !visible)
    {
        return Some(false);
    }
    if object.contains_key("passes") {
        return Some(scene_effect_passes_require_runtime(
            object.get("passes").and_then(Value::as_array),
        ));
    }
    Some(scene_effect_object_has_runtime_fields(object))
}

fn scene_effect_passes_require_runtime(passes: Option<&Vec<Value>>) -> bool {
    passes
        .into_iter()
        .flat_map(|passes| passes.iter())
        .filter_map(Value::as_object)
        .any(scene_effect_pass_requires_runtime)
}

fn scene_effect_pass_requires_runtime(pass: &Map<String, Value>) -> bool {
    if pass
        .get("visible")
        .or_else(|| pass.get("enabled"))
        .and_then(value_to_bool_unwrapped)
        .is_some_and(|enabled| !enabled)
    {
        return false;
    }
    pass.iter().any(|(key, value)| {
        !scene_effect_metadata_key(key) && scene_effect_value_requires_runtime(value)
    })
}

fn scene_effect_object_has_runtime_fields(object: &Map<String, Value>) -> bool {
    object.iter().any(|(key, value)| {
        !scene_effect_metadata_key(key) && scene_effect_value_requires_runtime(value)
    })
}

fn scene_effect_metadata_key(key: &str) -> bool {
    matches!(
        key,
        "id" | "name" | "visible" | "enabled" | "description" | "comment" | "passes"
    )
}

fn scene_effect_value_requires_runtime(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(_) => true,
        Value::String(value) => !value.is_empty(),
        Value::Array(values) => values.iter().any(scene_effect_value_requires_runtime),
        Value::Object(values) => !values.is_empty(),
    }
}

fn scene_effect_passes_from_object(effect: &Map<String, Value>) -> Vec<Value> {
    let Some(passes) = effect.get("passes").and_then(Value::as_array) else {
        return Vec::new();
    };
    passes
        .iter()
        .filter_map(Value::as_object)
        .map(|pass| {
            let mut output = Map::new();
            if let Some(id) = pass.get("id").and_then(value_to_i64) {
                output.insert("id".to_owned(), json!(id));
            }
            if let Some(textures) = scene_effect_pass_textures(pass) {
                output.insert("textures".to_owned(), textures);
            }
            if let Some(combos) = pass.get("combos").and_then(scene_i64_map_from_value) {
                output.insert("combos".to_owned(), combos);
            }
            if let Some(values) = pass.get("constantshadervalues").and_then(Value::as_object) {
                output.insert(
                    "constant_shader_values".to_owned(),
                    Value::Object(values.clone()),
                );
            }
            if let Some(user_textures) = pass.get("usertextures") {
                output.insert("user_textures".to_owned(), user_textures.clone());
            }
            Value::Object(output)
        })
        .collect()
}

fn scene_effect_pass_textures(pass: &Map<String, Value>) -> Option<Value> {
    let textures = pass.get("textures")?.as_array()?;
    Some(Value::Array(
        textures
            .iter()
            .map(|texture| {
                value_to_string(texture)
                    .map(Value::String)
                    .unwrap_or(Value::Null)
            })
            .collect(),
    ))
}
