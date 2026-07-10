//! Timeline sampling helpers for semantic scene state.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/model-and-animation.md`

use super::components::TransformComponent;
use super::matrix::{
    identity_matrix, interpolate_affine_matrix, inverse_affine_matrix, multiply_matrix,
    transform_matrix_radians,
};
use crate::engine::scene::abi::{
    SceneObjectHandle, ScenePuppetAnimationClipRecord, ScenePuppetAnimationTrackRecord,
    ScenePuppetBoneRecord, SceneVec3,
};
use crate::engine::scene::storage::SceneStorage;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SampledPuppetBoneLocalState {
    pub matrix: [f32; 16],
    pub alpha: f32,
}

pub fn sampled_puppet_bone_local_matrix(
    storage: &SceneStorage,
    object: SceneObjectHandle,
    puppet_index: u32,
    bone: &ScenePuppetBoneRecord,
    scene_time_seconds: f32,
) -> [f32; 16] {
    sampled_puppet_bone_local_state(storage, object, puppet_index, bone, scene_time_seconds).matrix
}

pub fn sampled_puppet_bone_local_state(
    storage: &SceneStorage,
    object: SceneObjectHandle,
    puppet_index: u32,
    bone: &ScenePuppetBoneRecord,
    scene_time_seconds: f32,
) -> SampledPuppetBoneLocalState {
    let mut local_matrix = bone.local_bind_matrix;
    let mut alpha = 1.0;
    let mut matched_non_additive_layer = false;
    for layer in storage
        .object_animation_layers()
        .iter()
        .filter(|layer| layer.object == object)
    {
        if !layer.visible || !layer.blend_weight.is_finite() || layer.blend_weight <= 1.0e-6 {
            continue;
        }
        let Some(clip) = storage
            .puppet_animation_clips()
            .iter()
            .find(|clip| clip.puppet == puppet_index && clip.clip_id == layer.animation_id)
        else {
            continue;
        };
        let Some(track) = storage
            .puppet_animation_tracks(clip)
            .iter()
            .find(|track| track.bone_index == bone.bone_index)
        else {
            continue;
        };
        let animation_time_seconds = layer_animation_time_seconds(layer, clip, scene_time_seconds);
        let Some(sample_matrix) =
            sampled_track_matrix(storage, clip, track, animation_time_seconds)
        else {
            continue;
        };
        let weight = layer.blend_weight.clamp(0.0, 1.0);
        let sampled_alpha = sampled_track_opacity(storage, clip, track, animation_time_seconds);
        if layer.additive {
            let Some(inverse_bind) = inverse_affine_matrix(&bone.local_bind_matrix) else {
                continue;
            };
            let additive_delta = multiply_matrix(&inverse_bind, &sample_matrix);
            let weighted_delta =
                interpolate_affine_matrix(&identity_matrix(), &additive_delta, weight);
            local_matrix = multiply_matrix(&local_matrix, &weighted_delta);
            if let Some(sampled_alpha) = sampled_alpha {
                alpha += (sampled_alpha - 1.0) * weight;
            }
        } else {
            let from = if matched_non_additive_layer {
                local_matrix
            } else {
                bone.local_bind_matrix
            };
            local_matrix = interpolate_affine_matrix(&from, &sample_matrix, weight);
            if let Some(sampled_alpha) = sampled_alpha {
                alpha = alpha * (1.0 - weight) + sampled_alpha * weight;
            }
            matched_non_additive_layer = true;
        }
    }
    SampledPuppetBoneLocalState {
        matrix: local_matrix,
        alpha: if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        },
    }
}

fn layer_animation_time_seconds(
    layer: &crate::engine::scene::abi::SceneObjectAnimationLayerRecord,
    clip: &ScenePuppetAnimationClipRecord,
    scene_time_seconds: f32,
) -> f32 {
    let playback_rate = if layer.playback_rate.is_finite() {
        layer.playback_rate
    } else {
        1.0
    };
    let initial_progress = if layer.initial_progress.is_finite() {
        layer.initial_progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let initial_time_seconds = if clip.fps.is_finite() && clip.fps > 0.0 {
        initial_progress * clip.frame_count as f32 / clip.fps
    } else {
        0.0
    };
    initial_time_seconds + scene_time_seconds * playback_rate
}

fn sampled_track_matrix(
    storage: &SceneStorage,
    clip: &ScenePuppetAnimationClipRecord,
    track: &ScenePuppetAnimationTrackRecord,
    scene_time_seconds: f32,
) -> Option<[f32; 16]> {
    let samples = storage.puppet_animation_transform_samples(track);
    let (current_index, next_index, fraction) = sample_window(
        storage.string(clip.playback),
        clip.fps,
        scene_time_seconds,
        clip.frame_count,
        samples.len(),
    )?;
    let current = samples.get(current_index)?;
    let next = samples.get(next_index)?;
    Some(transform_matrix_radians(&TransformComponent {
        origin: interpolate_vec3(current.translation, next.translation, fraction),
        angles: interpolate_vec3(current.rotation, next.rotation, fraction),
        scale: interpolate_vec3(current.scale, next.scale, fraction),
    }))
}

fn sampled_track_opacity(
    storage: &SceneStorage,
    clip: &ScenePuppetAnimationClipRecord,
    track: &ScenePuppetAnimationTrackRecord,
    scene_time_seconds: f32,
) -> Option<f32> {
    let samples = storage.puppet_animation_opacity_samples(track);
    let (current_index, next_index, fraction) = sample_window(
        storage.string(clip.playback),
        clip.fps,
        scene_time_seconds,
        clip.frame_count,
        samples.len(),
    )?;
    let current = *samples.get(current_index)?;
    let next = *samples.get(next_index)?;
    Some(current * (1.0 - fraction) + next * fraction)
}

fn sample_window(
    playback: Option<&str>,
    fps: f32,
    scene_time_seconds: f32,
    frame_count: u32,
    sample_count: usize,
) -> Option<(usize, usize, f32)> {
    if sample_count == 0 {
        return None;
    }
    if !fps.is_finite() || fps <= 0.0 || !scene_time_seconds.is_finite() {
        return Some((0, 0, 0.0));
    }
    let looping = playback
        .map(|value| value.eq_ignore_ascii_case("loop"))
        .unwrap_or(false);
    let frame_position = scene_time_seconds.max(0.0) * fps;
    if looping {
        let cycle_count = (frame_count as usize).clamp(1, sample_count);
        let cycle_position = frame_position % cycle_count as f32;
        let current = cycle_position.floor() as usize;
        let next = if current + 1 < sample_count {
            current + 1
        } else {
            0
        };
        Some((current, next, cycle_position - current as f32))
    } else {
        let clamped = frame_position.min((sample_count - 1) as f32);
        let current = clamped.floor() as usize;
        let next = (current + 1).min(sample_count - 1);
        Some((current, next, clamped - current as f32))
    }
}

fn interpolate_vec3(from: SceneVec3, to: SceneVec3, weight: f32) -> SceneVec3 {
    SceneVec3 {
        x: from.x * (1.0 - weight) + to.x * weight,
        y: from.y * (1.0 - weight) + to.y * weight,
        z: from.z * (1.0 - weight) + to.z * weight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_animation_time_applies_set_frame_progress_before_playback_rate() {
        let layer = crate::engine::scene::abi::SceneObjectAnimationLayerRecord {
            object: SceneObjectHandle(0),
            animation_id: 549,
            layer_index: 0,
            additive: true,
            autosort: false,
            visible: true,
            playback_rate: 2.0,
            blend_weight: 1.0,
            initial_progress: 0.9,
        };
        let clip = ScenePuppetAnimationClipRecord {
            puppet: 0,
            clip_id: 549,
            flags: 0,
            name: crate::engine::scene::abi::SceneStringId::NONE,
            playback: crate::engine::scene::abi::SceneStringId::NONE,
            fps: 30.0,
            frame_count: 360,
            frame_metadata: 0,
            track_start: 0,
            track_count: 0,
        };

        assert!((layer_animation_time_seconds(&layer, &clip, 0.5) - 11.8).abs() < 1.0e-5);
    }

    #[test]
    fn looping_sample_window_uses_authored_duplicate_endpoint() {
        assert_eq!(
            sample_window(Some("loop"), 30.0, 11.5 / 30.0, 12, 13),
            Some((11, 12, 0.5))
        );
        assert_eq!(
            sample_window(Some("loop"), 30.0, 12.0 / 30.0, 12, 13),
            Some((0, 1, 0.0))
        );
    }

    #[test]
    fn non_looping_sample_window_clamps_to_last_sample() {
        assert_eq!(
            sample_window(Some("single"), 30.0, 5.0, 12, 13),
            Some((12, 12, 0.0))
        );
    }
}
