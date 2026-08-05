//! Canonical SceneScript module extraction without parsing JavaScript expressions.

use std::collections::BTreeSet;

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
        if !has_runtime_entrypoint(&analysis) {
            continue;
        }
        let properties_json = resolved_script_properties(binding, project_properties);
        programs.push(WeIrScriptProgram {
            object,
            target,
            selector: 0,
            updates_target_value: analysis.exports_update,
            source: source.to_owned(),
            properties_json,
            initial_text: initial_text.unwrap_or_default().to_owned(),
            subscriptions: subscriptions(&analysis, target, Some(binding)),
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct SceneEffectVisibilityMutationPolicy {
    all_effects: bool,
    literal_targets: BTreeSet<(String, String)>,
}

impl SceneEffectVisibilityMutationPolicy {
    pub(super) fn may_mutate(&self, layer_name: &str, effect_name: &str) -> bool {
        self.all_effects
            || self
                .literal_targets
                .contains(&(layer_name.to_owned(), effect_name.to_owned()))
    }
}

pub(super) fn scene_effect_visibility_mutation_policy(
    scene: &Value,
) -> SceneEffectVisibilityMutationPolicy {
    fn visit(value: &Value, policy: &mut SceneEffectVisibilityMutationPolicy) {
        if policy.all_effects {
            return;
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    visit(value, policy);
                }
            }
            Value::Object(object) => {
                if let Some(source) = object.get("script").and_then(Value::as_str) {
                    match analyze_scene_script(source) {
                        Ok(analysis) if has_runtime_entrypoint(&analysis) => {
                            if !analysis.imports.is_empty()
                                || analysis.has_unresolved_effect_visibility_target
                            {
                                policy.all_effects = true;
                                policy.literal_targets.clear();
                                return;
                            }
                            policy.literal_targets.extend(
                                analysis
                                    .effect_visibility_targets
                                    .into_iter()
                                    .map(|target| (target.layer_name, target.effect_name)),
                            );
                        }
                        Err(_) => {
                            policy.all_effects = true;
                            policy.literal_targets.clear();
                            return;
                        }
                        _ => {}
                    }
                }
                for value in object.values() {
                    visit(value, policy);
                }
            }
            _ => {}
        }
    }

    let mut policy = SceneEffectVisibilityMutationPolicy::default();
    visit(scene, &mut policy);
    policy
}

fn has_runtime_entrypoint(analysis: &SceneScriptAnalysis) -> bool {
    analysis.exports_update
        || analysis.exports_init
        || analysis.handles_media
        || analysis.handles_user_properties
        || analysis.handles_pointer_click
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
            selector: 0,
            updates_target_value: analysis.exports_update,
            source: source.to_owned(),
            properties_json: binding
                .as_object()
                .map(|binding| resolved_script_properties(binding, project_properties))
                .unwrap_or_else(|| "{}".to_owned()),
            initial_text: String::new(),
            subscriptions: subscriptions(
                &analysis,
                SceneScriptTarget::TechCircleSectorWidth,
                binding.as_object(),
            ),
            initial_numeric: [value_f32(Some(binding)).unwrap_or(0.0), 0.0, 0.0, 0.0],
        });
    }
    Ok(programs)
}

pub(super) fn material_scalar_script_programs(
    object: u32,
    constant_start: u32,
    constants: &[crate::convert::we_ingest::ir::WeIrMaterialConstant],
    instance_pass: Option<&Value>,
    project_properties: &Map<String, Value>,
) -> Result<Vec<WeIrScriptProgram>, String> {
    const SPECIALIZED_TECH_CIRCLE_SECTOR_WIDTH: &str = "ui_editor_properties_5_sector_1_width";
    let Some(authored_constants) = instance_pass
        .and_then(|pass| pass.get("constantshadervalues"))
        .and_then(Value::as_object)
    else {
        return Ok(Vec::new());
    };
    let mut programs = Vec::new();
    for (name, binding) in authored_constants {
        if name == SPECIALIZED_TECH_CIRCLE_SECTOR_WIDTH {
            continue;
        }
        let Some(binding_object) = binding.as_object() else {
            continue;
        };
        let Some(source) = binding_object.get("script").and_then(Value::as_str) else {
            continue;
        };
        let analysis = analyze_scene_script(source).map_err(|error| error.to_string())?;
        if !has_runtime_entrypoint(&analysis) {
            continue;
        }
        let local_index = constants
            .iter()
            .position(|constant| constant.name == *name)
            .ok_or_else(|| format!("scripted material constant {name:?} was not merged"))?;
        let initial = value_f32(Some(binding))
            .ok_or_else(|| format!("scripted material constant {name:?} is not a scalar"))?;
        programs.push(WeIrScriptProgram {
            object,
            target: SceneScriptTarget::MaterialScalar,
            selector: constant_start + local_index as u32,
            updates_target_value: analysis.exports_update,
            source: source.to_owned(),
            properties_json: resolved_script_properties(binding_object, project_properties),
            initial_text: String::new(),
            subscriptions: subscriptions(
                &analysis,
                SceneScriptTarget::MaterialScalar,
                Some(binding_object),
            ),
            initial_numeric: [initial, 0.0, 0.0, 0.0],
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
    binding: Option<&Map<String, Value>>,
) -> SceneScriptSubscriptions {
    let local_time_only = target == SceneScriptTarget::Text
        && analysis.uses_local_time
        && !analysis.uses_runtime
        && !analysis.uses_frame_time
        && !analysis.uses_audio;
    let local_time = local_time_subscription(analysis, binding);
    let mut subscriptions = if !analysis.exports_update {
        SceneScriptSubscriptions::NONE
    } else if local_time_only {
        local_time
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
    if analysis.handles_pointer_click {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::POINTER_CLICK);
    }
    if analysis.uses_local_time {
        subscriptions = subscriptions.union(local_time);
    }
    if analysis.handles_media {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::MEDIA);
    }
    if analysis.handles_user_properties {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::USER_PROPERTY);
    }
    subscriptions
}

fn local_time_subscription(
    analysis: &SceneScriptAnalysis,
    binding: Option<&Map<String, Value>>,
) -> SceneScriptSubscriptions {
    let format = binding
        .and_then(|binding| binding.get("scriptproperties"))
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("format"))
        .and_then(Value::as_str);
    if let Some(format) = format {
        let mut escaped = false;
        let mut seconds = false;
        for character in format.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            if character == '\\' {
                escaped = true;
            } else if character == 'S' {
                return SceneScriptSubscriptions::FRAME;
            } else if character == 's' {
                seconds = true;
            }
        }
        return if seconds {
            SceneScriptSubscriptions::LOCAL_TIME_SECOND
        } else {
            SceneScriptSubscriptions::LOCAL_TIME
        };
    }
    if analysis.uses_local_time_subseconds {
        SceneScriptSubscriptions::FRAME
    } else if analysis.uses_local_time_seconds {
        SceneScriptSubscriptions::LOCAL_TIME_SECOND
    } else {
        SceneScriptSubscriptions::LOCAL_TIME
    }
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
    fn only_update_exports_can_replace_the_target_value() {
        let font_property = json!({
            "text": {
                "value": "DREAMLIKE",
                "script": "export function applyUserProperties() { thisLayer.font = 'font.ttf'; }"
            }
        });
        let clock = json!({
            "text": {
                "value": "23",
                "script": "export function update() { return new Date().getSeconds().toString(); }"
            }
        });

        let font_programs =
            object_script_programs(0, &font_property, Some("DREAMLIKE"), &Map::new())
                .expect("font program");
        let clock_programs =
            object_script_programs(1, &clock, Some("23"), &Map::new()).expect("clock program");

        assert_eq!(font_programs.len(), 1);
        assert!(!font_programs[0].updates_target_value);
        assert_eq!(clock_programs.len(), 1);
        assert!(clock_programs[0].updates_target_value);
    }

    #[test]
    fn typed_scene_effect_mutation_and_cursor_click_enter_runtime() {
        let object = json!({
            "visible": {
                "value": true,
                "script": "export function update(value) { thisScene.getLayer('body').getEffect('armor').visible = value; return value; } export function cursorClick(event) {}"
            }
        });
        let programs = object_script_programs(0, &object, None, &Map::new()).expect("programs");
        assert_eq!(programs.len(), 1);
        assert!(
            programs[0]
                .subscriptions
                .contains(SceneScriptSubscriptions::POINTER_CLICK)
        );
    }

    #[test]
    fn material_scalar_script_targets_the_merged_constant_index() {
        let pass = json!({
            "constantshadervalues": {
                "static": 1,
                "缺口大小": {
                    "value": 225,
                    "script": "export function update(value) { return value + engine.runtime; }"
                }
            }
        });
        let constants = vec![
            crate::convert::we_ingest::ir::WeIrMaterialConstant {
                name: "static".to_owned(),
                value_json: "1".to_owned(),
            },
            crate::convert::we_ingest::ir::WeIrMaterialConstant {
                name: "缺口大小".to_owned(),
                value_json: "{\"value\":225}".to_owned(),
            },
        ];
        let programs = material_scalar_script_programs(7, 40, &constants, Some(&pass), &Map::new())
            .expect("material script");
        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].target, SceneScriptTarget::MaterialScalar);
        assert_eq!(programs[0].selector, 41);
        assert_eq!(programs[0].initial_numeric[0], 225.0);
    }

    #[test]
    fn scene_effect_visibility_analysis_targets_literals_and_keeps_imports_conservative() {
        let direct = json!({
            "objects": [{
                "visible": {
                    "value": true,
                    "script": "export function update(value) { thisScene.getLayer('body').getEffect('shine').visible = value; return value; }"
                }
            }]
        });
        let direct = scene_effect_visibility_mutation_policy(&direct);
        assert!(direct.may_mutate("body", "shine"));
        assert!(!direct.may_mutate("body", "other"));
        assert!(!direct.may_mutate("other", "shine"));

        let imported = json!({
            "objects": [{
                "visible": {
                    "value": true,
                    "script": "import { updateEffect } from './effect.js'; export function update(value) { updateEffect(value); return value; }"
                }
            }]
        });
        let imported = scene_effect_visibility_mutation_policy(&imported);
        assert!(imported.may_mutate("body", "shine"));
        assert!(imported.may_mutate("other", "anything"));

        let layer_only = json!({
            "objects": [{
                "visible": {
                    "value": true,
                    "script": "export function update(value) { thisScene.getLayer('notice').visible = value; return value; }"
                }
            }]
        });
        let layer_only = scene_effect_visibility_mutation_policy(&layer_only);
        assert!(!layer_only.may_mutate("notice", "shine"));
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
    fn formatted_time_text_subscribes_at_authored_precision() {
        let object = json!({
            "text": {
                "value": "23",
                "script": "export function update() { return new Date().getSeconds(); }",
                "scriptproperties": {"format": "ss"}
            }
        });
        let programs =
            object_script_programs(7, &object, Some("23"), &Map::new()).expect("programs");
        assert_eq!(
            programs[0].subscriptions,
            SceneScriptSubscriptions::LOCAL_TIME_SECOND
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
