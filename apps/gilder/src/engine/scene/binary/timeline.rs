//! Timeline chunk codec for scene binary records.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/scene-format.md`
//! - `reverse-engineered/gilder/docs/mdl-format.md`
//! - `reverse-engineered/gilder/docs/exe/model-and-animation.md`

use super::SceneBinaryError;
use crate::engine::scene::abi::{
    SceneObjectAnimationLayerRecord, SceneObjectHandle, SceneObjectTransformChannelKind,
    SceneObjectTransformChannelRecord, SceneObjectTransformKeyframeRecord,
    SceneObjectTransformProperty, SceneObjectTransformTrackRecord, ScenePuppetAnimationClipRecord,
    ScenePuppetAnimationTrackRecord, ScenePuppetAnimationTransformSampleRecord, SceneStringId,
    SceneVec3,
};

pub(super) struct SceneTimelineRecords {
    pub object_animation_layers: Vec<SceneObjectAnimationLayerRecord>,
    pub puppet_animation_clips: Vec<ScenePuppetAnimationClipRecord>,
    pub puppet_animation_tracks: Vec<ScenePuppetAnimationTrackRecord>,
    pub puppet_animation_transform_samples: Vec<ScenePuppetAnimationTransformSampleRecord>,
    pub puppet_animation_opacity_samples: Vec<f32>,
    pub object_transform_tracks: Vec<SceneObjectTransformTrackRecord>,
    pub object_transform_channels: Vec<SceneObjectTransformChannelRecord>,
    pub object_transform_keyframes: Vec<SceneObjectTransformKeyframeRecord>,
}

// Timeline chunks intentionally expose each typed table instead of hiding
// ordering behind a compatibility container.
#[allow(clippy::too_many_arguments)]
pub(super) fn encode_timelines(
    animation_layers: &[SceneObjectAnimationLayerRecord],
    puppet_clips: &[ScenePuppetAnimationClipRecord],
    puppet_tracks: &[ScenePuppetAnimationTrackRecord],
    transform_samples: &[ScenePuppetAnimationTransformSampleRecord],
    opacity_samples: &[f32],
    object_transform_tracks: &[SceneObjectTransformTrackRecord],
    object_transform_channels: &[SceneObjectTransformChannelRecord],
    object_transform_keyframes: &[SceneObjectTransformKeyframeRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(
        &mut out,
        checked_u32(animation_layers.len(), "object animation layer count")?,
    );
    for record in animation_layers {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.animation_id);
        put_u32(&mut out, record.layer_index);
        put_bool(&mut out, record.additive);
        put_bool(&mut out, record.autosort);
        put_bool(&mut out, record.visible);
        put_f32(&mut out, record.playback_rate);
        put_f32(&mut out, record.blend_weight);
        put_f32(&mut out, record.initial_progress);
    }
    put_u32(
        &mut out,
        checked_u32(puppet_clips.len(), "puppet animation clip count")?,
    );
    for record in puppet_clips {
        put_u32(&mut out, record.puppet);
        put_u32(&mut out, record.clip_id);
        put_u32(&mut out, record.flags);
        put_u32(&mut out, record.name.0);
        put_u32(&mut out, record.playback.0);
        put_f32(&mut out, record.fps);
        put_u32(&mut out, record.frame_count);
        put_u32(&mut out, record.frame_metadata);
        put_u32(&mut out, record.track_start);
        put_u32(&mut out, record.track_count);
    }
    put_u32(
        &mut out,
        checked_u32(puppet_tracks.len(), "puppet animation track count")?,
    );
    for record in puppet_tracks {
        put_u32(&mut out, record.clip);
        put_u32(&mut out, record.bone_index);
        put_u32(&mut out, record.track_flags);
        put_u32(&mut out, record.sample_start);
        put_u32(&mut out, record.sample_count);
        put_u32(&mut out, record.opacity_flags);
        put_u32(&mut out, record.opacity_sample_start);
        put_u32(&mut out, record.opacity_sample_count);
    }
    put_u32(
        &mut out,
        checked_u32(
            transform_samples.len(),
            "puppet animation transform sample count",
        )?,
    );
    for record in transform_samples {
        put_vec3(&mut out, record.translation);
        put_vec3(&mut out, record.rotation);
        put_vec3(&mut out, record.scale);
    }
    put_u32(
        &mut out,
        checked_u32(
            opacity_samples.len(),
            "puppet animation opacity sample count",
        )?,
    );
    for sample in opacity_samples {
        put_f32(&mut out, *sample);
    }
    put_u32(
        &mut out,
        checked_u32(
            object_transform_tracks.len(),
            "object transform track count",
        )?,
    );
    for record in object_transform_tracks {
        put_u32(&mut out, record.object.0);
        put_u32(&mut out, record.property.to_u32());
        put_u32(&mut out, record.flags);
        put_u32(&mut out, record.playback.0);
        put_f32(&mut out, record.fps);
        put_u32(&mut out, record.frame_count);
        put_u32(&mut out, record.channel_start);
        put_u32(&mut out, record.channel_count);
    }
    put_u32(
        &mut out,
        checked_u32(
            object_transform_channels.len(),
            "object transform channel count",
        )?,
    );
    for record in object_transform_channels {
        put_u32(&mut out, record.track);
        put_u32(&mut out, record.component);
        put_u32(&mut out, record.kind.to_u32());
        put_f32(&mut out, record.offset);
        put_f32(&mut out, record.amplitude);
        put_f32(&mut out, record.frequency);
        put_f32(&mut out, record.phase);
        put_u32(&mut out, record.keyframe_start);
        put_u32(&mut out, record.keyframe_count);
    }
    put_u32(
        &mut out,
        checked_u32(
            object_transform_keyframes.len(),
            "object transform keyframe count",
        )?,
    );
    for record in object_transform_keyframes {
        put_f32(&mut out, record.frame);
        put_f32(&mut out, record.value);
        put_f32(&mut out, record.back[0]);
        put_f32(&mut out, record.back[1]);
        put_f32(&mut out, record.front[0]);
        put_f32(&mut out, record.front[1]);
        put_u32(&mut out, record.flags);
    }
    Ok(out)
}

pub(super) fn decode_timelines(data: &[u8]) -> Result<SceneTimelineRecords, SceneBinaryError> {
    let mut decoder = TimelineDecoder::new(data);
    let layer_count = decoder.u32()? as usize;
    let mut object_animation_layers = Vec::with_capacity(layer_count);
    for _ in 0..layer_count {
        object_animation_layers.push(SceneObjectAnimationLayerRecord {
            object: SceneObjectHandle(decoder.u32()?),
            animation_id: decoder.u32()?,
            layer_index: decoder.u32()?,
            additive: decoder.bool()?,
            autosort: decoder.bool()?,
            visible: decoder.bool()?,
            playback_rate: decoder.f32()?,
            blend_weight: decoder.f32()?,
            initial_progress: decoder.f32()?,
        });
    }
    let clip_count = decoder.u32()? as usize;
    let mut puppet_animation_clips = Vec::with_capacity(clip_count);
    for _ in 0..clip_count {
        puppet_animation_clips.push(ScenePuppetAnimationClipRecord {
            puppet: decoder.u32()?,
            clip_id: decoder.u32()?,
            flags: decoder.u32()?,
            name: SceneStringId(decoder.u32()?),
            playback: SceneStringId(decoder.u32()?),
            fps: decoder.f32()?,
            frame_count: decoder.u32()?,
            frame_metadata: decoder.u32()?,
            track_start: decoder.u32()?,
            track_count: decoder.u32()?,
        });
    }
    let track_count = decoder.u32()? as usize;
    let mut puppet_animation_tracks = Vec::with_capacity(track_count);
    for _ in 0..track_count {
        puppet_animation_tracks.push(ScenePuppetAnimationTrackRecord {
            clip: decoder.u32()?,
            bone_index: decoder.u32()?,
            track_flags: decoder.u32()?,
            sample_start: decoder.u32()?,
            sample_count: decoder.u32()?,
            opacity_flags: decoder.u32()?,
            opacity_sample_start: decoder.u32()?,
            opacity_sample_count: decoder.u32()?,
        });
    }
    let sample_count = decoder.u32()? as usize;
    let mut puppet_animation_transform_samples = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        puppet_animation_transform_samples.push(ScenePuppetAnimationTransformSampleRecord {
            translation: decoder.vec3()?,
            rotation: decoder.vec3()?,
            scale: decoder.vec3()?,
        });
    }
    let opacity_sample_count = decoder.u32()? as usize;
    let mut puppet_animation_opacity_samples = Vec::with_capacity(opacity_sample_count);
    for _ in 0..opacity_sample_count {
        puppet_animation_opacity_samples.push(decoder.f32()?);
    }
    let mut object_transform_tracks = Vec::new();
    let mut object_transform_channels = Vec::new();
    let mut object_transform_keyframes = Vec::new();
    if decoder.remaining() != 0 {
        let object_track_count = decoder.u32()? as usize;
        object_transform_tracks.reserve(object_track_count);
        for _ in 0..object_track_count {
            let object = SceneObjectHandle(decoder.u32()?);
            let property_value = decoder.u32()?;
            object_transform_tracks.push(SceneObjectTransformTrackRecord {
                object,
                property: SceneObjectTransformProperty::from_u32(property_value).ok_or(
                    SceneBinaryError::InvalidChunkValue(
                        "object transform property",
                        property_value,
                    ),
                )?,
                flags: decoder.u32()?,
                playback: SceneStringId(decoder.u32()?),
                fps: decoder.f32()?,
                frame_count: decoder.u32()?,
                channel_start: decoder.u32()?,
                channel_count: decoder.u32()?,
            });
        }
        let object_channel_count = decoder.u32()? as usize;
        object_transform_channels.reserve(object_channel_count);
        for _ in 0..object_channel_count {
            let track = decoder.u32()?;
            let component = decoder.u32()?;
            let kind_value = decoder.u32()?;
            object_transform_channels.push(SceneObjectTransformChannelRecord {
                track,
                component,
                kind: SceneObjectTransformChannelKind::from_u32(kind_value).ok_or(
                    SceneBinaryError::InvalidChunkValue(
                        "object transform channel kind",
                        kind_value,
                    ),
                )?,
                offset: decoder.f32()?,
                amplitude: decoder.f32()?,
                frequency: decoder.f32()?,
                phase: decoder.f32()?,
                keyframe_start: decoder.u32()?,
                keyframe_count: decoder.u32()?,
            });
        }
        let object_keyframe_count = decoder.u32()? as usize;
        object_transform_keyframes.reserve(object_keyframe_count);
        for _ in 0..object_keyframe_count {
            object_transform_keyframes.push(SceneObjectTransformKeyframeRecord {
                frame: decoder.f32()?,
                value: decoder.f32()?,
                back: [decoder.f32()?, decoder.f32()?],
                front: [decoder.f32()?, decoder.f32()?],
                flags: decoder.u32()?,
            });
        }
    }
    Ok(SceneTimelineRecords {
        object_animation_layers,
        puppet_animation_clips,
        puppet_animation_tracks,
        puppet_animation_transform_samples,
        puppet_animation_opacity_samples,
        object_transform_tracks,
        object_transform_channels,
        object_transform_keyframes,
    })
}

fn checked_u32(value: usize, name: &'static str) -> Result<u32, SceneBinaryError> {
    u32::try_from(value).map_err(|_| SceneBinaryError::SizeOverflow(name))
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_bool(out: &mut Vec<u8>, value: bool) {
    out.push(u8::from(value));
}

fn put_vec3(out: &mut Vec<u8>, value: SceneVec3) {
    put_f32(out, value.x);
    put_f32(out, value.y);
    put_f32(out, value.z);
}

struct TimelineDecoder<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> TimelineDecoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], SceneBinaryError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(SceneBinaryError::SizeOverflow("timeline decoder offset"))?;
        if end > self.data.len() {
            return Err(SceneBinaryError::Truncated("timeline"));
        }
        let bytes = &self.data[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32, SceneBinaryError> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().expect("u32 slice"),
        ))
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    fn f32(&mut self) -> Result<f32, SceneBinaryError> {
        Ok(f32::from_le_bytes(
            self.bytes(4)?.try_into().expect("f32 slice"),
        ))
    }

    fn bool(&mut self) -> Result<bool, SceneBinaryError> {
        Ok(*self
            .bytes(1)?
            .first()
            .ok_or(SceneBinaryError::Truncated("timeline bool"))?
            != 0)
    }

    fn vec3(&mut self) -> Result<SceneVec3, SceneBinaryError> {
        Ok(SceneVec3 {
            x: self.f32()?,
            y: self.f32()?,
            z: self.f32()?,
        })
    }
}
