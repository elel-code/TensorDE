//! `.gscn` material fact lowering for the scene engine ingest boundary.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use crate::core::scene::binary::{
    SceneBinaryChunkKind, SceneBinaryMaterialPassRecord, decode_effect_pass_record,
};
use crate::engine::scene_engine::ingest::gscn::GscnMaterialFact;
use crate::engine::scene_engine::{SceneAlphaWriteMode, SceneCullMode, SceneDepthTest};
use crate::renderer::RendererPlanError;

use super::super::facts::{BinarySceneNames, binary_name};
use super::super::reader::BinarySceneReader;

pub(super) fn gscn_material_fact(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    material: Option<SceneBinaryMaterialPassRecord>,
) -> Result<GscnMaterialFact, RendererPlanError> {
    Ok(GscnMaterialFact {
        shader: gscn_effect_shader_name(reader, names, material)?,
        blending: material
            .and_then(|material| binary_name(names, material.blending_name))
            .map(str::to_owned),
        depth_test: material
            .map(|material| gscn_depth_test(material.depth_test))
            .unwrap_or(SceneDepthTest::Disabled),
        depth_write: material
            .map(|material| gscn_depth_write(material.depth_write))
            .unwrap_or(false),
        cull_mode: material
            .map(|material| gscn_cull_mode(material.cull_mode))
            .unwrap_or(SceneCullMode::None),
        alpha_write: material
            .map(|material| gscn_alpha_write(material.alpha_write))
            .unwrap_or(SceneAlphaWriteMode::Default),
    })
}

fn gscn_depth_test(code: u16) -> SceneDepthTest {
    match code {
        1 => SceneDepthTest::LessEqual,
        2 => SceneDepthTest::Disabled,
        _ => SceneDepthTest::Disabled,
    }
}

fn gscn_depth_write(code: u16) -> bool {
    matches!(code, 1)
}

fn gscn_cull_mode(code: u16) -> SceneCullMode {
    match code {
        2 => SceneCullMode::Back,
        3 => SceneCullMode::Front,
        _ => SceneCullMode::None,
    }
}

fn gscn_alpha_write(code: u16) -> SceneAlphaWriteMode {
    match code {
        1 => SceneAlphaWriteMode::Enabled,
        2 => SceneAlphaWriteMode::Disabled,
        _ => SceneAlphaWriteMode::Default,
    }
}

fn gscn_effect_shader_name(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    material: Option<SceneBinaryMaterialPassRecord>,
) -> Result<Option<String>, RendererPlanError> {
    if let Some(material) = material
        && material.effect_pass_count > 0
    {
        let passes = reader.record_range(
            SceneBinaryChunkKind::EffectPass,
            reader.layout_record_size(SceneBinaryChunkKind::EffectPass)?,
            material.first_effect_pass,
            material.effect_pass_count,
            decode_effect_pass_record,
        )?;
        if let Some(shader) = passes
            .iter()
            .rev()
            .find_map(|pass| binary_name(names, pass.shader_name))
        {
            return Ok(Some(shader.to_owned()));
        }
    }
    Ok(None)
}
