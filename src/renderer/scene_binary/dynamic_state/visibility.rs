//! Runtime visibility condition evaluation for binary scene dynamic state.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use serde_json::Value;

use super::properties::{
    binary_scene_normalized_text, binary_scene_text_bool, binary_scene_text_number,
    binary_scene_value_bool, binary_scene_value_number, binary_scene_value_string,
};

pub(super) fn binary_scene_dynamic_visibility_condition_matches<N, T>(
    condition: &Value,
    resolve_number: N,
    resolve_text: T,
) -> bool
where
    N: Fn(&str) -> Option<f64>,
    T: Fn(&str) -> Option<String>,
{
    let Some(condition) = condition.as_object() else {
        return true;
    };
    if condition
        .get("runtime")
        .and_then(Value::as_str)
        .is_some_and(|runtime| runtime != "wallpaper-engine-user-condition")
    {
        return true;
    }
    let default_visible = condition
        .get("default_visible")
        .and_then(binary_scene_value_bool)
        .unwrap_or_else(|| {
            condition
                .get("authored_value")
                .and_then(binary_scene_value_bool)
                .unwrap_or(true)
        });
    let Some(property) = condition
        .get("property")
        .and_then(binary_scene_value_string)
    else {
        return default_visible;
    };
    let Some(expected) = condition.get("condition") else {
        return default_visible;
    };
    let actual_number = resolve_number(&property);
    let actual_text = resolve_text(&property);
    if actual_number.is_none() && actual_text.is_none() {
        return default_visible;
    }
    binary_scene_dynamic_expected_matches(expected, actual_number, actual_text.as_deref())
}

fn binary_scene_dynamic_expected_matches(
    expected: &Value,
    actual_number: Option<f64>,
    actual_text: Option<&str>,
) -> bool {
    let expected = expected.get("value").unwrap_or(expected);
    if let Some(expected_bool) = binary_scene_value_bool(expected) {
        if let Some(actual_number) = actual_number {
            return (actual_number.abs() > f64::EPSILON) == expected_bool;
        }
        return actual_text
            .and_then(binary_scene_text_bool)
            .is_some_and(|actual| actual == expected_bool);
    }
    if let Some(expected_number) = binary_scene_value_number(expected) {
        if let Some(actual_number) = actual_number {
            return (actual_number - expected_number).abs() <= 0.000_001;
        }
        return actual_text
            .and_then(binary_scene_text_number)
            .is_some_and(|actual| (actual - expected_number).abs() <= 0.000_001);
    }
    let Some(expected_text) = binary_scene_value_string(expected) else {
        return false;
    };
    if let Some(actual_text) = actual_text
        && binary_scene_normalized_text(actual_text) == binary_scene_normalized_text(&expected_text)
    {
        return true;
    }
    if let Some(expected_number) = binary_scene_text_number(&expected_text)
        && let Some(actual_number) = actual_number
    {
        return (actual_number - expected_number).abs() <= 0.000_001;
    }
    false
}
