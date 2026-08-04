//! Strict project user-property resolution shared by scripts and typed scene bindings.

use std::fmt;

use serde_json::{Map, Value};

use super::SceneStorage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneUserPropertyError(String);

impl fmt::Display for SceneUserPropertyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SceneUserPropertyError {}

pub fn resolve_scene_user_properties(
    storage: &SceneStorage,
    user_property_overrides: &Map<String, Value>,
) -> Result<Map<String, Value>, SceneUserPropertyError> {
    let properties_id = storage.project().properties_json;
    let raw_properties = if properties_id.is_some() {
        storage
            .string(properties_id)
            .expect("scene storage validates project property strings")
    } else {
        "{}"
    };
    resolve_raw_scene_user_properties(raw_properties, user_property_overrides)
}

pub(crate) fn resolve_raw_scene_user_properties(
    raw_properties: &str,
    user_property_overrides: &Map<String, Value>,
) -> Result<Map<String, Value>, SceneUserPropertyError> {
    let definitions = serde_json::from_str::<Value>(raw_properties)
        .map_err(|error| invalid(error.to_string()))?;
    let definitions = definitions
        .as_object()
        .ok_or_else(|| invalid("project properties root must be an object"))?;
    let mut user_properties = Map::with_capacity(definitions.len());
    for (name, definition) in definitions {
        let definition = definition
            .as_object()
            .ok_or_else(|| invalid(format!("scene project entry {name:?} must be an object")))?;
        if let Some(authored) = definition.get("value") {
            user_properties.insert(name.clone(), authored.clone());
        }
    }
    for (name, value) in user_property_overrides {
        let definition = definitions
            .get(name)
            .ok_or_else(|| invalid(format!("unknown scene user property {name:?}")))?;
        let authored = definition.get("value").ok_or_else(|| {
            invalid(format!(
                "scene project entry {name:?} has no authored runtime value"
            ))
        })?;
        if !same_json_value_kind(authored, value) {
            return Err(invalid(format!(
                "scene user property {name:?} requires {}, got {}",
                json_value_kind(authored),
                json_value_kind(value)
            )));
        }
        user_properties.insert(name.clone(), value.clone());
    }
    Ok(user_properties)
}

fn same_json_value_kind(authored: &Value, override_value: &Value) -> bool {
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

fn json_value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn invalid(message: impl Into<String>) -> SceneUserPropertyError {
    SceneUserPropertyError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_is_exact_and_value_kind_strict() {
        let authored = r#"{"jia":{"value":true},"speed":{"value":1}}"#;
        let overrides = [("jia".to_owned(), Value::Bool(false))]
            .into_iter()
            .collect();
        let resolved = resolve_raw_scene_user_properties(authored, &overrides).expect("properties");
        assert_eq!(resolved["jia"], Value::Bool(false));
        assert_eq!(resolved["speed"], Value::from(1));

        for invalid_overrides in [
            [("Jia".to_owned(), Value::Bool(false))]
                .into_iter()
                .collect(),
            [("jia".to_owned(), Value::from(0))].into_iter().collect(),
        ] {
            assert!(resolve_raw_scene_user_properties(authored, &invalid_overrides).is_err());
        }
    }
}
