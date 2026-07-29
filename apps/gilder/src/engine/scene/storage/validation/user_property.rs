//! Strict project user-property binding validation.

use super::*;

pub(super) fn validate_user_property_bindings(
    document: &SceneBinaryDocument,
) -> Result<(), SceneStorageError> {
    let raw_properties = if document.project.properties_json.is_some() {
        document
            .strings
            .get(document.project.properties_json.0 as usize)
            .expect("project property string was validated")
            .as_str()
    } else {
        "{}"
    };
    let properties =
        serde_json::from_str::<serde_json::Value>(raw_properties).map_err(|error| {
            SceneStorageError::InvalidUserPropertyBinding {
                object: SceneObjectHandle(INVALID_OBJECT_ID),
                reason: format!("project property schema is invalid JSON: {error}"),
            }
        })?;
    let properties =
        properties
            .as_object()
            .ok_or_else(|| SceneStorageError::InvalidUserPropertyBinding {
                object: SceneObjectHandle(INVALID_OBJECT_ID),
                reason: "project property schema root must be an object".to_owned(),
            })?;
    let mut previous = None;
    for binding in &document.user_property_bindings {
        validate_range(
            "user_property_binding.object",
            binding.object.0,
            1,
            document.objects.len(),
        )?;
        validate_string(document, "user_property_binding.property", binding.property)?;
        if !binding.property.is_some() {
            return Err(SceneStorageError::InvalidUserPropertyBinding {
                object: binding.object,
                reason: "property name must not be empty".to_owned(),
            });
        }
        let key = (binding.object.0, binding.target.to_u32());
        if previous.is_some_and(|previous| previous >= key) {
            return Err(SceneStorageError::InvalidUserPropertyBinding {
                object: binding.object,
                reason: "records must be strictly ordered without duplicate object targets"
                    .to_owned(),
            });
        }
        previous = Some(key);
        let property = &document.strings[binding.property.0 as usize];
        let definition = properties
            .get(property)
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| SceneStorageError::InvalidUserPropertyBinding {
                object: binding.object,
                reason: format!("unknown project property {property:?}"),
            })?;
        match binding.predicate {
            SceneUserPropertyPredicate::BooleanValue => {
                if definition.get("type").and_then(serde_json::Value::as_str) != Some("bool") {
                    return Err(SceneStorageError::InvalidUserPropertyBinding {
                        object: binding.object,
                        reason: format!("project property {property:?} must declare type \"bool\""),
                    });
                }
                let authored = definition
                    .get("value")
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| SceneStorageError::InvalidUserPropertyBinding {
                        object: binding.object,
                        reason: format!("project property {property:?} must have a boolean value"),
                    })?;
                if document.objects[binding.object.0 as usize].visible != authored {
                    return Err(SceneStorageError::InvalidUserPropertyBinding {
                        object: binding.object,
                        reason: format!(
                            "authored visibility does not match boolean default for {property:?}"
                        ),
                    });
                }
            }
            SceneUserPropertyPredicate::StringEquals(condition_id) => {
                validate_string(
                    document,
                    "user_property_binding.predicate_string",
                    condition_id,
                )?;
                if !condition_id.is_some() {
                    return Err(SceneStorageError::InvalidUserPropertyBinding {
                        object: binding.object,
                        reason: "conditional predicate string must not be empty".to_owned(),
                    });
                }
                if definition.get("type").and_then(serde_json::Value::as_str) != Some("combo") {
                    return Err(SceneStorageError::InvalidUserPropertyBinding {
                        object: binding.object,
                        reason: format!(
                            "project property {property:?} must declare type \"combo\""
                        ),
                    });
                }
                let authored = definition
                    .get("value")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| SceneStorageError::InvalidUserPropertyBinding {
                        object: binding.object,
                        reason: format!("project property {property:?} must have a string value"),
                    })?;
                let options = definition
                    .get("options")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| SceneStorageError::InvalidUserPropertyBinding {
                        object: binding.object,
                        reason: format!("project property {property:?} must declare combo options"),
                    })?;
                let mut option_values = std::collections::BTreeSet::new();
                for option in options {
                    let value = option
                        .as_object()
                        .and_then(|option| option.get("value"))
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| SceneStorageError::InvalidUserPropertyBinding {
                            object: binding.object,
                            reason: format!(
                                "project property {property:?} has an option without string value"
                            ),
                        })?;
                    if !option_values.insert(value) {
                        return Err(SceneStorageError::InvalidUserPropertyBinding {
                            object: binding.object,
                            reason: format!(
                                "project property {property:?} has duplicate option {value:?}"
                            ),
                        });
                    }
                }
                let condition = &document.strings[condition_id.0 as usize];
                if !option_values.contains(condition.as_str()) {
                    return Err(SceneStorageError::InvalidUserPropertyBinding {
                        object: binding.object,
                        reason: format!("condition {condition:?} is not an option of {property:?}"),
                    });
                }
                let authored_visibility = authored == condition;
                if document.objects[binding.object.0 as usize].visible != authored_visibility {
                    return Err(SceneStorageError::InvalidUserPropertyBinding {
                        object: binding.object,
                        reason: format!(
                            "authored visibility does not match default condition result for {property:?}"
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}
