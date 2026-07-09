//! Timeline sampling helpers for semantic scene state.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/model-and-animation.md`

use super::components::TransformComponent;
use super::matrix::{multiply_matrix, transform_matrix};
use crate::engine::scene::abi::{
    SceneObjectHandle, ScenePuppetAnimationClipRecord, ScenePuppetAnimationTrackRecord,
    ScenePuppetBoneRecord,
};
use crate::engine::scene::storage::SceneStorage;

pub fn sampled_puppet_bone_local_matrix(
    storage: &SceneStorage,
    object: SceneObjectHandle,
    puppet_index: u32,
    bone: &ScenePuppetBoneRecord,
    scene_time_seconds: f32,
) -> [f32; 16] {
    let mut local_matrix = bone.local_matrix;
    let mut matched_layer = false;
    for layer in storage
        .object_animation_layers()
        .iter()
        .filter(|layer| layer.object == object)
    {
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
        let Some(sample_matrix) = sampled_track_matrix(storage, clip, track, scene_time_seconds)
        else {
            continue;
        };
        if layer.additive || matched_layer {
            local_matrix = multiply_matrix(&local_matrix, &sample_matrix);
        } else {
            local_matrix = sample_matrix;
            matched_layer = true;
        }
    }
    local_matrix
}

fn sampled_track_matrix(
    storage: &SceneStorage,
    clip: &ScenePuppetAnimationClipRecord,
    track: &ScenePuppetAnimationTrackRecord,
    scene_time_seconds: f32,
) -> Option<[f32; 16]> {
    let samples = storage.puppet_animation_transform_samples(track);
    let sample = samples.get(sample_index(
        storage.string(clip.playback),
        clip.fps,
        scene_time_seconds,
        samples.len(),
    )?)?;
    Some(transform_matrix(&TransformComponent {
        origin: sample.translation,
        angles: sample.rotation,
        scale: sample.scale,
    }))
}

fn sample_index(
    playback: Option<&str>,
    fps: f32,
    scene_time_seconds: f32,
    sample_count: usize,
) -> Option<usize> {
    if sample_count == 0 {
        return None;
    }
    if !fps.is_finite() || fps <= 0.0 || !scene_time_seconds.is_finite() {
        return Some(0);
    }
    let frame = (scene_time_seconds.max(0.0) * fps).floor() as usize;
    if playback
        .map(|value| value.eq_ignore_ascii_case("loop"))
        .unwrap_or(false)
    {
        Some(frame % sample_count)
    } else {
        Some(frame.min(sample_count - 1))
    }
}
