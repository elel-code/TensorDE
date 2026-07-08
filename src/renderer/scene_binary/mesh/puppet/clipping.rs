//! Puppet clipping record and active-source decoding.
//!
//! References:
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `references/godot/servers/rendering/storage/`

use std::path::Path;

use crate::core::scene::binary::{
    SCENE_BINARY_PUPPET_ACTIVE_SOURCE_RECORD_SIZE, SCENE_BINARY_PUPPET_CLIPPING_BONE_RECORD_SIZE,
    SCENE_BINARY_PUPPET_CLIPPING_FRAME_KEY_RECORD_SIZE, SCENE_BINARY_PUPPET_CLIPPING_RECORD_SIZE,
    SceneBinaryChunkKind, SceneBinaryPuppetRecord, decode_puppet_active_source_record,
    decode_puppet_clipping_bone_record, decode_puppet_clipping_frame_key_record,
    decode_puppet_clipping_record,
};
use crate::core::scene::{SceneMeshPuppetClippingActiveSource, SceneMeshPuppetClippingRecord};
use crate::renderer::RendererPlanError;

use super::super::super::facts::{BinarySceneNames, binary_name, binary_scene_resource_path};
use super::super::super::reader::BinarySceneReader;

pub(in crate::renderer::scene_binary) fn binary_scene_puppet_clipping_records(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Vec<SceneMeshPuppetClippingRecord>, RendererPlanError> {
    let clipping_records = reader.record_range(
        SceneBinaryChunkKind::PuppetClipping,
        SCENE_BINARY_PUPPET_CLIPPING_RECORD_SIZE,
        puppet.first_clipping_record,
        puppet.clipping_record_count,
        decode_puppet_clipping_record,
    )?;
    let mut records = Vec::with_capacity(clipping_records.len());
    for clipping in clipping_records {
        let Some(mask) = binary_name(names, clipping.mask_name) else {
            continue;
        };
        let source_name = binary_name(names, clipping.owner_name).map(str::to_owned);
        let bone_records = reader.record_range(
            SceneBinaryChunkKind::PuppetClippingBones,
            SCENE_BINARY_PUPPET_CLIPPING_BONE_RECORD_SIZE,
            clipping.first_bone,
            clipping.bone_count,
            decode_puppet_clipping_bone_record,
        )?;
        let frame_key_records = reader.record_range(
            SceneBinaryChunkKind::PuppetClippingFrameKeys,
            SCENE_BINARY_PUPPET_CLIPPING_FRAME_KEY_RECORD_SIZE,
            clipping.first_frame_key,
            clipping.frame_key_count,
            decode_puppet_clipping_frame_key_record,
        )?;
        records.push(SceneMeshPuppetClippingRecord {
            source_name,
            mask: mask.to_owned(),
            mask_resource: binary_scene_puppet_clipping_mask_resource(reader, mask),
            duration_frames: clipping.duration_frames,
            flags: clipping.flags,
            bones: bone_records
                .iter()
                .map(|bone| bone.bone_index as usize)
                .collect(),
            frame_keys: frame_key_records
                .iter()
                .map(|frame_key| frame_key.frame_key)
                .collect(),
        });
    }
    Ok(records)
}

pub(in crate::renderer::scene_binary) fn binary_scene_puppet_active_sources(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    puppet: SceneBinaryPuppetRecord,
) -> Result<Vec<SceneMeshPuppetClippingActiveSource>, RendererPlanError> {
    let records = reader.record_range(
        SceneBinaryChunkKind::PuppetActiveSources,
        SCENE_BINARY_PUPPET_ACTIVE_SOURCE_RECORD_SIZE,
        puppet.first_active_source,
        puppet.active_source_count,
        decode_puppet_active_source_record,
    )?;
    let mut sources = Vec::with_capacity(records.len());
    for record in records {
        let Some(source_name) = binary_name(names, record.source_name) else {
            continue;
        };
        sources.push(SceneMeshPuppetClippingActiveSource {
            source_name: source_name.to_owned(),
            source_id: record.source_id,
            scalar_bits: record.scalar_bits,
            source_scale: record.source_scale,
            flags: record.flags,
            transform_index: record.transform_index,
            parameter0: record.parameter0,
            parameter1: record.parameter1,
        });
    }
    Ok(sources)
}

fn binary_scene_puppet_clipping_mask_resource(
    reader: &BinarySceneReader,
    mask: &str,
) -> Option<String> {
    if Path::new(mask).is_absolute()
        || mask.ends_with(".gtex")
        || mask.starts_with("assets/")
        || mask.starts_with("assets\\")
    {
        Some(
            binary_scene_resource_path(&reader.package_root, mask)
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        None
    }
}
