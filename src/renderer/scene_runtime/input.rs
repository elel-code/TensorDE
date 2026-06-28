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
        if let Some(value) = render_properties.and_then(|source| source.get(&binding.property))
            && scene_runtime_number(value).is_some()
        {
            properties.insert(binding.property.clone(), value.clone());
            continue;
        }
        if let Some(default) = manifest_properties
            .and_then(|source| source.get(&binding.property))
            .and_then(scene_manifest_property_default_number)
        {
            properties.insert(binding.property.clone(), Value::from(default));
        }
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
    let zoned = scene_runtime_clock_zoned(input_properties)?;
    match property {
        "scene.clock.local.time.hm24" => Some(zoned.strftime("%H:%M").to_string()),
        "scene.clock.local.time.hms24" => Some(zoned.strftime("%H:%M:%S").to_string()),
        "scene.clock.local.time.hm12" => Some(zoned.strftime("%I:%M").to_string()),
        "scene.clock.local.time.hms12" => Some(zoned.strftime("%I:%M:%S").to_string()),
        "scene.clock.local.we-date.vertical-month-abbrev" => {
            let day = scene_runtime_vertical_text(&zoned.strftime("%d").to_string());
            let month =
                scene_runtime_vertical_text(&zoned.strftime("%b").to_string().to_uppercase());
            let year = scene_runtime_vertical_text(&zoned.strftime("%Y").to_string());
            Some(format!("{day}\n\n{month}\n\n{year}"))
        }
        "scene.clock.local.we-day.vertical-weekday-abbrev-upper" => Some(
            scene_runtime_vertical_text(&zoned.strftime("%a").to_string().to_uppercase()),
        ),
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
        PropertySpec::Choice { .. }
        | PropertySpec::Color { .. }
        | PropertySpec::Text { .. }
        | PropertySpec::File { .. } => return None,
    };
    number.is_finite().then_some(number)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
