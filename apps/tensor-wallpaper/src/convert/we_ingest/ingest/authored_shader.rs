//! Cold-compile package-owned Wallpaper Engine shaders into descriptor heap SPIR-V.
//!
//! The runtime receives only Slang O2 words and a compact binding ABI.
//! Package source, Rust specialization, compiler binaries, and validation
//! tools remain in this cold conversion path.

mod compiler_environment;
mod package_preprocess;
mod stage_io;
mod uniform_metadata;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vulkan_renderer_build::{
    DescriptorHeapBindingKind, ShaderCompileRequest, ShaderContract, ShaderIoDirection,
    ShaderScalarType, ShaderStage, SlangCompiler, lower_generated_stage_to_slang,
    lower_slang_bindings_to_descriptor_heap_at_offset, reflect_shader_interface,
};

use crate::convert::we_ingest::ir::{
    WeIrMaterialConstant, WeIrShaderBinding, WeIrShaderBindingKind, WeIrShaderIoDirection,
    WeIrShaderOrigin, WeIrShaderProgram, WeIrShaderScalarType, WeIrShaderStage, WeIrShaderStageIo,
    WeIrShaderUniformBuffer, WeIrShaderUniformMember, WeSceneIr,
};

use super::WeIngestError;
use super::asset_source::WeAssetSource;
use compiler_environment::compiler_definitions;
use package_preprocess::{specialize_stage, strip_specialized_stage_comments};
use stage_io::normalize_stage_io_pair;
use uniform_metadata::{ShaderUniformMetadata, parse_shader_uniform_metadata};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub(in crate::convert::we_ingest) fn compile_authored_shader_programs(
    project_root: &Path,
    ir: &mut WeSceneIr,
) -> Result<(), WeIngestError> {
    let specs = authored_program_specs(ir)?;
    if specs.is_empty() {
        return Ok(());
    }
    let source = WeAssetSource::open(project_root.to_path_buf())?;
    let temporary = TemporaryShaderDirectory::new()?;
    let compiler = SlangCompiler::from_environment();
    let mut programs = Vec::with_capacity(specs.len() * 2);
    for (program_index, spec) in specs.values().enumerate() {
        programs.extend(compile_program(
            &source,
            &compiler,
            temporary.path(),
            program_index,
            spec,
        )?);
    }
    materialize_shader_uniform_defaults(ir, &programs)?;
    ir.shader_programs = programs;
    Ok(())
}

fn materialize_shader_uniform_defaults(
    ir: &mut WeSceneIr,
    programs: &[WeIrShaderProgram],
) -> Result<(), WeIngestError> {
    let mut defaults_by_program = BTreeMap::<String, BTreeMap<String, String>>::new();
    for program in programs {
        let defaults = defaults_by_program
            .entry(program.program_key.clone())
            .or_default();
        for member in program
            .uniform_buffers
            .iter()
            .flat_map(|buffer| &buffer.members)
        {
            let (Some(parameter), Some(default)) = (
                member.material_parameter.as_ref(),
                member.material_default_value_json.as_ref(),
            ) else {
                continue;
            };
            if let Some(existing) = defaults.insert(parameter.clone(), default.clone())
                && existing != *default
            {
                return Err(shader_error(
                    &program.program_key,
                    "uniform-defaults",
                    format!("material parameter {parameter:?} has conflicting defaults"),
                ));
            }
        }
    }

    let old_constants = std::mem::take(&mut ir.material_constants);
    let mut old_to_new = vec![u32::MAX; old_constants.len()];
    let mut material_constants = Vec::with_capacity(old_constants.len());
    for pass in &mut ir.material_passes {
        let start = pass.constant_start as usize;
        let end = start
            .checked_add(pass.constant_count as usize)
            .filter(|end| *end <= old_constants.len())
            .ok_or_else(|| {
                shader_error(
                    &pass.shader_key,
                    "uniform-defaults",
                    "material constant range is invalid",
                )
            })?;
        pass.constant_start = u32::try_from(material_constants.len()).map_err(|_| {
            shader_error(
                &pass.shader_key,
                "uniform-defaults",
                "material constant count exceeds u32",
            )
        })?;
        let mut names = BTreeSet::new();
        for (old_index, constant) in old_constants.iter().enumerate().take(end).skip(start) {
            if !names.insert(constant.name.clone()) {
                return Err(shader_error(
                    &pass.shader_key,
                    "uniform-defaults",
                    format!("material parameter {:?} is duplicated", constant.name),
                ));
            }
            let new_index = u32::try_from(material_constants.len()).map_err(|_| {
                shader_error(
                    &pass.shader_key,
                    "uniform-defaults",
                    "material constant count exceeds u32",
                )
            })?;
            if old_to_new[old_index] != u32::MAX {
                return Err(shader_error(
                    &pass.shader_key,
                    "uniform-defaults",
                    "material constant ranges overlap",
                ));
            }
            old_to_new[old_index] = new_index;
            material_constants.push(constant.clone());
        }
        if let Some(defaults) = defaults_by_program.get(&pass.shader_key) {
            for (name, value_json) in defaults {
                if names.insert(name.clone()) {
                    material_constants.push(WeIrMaterialConstant {
                        name: name.clone(),
                        value_json: value_json.clone(),
                    });
                }
            }
        }
        pass.constant_count = u32::try_from(material_constants.len())
            .ok()
            .and_then(|end| end.checked_sub(pass.constant_start))
            .ok_or_else(|| {
                shader_error(
                    &pass.shader_key,
                    "uniform-defaults",
                    "material constant count exceeds u32",
                )
            })?;
    }
    if old_to_new.contains(&u32::MAX) {
        return Err(shader_error(
            "material",
            "uniform-defaults",
            "material constant is not owned by exactly one pass",
        ));
    }
    for program in &mut ir.script_programs {
        if program.target == crate::engine::scene::SceneScriptTarget::MaterialScalar {
            program.selector = old_to_new
                .get(program.selector as usize)
                .copied()
                .filter(|selector| *selector != u32::MAX)
                .ok_or_else(|| {
                    shader_error(
                        "material",
                        "uniform-defaults",
                        "script material selector is invalid",
                    )
                })?;
        }
    }
    ir.material_constants = material_constants;
    Ok(())
}

#[derive(Debug)]
struct AuthoredProgramSpec {
    program_key: String,
    source_key: String,
    texture_slot_mask: u32,
}

fn authored_program_specs(
    ir: &WeSceneIr,
) -> Result<BTreeMap<String, AuthoredProgramSpec>, WeIngestError> {
    let mut specs = BTreeMap::<String, AuthoredProgramSpec>::new();
    for contract in ir
        .shader_contracts
        .iter()
        .filter(|contract| contract.origin == WeIrShaderOrigin::AuthoredPackage)
    {
        let candidate = AuthoredProgramSpec {
            program_key: contract.shader_key.clone(),
            source_key: contract.shader_source_key.clone(),
            texture_slot_mask: contract.texture_slot_mask,
        };
        if let Some(existing) = specs.get(&candidate.program_key) {
            if existing.source_key != candidate.source_key
                || existing.texture_slot_mask != candidate.texture_slot_mask
            {
                return Err(shader_error(
                    &candidate.program_key,
                    "program",
                    "conflicting source identity or texture-slot mask",
                ));
            }
        } else {
            specs.insert(candidate.program_key.clone(), candidate);
        }
    }
    Ok(specs)
}

fn compile_program(
    source: &WeAssetSource,
    compiler: &SlangCompiler,
    temporary: &Path,
    program_index: usize,
    spec: &AuthoredProgramSpec,
) -> Result<[WeIrShaderProgram; 2], WeIngestError> {
    let vertex = read_stage_source(source, spec, "vert")?;
    let fragment = read_stage_source(source, spec, "frag")?;
    let definitions = compiler_definitions(spec, [&vertex, &fragment])?;
    let specialized_vertex =
        specialize_stage(source, spec, ShaderStage::Vertex, &vertex, &definitions)?;
    let specialized_fragment =
        specialize_stage(source, spec, ShaderStage::Fragment, &fragment, &definitions)?;
    let vertex_uniforms = parse_shader_uniform_metadata(&specialized_vertex)
        .map_err(|error| shader_error(&spec.program_key, "vertex", error))?;
    let fragment_uniforms = parse_shader_uniform_metadata(&specialized_fragment)
        .map_err(|error| shader_error(&spec.program_key, "fragment", error))?;
    let specialized_vertex = strip_specialized_stage_comments(&specialized_vertex)
        .map_err(|error| shader_error(&spec.program_key, "vertex", error))?;
    let specialized_fragment = strip_specialized_stage_comments(&specialized_fragment)
        .map_err(|error| shader_error(&spec.program_key, "fragment", error))?;
    let [vertex, fragment] = normalize_stage_io_pair(
        &specialized_vertex,
        &specialized_fragment,
        &specialized_vertex,
        &specialized_fragment,
        &spec.program_key,
    )?;
    let vertex = compile_stage(StageCompileInput {
        compiler,
        temporary,
        program_index,
        spec,
        stage: ShaderStage::Vertex,
        ir_stage: WeIrShaderStage::Vertex,
        source: &vertex,
        push_base_bytes: 0,
        uniform_metadata: &vertex_uniforms,
    })?;
    let fragment = compile_stage(StageCompileInput {
        compiler,
        temporary,
        program_index,
        spec,
        stage: ShaderStage::Fragment,
        ir_stage: WeIrShaderStage::Fragment,
        source: &fragment,
        push_base_bytes: vertex.push_constant_bytes,
        uniform_metadata: &fragment_uniforms,
    })?;
    Ok([vertex, fragment])
}

fn read_stage_source(
    source: &WeAssetSource,
    spec: &AuthoredProgramSpec,
    extension: &'static str,
) -> Result<String, WeIngestError> {
    let path = format!("shaders/{}.{}", spec.source_key, extension);
    let asset = source.read_required_asset(&path)?;
    String::from_utf8(asset.bytes).map_err(|error| {
        shader_error(
            &spec.program_key,
            extension,
            format!("{path} is not UTF-8: {error}"),
        )
    })
}

struct StageCompileInput<'a> {
    compiler: &'a SlangCompiler,
    temporary: &'a Path,
    program_index: usize,
    spec: &'a AuthoredProgramSpec,
    stage: ShaderStage,
    ir_stage: WeIrShaderStage,
    source: &'a str,
    push_base_bytes: u32,
    uniform_metadata: &'a ShaderUniformMetadata,
}

fn compile_stage(input: StageCompileInput<'_>) -> Result<WeIrShaderProgram, WeIngestError> {
    let stage_name = input.stage.slang_name();
    let stem = format!("program-{}-{stage_name}", input.program_index);
    let source_path = input.temporary.join(format!("{stem}.source.slang"));
    let direct_path = input.temporary.join(format!("{stem}.direct.slang"));
    let slang_path = input
        .temporary
        .join(format!("{stem}.descriptor-heap.slang"));
    let spirv_path = input.temporary.join(format!("{stem}.spv"));
    fs::write(&source_path, input.source).map_err(|error| {
        shader_error(
            &input.spec.program_key,
            stage_name,
            format!("failed to stage specialized source: {error}"),
        )
    })?;
    let direct = lower_generated_stage_to_slang(input.source, input.stage)
        .map_err(|error| shader_error(&input.spec.program_key, stage_name, error))?;
    fs::write(&direct_path, &direct).map_err(|error| {
        shader_error(
            &input.spec.program_key,
            stage_name,
            format!("failed to stage direct Slang: {error}"),
        )
    })?;
    let direct_reflection = input
        .compiler
        .reflect_slang_source(
            &direct_path,
            "main",
            input.stage,
            &input
                .temporary
                .join(format!("{stem}.direct-reflection.spv")),
        )
        .map_err(|error| shader_error(&input.spec.program_key, stage_name, error))?;
    if std::env::var_os("TENSOR_WALLPAPER_DIAGNOSTIC_KEEP_AUTHORED_SHADER_SOURCES").is_some() {
        let reflection_path = input
            .temporary
            .join(format!("{stem}.direct-reflection.json"));
        let reflection = serde_json::to_vec_pretty(&direct_reflection).map_err(|error| {
            shader_error(
                &input.spec.program_key,
                stage_name,
                format!("failed to encode direct Slang reflection diagnostic: {error}"),
            )
        })?;
        fs::write(&reflection_path, reflection).map_err(|error| {
            shader_error(
                &input.spec.program_key,
                stage_name,
                format!("failed to write direct Slang reflection diagnostic: {error}"),
            )
        })?;
    }
    let interface = reflect_shader_interface(&direct_reflection, "main", input.stage)
        .map_err(|error| shader_error(&input.spec.program_key, stage_name, error))?;
    let lowered =
        lower_slang_bindings_to_descriptor_heap_at_offset(&direct, "main", input.push_base_bytes)
            .map_err(|error| shader_error(&input.spec.program_key, stage_name, error))?;
    fs::write(&slang_path, &lowered.source).map_err(|error| {
        shader_error(
            &input.spec.program_key,
            stage_name,
            format!("failed to stage Slang: {error}"),
        )
    })?;
    let compiled = input
        .compiler
        .compile(&ShaderCompileRequest {
            source: slang_path,
            entry_point: "main".to_owned(),
            stage: input.stage,
            output: spirv_path.clone(),
            contract: ShaderContract::descriptor_heap(u64::from(lowered.push_constant_bytes)),
        })
        .map_err(|error| shader_error(&input.spec.program_key, stage_name, error))?;
    let final_interface = reflect_shader_interface(&compiled.reflection, "main", input.stage)
        .map_err(|error| shader_error(&input.spec.program_key, stage_name, error))?;
    let spirv = read_spirv_words(&spirv_path, &input.spec.program_key, stage_name)?;
    let bindings = lowered
        .bindings
        .into_iter()
        .map(|binding| {
            let kind = lower_binding_kind(binding.kind).ok_or_else(|| {
                shader_error(
                    &input.spec.program_key,
                    stage_name,
                    "authored shader unexpectedly exposes an engine-owned input attachment",
                )
            })?;
            Ok(WeIrShaderBinding {
                kind,
                register: binding.register,
                descriptor_count: 1,
                push_offset: binding.push_offset,
            })
        })
        .collect::<Result<Vec<_>, WeIngestError>>()?;
    Ok(WeIrShaderProgram {
        program_key: input.spec.program_key.clone(),
        stage: input.ir_stage,
        entry_point: "main".to_owned(),
        push_constant_bytes: lowered.push_constant_bytes,
        bindings,
        stage_io: final_interface
            .stage_io
            .into_iter()
            .map(|item| WeIrShaderStageIo {
                name: item.name,
                direction: match item.direction {
                    ShaderIoDirection::Input => WeIrShaderIoDirection::Input,
                    ShaderIoDirection::Output => WeIrShaderIoDirection::Output,
                },
                location: item.location,
                scalar_type: lower_scalar_type(item.scalar_type),
                rows: item.rows,
                columns: item.columns,
                location_count: item.location_count,
            })
            .collect(),
        uniform_buffers: interface
            .uniform_buffers
            .into_iter()
            .map(|buffer| WeIrShaderUniformBuffer {
                name: buffer.name,
                register: buffer.register,
                byte_size: buffer.byte_size,
                members: buffer
                    .members
                    .into_iter()
                    .map(|member| WeIrShaderUniformMember {
                        material_default_value_json: input
                            .uniform_metadata
                            .material_default(&member.name)
                            .map(str::to_owned),
                        material_parameter: input
                            .uniform_metadata
                            .material_parameter(&member.name)
                            .map(str::to_owned),
                        name: member.name,
                        byte_offset: member.byte_offset,
                        byte_size: member.byte_size,
                        scalar_type: lower_scalar_type(member.scalar_type),
                        rows: member.rows,
                        columns: member.columns,
                        array_count: member.array_count,
                        array_stride: member.array_stride,
                        matrix_stride: member.matrix_stride,
                    })
                    .collect(),
            })
            .collect(),
        spirv,
    })
}

fn lower_scalar_type(scalar_type: ShaderScalarType) -> WeIrShaderScalarType {
    match scalar_type {
        ShaderScalarType::Bool => WeIrShaderScalarType::Bool,
        ShaderScalarType::I32 => WeIrShaderScalarType::I32,
        ShaderScalarType::U32 => WeIrShaderScalarType::U32,
        ShaderScalarType::F32 => WeIrShaderScalarType::F32,
    }
}

fn read_spirv_words(
    path: &Path,
    program: &str,
    stage: &'static str,
) -> Result<Vec<u32>, WeIngestError> {
    let bytes = fs::read(path).map_err(|error| {
        shader_error(
            program,
            stage,
            format!("failed to read optimized SPIR-V: {error}"),
        )
    })?;
    if !bytes.len().is_multiple_of(4) {
        return Err(shader_error(
            program,
            stage,
            format!(
                "optimized SPIR-V byte count {} is not word aligned",
                bytes.len()
            ),
        ));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().expect("four-byte SPIR-V word")))
        .collect())
}

fn lower_binding_kind(kind: DescriptorHeapBindingKind) -> Option<WeIrShaderBindingKind> {
    match kind {
        DescriptorHeapBindingKind::InputAttachment => None,
        DescriptorHeapBindingKind::SampledImage => Some(WeIrShaderBindingKind::SampledImage),
        DescriptorHeapBindingKind::StorageImage => Some(WeIrShaderBindingKind::StorageImage),
        DescriptorHeapBindingKind::Sampler => Some(WeIrShaderBindingKind::Sampler),
        DescriptorHeapBindingKind::UniformBuffer => Some(WeIrShaderBindingKind::UniformBuffer),
        DescriptorHeapBindingKind::StorageBuffer => Some(WeIrShaderBindingKind::StorageBuffer),
    }
}

fn shader_error(
    program: &str,
    stage: &'static str,
    message: impl std::fmt::Display,
) -> WeIngestError {
    WeIngestError::ShaderCompile {
        program: program.to_owned(),
        stage,
        message: message.to_string(),
    }
}

struct TemporaryShaderDirectory {
    path: PathBuf,
}

impl TemporaryShaderDirectory {
    fn new() -> Result<Self, WeIngestError> {
        let prefix = format!("tensor-wallpaper-we-shaders-{}", std::process::id());
        let path = create_unique_shader_directory(&std::env::temp_dir(), &prefix, &TEMP_ID)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

fn create_unique_shader_directory(
    root: &Path,
    prefix: &str,
    ids: &AtomicU64,
) -> Result<PathBuf, WeIngestError> {
    loop {
        let id = ids.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!("{prefix}-{id}"));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(WeIngestError::Io { path, source }),
        }
    }
}

impl Drop for TemporaryShaderDirectory {
    fn drop(&mut self) {
        if std::env::var_os("TENSOR_WALLPAPER_DIAGNOSTIC_KEEP_AUTHORED_SHADER_SOURCES").is_some() {
            eprintln!(
                "tensor-wallpaper-diagnostic-authored-shader-directory={}",
                self.path.display()
            );
            return;
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deduplicates_pipeline_contracts_by_owned_program_identity() {
        let mut ir = empty_ir();
        let contract = crate::convert::we_ingest::ir::WeIrShaderContract {
            shader_key: "workshop/test/effects/example__SLOTS_1".to_owned(),
            shader_source_key: "workshop/test/effects/example".to_owned(),
            origin: WeIrShaderOrigin::AuthoredPackage,
            pipeline_key: "first".to_owned(),
            texture_slot_mask: 1,
            input_attachment_slot_mask: 0,
            constants: Vec::new(),
            resource_heap_count: 1,
            sampler_heap_count: 1,
        };
        ir.shader_contracts.push(contract.clone());
        ir.shader_contracts
            .push(crate::convert::we_ingest::ir::WeIrShaderContract {
                pipeline_key: "second".to_owned(),
                ..contract
            });

        assert_eq!(authored_program_specs(&ir).expect("program specs").len(), 1);
    }

    #[test]
    fn materializes_missing_shader_defaults_without_replacing_instance_values() {
        let key = "workshop/test/effects/noise__SLOTS_1";
        let mut ir = empty_ir();
        ir.material_constants.push(WeIrMaterialConstant {
            name: "Offset".to_owned(),
            value_json: "\"4 5\"".to_owned(),
        });
        ir.material_passes
            .push(crate::convert::we_ingest::ir::WeIrMaterialPass {
                material: 0,
                shader_key: key.to_owned(),
                shader_source_key: key.to_owned(),
                shader_origin: WeIrShaderOrigin::AuthoredPackage,
                target: String::new(),
                texture_start: 0,
                texture_count: 0,
                constant_start: 0,
                constant_count: 1,
                pipeline_blend: crate::engine::scene::ScenePipelineBlend::Normal,
                depth_test: crate::engine::scene::SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: crate::engine::scene::SceneCullMode::None,
                alpha_writing: String::new(),
                clear_target: false,
            });
        let uniform =
            |name: &str, parameter: &str, default: &str, byte_offset| WeIrShaderUniformMember {
                name: name.to_owned(),
                material_parameter: Some(parameter.to_owned()),
                material_default_value_json: Some(default.to_owned()),
                byte_offset,
                byte_size: if name == "u_Offset" { 8 } else { 4 },
                scalar_type: WeIrShaderScalarType::F32,
                rows: if name == "u_Offset" { 2 } else { 1 },
                columns: 1,
                array_count: 1,
                array_stride: 0,
                matrix_stride: 0,
            };
        let program = WeIrShaderProgram {
            program_key: key.to_owned(),
            stage: WeIrShaderStage::Vertex,
            entry_point: "main".to_owned(),
            push_constant_bytes: 4,
            bindings: Vec::new(),
            stage_io: Vec::new(),
            uniform_buffers: vec![WeIrShaderUniformBuffer {
                name: "GlobalParams".to_owned(),
                register: 0,
                byte_size: 12,
                members: vec![
                    uniform("u_Offset", "Offset", "\"0 0\"", 0),
                    uniform("u_Shift", "Sample shift amount", "1", 8),
                ],
            }],
            spirv: Vec::new(),
        };

        materialize_shader_uniform_defaults(&mut ir, &[program]).expect("material defaults");

        assert_eq!(ir.material_passes[0].constant_count, 2);
        assert_eq!(ir.material_constants[0].name, "Offset");
        assert_eq!(ir.material_constants[0].value_json, "\"4 5\"");
        assert_eq!(ir.material_constants[1].name, "Sample shift amount");
        assert_eq!(ir.material_constants[1].value_json, "1");
    }

    #[test]
    fn unique_shader_directory_preserves_existing_diagnostics() {
        let roots = AtomicU64::new(0);
        let root = create_unique_shader_directory(
            &std::env::temp_dir(),
            &format!(
                "tensor-wallpaper-authored-shader-test-{}",
                std::process::id()
            ),
            &roots,
        )
        .expect("unique test root");
        let preserved = root.join("evidence-0");
        fs::create_dir(&preserved).expect("preserved evidence directory");

        let ids = AtomicU64::new(0);
        let created = create_unique_shader_directory(&root, "evidence", &ids)
            .expect("next unique evidence directory");

        assert_eq!(
            created.file_name().and_then(|name| name.to_str()),
            Some("evidence-1")
        );
        assert!(preserved.is_dir());

        let _ = fs::remove_dir_all(root);
    }

    fn empty_ir() -> WeSceneIr {
        WeSceneIr {
            project_root: PathBuf::new(),
            project: crate::convert::we_ingest::ir::WeProjectIr {
                title: String::new(),
                wallpaper_type: "scene".to_owned(),
                scene_file: "scene.json".to_owned(),
                preview: String::new(),
                properties_json: "{}".to_owned(),
            },
            scene: crate::convert::we_ingest::ir::WeSceneRootIr {
                logical_width: 1,
                logical_height: 1,
                orthogonal_projection_auto: false,
                clear_color: [0.0; 4],
                ambient_color: [0.0; 4],
                skylight_color: [0.0; 4],
                camera_eye: crate::engine::scene::SceneVec3::default(),
                camera_center: crate::engine::scene::SceneVec3::default(),
                camera_up: crate::engine::scene::SceneVec3::default(),
                camera_parallax_enabled: false,
                camera_parallax_amount: 0.0,
                camera_parallax_delay: 0.0,
                camera_parallax_mouse_influence: 0.0,
            },
            resources: Vec::new(),
            textures: Vec::new(),
            objects: Vec::new(),
            object_effects: Vec::new(),
            object_animation_layers: Vec::new(),
            object_transform_tracks: Vec::new(),
            object_transform_channels: Vec::new(),
            object_transform_keyframes: Vec::new(),
            script_programs: Vec::new(),
            dynamic_texts: Vec::new(),
            dynamic_text_glyphs: Vec::new(),
            user_property_bindings: Vec::new(),
            puppet_animation_clips: Vec::new(),
            puppet_animation_tracks: Vec::new(),
            puppet_animation_transform_samples: Vec::new(),
            puppet_animation_opacity_samples: Vec::new(),
            materials: Vec::new(),
            material_passes: Vec::new(),
            material_textures: Vec::new(),
            material_constants: Vec::new(),
            meshes: Vec::new(),
            mesh_vertices: Vec::new(),
            mesh_indices: Vec::new(),
            mesh_source_records: Vec::new(),
            mesh_clipping_subdraws: Vec::new(),
            mesh_clipping_source_ordinals: Vec::new(),
            mesh_clipping_slices: Vec::new(),
            puppets: Vec::new(),
            puppet_bones: Vec::new(),
            puppet_attachments: Vec::new(),
            particles: Vec::new(),
            effects: Vec::new(),
            effect_passes: Vec::new(),
            effect_bindings: Vec::new(),
            effect_combos: Vec::new(),
            shader_combo_definitions: Vec::new(),
            effect_fbos: Vec::new(),
            render_graphs: Vec::new(),
            image_targets: Vec::new(),
            shader_contracts: Vec::new(),
            shader_programs: Vec::new(),
            unsupported: Vec::new(),
        }
    }
}
