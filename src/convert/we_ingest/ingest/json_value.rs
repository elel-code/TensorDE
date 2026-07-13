//! Normalized JSON value access for Wallpaper Engine's literal-or-bound property encoding.

use serde_json::Value;

use crate::engine::scene::SceneVec3;

use super::WeIngestError;

pub(super) fn non_empty_string(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

pub(super) fn parse_json_bytes(path: &str, bytes: &[u8]) -> Result<Value, WeIngestError> {
    serde_json::from_slice(bytes).map_err(|source| WeIngestError::Json {
        path: path.to_owned(),
        source,
    })
}

pub(super) fn infer_project_type(scene_file: &str) -> &'static str {
    if scene_file.ends_with(".mp4") {
        "video"
    } else if scene_file.ends_with(".html") || scene_file.ends_with(".htm") {
        "web"
    } else {
        "scene"
    }
}

pub(super) fn normalize_we_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_owned()
}

fn bound_value(value: Option<&Value>) -> Option<&Value> {
    match value? {
        Value::Object(object) => object.get("value").or(value),
        value => Some(value),
    }
}

pub(super) fn bound_string(value: Option<&Value>) -> Option<String> {
    bound_value(value).and_then(|value| match value {
        Value::String(value) => Some(normalize_we_path(value)),
        _ => None,
    })
}

pub(super) fn bound_bool(value: Option<&Value>) -> Option<bool> {
    bound_value(value).and_then(Value::as_bool)
}

pub(super) fn value_u32(value: Option<&Value>) -> Option<u32> {
    let value = bound_value(value)?;
    value.as_u64().and_then(|value| u32::try_from(value).ok())
}

pub(super) fn value_i32(value: Option<&Value>) -> Option<i32> {
    let value = bound_value(value)?;
    value.as_i64().and_then(|value| i32::try_from(value).ok())
}

pub(super) fn value_i64(value: Option<&Value>) -> Option<i64> {
    let value = bound_value(value)?;
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
}

pub(super) fn value_f32(value: Option<&Value>) -> Option<f32> {
    let value = bound_value(value)?;
    value.as_f64().map(|value| value as f32)
}

pub(super) fn parse_vec3(value: Option<&Value>) -> Option<SceneVec3> {
    let value = bound_value(value)?;
    match value {
        Value::String(text) => {
            let mut parts = text
                .split_ascii_whitespace()
                .filter_map(|part| part.parse::<f32>().ok());
            Some(SceneVec3 {
                x: parts.next()?,
                y: parts.next()?,
                z: parts.next().unwrap_or(0.0),
            })
        }
        Value::Array(values) => Some(SceneVec3 {
            x: values.first()?.as_f64()? as f32,
            y: values.get(1)?.as_f64()? as f32,
            z: values.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        }),
        _ => None,
    }
}

pub(super) fn parse_color4(value: Option<&Value>, fallback: [f32; 4]) -> [f32; 4] {
    parse_vec3(value)
        .map(|color| [color.x, color.y, color.z, 1.0])
        .unwrap_or(fallback)
}

pub(super) fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}
