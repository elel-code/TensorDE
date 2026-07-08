//! Runtime property value coercion and scalar conversion for `.gscn` metadata.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`

use std::collections::BTreeMap;

use serde_json::Value;

pub(in crate::renderer::scene_binary) fn binary_scene_property_number(
    value: &Value,
) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    if let Some(value) = value.as_bool() {
        return Some(if value { 1.0 } else { 0.0 });
    }
    None
}

pub(super) fn binary_scene_property_text(value: &Value) -> Option<&str> {
    value.as_str()
}

pub(super) fn binary_scene_runtime_default_properties(
    properties: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    properties
        .iter()
        .filter_map(|(name, spec)| {
            let value = spec
                .as_object()
                .and_then(|spec| spec.get("default"))
                .cloned()
                .unwrap_or_else(|| spec.clone());
            (!value.is_null()).then(|| (name.clone(), value))
        })
        .collect()
}

pub(super) fn binary_scene_coerce_runtime_property_override(
    default: Option<&Value>,
    value: Value,
) -> Value {
    if default.is_some_and(Value::is_string) && !value.is_string() {
        return Value::String(
            binary_scene_value_string(&value).unwrap_or_else(|| value.to_string()),
        );
    }
    if default.is_some_and(Value::is_boolean)
        && let Some(value) = binary_scene_value_bool(&value)
    {
        return Value::Bool(value);
    }
    value
}

pub(super) fn binary_scene_push_unique_property(properties: &mut Vec<String>, property: &str) {
    if !properties.iter().any(|existing| existing == property) {
        properties.push(property.to_owned());
    }
}

pub(super) fn binary_scene_value_bool(value: &Value) -> Option<bool> {
    value
        .as_bool()
        .or_else(|| binary_scene_text_bool(value.as_str()?))
}

pub(super) fn binary_scene_value_number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| binary_scene_text_number(value.as_str()?))
}

pub(super) fn binary_scene_value_string(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return Some(value.to_owned());
    }
    if let Some(value) = value.as_bool() {
        return Some(if value { "1" } else { "0" }.to_owned());
    }
    if let Some(value) = value.as_i64() {
        return Some(value.to_string());
    }
    if let Some(value) = value.as_u64() {
        return Some(value.to_string());
    }
    if let Some(value) = value.as_f64() {
        if value.is_finite() && (value.fract()).abs() <= f64::EPSILON {
            return Some(format!("{value:.0}"));
        }
        return Some(value.to_string());
    }
    None
}

pub(super) fn binary_scene_text_bool(value: &str) -> Option<bool> {
    match binary_scene_normalized_text(value).as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub(super) fn binary_scene_text_number(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

pub(super) fn binary_scene_normalized_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
