use serde_json::{Map, Value, json};
use std::fs;
use std::path::Path;

use super::{
    ConversionReport, SceneDocumentBuildContext, WallpaperEngineProject, ir::SceneOpacityEffectIr,
    push_unique, scene_copy_resource_as, scene_i64_map_from_value, scene_next_timeline_id,
    scene_push_unsupported, scene_record_native_script_lowering, string_field,
    value_to_bool_unwrapped, value_to_i64, value_to_string,
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
            if let Some(properties) = scene_native_text_glow_effect_properties(&file, effect) {
                output.insert("runtime".to_owned(), Value::String("native-text-glow".to_owned()));
                output.insert("properties".to_owned(), properties);
                push_unique(
                    &mut context.converted_features,
                    "native-text-glow-effect-runtime",
                );
                return Some(Value::Object(output));
            }
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

fn scene_native_text_glow_effect_properties(
    file: &str,
    effect: &Map<String, Value>,
) -> Option<Value> {
    if !file.replace('\\', "/").ends_with("blurprecise/effect.json") {
        return None;
    }
    if effect
        .get("visible")
        .and_then(value_to_bool_unwrapped)
        .is_some_and(|visible| !visible)
    {
        return None;
    }
    let scale = scene_blurprecise_effect_scale(effect).unwrap_or(1.0);
    Some(json!({
        "kind": "blurprecise",
        "radius": (scale * 2.0).clamp(0.5, 8.0),
        "opacity": 0.12,
        "samples": 8
    }))
}

fn scene_blurprecise_effect_scale(effect: &Map<String, Value>) -> Option<f64> {
    effect
        .get("passes")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|pass| pass.get("constantshadervalues").and_then(Value::as_object))
        .filter_map(|values| values.get("scale"))
        .filter_map(value_to_string)
        .find_map(|scale| {
            scale
                .split_whitespace()
                .filter_map(|part| part.parse::<f64>().ok())
                .find(|value| value.is_finite() && *value > 0.0)
        })
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
    let Some(opacity_effect) = SceneOpacityEffectIr::from_wallpaper_engine_effect(file, effect)
    else {
        return false;
    };
    let timeline_id = scene_next_timeline_id(context, Some(&format!("{node_id}-opacity-effect")));
    context
        .timelines
        .push(opacity_effect.timeline_value(timeline_id, node_id));
    if scene_effect_has_alpha_script(effect) {
        scene_record_native_script_lowering(context);
    }
    push_unique(
        &mut report.converted_features,
        "scene-we-opacity-effect-timeline",
    );
    true
}

fn scene_effect_has_alpha_script(effect: &Map<String, Value>) -> bool {
    effect
        .get("passes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|pass| {
            pass.get("constantshadervalues")
                .or_else(|| pass.get("constant_shader_values"))
                .and_then(Value::as_object)
        })
        .filter_map(|values| values.get("alpha").and_then(Value::as_object))
        .any(|alpha| alpha.get("script").and_then(Value::as_str).is_some())
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
