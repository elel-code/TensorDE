//! Puppet attachment-delta sampling cache for legacy render-layer lowering.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::core::scene::binary::{SCENE_BINARY_NONE_ID, SceneBinaryPuppetRecord};
use crate::core::scene::{SceneMesh, ScenePuppetAttachmentDelta};
use crate::renderer::RendererPlanError;

use super::super::super::facts::BinarySceneNames;
use super::super::super::reader::BinarySceneReader;
use super::animation::{binary_scene_puppet_clips_cached, binary_scene_puppet_layers_cached};
use super::skin::binary_scene_puppet_skin;

pub(in crate::renderer::scene_binary) fn binary_scene_puppet_attachment_deltas(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    puppet_index: u32,
    snapshot_time_ms: u64,
) -> Result<Option<BTreeMap<String, ScenePuppetAttachmentDelta>>, RendererPlanError> {
    if puppet_index == SCENE_BINARY_NONE_ID {
        return Ok(None);
    }
    let cache_key = (puppet_index, snapshot_time_ms);
    if let Some(cached) = reader.puppet_attachment_delta_cache.get(&cache_key) {
        return Ok(cached.as_ref().map(|deltas| (**deltas).clone()));
    }
    let puppet = reader.puppet_record_cached(puppet_index)?;
    if puppet.attachment_count == 0
        || puppet.animation_layer_count == 0
        || puppet.bone_count == 0
        || puppet.clip_count == 0
    {
        reader.puppet_attachment_delta_cache.insert(cache_key, None);
        return Ok(None);
    }
    let mesh = binary_scene_puppet_attachment_mesh_cached(reader, names, puppet_index, puppet)?;
    if mesh
        .skin
        .as_ref()
        .is_none_or(|skin| skin.attachments.is_empty())
    {
        reader.puppet_attachment_delta_cache.insert(cache_key, None);
        return Ok(None);
    }
    let layers = binary_scene_puppet_layers_cached(reader, puppet_index, puppet)?;
    let clips = binary_scene_puppet_clips_cached(reader, puppet_index, puppet)?;
    let deltas = mesh.sample_puppet_attachment_deltas_with_clips(
        clips.as_slice(),
        layers.as_slice(),
        snapshot_time_ms,
    );
    reader.puppet_attachment_delta_cache.insert(
        cache_key,
        deltas.as_ref().map(|deltas| Arc::new(deltas.clone())),
    );
    Ok(deltas)
}

fn binary_scene_puppet_attachment_mesh_cached(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    puppet_index: u32,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Arc<SceneMesh>, RendererPlanError> {
    if let Some(mesh) = reader.puppet_attachment_mesh_cache.get(&puppet_index) {
        return Ok(Arc::clone(mesh));
    }
    let mesh = Arc::new(SceneMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        skin: Some(binary_scene_puppet_skin(reader, names, puppet, false)?),
        puppet_clips: Vec::new(),
        puppet_clipping_records: Vec::new(),
        puppet_clipping_active_sources: Vec::new(),
    });
    reader
        .puppet_attachment_mesh_cache
        .insert(puppet_index, Arc::clone(&mesh));
    Ok(mesh)
}
