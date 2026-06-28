use crate::core::manifest::PropertySpec;
use crate::core::{SceneDocument, SceneNode, SceneSystemStatus};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn scene_render_property_value(
    property: &str,
    render_properties: Option<&BTreeMap<String, Value>>,
) -> Option<f64> {
    render_properties
        .and_then(|properties| properties.get(property))
        .and_then(scene_runtime_number)
}

pub(super) fn scene_property_value(
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

pub(super) fn scene_runtime_property_value(
    document: &SceneDocument,
    time_ms: u64,
    property: &str,
) -> Option<f64> {
    scene_controller_property_value(document, time_ms, property)
        .or_else(|| scene_audio_response_property_value(document, time_ms, property))
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

pub(super) fn scene_audio_response_property_value(
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
