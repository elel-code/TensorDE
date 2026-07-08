//! Puppet clip and animation-layer decoding.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::sync::Arc;

use crate::core::scene::binary::{
    SCENE_BINARY_PUPPET_CLIP_FLAG_LOOPING, SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
    SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE, SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE,
    SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS, SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE,
    SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE, SceneBinaryChunkKind, SceneBinaryPuppetRecord,
    decode_puppet_clip_record, decode_puppet_frame_record, decode_puppet_layer_record,
};
use crate::core::scene::{
    ScenePuppetAnimationBone, ScenePuppetAnimationClip, ScenePuppetAnimationLayer,
};
use crate::renderer::RendererPlanError;

use super::super::super::reader::BinarySceneReader;

pub(in crate::renderer::scene_binary::mesh) fn binary_scene_puppet_clips_cached(
    reader: &mut BinarySceneReader,
    puppet_index: u32,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Arc<Vec<ScenePuppetAnimationClip>>, RendererPlanError> {
    if let Some(clips) = reader.puppet_clips_cache.get(&puppet_index) {
        return Ok(Arc::clone(clips));
    }
    let clips = Arc::new(binary_scene_puppet_clips(reader, puppet)?);
    reader
        .puppet_clips_cache
        .insert(puppet_index, Arc::clone(&clips));
    Ok(clips)
}

pub(super) fn binary_scene_puppet_layers_cached(
    reader: &mut BinarySceneReader,
    puppet_index: u32,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Arc<Vec<ScenePuppetAnimationLayer>>, RendererPlanError> {
    if let Some(layers) = reader.puppet_layers_cache.get(&puppet_index) {
        return Ok(Arc::clone(layers));
    }
    let layers = Arc::new(binary_scene_puppet_layers(reader, puppet)?);
    reader
        .puppet_layers_cache
        .insert(puppet_index, Arc::clone(&layers));
    Ok(layers)
}

pub(in crate::renderer::scene_binary) fn binary_scene_puppet_clips(
    reader: &mut BinarySceneReader,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Vec<ScenePuppetAnimationClip>, RendererPlanError> {
    let clip_records = reader.record_range(
        SceneBinaryChunkKind::PuppetClips,
        SCENE_BINARY_PUPPET_CLIP_RECORD_SIZE,
        puppet.first_clip,
        puppet.clip_count,
        decode_puppet_clip_record,
    )?;
    let mut clips = Vec::with_capacity(clip_records.len());
    for clip in clip_records {
        let frame_records = reader.record_range(
            SceneBinaryChunkKind::PuppetFrames,
            SCENE_BINARY_PUPPET_FRAME_RECORD_SIZE,
            clip.first_frame,
            clip.frame_record_count,
            decode_puppet_frame_record,
        )?;
        let mut bones = (0..clip.bone_count)
            .map(|_| ScenePuppetAnimationBone { frames: Vec::new() })
            .collect::<Vec<_>>();
        for frame in frame_records {
            if let Some(bone) = bones.get_mut(frame.bone_index as usize) {
                bone.frames.push(frame.transform);
            }
        }
        clips.push(ScenePuppetAnimationClip {
            id: clip.clip_id,
            name: None,
            fps: f64::from(clip.fps),
            frame_count: clip.frame_count,
            looping: clip.flags & SCENE_BINARY_PUPPET_CLIP_FLAG_LOOPING != 0,
            bones,
        });
    }
    Ok(clips)
}

pub(in crate::renderer::scene_binary) fn binary_scene_puppet_layers(
    reader: &mut BinarySceneReader,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Vec<ScenePuppetAnimationLayer>, RendererPlanError> {
    let layer_records = reader.record_range(
        SceneBinaryChunkKind::PuppetLayers,
        SCENE_BINARY_PUPPET_LAYER_RECORD_SIZE,
        puppet.first_layer,
        puppet.animation_layer_count,
        decode_puppet_layer_record,
    )?;
    let mut layers = Vec::with_capacity(layer_records.len());
    for layer in layer_records {
        layers.push(ScenePuppetAnimationLayer {
            clip_id: layer.clip_id,
            name: None,
            additive: layer.flags & SCENE_BINARY_PUPPET_LAYER_FLAG_ADDITIVE != 0,
            lock_transforms: layer.flags & SCENE_BINARY_PUPPET_LAYER_FLAG_LOCK_TRANSFORMS != 0,
            blend: f64::from(layer.blend),
            visible: layer.flags & SCENE_BINARY_PUPPET_LAYER_FLAG_VISIBLE != 0,
            rate: f64::from(layer.rate),
            initial_phase: f64::from(layer.initial_phase),
        });
    }
    Ok(layers)
}
