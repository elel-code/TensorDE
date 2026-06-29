use crate::core::manifest::PropertySpec;
use crate::core::{SceneDocument, SceneNode, SceneSystemStatus};
use jiff::{Timestamp, Zoned, tz::TimeZone};
use serde_json::Value;
use std::collections::BTreeMap;

pub(in crate::renderer) fn scene_render_property_value(
    property: &str,
    render_properties: Option<&BTreeMap<String, Value>>,
) -> Option<f64> {
    render_properties
        .and_then(|properties| properties.get(property))
        .and_then(scene_runtime_number)
}

pub(in crate::renderer) fn scene_property_value(
    property: &str,
    render_properties: Option<&BTreeMap<String, Value>>,
    manifest_properties: &BTreeMap<String, PropertySpec>,
) -> Option<f64> {
    scene_render_property_value(property, render_properties).or_else(|| {
        manifest_properties
            .get(property)
            .and_then(scene_manifest_property_default_number)
    })
}

pub(in crate::renderer) fn scene_input_properties_from_sources(
    document: &SceneDocument,
    render_properties: Option<&BTreeMap<String, Value>>,
    manifest_properties: Option<&BTreeMap<String, PropertySpec>>,
) -> BTreeMap<String, Value> {
    let mut properties = BTreeMap::new();
    for binding in &document.property_bindings {
        if let Some(value) = render_properties.and_then(|source| source.get(&binding.property)) {
            properties.insert(binding.property.clone(), value.clone());
            continue;
        }
        if let Some(default) = manifest_properties
            .and_then(|source| source.get(&binding.property))
            .and_then(scene_manifest_property_default_value)
        {
            properties.insert(binding.property.clone(), default);
            continue;
        }
        if let Some(default) = document
            .properties
            .get(&binding.property)
            .and_then(scene_document_property_default_value)
        {
            properties.insert(binding.property.clone(), default);
        }
    }
    for node in &document.nodes {
        scene_collect_bound_input_properties(
            node,
            &document.properties,
            render_properties,
            manifest_properties,
            &mut properties,
        );
        scene_collect_audio_condition_properties(
            node,
            &document.properties,
            render_properties,
            manifest_properties,
            &mut properties,
        );
    }

    if let Some(render_properties) = render_properties {
        for property in ["scene.parallax.x", "scene.parallax.y"] {
            if let Some(value) = render_properties.get(property)
                && scene_runtime_number(value).is_some()
            {
                properties.insert(property.to_owned(), value.clone());
            }
        }
        for node in &document.nodes {
            scene_collect_controller_input_properties(node, render_properties, &mut properties);
        }
    }

    properties
}

fn scene_collect_bound_input_properties(
    node: &SceneNode,
    document_properties: &BTreeMap<String, Value>,
    render_properties: Option<&BTreeMap<String, Value>>,
    manifest_properties: Option<&BTreeMap<String, PropertySpec>>,
    output: &mut BTreeMap<String, Value>,
) {
    for binding_key in [
        "visibility_condition",
        "color_binding",
        "stroke_color_binding",
        "text_binding",
    ] {
        if let Some(property) = node
            .properties
            .get(binding_key)
            .and_then(Value::as_object)
            .and_then(|binding| binding.get("property"))
            .and_then(Value::as_str)
        {
            scene_collect_input_property(
                property,
                document_properties,
                render_properties,
                manifest_properties,
                output,
            );
        }
    }
    for child in &node.children {
        scene_collect_bound_input_properties(
            child,
            document_properties,
            render_properties,
            manifest_properties,
            output,
        );
    }
}

fn scene_collect_audio_condition_properties(
    node: &SceneNode,
    document_properties: &BTreeMap<String, Value>,
    render_properties: Option<&BTreeMap<String, Value>>,
    manifest_properties: Option<&BTreeMap<String, PropertySpec>>,
    output: &mut BTreeMap<String, Value>,
) {
    for cue in &node.audio {
        for condition in &cue.active_conditions {
            scene_collect_input_property(
                condition.property.trim(),
                document_properties,
                render_properties,
                manifest_properties,
                output,
            );
        }
    }
    for child in &node.children {
        scene_collect_audio_condition_properties(
            child,
            document_properties,
            render_properties,
            manifest_properties,
            output,
        );
    }
}

fn scene_collect_input_property(
    property: &str,
    document_properties: &BTreeMap<String, Value>,
    render_properties: Option<&BTreeMap<String, Value>>,
    manifest_properties: Option<&BTreeMap<String, PropertySpec>>,
    output: &mut BTreeMap<String, Value>,
) {
    if property.is_empty() || output.contains_key(property) {
        return;
    }
    if let Some(value) = render_properties.and_then(|source| source.get(property)) {
        output.insert(property.to_owned(), value.clone());
        return;
    }
    if let Some(default) = manifest_properties
        .and_then(|source| source.get(property))
        .and_then(scene_manifest_property_default_value)
    {
        output.insert(property.to_owned(), default);
        return;
    }
    if let Some(default) = document_properties
        .get(property)
        .and_then(scene_document_property_default_value)
    {
        output.insert(property.to_owned(), default);
    }
}

fn scene_collect_controller_input_properties(
    node: &SceneNode,
    render_properties: &BTreeMap<String, Value>,
    output: &mut BTreeMap<String, Value>,
) {
    if let Some(controller) = node.properties.get("controller").and_then(Value::as_object)
        && controller
            .get("runtime")
            .and_then(Value::as_str)
            .is_some_and(|runtime| runtime == "native")
        && let Some(property) = controller.get("property").and_then(Value::as_str)
    {
        let property = property.trim();
        for alias in scene_controller_input_aliases(controller) {
            if let Some(value) = render_properties.get(&alias)
                && scene_runtime_number(value).is_some()
            {
                output.insert(property.to_owned(), value.clone());
                break;
            }
        }
    }
    for child in &node.children {
        scene_collect_controller_input_properties(child, render_properties, output);
    }
}

fn scene_controller_input_aliases(controller: &serde_json::Map<String, Value>) -> Vec<String> {
    controller
        .get("input_aliases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|alias| !alias.is_empty())
        .map(str::to_owned)
        .collect()
}

fn scene_runtime_property_value(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    scene_controller_property_value(document, time_ms, property)
        .or_else(|| scene_audio_response_property_value(document, time_ms, property))
}

pub(in crate::renderer) fn scene_runtime_property_value_with_inputs(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
    input_properties: &BTreeMap<String, Value>,
) -> Option<f64> {
    scene_runtime_input_property_value(input_properties, property)
        .or_else(|| scene_runtime_property_value(document, time_ms, property))
}

pub(in crate::renderer) fn scene_runtime_text_property_value_with_inputs(
    property: &str,
    input_properties: &BTreeMap<String, Value>,
) -> Option<String> {
    let property = property.trim();
    if let Some(value) = input_properties.get(property).and_then(scene_runtime_text) {
        return Some(value);
    }
    match property {
        "scene.clock.local.time.hm24" => Some(
            scene_runtime_clock_zoned(input_properties)?
                .strftime("%H:%M")
                .to_string(),
        ),
        "scene.clock.local.time.hms24" => Some(
            scene_runtime_clock_zoned(input_properties)?
                .strftime("%H:%M:%S")
                .to_string(),
        ),
        "scene.clock.local.time.hm12" => Some(
            scene_runtime_clock_zoned(input_properties)?
                .strftime("%I:%M")
                .to_string(),
        ),
        "scene.clock.local.time.hms12" => Some(
            scene_runtime_clock_zoned(input_properties)?
                .strftime("%I:%M:%S")
                .to_string(),
        ),
        "scene.clock.local.we-date.vertical-month-abbrev" => {
            let zoned = scene_runtime_clock_zoned(input_properties)?;
            let day = scene_runtime_vertical_text(&zoned.strftime("%d").to_string());
            let month =
                scene_runtime_vertical_text(&zoned.strftime("%b").to_string().to_uppercase());
            let year = scene_runtime_vertical_text(&zoned.strftime("%Y").to_string());
            Some(format!("{day}\n\n{month}\n\n{year}"))
        }
        "scene.clock.local.we-day.vertical-weekday-abbrev-upper" => {
            let zoned = scene_runtime_clock_zoned(input_properties)?;
            Some(scene_runtime_vertical_text(
                &zoned.strftime("%a").to_string().to_uppercase(),
            ))
        }
        _ => None,
    }
}

fn scene_runtime_clock_zoned(input_properties: &BTreeMap<String, Value>) -> Option<Zoned> {
    let timestamp = input_properties
        .get("scene.clock.local.unix_ms")
        .or_else(|| input_properties.get("scene.clock.unix_ms"))
        .and_then(scene_runtime_number)
        .and_then(|value| Timestamp::from_millisecond(value.round() as i64).ok())
        .unwrap_or_else(Timestamp::now);
    Some(timestamp.to_zoned(TimeZone::system()))
}

fn scene_runtime_vertical_text(value: &str) -> String {
    value
        .chars()
        .map(|character| character.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn scene_runtime_input_property_value(
    input_properties: &BTreeMap<String, Value>,
    property: &str,
) -> Option<f64> {
    input_properties
        .get(property.trim())
        .and_then(scene_runtime_number)
}

fn scene_controller_property_value(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    let property = property.trim();
    if !property.starts_with("scene.controller.") {
        return None;
    }
    document
        .nodes
        .iter()
        .find_map(|node| scene_node_controller_property_value(node, time_ms, property))
}

fn scene_node_controller_property_value(
    node: &SceneNode,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    if let Some(controller) = node.properties.get("controller").and_then(Value::as_object)
        && controller
            .get("runtime")
            .and_then(Value::as_str)
            .is_some_and(|runtime| runtime == "native")
        && controller
            .get("property")
            .and_then(Value::as_str)
            .is_some_and(|controller_property| controller_property.trim() == property)
    {
        match controller.get("kind").and_then(Value::as_str)? {
            "idle-video-switch" => {
                let inactive_sec = controller
                    .get("mouse_inactive_sec")
                    .and_then(scene_runtime_config_number)?;
                let inactive_ms = (inactive_sec.max(0.0) * 1000.0).round();
                if (time_ms as f64) < inactive_ms {
                    return Some(0.0);
                }
                let fade_in_ms = controller
                    .get("fade_in_duration")
                    .and_then(scene_runtime_config_number)
                    .unwrap_or(0.0)
                    .max(0.0)
                    * 1000.0;
                if fade_in_ms <= 0.0 {
                    return Some(1.0);
                }
                return Some(((time_ms as f64 - inactive_ms) / fade_in_ms).clamp(0.0, 1.0));
            }
            "click-video-switch" => return Some(0.0),
            _ => return None,
        }
    }
    node.children
        .iter()
        .find_map(|child| scene_node_controller_property_value(child, time_ms, property))
}

pub(in crate::renderer) fn scene_audio_response_property_value(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    if document.systems.audio_response != SceneSystemStatus::Ready {
        return None;
    }
    let property = property
        .trim()
        .replace(['-', ' ', '/'], "_")
        .to_ascii_lowercase();
    if property.is_empty() {
        return None;
    }
    let band = if property == "audio"
        || property == "audio_level"
        || property == "audio_response"
        || property.ends_with("_audio")
    {
        "full"
    } else if property.contains("bass") || property.contains("low") {
        "bass"
    } else if property.contains("mid") || property.contains("vocal") {
        "mid"
    } else if property.contains("treble") || property.contains("high") {
        "treble"
    } else if property.contains("spectrum") || property.contains("frequency") {
        "spectrum"
    } else {
        return None;
    };
    let seconds = time_ms as f64 / 1000.0;
    let (frequency, phase, floor, gain) = match band {
        "bass" => (1.25, 0.0, 0.12, 0.88),
        "mid" => (2.5, 0.7, 0.08, 0.78),
        "treble" => (5.0, 1.3, 0.04, 0.72),
        "spectrum" => (
            3.5,
            scene_audio_response_spectrum_phase(&property),
            0.05,
            0.8,
        ),
        _ => (1.75, 0.35, 0.1, 0.82),
    };
    let wave = (seconds.mul_add(frequency * std::f64::consts::TAU, phase)).sin() * 0.5 + 0.5;
    Some((floor + wave.powf(1.35) * gain).clamp(0.0, 1.0))
}

fn scene_audio_response_spectrum_phase(property: &str) -> f64 {
    let bin = property
        .rsplit('_')
        .find_map(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    f64::from(bin % 32) * 0.196_349_540_849_362_07
}

fn scene_runtime_config_number(value: &Value) -> Option<f64> {
    scene_runtime_number(value.get("value").unwrap_or(value))
}

fn scene_runtime_number(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Bool(value) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(value) => value.as_f64()?,
        Value::String(value) => value.parse::<f64>().ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

fn scene_runtime_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Object(object) => object.get("value").and_then(scene_runtime_text),
        Value::Array(_) | Value::Null => None,
    }
}

fn scene_manifest_property_default_number(property: &PropertySpec) -> Option<f64> {
    let number = match property {
        PropertySpec::Bool { default } => {
            if (*default)? {
                1.0
            } else {
                0.0
            }
        }
        PropertySpec::Number { default } | PropertySpec::Range { default, .. } => (*default)?,
        PropertySpec::Choice { default, .. } => default.as_deref()?.parse::<f64>().ok()?,
        PropertySpec::Color { .. } | PropertySpec::Text { .. } | PropertySpec::File { .. } => {
            return None;
        }
    };
    number.is_finite().then_some(number)
}

fn scene_manifest_property_default_value(property: &PropertySpec) -> Option<Value> {
    match property {
        PropertySpec::Bool { default } => default.map(Value::from),
        PropertySpec::Number { default } | PropertySpec::Range { default, .. } => {
            default.map(Value::from)
        }
        PropertySpec::Choice { default, .. }
        | PropertySpec::Color { default }
        | PropertySpec::Text { default } => default.clone().map(Value::from),
        PropertySpec::File { default } => default
            .as_ref()
            .map(|path| Value::String(path.as_str().to_owned())),
    }
}

fn scene_document_property_default_value(property: &Value) -> Option<Value> {
    property.as_object()?.get("default").cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scene_clock_text_properties_format_fixed_local_time() {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "scene.clock.local.unix_ms".to_owned(),
            Value::from(1_720_646_365_000_i64),
        );

        let hm =
            scene_runtime_text_property_value_with_inputs("scene.clock.local.time.hm24", &inputs)
                .unwrap();
        assert_eq!(hm.len(), 5);
        assert_eq!(hm.as_bytes()[2], b':');

        let date = scene_runtime_text_property_value_with_inputs(
            "scene.clock.local.we-date.vertical-month-abbrev",
            &inputs,
        )
        .unwrap();
        assert!(date.contains("\n\n"));
        assert!(date.lines().any(|line| line.len() == 1));

        let day = scene_runtime_text_property_value_with_inputs(
            "scene.clock.local.we-day.vertical-weekday-abbrev-upper",
            &inputs,
        )
        .unwrap();
        assert!(
            day.chars()
                .all(|character| { character == '\n' || character.is_ascii_uppercase() })
        );
    }

    #[test]
    fn scene_input_properties_use_document_defaults_without_manifest() {
        let document: SceneDocument = serde_json::from_value(json!({
            "version": 1,
            "properties": {
                "newproperty1": {
                    "type": "range",
                    "min": 1100.0,
                    "max": 2500.0,
                    "default": 2000.0
                },
                "newproperty28": {
                    "type": "bool",
                    "default": true
                },
                "newproperty": {
                    "type": "choice",
                    "choices": ["1", "2"],
                    "default": "1"
                }
            },
            "nodes": [
                {
                    "id": "node-character",
                    "type": "group"
                },
                {
                    "id": "node-default-theme",
                    "type": "group",
                    "properties": {
                        "visibility_condition": {
                            "runtime": "wallpaper-engine-user-condition",
                            "property": "newproperty",
                            "condition": "1",
                            "default_visible": true,
                            "authored_value": true
                        }
                    }
                }
            ],
            "property_bindings": [
                {
                    "property": "newproperty1",
                    "target_node": "node-character",
                    "target": "x"
                },
                {
                    "property": "newproperty28",
                    "target_node": "node-character",
                    "target": "opacity"
                }
            ]
        }))
        .unwrap();

        let inputs = scene_input_properties_from_sources(&document, None, None);

        assert_eq!(inputs.get("newproperty1"), Some(&json!(2000.0)));
        assert_eq!(inputs.get("newproperty28"), Some(&json!(true)));
        assert_eq!(inputs.get("newproperty"), Some(&json!("1")));
    }

    #[test]
    fn scene_input_properties_collect_user_color_binding_defaults() {
        let document: SceneDocument = serde_json::from_value(json!({
            "version": 1,
            "properties": {
                "newproperty5": {
                    "type": "color",
                    "default": "#003ca4"
                }
            },
            "nodes": [
                {
                    "id": "node-slider",
                    "type": "rectangle",
                    "properties": {
                        "color_binding": {
                            "runtime": "wallpaper-engine-user-color",
                            "property": "newproperty5",
                            "default": "#003ca4"
                        }
                    }
                }
            ]
        }))
        .unwrap();

        let inputs = scene_input_properties_from_sources(&document, None, None);

        assert_eq!(inputs.get("newproperty5"), Some(&json!("#003ca4")));
    }
}
