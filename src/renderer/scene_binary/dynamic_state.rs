//! Runtime property metadata applied during binary `.gscn` ingest.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::collections::BTreeMap;

use serde_json::Value;

mod binding;
mod metadata;
mod properties;
mod visibility;

pub(super) use metadata::binary_scene_dynamic_state_from_source_path;
pub(super) use properties::binary_scene_property_number;

use binding::BinarySceneDynamicPropertyBinding;
use metadata::BinarySceneRuntimeMetadata;
use properties::{
    binary_scene_coerce_runtime_property_override, binary_scene_property_text,
    binary_scene_push_unique_property, binary_scene_runtime_default_properties,
};
use visibility::binary_scene_dynamic_visibility_condition_matches;

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
