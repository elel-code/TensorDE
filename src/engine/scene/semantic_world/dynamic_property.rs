//! Runtime property overrides consumed before hierarchy resolution.

use crate::engine::scene::{
    SceneAudioBandMaterialTarget, SceneObjectHandle, SceneScriptDelta, SceneScriptTarget, SceneVec3,
};

use super::{ResolvedAudioBandMaterialValue, TransformComponent};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ResolvedParentState {
    pub(super) parent: SceneObjectHandle,
    pub(super) inherited_visible: bool,
    pub(super) inherited_color: SceneVec3,
    pub(super) inherited_alpha: f32,
    pub(super) world_matrix: [f32; 16],
}

pub(super) fn object_uniform_scale(
    values: &[ResolvedAudioBandMaterialValue],
    object: SceneObjectHandle,
) -> Option<f32> {
    values
        .iter()
        .find(|value| {
            value.object == object
                && value.target == SceneAudioBandMaterialTarget::ObjectUniformScale
        })
        .map(|value| value.value)
        .filter(|value| value.is_finite())
}

pub(super) fn multiply_color(left: SceneVec3, right: SceneVec3) -> SceneVec3 {
    SceneVec3 {
        x: left.x * right.x,
        y: left.y * right.y,
        z: left.z * right.z,
    }
}

fn script_delta(
    deltas: &[SceneScriptDelta],
    object: SceneObjectHandle,
    target: SceneScriptTarget,
) -> Option<&SceneScriptDelta> {
    deltas
        .iter()
        .rev()
        .find(|delta| delta.object == object && delta.target == target)
}

pub(super) fn script_scalar(
    deltas: &[SceneScriptDelta],
    object: SceneObjectHandle,
    target: SceneScriptTarget,
) -> Option<f32> {
    script_delta(deltas, object, target).map(|delta| delta.numeric[0])
}

pub(super) fn script_vector(
    deltas: &[SceneScriptDelta],
    object: SceneObjectHandle,
    target: SceneScriptTarget,
) -> Option<SceneVec3> {
    script_delta(deltas, object, target).map(|delta| SceneVec3 {
        x: delta.numeric[0],
        y: delta.numeric[1],
        z: delta.numeric[2],
    })
}

pub(super) fn apply_script_transform(
    deltas: &[SceneScriptDelta],
    object: SceneObjectHandle,
    transform: &mut TransformComponent,
) {
    if let Some(origin) = script_vector(deltas, object, SceneScriptTarget::Origin) {
        transform.origin = origin;
    }
    if let Some(angles) = script_vector(deltas, object, SceneScriptTarget::Angles) {
        transform.angles = angles;
    }
    if let Some(scale) = script_vector(deltas, object, SceneScriptTarget::Scale) {
        transform.scale = scale;
    }
}
