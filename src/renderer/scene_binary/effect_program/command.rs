//! WE effect command lowering.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use crate::core::scene::binary::{
    SCENE_BINARY_NONE_ID, SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, SceneBinaryChunkKind,
    SceneBinaryEffectPassRecord, decode_texture_slot_record,
};
use crate::engine::scene_engine::{
    SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectCommand, SceneEffectCopyCommand,
    SceneEffectImageRef, SceneEffectMaterialPass, SceneEffectPassBlend, SceneEffectSwapCommand,
    SceneEffectTextureResourceBinding, SceneResourceId,
};
use crate::renderer::RendererPlanError;

use super::super::facts::{BinarySceneNames, BinarySceneResource, binary_name};
use super::super::reader::BinarySceneReader;
use super::parameters::GscnEffectPassParameters;

pub(super) fn gscn_effect_command(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    pass: SceneBinaryEffectPassRecord,
    parameters: GscnEffectPassParameters,
) -> Result<SceneEffectCommand, RendererPlanError> {
    let command = binary_name(names, pass.command_name);
    if command
        .map(|command| command.eq_ignore_ascii_case("swap"))
        .unwrap_or(false)
    {
        let a = binary_name(names, pass.source_name)
            .map(SceneEffectImageRef::from_we_name)
            .ok_or_else(|| {
                RendererPlanError::PackageLoad(format!(
                    "WE effect swap pass {} is missing source FBO",
                    pass.pass_index
                ))
            })?;
        let b = binary_name(names, pass.target_name)
            .map(SceneEffectImageRef::from_we_name)
            .ok_or_else(|| {
                RendererPlanError::PackageLoad(format!(
                    "WE effect swap pass {} is missing target FBO",
                    pass.pass_index
                ))
            })?;
        return Ok(SceneEffectCommand::Swap(SceneEffectSwapCommand {
            pass_index: pass.pass_index as usize,
            a,
            b,
        }));
    }
    if command
        .map(|command| command.eq_ignore_ascii_case("copy"))
        .unwrap_or(false)
    {
        let source = binary_name(names, pass.source_name)
            .map(SceneEffectImageRef::from_we_name)
            .ok_or_else(|| {
                RendererPlanError::PackageLoad(format!(
                    "WE effect copy pass {} is missing source FBO",
                    pass.pass_index
                ))
            })?;
        let target = binary_name(names, pass.target_name)
            .map(SceneEffectImageRef::from_we_name)
            .ok_or_else(|| {
                RendererPlanError::PackageLoad(format!(
                    "WE effect copy pass {} is missing target FBO",
                    pass.pass_index
                ))
            })?;
        return Ok(SceneEffectCommand::Copy(SceneEffectCopyCommand {
            pass_index: pass.pass_index as usize,
            source,
            target,
        }));
    }

    Ok(SceneEffectCommand::MaterialPass(SceneEffectMaterialPass {
        pass_index: pass.pass_index as usize,
        shader: binary_name(names, pass.shader_name).map(str::to_owned),
        source: binary_name(names, pass.source_name).map(SceneEffectImageRef::from_we_name),
        target: binary_name(names, pass.target_name).map(SceneEffectImageRef::from_we_name),
        blend: SceneEffectPassBlend::from_we_name(binary_name(names, pass.blending_name)),
        depth_test: gscn_depth_test(pass.depth_test),
        depth_write: gscn_depth_write(pass.depth_write),
        cull_mode: gscn_cull_mode(pass.cull_mode),
        alpha_write: gscn_alpha_write(pass.alpha_write),
        texture_resources: gscn_effect_texture_resources(reader, resources, pass)?,
        binds: parameters.binds,
        combos: parameters.combos,
        constants: parameters.constants,
    }))
}

fn gscn_effect_texture_resources(
    reader: &mut BinarySceneReader,
    resources: &[BinarySceneResource],
    pass: SceneBinaryEffectPassRecord,
) -> Result<Vec<SceneEffectTextureResourceBinding>, RendererPlanError> {
    let texture_slots = reader.record_range(
        SceneBinaryChunkKind::TextureSlots,
        SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
        pass.first_texture_slot,
        pass.texture_slot_count,
        decode_texture_slot_record,
    )?;
    let mut bindings = Vec::new();
    for slot in texture_slots {
        let Some(resource) = resources.get(slot.resource_index as usize) else {
            continue;
        };
        if resource.source.is_none() {
            continue;
        }
        bindings.push(SceneEffectTextureResourceBinding {
            slot: slot.slot,
            resource: gscn_scene_resource_id(slot.resource_index as usize, resource),
        });
    }
    Ok(bindings)
}

fn gscn_scene_resource_id(index: usize, resource: &BinarySceneResource) -> SceneResourceId {
    SceneResourceId(if resource.id_name != SCENE_BINARY_NONE_ID {
        resource.id_name
    } else {
        index.min(u32::MAX as usize) as u32
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
