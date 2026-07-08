//! Runtime property metadata applied during binary `.gscn` ingest.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::renderer::RendererPlanError;

use super::schema::{
    BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS, BINARY_TRANSFORM_PROPERTY_HEIGHT,
    BINARY_TRANSFORM_PROPERTY_OPACITY, BINARY_TRANSFORM_PROPERTY_ROTATION_DEG,
    BINARY_TRANSFORM_PROPERTY_SCALE_X, BINARY_TRANSFORM_PROPERTY_SCALE_Y,
    BINARY_TRANSFORM_PROPERTY_WIDTH, BINARY_TRANSFORM_PROPERTY_X, BINARY_TRANSFORM_PROPERTY_Y,
};

#[derive(Debug, Clone)]
pub(super) struct BinarySceneDynamicState {
    pub(super) nodes: BTreeMap<String, BinarySceneDynamicNode>,
    pub(super) property_bindings: Vec<BinarySceneDynamicPropertyBinding>,
    pub(super) properties: BTreeMap<String, Value>,
    pub(super) bound_properties: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct BinarySceneDynamicNode {
    pub(super) visible: bool,
    pub(super) visibility_condition: Option<Value>,
}

#[derive(Debug, Clone)]
pub(super) struct BinarySceneDynamicPropertyBinding {
    pub(super) property: String,
    pub(super) target_node: Option<String>,
    pub(super) target: u16,
    pub(super) scale: f64,
    pub(super) offset: f64,
}

#[derive(Debug, Deserialize)]
struct BinarySceneRuntimeMetadata {
    #[allow(dead_code)]
    version: Option<u32>,
    #[serde(default)]
    properties: BTreeMap<String, Value>,
    #[serde(default)]
    nodes: Vec<BinarySceneRuntimeMetadataNode>,
    #[serde(default)]
    property_bindings: Vec<BinarySceneRuntimeMetadataPropertyBinding>,
}

#[derive(Debug, Deserialize)]
struct BinarySceneRuntimeMetadataNode {
    id: String,
    #[serde(default = "binary_scene_runtime_metadata_default_visible")]
    visible: bool,
    #[serde(default)]
    visibility_condition: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct BinarySceneRuntimeMetadataPropertyBinding {
    property: String,
    #[serde(default)]
    target_node: Option<String>,
    target: String,
    #[serde(default = "binary_scene_runtime_metadata_default_scale")]
    scale: f64,
    #[serde(default)]
    offset: f64,
}

pub(super) fn binary_scene_dynamic_state_from_source_path(
    source_path: &Path,
    render_properties: Option<&BTreeMap<String, Value>>,
) -> Result<Option<BinarySceneDynamicState>, RendererPlanError> {
    let metadata_path = binary_scene_runtime_metadata_path(source_path);
    if !metadata_path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(&metadata_path).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to read binary scene runtime metadata {}: {err}",
            metadata_path.display()
        ))
    })?;
    let metadata: BinarySceneRuntimeMetadata = serde_json::from_slice(&bytes).map_err(|err| {
        RendererPlanError::PackageLoad(format!(
            "failed to parse binary scene runtime metadata {}: {err}",
            metadata_path.display()
        ))
    })?;
    Ok(Some(BinarySceneDynamicState::from_metadata(
        metadata,
        render_properties,
    )))
}

impl BinarySceneDynamicState {
    fn from_metadata(
        metadata: BinarySceneRuntimeMetadata,
        render_properties: Option<&BTreeMap<String, Value>>,
    ) -> Self {
        let mut properties = binary_scene_runtime_default_properties(&metadata.properties);
        if let Some(render_properties) = render_properties {
            for (property, value) in render_properties {
                let value = binary_scene_coerce_runtime_property_override(
                    properties.get(property),
                    value.clone(),
                );
                properties.insert(property.clone(), value);
            }
        }
        let nodes = metadata
            .nodes
            .into_iter()
            .map(|node| {
                (
                    node.id,
                    BinarySceneDynamicNode {
                        visible: node.visible,
                        visibility_condition: node.visibility_condition,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let property_bindings = metadata
            .property_bindings
            .into_iter()
            .filter_map(BinarySceneDynamicPropertyBinding::from_metadata)
            .collect::<Vec<_>>();
        let mut bound_properties = Vec::new();
        for node in nodes.values() {
            if let Some(property) = node
                .visibility_condition
                .as_ref()
                .and_then(Value::as_object)
                .and_then(|condition| condition.get("property"))
                .and_then(Value::as_str)
            {
                binary_scene_push_unique_property(&mut bound_properties, property);
            }
        }
        for binding in &property_bindings {
            binary_scene_push_unique_property(&mut bound_properties, &binding.property);
        }
        Self {
            nodes,
            property_bindings,
            properties,
            bound_properties,
        }
    }

    fn property_number(&self, property: &str) -> Option<f64> {
        binary_scene_property_number(self.properties.get(property)?)
    }

    pub(super) fn property_value(&self, property: &str) -> Option<&Value> {
        self.properties.get(property)
    }

    fn property_text(&self, property: &str) -> Option<String> {
        binary_scene_property_text(self.properties.get(property)?).map(str::to_owned)
    }

    pub(super) fn node_visible(&self, node_id: &str) -> Option<bool> {
        let node = self.nodes.get(node_id)?;
        if !node.visible {
            return Some(false);
        }
        let Some(condition) = node.visibility_condition.as_ref() else {
            return Some(true);
        };
        Some(binary_scene_dynamic_visibility_condition_matches(
            condition,
            |property| self.property_number(property),
            |property| self.property_text(property),
        ))
    }
}

impl BinarySceneDynamicPropertyBinding {
    fn from_metadata(binding: BinarySceneRuntimeMetadataPropertyBinding) -> Option<Self> {
        Some(Self {
            property: binding.property,
            target_node: binding.target_node,
            target: binary_scene_dynamic_property_target(&binding.target)?,
            scale: binding.scale,
            offset: binding.offset,
        })
    }
}

pub(super) fn binary_scene_property_number(value: &Value) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    if let Some(value) = value.as_bool() {
        return Some(if value { 1.0 } else { 0.0 });
    }
    None
}

fn binary_scene_runtime_metadata_default_visible() -> bool {
    true
}

fn binary_scene_runtime_metadata_default_scale() -> f64 {
    1.0
}

fn binary_scene_runtime_metadata_path(source_path: &Path) -> PathBuf {
    let mut path = source_path.as_os_str().to_os_string();
    path.push(".runtime.json");
    PathBuf::from(path)
}

fn binary_scene_coerce_runtime_property_override(default: Option<&Value>, value: Value) -> Value {
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

fn binary_scene_runtime_default_properties(
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

fn binary_scene_push_unique_property(properties: &mut Vec<String>, property: &str) {
    if !properties.iter().any(|existing| existing == property) {
        properties.push(property.to_owned());
    }
}

fn binary_scene_dynamic_property_target(target: &str) -> Option<u16> {
    match target {
        "x" => Some(BINARY_TRANSFORM_PROPERTY_X),
        "y" => Some(BINARY_TRANSFORM_PROPERTY_Y),
        "scale_x" | "scaleX" | "scalex" => Some(BINARY_TRANSFORM_PROPERTY_SCALE_X),
        "scale_y" | "scaleY" | "scaley" => Some(BINARY_TRANSFORM_PROPERTY_SCALE_Y),
        "opacity" | "alpha" => Some(BINARY_TRANSFORM_PROPERTY_OPACITY),
        "rotation" | "rotation_deg" | "angle" => Some(BINARY_TRANSFORM_PROPERTY_ROTATION_DEG),
        "width" => Some(BINARY_TRANSFORM_PROPERTY_WIDTH),
        "height" => Some(BINARY_TRANSFORM_PROPERTY_HEIGHT),
        "corner_radius" | "cornerRadius" => Some(BINARY_TRANSFORM_PROPERTY_CORNER_RADIUS),
        _ => None,
    }
}

fn binary_scene_property_text(value: &Value) -> Option<&str> {
    value.as_str()
}

fn binary_scene_dynamic_visibility_condition_matches<N, T>(
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

fn binary_scene_value_bool(value: &Value) -> Option<bool> {
    value
        .as_bool()
        .or_else(|| binary_scene_text_bool(value.as_str()?))
}

fn binary_scene_text_bool(value: &str) -> Option<bool> {
    match binary_scene_normalized_text(value).as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn binary_scene_value_number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| binary_scene_text_number(value.as_str()?))
}

fn binary_scene_text_number(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

fn binary_scene_value_string(value: &Value) -> Option<String> {
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

fn binary_scene_normalized_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
