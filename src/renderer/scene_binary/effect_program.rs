//! `.gscn` effect pass lowering into engine-owned effect programs.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use crate::core::scene::binary::{
    SceneBinaryChunkKind, SceneBinaryMaterialPassRecord, decode_effect_pass_record,
};
use crate::engine::scene_engine::we::WeEffectKind;
use crate::engine::scene_engine::{
    SceneEffectCommand, SceneEffectFboBinding, SceneEffectProgram, SceneGraphTarget,
};
use crate::renderer::RendererPlanError;

use super::facts::{BinarySceneNames, BinarySceneResource, binary_name};
use super::reader::BinarySceneReader;

mod command;
mod kind;
mod parameters;

use command::gscn_effect_command;
use kind::gscn_we_effect_kind;
use parameters::{GscnEffectFboFact, gscn_effect_pass_parameters};

pub(in crate::renderer) fn gscn_effect_programs(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    material: Option<SceneBinaryMaterialPassRecord>,
    next_named_fbo: &mut u32,
) -> Result<Vec<SceneEffectProgram>, RendererPlanError> {
    let Some(material) = material else {
        return Ok(Vec::new());
    };
    if material.effect_pass_count == 0 {
        return Ok(Vec::new());
    }

    let passes = reader.record_range(
        SceneBinaryChunkKind::EffectPass,
        reader.layout_record_size(SceneBinaryChunkKind::EffectPass)?,
        material.first_effect_pass,
        material.effect_pass_count,
        decode_effect_pass_record,
    )?;
    let mut programs = Vec::<GscnEffectProgramBuilder>::new();
    for pass in passes {
        let effect_file = binary_name(names, pass.effect_name)
            .unwrap_or("")
            .to_owned();
        let program_index = programs
            .iter()
            .position(|program| program.effect_name == pass.effect_name)
            .unwrap_or_else(|| {
                programs.push(GscnEffectProgramBuilder {
                    effect_name: pass.effect_name,
                    effect_file: effect_file.clone(),
                    effect: gscn_we_effect_kind(pass.kind, &effect_file),
                    fbos: Vec::new(),
                    commands: Vec::new(),
                });
                programs.len() - 1
            });
        let parameters = gscn_effect_pass_parameters(reader, names, pass)?;
        for fbo in parameters.fbos.iter().cloned() {
            gscn_effect_program_push_fbo(&mut programs[program_index], fbo, next_named_fbo);
        }
        let command = gscn_effect_command(reader, names, resources, pass, parameters)?;
        programs[program_index].commands.push(command);
    }

    Ok(programs
        .into_iter()
        .map(|program| SceneEffectProgram {
            effect_file: program.effect_file,
            effect: program.effect,
            fbos: program.fbos,
            commands: program.commands,
        })
        .collect())
}

struct GscnEffectProgramBuilder {
    effect_name: u32,
    effect_file: String,
    effect: WeEffectKind,
    fbos: Vec<SceneEffectFboBinding>,
    commands: Vec<SceneEffectCommand>,
}

fn gscn_effect_program_push_fbo(
    program: &mut GscnEffectProgramBuilder,
    fbo: GscnEffectFboFact,
    next_named_fbo: &mut u32,
) {
    if program
        .fbos
        .iter()
        .any(|existing| existing.name == fbo.name)
    {
        return;
    }
    let target = SceneGraphTarget::NamedFbo(*next_named_fbo);
    *next_named_fbo = next_named_fbo.saturating_add(1);
    program.fbos.push(SceneEffectFboBinding {
        name: fbo.name,
        target,
        format: fbo.format,
        scale: fbo.scale,
        unique: fbo.unique,
    });
}
