//! Strict scene-field bindings to authored project user properties.

use serde_json::{Map, Value};

use crate::convert::we_ingest::ir::{WeIrUserPropertyBinding, WeIrUserPropertyPredicate};
use crate::engine::scene::SceneUserPropertyTarget;

pub(super) fn object_visibility(
    object: u32,
    value: Option<&Value>,
    project_properties: &Map<String, Value>,
) -> Result<(bool, Option<WeIrUserPropertyBinding>), String> {
    let Some(value) = value else {
        return Ok((true, None));
    };
    let Value::Object(binding) = value else {
        return value
            .as_bool()
            .map(|visible| (visible, None))
            .ok_or_else(|| "visible must be a boolean or typed binding object".to_owned());
    };
    if !binding.contains_key("user") {
        return binding
            .get("value")
            .and_then(Value::as_bool)
            .map(|visible| (visible, None))
            .ok_or_else(|| "visible binding object must contain a boolean value".to_owned());
    }
    let visible = binding
        .get("value")
        .and_then(Value::as_bool)
        .ok_or_else(|| "visible user binding value must be a boolean".to_owned())?;
    match binding.get("user") {
        Some(Value::String(property)) => {
            direct_boolean_visibility(object, binding, property, visible, project_properties)
        }
        Some(Value::Object(user)) => {
            combo_condition_visibility(object, binding, user, visible, project_properties)
        }
        _ => Err("visible user binding must use a string or typed condition object".to_owned()),
    }
}

fn direct_boolean_visibility(
    object: u32,
    binding: &Map<String, Value>,
    property: &str,
    visible: bool,
    project_properties: &Map<String, Value>,
) -> Result<(bool, Option<WeIrUserPropertyBinding>), String> {
    let script_field_count = usize::from(binding.contains_key("script"));
    if binding.len() != 2 + script_field_count
        || property.is_empty()
        || binding
            .get("script")
            .is_some_and(|script| !script.is_string())
    {
        return Err(
            "direct visible user binding accepts string user, boolean value, and an optional string script"
                .to_owned(),
        );
    }
    let definition = project_properties
        .get(property)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("visible user binding references unknown property {property:?}"))?;
    match definition.get("type") {
        Some(Value::String(property_type)) if property_type == "bool" => {}
        Some(Value::String(property_type)) => {
            return Err(format!(
                "visible user binding property {property:?} must have type \"bool\", got {property_type:?}"
            ));
        }
        _ => {
            return Err(format!(
                "visible user binding property {property:?} must declare type \"bool\""
            ));
        }
    }
    let authored = definition
        .get("value")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            format!("visible user binding property {property:?} must have a boolean authored value")
        })?;
    if visible != authored {
        return Err(format!(
            "visible user binding authored value {visible} does not match {property:?} boolean default {authored}"
        ));
    }
    Ok((
        visible,
        Some(WeIrUserPropertyBinding {
            object,
            property: property.to_owned(),
            target: SceneUserPropertyTarget::Visible,
            predicate: WeIrUserPropertyPredicate::BooleanValue,
        }),
    ))
}

fn combo_condition_visibility(
    object: u32,
    binding: &Map<String, Value>,
    user: &Map<String, Value>,
    visible: bool,
    project_properties: &Map<String, Value>,
) -> Result<(bool, Option<WeIrUserPropertyBinding>), String> {
    if binding.len() != 2
        || user.len() != 2
        || !user.contains_key("name")
        || !user.contains_key("condition")
    {
        return Err(
            "conditional visible user binding requires exactly name and condition".to_owned(),
        );
    }
    let property = user
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "conditional visible user binding name must be non-empty".to_owned())?;
    let condition = user
        .get("condition")
        .and_then(Value::as_str)
        .filter(|condition| !condition.is_empty())
        .ok_or_else(|| "conditional visible user binding condition must be non-empty".to_owned())?;
    let definition = project_properties
        .get(property)
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!("conditional visible binding references unknown property {property:?}")
        })?;
    if definition.get("type").and_then(Value::as_str) != Some("combo") {
        return Err(format!(
            "conditional visible binding property {property:?} must declare type \"combo\""
        ));
    }
    let authored = definition
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "conditional visible binding property {property:?} must have a string authored value"
            )
        })?;
    let options = definition
        .get("options")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!("conditional visible binding property {property:?} must declare combo options")
        })?;
    let mut option_values = std::collections::BTreeSet::new();
    for option in options {
        let option = option.as_object().ok_or_else(|| {
            format!("conditional visible binding property {property:?} has a non-object option")
        })?;
        let value = option.get("value").and_then(Value::as_str).ok_or_else(|| {
            format!(
                "conditional visible binding property {property:?} has an option without string value"
            )
        })?;
        if !option_values.insert(value) {
            return Err(format!(
                "conditional visible binding property {property:?} has duplicate option {value:?}"
            ));
        }
    }
    if !option_values.contains(condition) {
        return Err(format!(
            "conditional visible binding condition {condition:?} is not an option of {property:?}"
        ));
    }
    let authored_visibility = authored == condition;
    if visible != authored_visibility {
        return Err(format!(
            "conditional visible binding authored value {visible} does not match {property:?} default condition result {authored_visibility}"
        ));
    }
    Ok((
        visible,
        Some(WeIrUserPropertyBinding {
            object,
            property: property.to_owned(),
            target: SceneUserPropertyTarget::Visible,
            predicate: WeIrUserPropertyPredicate::StringEquals(condition.to_owned()),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn visible_binding_is_exact_and_bool_typed() {
        let properties =
            Map::from_iter([("rain".to_owned(), json!({"type": "bool", "value": false}))]);
        let (visible, binding) = object_visibility(
            7,
            Some(&json!({"user": "rain", "value": false})),
            &properties,
        )
        .expect("typed binding");
        assert!(!visible);
        let binding = binding.expect("binding");
        assert_eq!(binding.property, "rain");
        assert_eq!(binding.predicate, WeIrUserPropertyPredicate::BooleanValue);
        let enabled_properties =
            Map::from_iter([("rain".to_owned(), json!({"type": "bool", "value": true}))]);
        let (visible, binding) = object_visibility(
            7,
            Some(&json!({"user": "rain", "value": true})),
            &enabled_properties,
        )
        .expect("enabled typed binding");
        assert!(visible);
        assert_eq!(
            binding.expect("enabled binding").predicate,
            WeIrUserPropertyPredicate::BooleanValue
        );
        assert!(
            object_visibility(
                7,
                Some(&json!({
                    "script": "export function applyUserProperties() {}",
                    "user": "rain",
                    "value": false
                })),
                &properties,
            )
            .is_ok()
        );

        for invalid in [
            json!({"user": "Rain", "value": false}),
            json!({"user": "rain", "value": 0}),
            json!({"user": {"name": "rain"}, "value": false}),
            json!({"user": "rain", "condition": "rain.value", "value": false}),
            json!({"script": 1, "user": "rain", "value": false}),
            json!({"user": "rain", "value": true}),
        ] {
            assert!(object_visibility(7, Some(&invalid), &properties).is_err());
        }
    }

    #[test]
    fn combo_visibility_condition_is_exact_and_validates_authored_default() {
        let properties = Map::from_iter([(
            "theme".to_owned(),
            json!({
                "type": "combo",
                "value": "1",
                "options": [
                    {"label": "First", "value": "1"},
                    {"label": "Second", "value": "2"}
                ]
            }),
        )]);
        for (condition, visible) in [("1", true), ("2", false)] {
            let (_, binding) = object_visibility(
                7,
                Some(&json!({
                    "user": {"condition": condition, "name": "theme"},
                    "value": visible
                })),
                &properties,
            )
            .expect("typed combo condition");
            assert_eq!(
                binding.expect("binding").predicate,
                WeIrUserPropertyPredicate::StringEquals(condition.to_owned())
            );
        }

        for invalid in [
            json!({"user": {"condition": "3", "name": "theme"}, "value": false}),
            json!({"user": {"condition": 1, "name": "theme"}, "value": true}),
            json!({"user": {"condition": "1", "name": "theme"}, "value": false}),
            json!({
                "user": {"condition": "1", "name": "theme", "type": "combo"},
                "value": true
            }),
            json!({
                "script": "export function update() {}",
                "user": {"condition": "1", "name": "theme"},
                "value": true
            }),
        ] {
            assert!(object_visibility(7, Some(&invalid), &properties).is_err());
        }
    }
}
