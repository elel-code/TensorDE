//! `.gscn` effect pass lowering into engine-owned effect programs.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use std::collections::BTreeMap;

use crate::core::scene::binary::{
    SCENE_BINARY_NONE_ID, SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO,
    SCENE_BINARY_PARAMETER_ROLE_PASS_BIND, SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO,
    SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT, SCENE_BINARY_PARAMETER_VALUE_BOOL,
    SCENE_BINARY_PARAMETER_VALUE_FLOAT, SCENE_BINARY_PARAMETER_VALUE_INTEGER,
    SCENE_BINARY_PARAMETER_VALUE_STRING, SCENE_BINARY_PARAMETER_VALUE_VEC2,
    SCENE_BINARY_PARAMETER_VALUE_VEC3, SCENE_BINARY_PARAMETER_VALUE_VEC4,
    SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, SceneBinaryChunkKind, SceneBinaryEffectParameterRecord,
    SceneBinaryEffectPassRecord, SceneBinaryMaterialPassRecord, decode_effect_parameter_record,
    decode_effect_pass_record, decode_texture_slot_record,
};
use crate::engine::scene_engine::we::WeEffectKind;
use crate::engine::scene_engine::{
    SceneCullMode, SceneDepthTest, SceneEffectCommand, SceneEffectConstantValue,
    SceneEffectCopyCommand, SceneEffectFboBinding, SceneEffectFboFormat, SceneEffectImageRef,
    SceneEffectMaterialPass, SceneEffectPassBlend, SceneEffectProgram, SceneEffectSwapCommand,
    SceneEffectTextureResourceBinding, SceneGraphTarget, SceneResourceId,
};
use crate::renderer::RendererPlanError;

use super::facts::{BinarySceneNames, BinarySceneResource, binary_name};
use super::reader::BinarySceneReader;

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

struct GscnEffectPassParameters {
    fbos: Vec<GscnEffectFboFact>,
    binds: BTreeMap<u32, SceneEffectImageRef>,
    combos: BTreeMap<String, i64>,
    constants: BTreeMap<String, SceneEffectConstantValue>,
}

#[derive(Clone)]
struct GscnEffectFboFact {
    name: String,
    format: Option<SceneEffectFboFormat>,
    scale: f32,
    unique: bool,
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

fn gscn_effect_pass_parameters(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    pass: SceneBinaryEffectPassRecord,
) -> Result<GscnEffectPassParameters, RendererPlanError> {
    let parameters = reader.record_range(
        SceneBinaryChunkKind::EffectParameter,
        crate::core::scene::binary::SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
        pass.first_parameter,
        pass.parameter_count,
        decode_effect_parameter_record,
    )?;
    let mut fbos = Vec::new();
    let mut binds = BTreeMap::new();
    let mut combos = BTreeMap::new();
    let mut constants = BTreeMap::new();
    for parameter in parameters {
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO != 0 {
            if let Some(name) = binary_name(names, parameter.parameter_name) {
                fbos.push(GscnEffectFboFact {
                    name: name.to_owned(),
                    format: binary_name(names, parameter.value_name)
                        .map(SceneEffectFboFormat::from_we_name),
                    scale: if parameter.value0.is_finite() && parameter.value0 > 0.0 {
                        parameter.value0
                    } else {
                        1.0
                    },
                    unique: parameter.integer_value != 0,
                });
            }
            continue;
        }
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_BIND != 0 {
            let slot = u32::try_from(parameter.integer_value)
                .ok()
                .or_else(|| {
                    binary_name(names, parameter.parameter_name).and_then(|name| name.parse().ok())
                })
                .unwrap_or(0);
            if let Some(name) = binary_name(names, parameter.value_name) {
                binds.insert(slot, SceneEffectImageRef::from_we_name(name));
            }
            continue;
        }
        let Some(name) = binary_name(names, parameter.parameter_name) else {
            continue;
        };
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO != 0 {
            combos.insert(name.to_owned(), parameter.integer_value);
            continue;
        }
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT != 0
            && let Some(value) = gscn_effect_constant_value(names, parameter)?
        {
            constants.insert(name.to_owned(), value);
        }
    }
    Ok(GscnEffectPassParameters {
        fbos,
        binds,
        combos,
        constants,
    })
}

fn gscn_effect_command(
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

fn gscn_effect_constant_value(
    names: &BinarySceneNames,
    parameter: SceneBinaryEffectParameterRecord,
) -> Result<Option<SceneEffectConstantValue>, RendererPlanError> {
    let finite = |values: &[f32]| values.iter().all(|value| value.is_finite());
    let value = match parameter.value_kind {
        SCENE_BINARY_PARAMETER_VALUE_BOOL => {
            SceneEffectConstantValue::Bool(parameter.integer_value != 0)
        }
        SCENE_BINARY_PARAMETER_VALUE_FLOAT if finite(&[parameter.value0]) => {
            SceneEffectConstantValue::Float(parameter.value0)
        }
        SCENE_BINARY_PARAMETER_VALUE_INTEGER => {
            SceneEffectConstantValue::Integer(parameter.integer_value)
        }
        SCENE_BINARY_PARAMETER_VALUE_STRING => {
            let Some(value) = binary_name(names, parameter.value_name) else {
                return Ok(None);
            };
            SceneEffectConstantValue::String(value.to_owned())
        }
        SCENE_BINARY_PARAMETER_VALUE_VEC2 if finite(&[parameter.value0, parameter.value1]) => {
            SceneEffectConstantValue::Vec2([parameter.value0, parameter.value1])
        }
        SCENE_BINARY_PARAMETER_VALUE_VEC3
            if finite(&[parameter.value0, parameter.value1, parameter.value2]) =>
        {
            SceneEffectConstantValue::Vec3([parameter.value0, parameter.value1, parameter.value2])
        }
        SCENE_BINARY_PARAMETER_VALUE_VEC4
            if finite(&[
                parameter.value0,
                parameter.value1,
                parameter.value2,
                parameter.value3,
            ]) =>
        {
            SceneEffectConstantValue::Vec4([
                parameter.value0,
                parameter.value1,
                parameter.value2,
                parameter.value3,
            ])
        }
        SCENE_BINARY_PARAMETER_VALUE_FLOAT
        | SCENE_BINARY_PARAMETER_VALUE_VEC2
        | SCENE_BINARY_PARAMETER_VALUE_VEC3
        | SCENE_BINARY_PARAMETER_VALUE_VEC4 => {
            return Err(RendererPlanError::PackageLoad(format!(
                "WE effect parameter {:?} contains non-finite float lanes",
                binary_name(names, parameter.parameter_name).unwrap_or("<unnamed>")
            )));
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}

fn gscn_scene_resource_id(index: usize, resource: &BinarySceneResource) -> SceneResourceId {
    SceneResourceId(if resource.id_name != SCENE_BINARY_NONE_ID {
        resource.id_name
    } else {
        index.min(u32::MAX as usize) as u32
    })
}

fn gscn_we_effect_kind(kind: u16, effect_file: &str) -> WeEffectKind {
    let file = effect_file.replace('\\', "/").to_ascii_lowercase();
    match kind {
        1 => WeEffectKind::Opacity,
        2 => WeEffectKind::Iris,
        3 => WeEffectKind::WaterRipple,
        4 => WeEffectKind::WaterWaves,
        5 => WeEffectKind::WaterFlow,
        7 if file.contains("foliagesway")
            || file.contains("foliage_sway")
            || file.contains("auto_sway")
            || file.contains("autosway") =>
        {
            WeEffectKind::FoliageSway
        }
        _ if file.contains("opacity") => WeEffectKind::Opacity,
        _ if file.contains("iris") => WeEffectKind::Iris,
        _ if file.contains("waterripple") || file.contains("water_ripple") => {
            WeEffectKind::WaterRipple
        }
        _ if file.contains("waterwaves") || file.contains("water_waves") => {
            WeEffectKind::WaterWaves
        }
        _ if file.contains("waterflow") || file.contains("water_flow") => WeEffectKind::WaterFlow,
        _ if file.contains("foliagesway")
            || file.contains("foliage_sway")
            || file.contains("auto_sway")
            || file.contains("autosway") =>
        {
            WeEffectKind::FoliageSway
        }
        _ if file.contains("scroll") => WeEffectKind::Scroll,
        _ if file.contains("skew") => WeEffectKind::Skew,
        _ if file.contains("tint") => WeEffectKind::Tint,
        _ => WeEffectKind::Unknown,
    }
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
