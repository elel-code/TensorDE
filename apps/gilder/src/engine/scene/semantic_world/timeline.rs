//! Timeline sampling helpers for semantic scene state.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/scene-format.md`
//! - `reverse-engineered/gilder/docs/mdl-format.md`
//! - `reverse-engineered/gilder/docs/exe/model-and-animation.md`

use super::components::TransformComponent;
use super::matrix::{
    identity_matrix, interpolate_affine_matrix, inverse_affine_matrix, multiply_matrix,
    transform_matrix_radians,
};
use super::transform_animation::TransformAnimationComponent;
use crate::engine::scene::abi::{
    SCENE_OBJECT_TRANSFORM_KEYFRAME_BACK_ENABLED, SCENE_OBJECT_TRANSFORM_KEYFRAME_FRONT_ENABLED,
    SCENE_OBJECT_TRANSFORM_TRACK_RELATIVE, SCENE_OBJECT_TRANSFORM_TRACK_WRAP_LOOP,
    SceneObjectHandle, SceneObjectTransformChannelKind, SceneObjectTransformChannelRecord,
    SceneObjectTransformKeyframeRecord, SceneObjectTransformProperty,
    SceneObjectTransformTrackRecord, ScenePuppetAnimationClipRecord,
    ScenePuppetAnimationTrackRecord, ScenePuppetBoneRecord, SceneVec3,
};
use crate::engine::scene::storage::SceneStorage;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SampledPuppetBoneLocalState {
    pub matrix: [f32; 16],
    pub alpha: f32,
}

pub fn sampled_object_transform(
    storage: &SceneStorage,
    authored: &TransformComponent,
    animation: Option<&TransformAnimationComponent>,
    scene_time_seconds: f32,
) -> TransformComponent {
    let mut sampled = *authored;
    let Some(animation) = animation else {
        return sampled;
    };
    for &track_index in animation.track_indices() {
        let Some(track) = storage.object_transform_tracks().get(track_index as usize) else {
            continue;
        };
        for channel in storage.object_transform_channels(track) {
            let Some(value) =
                sample_object_transform_channel(storage, track, channel, scene_time_seconds)
            else {
                continue;
            };
            let Some(component) =
                transform_property_mut(&mut sampled, track.property, channel.component)
            else {
                continue;
            };
            if track.flags & SCENE_OBJECT_TRANSFORM_TRACK_RELATIVE != 0 {
                *component += value;
            } else {
                *component = value;
            }
        }
    }
    sampled
}

fn sample_object_transform_channel(
    storage: &SceneStorage,
    track: &SceneObjectTransformTrackRecord,
    channel: &SceneObjectTransformChannelRecord,
    scene_time_seconds: f32,
) -> Option<f32> {
    match channel.kind {
        SceneObjectTransformChannelKind::Sine => {
            let phase = scene_time_seconds * channel.frequency + channel.phase;
            let value = channel.offset + phase.sin() * channel.amplitude;
            value.is_finite().then_some(value)
        }
        SceneObjectTransformChannelKind::Keyframed => {
            let keyframes = storage.object_transform_keyframes(channel);
            let frame = object_track_frame(storage, track, scene_time_seconds)?;
            sample_object_keyframes(track, keyframes, frame)
        }
    }
}

fn object_track_frame(
    storage: &SceneStorage,
    track: &SceneObjectTransformTrackRecord,
    scene_time_seconds: f32,
) -> Option<f32> {
    if !track.fps.is_finite()
        || track.fps <= 0.0
        || track.frame_count == 0
        || !scene_time_seconds.is_finite()
    {
        return None;
    }
    let frame_count = track.frame_count as f32;
    let frame = scene_time_seconds.max(0.0) * track.fps;
    let playback = storage.string(track.playback).unwrap_or("loop");
    if playback.eq_ignore_ascii_case("single") {
        Some(frame.min(frame_count))
    } else if playback.eq_ignore_ascii_case("mirror") {
        let position = frame % (frame_count * 2.0);
        Some(if position > frame_count {
            frame_count * 2.0 - position
        } else {
            position
        })
    } else {
        Some(frame % frame_count)
    }
}

fn sample_object_keyframes(
    track: &SceneObjectTransformTrackRecord,
    keyframes: &[SceneObjectTransformKeyframeRecord],
    frame: f32,
) -> Option<f32> {
    let first = keyframes.first()?;
    if keyframes.len() == 1 {
        return Some(first.value);
    }
    for pair in keyframes.windows(2) {
        if frame >= pair[0].frame && frame <= pair[1].frame {
            return Some(interpolate_object_keyframes(
                &pair[0],
                pair[0].frame,
                &pair[1],
                pair[1].frame,
                frame,
            ));
        }
    }

    let wrap_loop = track.flags & SCENE_OBJECT_TRANSFORM_TRACK_WRAP_LOOP != 0;
    if !wrap_loop {
        return Some(if frame < first.frame {
            first.value
        } else {
            keyframes.last()?.value
        });
    }
    let last = keyframes.last()?;
    let frame_count = track.frame_count as f32;
    if frame < first.frame {
        Some(interpolate_object_keyframes(
            last,
            last.frame - frame_count,
            first,
            first.frame,
            frame,
        ))
    } else {
        Some(interpolate_object_keyframes(
            last,
            last.frame,
            first,
            first.frame + frame_count,
            frame,
        ))
    }
}

fn interpolate_object_keyframes(
    from: &SceneObjectTransformKeyframeRecord,
    from_frame: f32,
    to: &SceneObjectTransformKeyframeRecord,
    to_frame: f32,
    frame: f32,
) -> f32 {
    let duration = to_frame - from_frame;
    if !duration.is_finite() || duration <= f32::EPSILON {
        return to.value;
    }
    if frame <= from_frame {
        return from.value;
    }
    if frame >= to_frame {
        return to.value;
    }

    // Wallpaper Engine bakes one curve sample for every integral authored frame, then linearly
    // blends adjacent baked samples for fractional playback positions. Its handle X coordinates
    // are half-span-relative, while handle Y coordinates are value-relative. The X Bezier is
    // inverted with a bounded binary refinement before the same parameter is applied to Y.
    let lower_frame = frame.floor().max(from_frame);
    let upper_frame = (lower_frame + 1.0).min(to_frame);
    let lower = sample_object_keyframe_curve(from, from_frame, to, to_frame, lower_frame);
    if upper_frame <= lower_frame {
        return lower;
    }
    let upper = sample_object_keyframe_curve(from, from_frame, to, to_frame, upper_frame);
    let fraction = (frame - lower_frame).clamp(0.0, 1.0);
    lower + (upper - lower) * fraction
}

fn sample_object_keyframe_curve(
    from: &SceneObjectTransformKeyframeRecord,
    from_frame: f32,
    to: &SceneObjectTransformKeyframeRecord,
    to_frame: f32,
    frame: f32,
) -> f32 {
    if frame <= from_frame {
        return from.value;
    }
    if frame >= to_frame {
        return to.value;
    }
    let duration = to_frame - from_frame;
    let from_front = if from.flags & SCENE_OBJECT_TRANSFORM_KEYFRAME_FRONT_ENABLED != 0 {
        from.front
    } else {
        [0.0; 2]
    };
    let to_back = if to.flags & SCENE_OBJECT_TRANSFORM_KEYFRAME_BACK_ENABLED != 0 {
        to.back
    } else {
        [0.0; 2]
    };
    let control_frame_1 = from_frame + duration * 0.5 * from_front[0];
    let control_frame_2 = to_frame + duration * 0.5 * to_back[0];
    let mut parameter = 0.0_f32;
    let mut refinement = 0.999_f32;
    for _ in 0..1_000 {
        let curve_frame = cubic_bezier(
            from_frame,
            control_frame_1,
            control_frame_2,
            to_frame,
            parameter,
        );
        if (curve_frame - frame).abs() < 0.01 {
            break;
        }
        refinement *= 0.5;
        if curve_frame > frame {
            parameter -= refinement;
        } else {
            parameter += refinement;
        }
    }
    cubic_bezier(
        from.value,
        from.value + from_front[1],
        to.value + to_back[1],
        to.value,
        parameter.clamp(0.0, 1.0),
    )
}

fn cubic_bezier(from: f32, control_1: f32, control_2: f32, to: f32, t: f32) -> f32 {
    let inverse = 1.0 - t;
    inverse * inverse * inverse * from
        + 3.0 * inverse * inverse * t * control_1
        + 3.0 * inverse * t * t * control_2
        + t * t * t * to
}

fn transform_property_mut(
    transform: &mut TransformComponent,
    property: SceneObjectTransformProperty,
    component: u32,
) -> Option<&mut f32> {
    match property {
        SceneObjectTransformProperty::Origin => {
            vec3_component_mut(&mut transform.origin, component)
        }
        SceneObjectTransformProperty::Angles => {
            vec3_component_mut(&mut transform.angles, component)
        }
        SceneObjectTransformProperty::Scale => vec3_component_mut(&mut transform.scale, component),
        SceneObjectTransformProperty::CameraZoom => {
            (component == 0).then_some(&mut transform.camera_zoom)
        }
    }
}

fn vec3_component_mut(value: &mut SceneVec3, component: u32) -> Option<&mut f32> {
    match component {
        0 => Some(&mut value.x),
        1 => Some(&mut value.y),
        2 => Some(&mut value.z),
        _ => None,
    }
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
        camera_zoom: 1.0,
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

    #[test]
    fn wrapped_object_keyframes_interpolate_back_to_the_first_value() {
        let track = SceneObjectTransformTrackRecord {
            object: SceneObjectHandle(0),
            property: SceneObjectTransformProperty::Origin,
            flags: SCENE_OBJECT_TRANSFORM_TRACK_RELATIVE | SCENE_OBJECT_TRANSFORM_TRACK_WRAP_LOOP,
            playback: crate::engine::scene::abi::SceneStringId::NONE,
            fps: 30.0,
            frame_count: 360,
            channel_start: 0,
            channel_count: 1,
        };
        let keyframes = [
            SceneObjectTransformKeyframeRecord {
                frame: 0.0,
                value: 0.0,
                back: [-1.0, 0.0],
                front: [1.0, 0.0],
                flags: SCENE_OBJECT_TRANSFORM_KEYFRAME_BACK_ENABLED
                    | SCENE_OBJECT_TRANSFORM_KEYFRAME_FRONT_ENABLED,
            },
            SceneObjectTransformKeyframeRecord {
                frame: 180.0,
                value: 24.0,
                back: [-1.0, 0.0],
                front: [1.0, 0.0],
                flags: SCENE_OBJECT_TRANSFORM_KEYFRAME_BACK_ENABLED
                    | SCENE_OBJECT_TRANSFORM_KEYFRAME_FRONT_ENABLED,
            },
        ];

        assert_eq!(
            sample_object_keyframes(&track, &keyframes, 180.0),
            Some(24.0)
        );
        let wrapped_midpoint =
            sample_object_keyframes(&track, &keyframes, 270.0).expect("wrapped midpoint");
        assert!((wrapped_midpoint - 12.0).abs() < 5.0e-4);
        assert_eq!(
            sample_object_keyframes(&track, &keyframes, 360.0),
            Some(0.0)
        );
    }

    #[test]
    fn object_keyframes_use_half_span_bezier_handles_and_baked_frame_blending() {
        let from = SceneObjectTransformKeyframeRecord {
            frame: 0.0,
            value: 0.0,
            back: [0.0; 2],
            front: [0.5, 0.0],
            flags: SCENE_OBJECT_TRANSFORM_KEYFRAME_FRONT_ENABLED,
        };
        let to = SceneObjectTransformKeyframeRecord {
            frame: 100.0,
            value: 10.0,
            back: [-0.5, 0.0],
            front: [0.0; 2],
            flags: SCENE_OBJECT_TRANSFORM_KEYFRAME_BACK_ENABLED,
        };

        let sampled = interpolate_object_keyframes(&from, 0.0, &to, 100.0, 20.25);
        assert!((sampled - 1.305_890_7).abs() < 1.0e-6);
    }

    #[test]
    fn sine_channel_uses_engine_runtime_seconds() {
        let storage = SceneStorage::from_document(
            crate::engine::scene::binary::SceneBinaryDocument::default(),
        )
        .expect("empty storage");
        let track = SceneObjectTransformTrackRecord {
            object: SceneObjectHandle(0),
            property: SceneObjectTransformProperty::Angles,
            flags: 0,
            playback: crate::engine::scene::abi::SceneStringId::NONE,
            fps: 0.0,
            frame_count: 0,
            channel_start: 0,
            channel_count: 1,
        };
        let channel = SceneObjectTransformChannelRecord {
            track: 0,
            component: 2,
            kind: SceneObjectTransformChannelKind::Sine,
            offset: 2.0,
            amplitude: 8.0,
            frequency: 0.5,
            phase: 0.0,
            keyframe_start: 0,
            keyframe_count: 0,
        };

        let value =
            sample_object_transform_channel(&storage, &track, &channel, std::f32::consts::PI)
                .expect("sample");
        assert!((value - 10.0).abs() < 1.0e-5);
    }
}
