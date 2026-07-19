//! Canonical SceneScript module extraction without parsing JavaScript expressions.

use serde_json::{Map, Value};

use crate::convert::we_ingest::ir::WeIrScriptProgram;
use crate::engine::scene::{SceneScriptSubscriptions, SceneScriptTarget};

use super::super::script_analysis::{
    SceneScriptAnalysis, SceneScriptParseError, analyze_scene_script,
};
use super::json_value::{bound_bool, compact_json, parse_vec3, value_f32};

pub(super) fn object_script_programs(
    object: u32,
    value: &Value,
    initial_text: Option<&str>,
    project_properties: &Map<String, Value>,
) -> Result<Vec<WeIrScriptProgram>, SceneScriptParseError> {
    let mut programs = Vec::new();
    for (property, target) in [
        ("origin", SceneScriptTarget::Origin),
        ("angles", SceneScriptTarget::Angles),
        ("scale", SceneScriptTarget::Scale),
        ("color", SceneScriptTarget::Color),
        ("alpha", SceneScriptTarget::Alpha),
        ("visible", SceneScriptTarget::Visible),
        ("text", SceneScriptTarget::Text),
    ] {
        let Some(binding) = value.get(property).and_then(Value::as_object) else {
            continue;
        };
        let Some(source) = binding.get("script").and_then(Value::as_str) else {
            continue;
        };
        let analysis = analyze_scene_script(source)?;
        if !has_runtime_entrypoint(&analysis) || analysis.uses_scene_api {
            continue;
        }
        let properties_json = resolved_script_properties(binding, project_properties);
        programs.push(WeIrScriptProgram {
            object,
            target,
            source: source.to_owned(),
            properties_json,
            initial_text: initial_text.unwrap_or_default().to_owned(),
            subscriptions: subscriptions(&analysis, target),
            initial_numeric: initial_numeric(binding, target),
        });
    }
    Ok(programs)
}

pub(super) fn project_property_defaults(project: &Value) -> Map<String, Value> {
    project
        .pointer("/general/properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn has_runtime_entrypoint(analysis: &SceneScriptAnalysis) -> bool {
    analysis.exports_update
        || analysis.exports_init
        || analysis.handles_media
        || analysis.handles_user_properties
}

pub(super) fn effect_script_programs(
    object: u32,
    value: &Value,
    project_properties: &Map<String, Value>,
) -> Result<Vec<WeIrScriptProgram>, SceneScriptParseError> {
    const TECH_CIRCLE_SECTOR_WIDTH: &str = "ui_editor_properties_5_sector_1_width";
    let mut programs = Vec::new();
    for constants in value
        .get("effects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|effect| effect.get("passes").and_then(Value::as_array))
        .flatten()
        .filter_map(|pass| pass.get("constantshadervalues"))
    {
        let Some(binding) = constants.get(TECH_CIRCLE_SECTOR_WIDTH) else {
            continue;
        };
        let Some(source) = binding.get("script").and_then(Value::as_str) else {
            continue;
        };
        let analysis = analyze_scene_script(source)?;
        if !has_runtime_entrypoint(&analysis) {
            continue;
        }
        programs.push(WeIrScriptProgram {
            object,
            target: SceneScriptTarget::TechCircleSectorWidth,
            source: source.to_owned(),
            properties_json: binding
                .as_object()
                .map(|binding| resolved_script_properties(binding, project_properties))
                .unwrap_or_else(|| "{}".to_owned()),
            initial_text: String::new(),
            subscriptions: subscriptions(&analysis, SceneScriptTarget::TechCircleSectorWidth),
            initial_numeric: [value_f32(Some(binding)).unwrap_or(0.0), 0.0, 0.0, 0.0],
        });
    }
    Ok(programs)
}

fn resolved_script_properties(
    binding: &Map<String, Value>,
    project_properties: &Map<String, Value>,
) -> String {
    let Some(mut properties) = binding
        .get("scriptproperties")
        .and_then(Value::as_object)
        .cloned()
    else {
        return "{}".to_owned();
    };
    for property in properties.values_mut() {
        let Some(bound) = property.as_object_mut() else {
            continue;
        };
        let Some(user) = bound.get("user").and_then(Value::as_str) else {
            continue;
        };
        let Some(default) = project_properties
            .get(user)
            .and_then(|definition| definition.get("value"))
        else {
            continue;
        };
        bound.insert("value".to_owned(), default.clone());
    }
    compact_json(&Value::Object(properties))
}

fn subscriptions(
    analysis: &SceneScriptAnalysis,
    target: SceneScriptTarget,
) -> SceneScriptSubscriptions {
    let local_time_only = target == SceneScriptTarget::Text
        && analysis.uses_local_time
        && !analysis.uses_runtime
        && !analysis.uses_frame_time
        && !analysis.uses_audio;
    let mut subscriptions = if !analysis.exports_update {
        SceneScriptSubscriptions::NONE
    } else if local_time_only {
        SceneScriptSubscriptions::LOCAL_TIME
    } else {
        SceneScriptSubscriptions::FRAME
    };
    if analysis.exports_init {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::INITIALIZE);
    }
    if analysis.uses_audio {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::AUDIO);
    }
    if analysis.uses_pointer {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::POINTER);
    }
    if analysis.uses_local_time {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::LOCAL_TIME);
    }
    if analysis.handles_media {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::MEDIA);
    }
    if analysis.handles_user_properties {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::USER_PROPERTY);
    }
    subscriptions
}

fn initial_numeric(
    binding: &serde_json::Map<String, Value>,
    target: SceneScriptTarget,
) -> [f32; 4] {
    let value = Value::Object(binding.clone());
    if target.is_vector() {
        let mut vector = parse_vec3(Some(&value)).unwrap_or_default();
        if target == SceneScriptTarget::Angles {
            vector.x = vector.x.to_degrees();
            vector.y = vector.y.to_degrees();
            vector.z = vector.z.to_degrees();
        }
        return [vector.x, vector.y, vector.z, 0.0];
    }
    match target {
        SceneScriptTarget::Alpha => [value_f32(Some(&value)).unwrap_or(1.0), 0.0, 0.0, 0.0],
        SceneScriptTarget::Visible => [
            f32::from(bound_bool(Some(&value)).unwrap_or(true)),
            0.0,
            0.0,
            0.0,
        ],
        _ => [0.0; 4],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_module_source_and_typed_host_binding_without_js_parsing() {
        let object = json!({
            "origin": {
                "value": "10 20 30",
                "script": "export function update(value) { value.y += engine.runtime; return value; }",
                "scriptproperties": {"speed": {"value": 2}}
            }
        });
        let programs = object_script_programs(7, &object, None, &Map::new()).expect("programs");
        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].target, SceneScriptTarget::Origin);
        assert_eq!(programs[0].initial_numeric, [10.0, 20.0, 30.0, 0.0]);
        assert!(programs[0].source.starts_with("export function"));
    }

    #[test]
    fn angle_script_enters_runtime_in_degrees() {
        let object = json!({
            "angles": {
                "value": "0 0 -0.610865238",
                "script": "export function update(value) { value.z += 10; return value; }"
            }
        });
        let programs = object_script_programs(7, &object, None, &Map::new()).expect("programs");
        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].target, SceneScriptTarget::Angles);
        assert!((programs[0].initial_numeric[2] + 35.0).abs() < 0.000_01);
    }

    #[test]
    fn init_only_module_subscribes_only_to_initialization() {
        let object = json!({
            "alpha": {
                "value": 0.5,
                "script": "export function init(value) { return value * 0.5; }"
            }
        });
        let programs = object_script_programs(7, &object, None, &Map::new()).expect("programs");
        assert_eq!(
            programs[0].subscriptions,
            SceneScriptSubscriptions::INITIALIZE
        );
    }

    #[test]
    fn metadata_only_script_properties_do_not_create_runtime_programs() {
        let object = json!({
            "visible": {
                "value": false,
                "script": "export var scriptProperties = createScriptProperties().addText({name: 'author', value: 'link'}).finish();"
            }
        });
        assert!(
            object_script_programs(0, &object, None, &Map::new())
                .expect("programs")
                .is_empty()
        );
    }

    #[test]
    fn scene_api_side_effect_module_stays_static_until_typed_scene_mutation_exists() {
        let object = json!({
            "visible": {
                "value": true,
                "script": "export function update(value) { thisScene.getLayer('body').visible = value; return value; }"
            }
        });
        assert!(
            object_script_programs(0, &object, None, &Map::new())
                .expect("programs")
                .is_empty()
        );
    }

    #[test]
    fn local_time_text_does_not_subscribe_to_every_frame() {
        let object = json!({
            "text": {
                "value": "clock",
                "script": "export function update() { return String(new Date().getMinutes()); }"
            }
        });
        let programs =
            object_script_programs(7, &object, Some("clock"), &Map::new()).expect("programs");
        assert_eq!(
            programs[0].subscriptions,
            SceneScriptSubscriptions::LOCAL_TIME
        );
    }

    #[test]
    fn project_user_default_overrides_script_property_placeholder() {
        let object = json!({
            "origin": {
                "value": "1910 1366 0",
                "script": "export function update(value) { value.x = scriptProperties.newSlider; return value; }",
                "scriptproperties": {
                    "newSlider": {"user": "newproperty1", "value": 50}
                }
            }
        });
        let project = Map::from_iter([(
            "newproperty1".to_owned(),
            json!({"type": "slider", "value": 2000}),
        )]);
        let programs = object_script_programs(7, &object, None, &project).expect("programs");
        assert_eq!(
            serde_json::from_str::<Value>(&programs[0].properties_json).expect("properties")["newSlider"]
                ["value"],
            2000
        );
    }
}
