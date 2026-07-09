//! Timeline chunk codec for scene binary records.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/model-and-animation.md`

use super::SceneBinaryError;
use crate::engine::scene::abi::{
    SceneObjectAnimationLayerRecord, SceneObjectHandle, ScenePuppetAnimationClipRecord,
    ScenePuppetAnimationTrackRecord, ScenePuppetAnimationTransformSampleRecord, SceneStringId,
    SceneVec3,
};

pub(super) struct SceneTimelineRecords {
    pub object_animation_layers: Vec<SceneObjectAnimationLayerRecord>,
    pub puppet_animation_clips: Vec<ScenePuppetAnimationClipRecord>,
    pub puppet_animation_tracks: Vec<ScenePuppetAnimationTrackRecord>,
    pub puppet_animation_transform_samples: Vec<ScenePuppetAnimationTransformSampleRecord>,
}

pub(super) fn encode_timelines(
    animation_layers: &[SceneObjectAnimationLayerRecord],
    puppet_clips: &[ScenePuppetAnimationClipRecord],
    puppet_tracks: &[ScenePuppetAnimationTrackRecord],
    transform_samples: &[ScenePuppetAnimationTransformSampleRecord],
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
    Ok(SceneTimelineRecords {
        object_animation_layers,
        puppet_animation_clips,
        puppet_animation_tracks,
        puppet_animation_transform_samples,
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
