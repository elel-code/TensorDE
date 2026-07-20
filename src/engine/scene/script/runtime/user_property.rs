use serde_json::{Map, Value};

use super::SceneScriptError;

pub(super) fn resolve_user_properties(
    raw_properties: &str,
    user_property_overrides: &Map<String, Value>,
) -> Result<Map<String, Value>, SceneScriptError> {
    let definitions = serde_json::from_str::<Value>(raw_properties)
        .map_err(|error| SceneScriptError::InvalidProjectProperties(error.to_string()))?;
    let definitions = definitions.as_object().ok_or_else(|| {
        SceneScriptError::InvalidProjectProperties(
            "project properties root must be an object".to_owned(),
        )
    })?;
    let mut user_properties = Map::with_capacity(definitions.len());
    for (name, definition) in definitions {
        let definition = definition.as_object().ok_or_else(|| {
            SceneScriptError::InvalidProjectProperties(format!(
                "scene project entry {name:?} must be an object"
            ))
        })?;
        if let Some(authored) = definition.get("value") {
            user_properties.insert(name.clone(), authored.clone());
        }
    }
    for (name, value) in user_property_overrides {
        let definition = definitions.get(name).ok_or_else(|| {
            SceneScriptError::InvalidProjectProperties(format!(
                "unknown scene user property {name:?}"
            ))
        })?;
        let authored = definition.get("value").ok_or_else(|| {
            SceneScriptError::InvalidProjectProperties(format!(
                "scene project entry {name:?} has no authored runtime value"
            ))
        })?;
        if !same_json_value_kind(authored, value) {
            return Err(SceneScriptError::InvalidProjectProperties(format!(
                "scene user property {name:?} requires {}, got {}",
                json_value_kind(authored),
                json_value_kind(value)
            )));
        }
        user_properties.insert(name.clone(), value.clone());
    }
    Ok(user_properties)
}

pub(super) fn same_json_value_kind(authored: &Value, override_value: &Value) -> bool {
    matches!(
        (authored, override_value),
        (Value::Null, Value::Null)
            | (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Array(_), Value::Array(_))
            | (Value::Object(_), Value::Object(_))
    )
}

pub(super) fn json_value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
