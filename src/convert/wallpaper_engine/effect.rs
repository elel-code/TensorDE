use serde_json::{Map, Value, json};
use std::fs;
use std::path::Path;

use super::{
    ConversionReport, SceneDocumentBuildContext, WallpaperEngineProject, ir::SceneOpacityEffectIr,
    number_value_field, push_unique, scene_copy_resource_as,
    scene_effect_texture_resource_from_reference, scene_i64_map_from_value, scene_next_timeline_id,
    scene_push_unsupported, scene_record_native_script_lowering, string_field,
    value_to_bool_unwrapped, value_to_i64, value_to_string, value_to_u32,
};

pub(super) fn scene_effects_from_object(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    object: &Map<String, Value>,
    node: &Map<String, Value>,
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
            let passes = scene_effect_passes_from_object(
                project, output_dir, node, &file, effect, report, context, resources,
            );
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
            if opacity_timeline_lowered {
                output.insert(
                    "runtime".to_owned(),
                    Value::String("native-opacity-timeline".to_owned()),
                );
                return Some(Value::Object(output));
            }
            if let Some(runtime) = scene_native_effect_runtime(&file, effect) {
                output.insert("runtime".to_owned(), Value::String(runtime.to_owned()));
                push_unique(&mut context.converted_features, scene_native_effect_feature(runtime));
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

fn scene_native_effect_runtime(file: &str, effect: &Map<String, Value>) -> Option<&'static str> {
    if effect
        .get("visible")
        .and_then(value_to_bool_unwrapped)
        .is_some_and(|visible| !visible)
    {
        return None;
    }
    if file.contains("watercaustics") {
        return Some("native-water-caustics");
    }
    if file.ends_with("effects/opacity/effect.json") || file == "effects/opacity/effect.json" {
        return Some("native-opacity-mask");
    }
    if file.ends_with("effects/iris/effect.json") || file == "effects/iris/effect.json" {
        return Some("native-iris-mask");
    }
    if file.contains("waterwaves")
        || file.contains("waterripple")
        || file.contains("waterflow")
        || file.contains("cloudmotion")
        || file.contains("foliagesway")
        || file.contains("auto_sway")
        || file.contains("shake")
        || file.contains("skew")
    {
        return Some("native-effect-motion");
    }
    None
}

fn scene_native_effect_feature(runtime: &str) -> &'static str {
    match runtime {
        "native-water-caustics" => "native-water-caustics-effect-runtime",
        "native-opacity-mask" => "native-opacity-mask-effect-runtime",
        "native-iris-mask" => "native-iris-mask-effect-runtime",
        "native-effect-motion" => "native-effect-motion-runtime",
        _ => "native-effect-runtime",
    }
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

fn scene_effect_passes_from_object(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    node: &Map<String, Value>,
    effect_file: &str,
    effect: &Map<String, Value>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Vec<Value> {
    let Some(passes) = effect.get("passes").and_then(Value::as_array) else {
        return Vec::new();
    };
    let material_passes = scene_effect_material_passes(project, effect_file);
    passes
        .iter()
        .enumerate()
        .filter_map(|(pass_index, pass)| pass.as_object().map(|pass| (pass_index, pass)))
        .map(|(pass_index, pass)| {
            let mut output = Map::new();
            if let Some(id) = pass.get("id").and_then(value_to_i64) {
                output.insert("id".to_owned(), json!(id));
            }
            if let Some(material_pass) = material_passes.get(pass_index) {
                scene_copy_effect_material_pass_fields(material_pass, &mut output);
            }
            if let Some(textures) = scene_effect_pass_textures(pass) {
                output.insert("textures".to_owned(), textures);
            }
            let texture_resources = scene_effect_pass_texture_resources(
                project, output_dir, pass, report, context, resources,
            );
            if let Some(texture_resources) = texture_resources.as_ref() {
                output.insert("texture_resources".to_owned(), texture_resources.clone());
            }
            if let Some(effect_uv_transform) = scene_effect_pass_uv_transform(
                node,
                effect_file,
                pass,
                texture_resources.as_ref(),
                resources,
            ) {
                output.insert("effect_uv_transform".to_owned(), effect_uv_transform);
                push_unique(
                    &mut context.converted_features,
                    "scene-we-effect-uv-transform",
                );
                push_unique(
                    &mut report.converted_features,
                    "scene-we-effect-uv-transform",
                );
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

fn scene_effect_material_passes(
    project: &WallpaperEngineProject,
    effect_file: &str,
) -> Vec<Map<String, Value>> {
    let Some(effect) = fs::read_to_string(project.root.join(effect_file))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return Vec::new();
    };
    let Some(passes) = effect.get("passes").and_then(Value::as_array) else {
        return Vec::new();
    };
    passes
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|pass| string_field(pass, &["material"]))
        .filter_map(|material| scene_effect_material_first_pass(project, &material))
        .collect()
}

fn scene_effect_material_first_pass(
    project: &WallpaperEngineProject,
    material: &str,
) -> Option<Map<String, Value>> {
    let material = fs::read_to_string(project.root.join(material)).ok()?;
    let material = serde_json::from_str::<Value>(&material).ok()?;
    material
        .get("passes")?
        .as_array()?
        .first()?
        .as_object()
        .cloned()
}

fn scene_copy_effect_material_pass_fields(
    material_pass: &Map<String, Value>,
    output: &mut Map<String, Value>,
) {
    for key in ["shader", "blending", "depthtest", "depthwrite", "cullmode"] {
        if let Some(value) = material_pass.get(key) {
            output.insert(key.to_owned(), value.clone());
        }
    }
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

fn scene_effect_pass_texture_resources(
    project: &WallpaperEngineProject,
    output_dir: &Path,
    pass: &Map<String, Value>,
    report: &mut ConversionReport,
    context: &mut SceneDocumentBuildContext,
    resources: &mut Vec<Value>,
) -> Option<Value> {
    let textures = pass.get("textures")?.as_array()?;
    let mut has_resource = false;
    let texture_resources = textures
        .iter()
        .map(|texture| {
            let Some(texture) = value_to_string(texture) else {
                return Value::Null;
            };
            let Some(resource) = scene_effect_texture_resource_from_reference(
                project, output_dir, &texture, report, context, resources,
            ) else {
                return Value::Null;
            };
            has_resource = true;
            Value::String(resource)
        })
        .collect::<Vec<_>>();
    has_resource.then(|| Value::Array(texture_resources))
}

fn scene_effect_pass_uv_transform(
    node: &Map<String, Value>,
    effect_file: &str,
    pass: &Map<String, Value>,
    texture_resources: Option<&Value>,
    resources: &[Value],
) -> Option<Value> {
    if !scene_effect_file_uses_mask_uv(effect_file) {
        return None;
    }
    let texture_resources = texture_resources?.as_array()?;
    let (mask_slot, mask_resource) = texture_resources
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(slot, resource)| value_to_string(resource).map(|resource| (slot, resource)))?;
    let input_extent = scene_effect_node_extent(node);
    let mask_extent = scene_effect_resource_extent(resources, &mask_resource);
    let (mut scale, offset, has_explicit_scale) = scene_effect_pass_uv_transform_values(pass);
    if !has_explicit_scale
        && let (Some(input_extent), Some(mask_extent)) =
            (input_extent.as_ref(), mask_extent.as_ref())
        && let Some(extent_scale) =
            scene_effect_uv_transform_scale_from_extents(input_extent, mask_extent)
    {
        scale = extent_scale;
    }
    let mut transform = Map::new();
    transform.insert(
        "mapping".to_owned(),
        Value::String("texture-resolution".to_owned()),
    );
    transform.insert("source_slot".to_owned(), json!(0));
    transform.insert(
        "mask_slot".to_owned(),
        json!(mask_slot.min(u32::MAX as usize) as u32),
    );
    transform.insert("scale".to_owned(), json!([scale[0], scale[1]]));
    transform.insert("offset".to_owned(), json!([offset[0], offset[1]]));
    if let Some(extent) = input_extent {
        transform.insert("input_extent".to_owned(), extent);
    }
    if let Some(extent) = mask_extent {
        transform.insert("mask_extent".to_owned(), extent.clone());
        transform.insert("mask_backing_extent".to_owned(), extent);
    }
    Some(Value::Object(transform))
}

fn scene_effect_file_uses_mask_uv(effect_file: &str) -> bool {
    let file = effect_file.replace('\\', "/").to_ascii_lowercase();
    file == "effects/opacity/effect.json"
        || file.ends_with("/effects/opacity/effect.json")
        || file == "effects/iris/effect.json"
        || file.ends_with("/effects/iris/effect.json")
}

fn scene_effect_pass_uv_transform_values(pass: &Map<String, Value>) -> ([f64; 2], [f64; 2], bool) {
    let mut scale = [1.0, 1.0];
    let mut offset = [0.0, 0.0];
    let mut has_explicit_scale = false;
    if let Some(values) = pass
        .get("constantshadervalues")
        .or_else(|| pass.get("constant_shader_values"))
        .and_then(Value::as_object)
    {
        if let Some(resolution) = values
            .get("g_Texture1Resolution")
            .or_else(|| values.get("Texture1Resolution"))
            .and_then(scene_effect_vec4)
            && resolution[2].abs() > f64::EPSILON
            && resolution[3].abs() > f64::EPSILON
        {
            // WE's opacity vertex shader computes mask UV as base_uv * base_extent / mask_extent.
            scale = [resolution[2] / resolution[0], resolution[3] / resolution[1]];
            has_explicit_scale = true;
        }
    }
    if let Some(value) = pass
        .get("effect_uv_scale")
        .or_else(|| pass.get("effectuvscale"))
        .and_then(scene_effect_vec2)
    {
        scale = value;
        has_explicit_scale = true;
    }
    if let Some(value) = pass
        .get("effect_uv_offset")
        .or_else(|| pass.get("effectuvoffset"))
        .and_then(scene_effect_vec2)
    {
        offset = value;
    }
    (scale, offset, has_explicit_scale)
}

fn scene_effect_vec2(value: &Value) -> Option<[f64; 2]> {
    let values = scene_effect_number_values(value)?;
    if values.len() < 2 {
        return None;
    }
    Some([values[0], values[1]])
}

fn scene_effect_vec4(value: &Value) -> Option<[f64; 4]> {
    let values = scene_effect_number_values(value)?;
    if values.len() < 4 {
        return None;
    }
    Some([values[0], values[1], values[2], values[3]])
}

fn scene_effect_number_values(value: &Value) -> Option<Vec<f64>> {
    match value {
        Value::Array(values) => {
            let values = values
                .iter()
                .filter_map(Value::as_f64)
                .filter(|value| value.is_finite())
                .collect::<Vec<_>>();
            (!values.is_empty()).then_some(values)
        }
        Value::String(value) => {
            let values = value
                .split_whitespace()
                .filter_map(|part| part.parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .collect::<Vec<_>>();
            (!values.is_empty()).then_some(values)
        }
        Value::Number(value) => value
            .as_f64()
            .filter(|value| value.is_finite())
            .map(|value| vec![value]),
        Value::Null | Value::Bool(_) | Value::Object(_) => None,
    }
}

fn scene_effect_node_extent(node: &Map<String, Value>) -> Option<Value> {
    let width = number_value_field(node, &["width", "w"])?;
    let height = number_value_field(node, &["height", "h"])?;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(json!({
        "width": width.round().clamp(1.0, f64::from(u32::MAX)) as u32,
        "height": height.round().clamp(1.0, f64::from(u32::MAX)) as u32
    }))
}

fn scene_effect_resource_extent(resources: &[Value], resource_id: &str) -> Option<Value> {
    resources
        .iter()
        .rev()
        .filter_map(Value::as_object)
        .find_map(|resource| {
            if resource.get("id").and_then(Value::as_str) != Some(resource_id) {
                return None;
            }
            let width = resource.get("width").and_then(value_to_u32)?;
            let height = resource.get("height").and_then(value_to_u32)?;
            if width == 0 || height == 0 {
                return None;
            }
            Some(json!({ "width": width, "height": height }))
        })
}

fn scene_effect_uv_transform_scale_from_extents(
    input_extent: &Value,
    mask_extent: &Value,
) -> Option<[f64; 2]> {
    let input = input_extent.as_object()?;
    let mask = mask_extent.as_object()?;
    let input_width = f64::from(input.get("width").and_then(value_to_u32)?);
    let input_height = f64::from(input.get("height").and_then(value_to_u32)?);
    let mask_width = f64::from(mask.get("width").and_then(value_to_u32)?);
    let mask_height = f64::from(mask.get("height").and_then(value_to_u32)?);
    if input_width <= f64::EPSILON || input_height <= f64::EPSILON {
        return None;
    }
    Some([input_width / mask_width, input_height / mask_height])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_transform_resolution_scale_uses_base_over_mask_extent() {
        let pass = json!({
            "constantshadervalues": {
                "g_Texture1Resolution": [331.0, 115.0, 663.0, 230.0]
            }
        });
        let pass = pass.as_object().expect("pass object");

        let (scale, offset, has_explicit_scale) = scene_effect_pass_uv_transform_values(pass);

        assert!((scale[0] - (663.0 / 331.0)).abs() < f64::EPSILON);
        assert_eq!(scale[1], 2.0);
        assert_eq!(offset, [0.0, 0.0]);
        assert!(has_explicit_scale);
    }

    #[test]
    fn uv_transform_fills_missing_resolution_scale_from_extents() {
        let node = json!({ "width": 663.0, "height": 230.0 });
        let node = node.as_object().expect("node object");
        let pass = json!({
            "textures": [null, "masks/opacity_mask"],
            "constantshadervalues": { "alpha": 1.0 }
        });
        let pass = pass.as_object().expect("pass object");
        let texture_resources = json!([null, "mask-resource"]);
        let resources = json!([
            {
                "id": "mask-resource",
                "width": 331,
                "height": 115
            }
        ]);
        let resources = resources.as_array().expect("resources array");

        let transform = scene_effect_pass_uv_transform(
            node,
            "effects/opacity/effect.json",
            pass,
            Some(&texture_resources),
            resources,
        )
        .expect("effect uv transform");

        assert_eq!(transform["scale"][0].as_f64(), Some(663.0 / 331.0));
        assert_eq!(transform["scale"][1].as_f64(), Some(2.0));
    }
}
