//! Runtime property overrides consumed before hierarchy resolution.

use crate::engine::scene::{SceneObjectHandle, SceneScriptDelta, SceneScriptTarget, SceneVec3};

use super::TransformComponent;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ResolvedParentState {
    pub(super) parent: SceneObjectHandle,
    pub(super) inherited_visible: bool,
    pub(super) world_matrix: [f32; 16],
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
    script_delta(deltas, object, target).map(|delta| {
        let mut vector = SceneVec3 {
            x: delta.numeric[0],
            y: delta.numeric[1],
            z: delta.numeric[2],
        };
        if target == SceneScriptTarget::Angles {
            vector.x = vector.x.to_radians();
            vector.y = vector.y.to_radians();
            vector.z = vector.z.to_radians();
        }
        vector
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_angle_degrees_return_to_scene_radians() {
        let deltas = [SceneScriptDelta {
            object: SceneObjectHandle(4),
            target: SceneScriptTarget::Angles,
            selector: 0,
            numeric: [0.0, 0.0, -25.0, 0.0],
            text: None,
        }];
        let angles = script_vector(&deltas, SceneObjectHandle(4), SceneScriptTarget::Angles)
            .expect("angles");
        assert!((angles.z + 25.0_f32.to_radians()).abs() < 0.000_001);
    }
}
