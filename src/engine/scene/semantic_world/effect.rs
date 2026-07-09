//! Semantic ECS-like effect state for object-bound WE effects.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`

use super::super::abi::*;
use super::entity::SemanticEntity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectEffectBindingComponent {
    pub binding_start: u32,
    pub binding_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticObjectEffectBinding {
    pub object: SceneObjectHandle,
    pub effect: SceneEffectHandle,
    pub instance_id: u32,
    pub visible: bool,
}

impl SemanticObjectEffectBinding {
    pub fn from_record(record: &SceneObjectEffectRecord) -> Self {
        Self {
            object: record.object,
            effect: record.effect,
            instance_id: record.instance_id,
            visible: record.visible,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedObjectEffectState {
    pub entity: SemanticEntity,
    pub object: SceneObjectHandle,
    pub object_index: u32,
    pub effect: SceneEffectHandle,
    pub effect_index: u32,
    pub instance_id: u32,
    pub self_visible: bool,
    pub object_resolved_visible: bool,
    pub resolved_visible: bool,
    pub pass_start: u32,
    pub pass_count: u32,
    pub fbo_start: u32,
    pub fbo_count: u32,
}

pub fn object_effect_binding_from_object(
    object: &SceneObjectRecord,
) -> Option<ObjectEffectBindingComponent> {
    if object.effect_count == 0 {
        return None;
    }
    Some(ObjectEffectBindingComponent {
        binding_start: object.effect_start,
        binding_count: object.effect_count,
    })
}
