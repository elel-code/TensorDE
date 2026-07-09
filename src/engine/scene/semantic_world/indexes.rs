//! Lookup indexes for the scene semantic world.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`

use std::collections::BTreeMap;

use super::super::abi::SceneObjectHandle;
use super::entity::SemanticEntity;

#[derive(Debug, Default, Clone)]
pub struct SemanticIndexTable {
    object_to_entity: BTreeMap<SceneObjectHandle, SemanticEntity>,
    we_id_to_entity: BTreeMap<u32, SemanticEntity>,
}

impl SemanticIndexTable {
    pub fn insert_object(
        &mut self,
        object: SceneObjectHandle,
        entity: SemanticEntity,
    ) -> Option<SemanticEntity> {
        self.object_to_entity.insert(object, entity)
    }

    pub fn insert_we_id(&mut self, we_id: u32, entity: SemanticEntity) -> Option<SemanticEntity> {
        self.we_id_to_entity.insert(we_id, entity)
    }

    pub fn entity_for_object(&self, object: SceneObjectHandle) -> Option<SemanticEntity> {
        self.object_to_entity.get(&object).copied()
    }

    pub fn entity_for_we_id(&self, we_id: u32) -> Option<SemanticEntity> {
        self.we_id_to_entity.get(&we_id).copied()
    }
}
