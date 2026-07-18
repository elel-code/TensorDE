//! Canonical SceneScript module extraction without parsing JavaScript expressions.

use serde_json::Value;

use crate::convert::we_ingest::ir::WeIrScriptProgram;
use crate::engine::scene::{SceneScriptSubscriptions, SceneScriptTarget};

use super::json_value::{bound_bool, compact_json, parse_vec3, value_f32};

pub(super) fn object_script_programs(
    object: u32,
    value: &Value,
    initial_text: Option<&str>,
) -> Vec<WeIrScriptProgram> {
    let mut programs = Vec::new();
    for (property, target) in [
        ("origin", SceneScriptTarget::Origin),
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
        let properties_json = binding
            .get("scriptproperties")
            .map(compact_json)
            .unwrap_or_else(|| "{}".to_owned());
        programs.push(WeIrScriptProgram {
            object,
            target,
            source: source.to_owned(),
            properties_json,
            initial_text: initial_text.unwrap_or_default().to_owned(),
            subscriptions: subscriptions(source, target),
            initial_numeric: initial_numeric(binding, target),
        });
    }
    programs
}

fn subscriptions(source: &str, target: SceneScriptTarget) -> SceneScriptSubscriptions {
    let local_time_only = target == SceneScriptTarget::Text
        && (source.contains("new Date") || source.contains("Date.now"))
        && !source.contains("engine.runtime")
        && !source.contains("registerAudioBuffers");
    let mut subscriptions = if local_time_only {
        SceneScriptSubscriptions::LOCAL_TIME
    } else {
        SceneScriptSubscriptions::FRAME
    };
    if source.contains("registerAudioBuffers") || source.contains("audioBuffer") {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::AUDIO);
    }
    if source.contains("pointer") || source.contains("cursor") || source.contains("mouse") {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::POINTER);
    }
    if source.contains("new Date") || source.contains("Date.now") {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::LOCAL_TIME);
    }
    if source.contains("mediaPlaybackChanged") {
        subscriptions = subscriptions.union(SceneScriptSubscriptions::MEDIA);
    }
    if source.contains("applyUserProperties") || source.contains("scriptProperties") {
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
        let vector = parse_vec3(Some(&value)).unwrap_or_default();
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
        let programs = object_script_programs(7, &object, None);
        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].target, SceneScriptTarget::Origin);
        assert_eq!(programs[0].initial_numeric, [10.0, 20.0, 30.0, 0.0]);
        assert!(programs[0].source.starts_with("export function"));
    }
}
