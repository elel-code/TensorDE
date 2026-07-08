//! Material texture-slot lowering for binary scene render layers.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `references/godot/servers/rendering/storage/`

use std::sync::Arc;

use crate::core::scene::binary::{
    SCENE_BINARY_NONE_ID, SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, SceneBinaryChunkKind,
    SceneBinaryMaterialPassRecord, SceneBinaryTextureSlotRecord, decode_texture_slot_record,
};
use crate::renderer::{RendererPlanError, SceneRenderAlphaTextureMode, SceneRenderTextureSlot};

use super::super::super::facts::BinarySceneResource;
use super::super::super::reader::BinarySceneReader;
use super::super::super::schema::BINARY_TEXTURE_ROLE_BASE_COLOR;

pub(in crate::renderer::scene_binary) fn binary_scene_material_texture_slots_cached(
    reader: &mut BinarySceneReader,
    material_index: u32,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    if let Some(slots) = reader.material_texture_slots_cache.get(&material_index) {
        return Ok((**slots).clone());
    }
    let slots = Arc::new(binary_scene_material_texture_slots(
        reader, material, resources,
    )?);
    reader
        .material_texture_slots_cache
        .insert(material_index, Arc::clone(&slots));
    Ok((*slots).clone())
}

pub(in crate::renderer::scene_binary::render_layers) fn binary_scene_alpha_texture_slot(
    material: SceneBinaryMaterialPassRecord,
) -> Option<u32> {
    (material.alpha_texture_slot != SCENE_BINARY_NONE_ID).then_some(material.alpha_texture_slot)
}

pub(in crate::renderer::scene_binary::render_layers) fn binary_scene_alpha_texture_mode(
    material: SceneBinaryMaterialPassRecord,
) -> SceneRenderAlphaTextureMode {
    match material.alpha_texture_mode {
        2 => SceneRenderAlphaTextureMode::Inverse,
        3 => SceneRenderAlphaTextureMode::Iris,
        4 => SceneRenderAlphaTextureMode::Coverage,
        _ => SceneRenderAlphaTextureMode::Multiply,
    }
}

pub(super) fn binary_scene_texture_slots(
    slots: Vec<SceneBinaryTextureSlotRecord>,
    resources: &[BinarySceneResource],
    keep: impl Fn(&SceneBinaryTextureSlotRecord) -> bool,
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    let mut output = Vec::with_capacity(slots.len());
    for slot in slots {
        if !keep(&slot) {
            continue;
        }
        let Some(resource) = resources.get(slot.resource_index as usize) else {
            continue;
        };
        let Some(source) = resource.source.clone() else {
            continue;
        };
        output.push(SceneRenderTextureSlot {
            slot: slot.slot,
            source,
            width: resource.width.or((slot.width > 0).then_some(slot.width)),
            height: resource.height.or((slot.height > 0).then_some(slot.height)),
        });
    }
    Ok(output)
}

fn binary_scene_material_texture_slots(
    reader: &mut BinarySceneReader,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    let slots = reader.record_range(
        SceneBinaryChunkKind::TextureSlots,
        SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
        material.first_texture_slot,
        material.texture_slot_count,
        decode_texture_slot_record,
    )?;
    binary_scene_texture_slots(slots, resources, |slot| {
        slot.role_flags & BINARY_TEXTURE_ROLE_BASE_COLOR != 0
    })
}
