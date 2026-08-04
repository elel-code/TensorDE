//! Typed semantic bindings from project user properties to scene components.

use serde_json::{Map, Value};

use crate::engine::scene::{
    SceneObjectHandle, SceneStringId, SceneUserPropertyBindingRecord, SceneUserPropertyPredicate,
    SceneUserPropertyTarget, resolve_scene_user_properties,
};

use super::{SceneSemanticWorld, SceneSemanticWorldError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticUserPropertyBinding {
    pub object: SceneObjectHandle,
    pub property: SceneStringId,
    pub target: SceneUserPropertyTarget,
    pub predicate: SceneUserPropertyPredicate,
}

impl SemanticUserPropertyBinding {
    pub(super) fn from_record(record: &SceneUserPropertyBindingRecord) -> Self {
        Self {
            object: record.object,
            property: record.property,
            target: record.target,
            predicate: record.predicate,
        }
    }
}

pub(super) fn resolved_visibility(
    world: &SceneSemanticWorld<'_>,
    overrides: &Map<String, Value>,
) -> Result<Vec<Option<bool>>, SceneSemanticWorldError> {
    let properties = resolve_scene_user_properties(world.storage, overrides)
        .map_err(|error| SceneSemanticWorldError::UserProperty(error.to_string()))?;
    let mut visibility = vec![None; world.entities.len()];
    for binding in &world.user_property_bindings {
        let property = world
            .storage
            .string(binding.property)
            .expect("scene storage validates user property binding strings");
        let value = properties
            .get(property)
            .expect("scene storage validates bound user properties");
        let predicate_matches = match binding.predicate {
            SceneUserPropertyPredicate::BooleanValue => value
                .as_bool()
                .expect("scene storage validates boolean user properties"),
            SceneUserPropertyPredicate::StringEquals(expected) => {
                value
                    .as_str()
                    .expect("scene storage validates string user predicates")
                    == world
                        .storage
                        .string(expected)
                        .expect("scene storage validates predicate strings")
            }
        };
        let entity = world
            .index
            .entity_for_object(binding.object)
            .expect("scene storage validates user property binding objects");
        match binding.target {
            SceneUserPropertyTarget::Visible => {
                visibility[entity.index()] = Some(predicate_matches)
            }
        }
    }
    Ok(visibility)
}
