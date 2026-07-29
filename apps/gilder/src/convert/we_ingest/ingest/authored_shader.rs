//! Cold-compile package-owned Wallpaper Engine shaders into native heap SPIR-V.
//!
//! The runtime receives only the resulting words and compact binding ABI. GLSL,
//! normalized Slang, compiler binaries, and validation tools stay in this formal
//! conversion path.

mod compiler_environment;
mod stage_io;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use vulkan_renderer_build::{
    DescriptorHeapBindingKind, GlslToSlangRequest, ShaderCompileRequest, ShaderContract,
    ShaderIoDirection, ShaderScalarType, ShaderStage, SlangCompiler,
    lower_slang_bindings_to_descriptor_heap, reflect_shader_interface,
};

use crate::convert::we_ingest::ir::{
    WeIrShaderBinding, WeIrShaderBindingKind, WeIrShaderIoDirection, WeIrShaderOrigin,
    WeIrShaderProgram, WeIrShaderScalarType, WeIrShaderStage, WeIrShaderStageIo,
    WeIrShaderUniformBuffer, WeIrShaderUniformMember, WeSceneIr,
};

use super::WeIngestError;
use super::asset_source::WeAssetSource;
use compiler_environment::{compiler_definitions, inject_we_compiler_preamble};
use stage_io::normalize_stage_io_pair;

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
    ir.shader_programs = programs;
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
    let [vertex, fragment] = normalize_stage_io_pair(&vertex, &fragment, &spec.program_key)?;
    let vertex = inject_we_compiler_preamble(&vertex);
    let fragment = inject_we_compiler_preamble(&fragment);
    let include_directories = source.shader_include_directories(&spec.source_key);
    Ok([
        compile_stage(StageCompileInput {
            compiler,
            temporary,
            program_index,
            spec,
            stage: ShaderStage::Vertex,
            ir_stage: WeIrShaderStage::Vertex,
            source: &vertex,
            definitions: &definitions,
            include_directories: &include_directories,
        })?,
        compile_stage(StageCompileInput {
            compiler,
            temporary,
            program_index,
            spec,
            stage: ShaderStage::Fragment,
            ir_stage: WeIrShaderStage::Fragment,
            source: &fragment,
            definitions: &definitions,
            include_directories: &include_directories,
        })?,
    ])
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
    definitions: &'a [(String, String)],
    include_directories: &'a [PathBuf],
}

fn compile_stage(input: StageCompileInput<'_>) -> Result<WeIrShaderProgram, WeIngestError> {
    let stage_name = input.stage.slang_name();
    let stem = format!("program-{}-{stage_name}", input.program_index);
    let glsl_path = input.temporary.join(format!("{stem}.glsl"));
    let normalized_path = input.temporary.join(format!("{stem}.normalized.slang"));
    let native_path = input.temporary.join(format!("{stem}.native.slang"));
    let spirv_path = input.temporary.join(format!("{stem}.spv"));
    fs::write(&glsl_path, input.source).map_err(|error| {
        shader_error(
            &input.spec.program_key,
            stage_name,
            format!("failed to stage GLSL source: {error}"),
        )
    })?;
    let frontend = input
        .compiler
        .transpile_glsl(&GlslToSlangRequest {
            source: glsl_path,
            entry_point: "main".to_owned(),
            stage: input.stage,
            output: normalized_path.clone(),
            include_directories: input.include_directories.to_vec(),
            definitions: input.definitions.to_vec(),
            disabled_warnings: vec![30081],
        })
        .map_err(|error| shader_error(&input.spec.program_key, stage_name, error))?;
    let interface = reflect_shader_interface(&frontend.reflection, "main", input.stage)
        .map_err(|error| shader_error(&input.spec.program_key, stage_name, error))?;
    let normalized = fs::read_to_string(&normalized_path).map_err(|error| {
        shader_error(
            &input.spec.program_key,
            stage_name,
            format!("failed to read normalized Slang: {error}"),
        )
    })?;
    let lowered = lower_slang_bindings_to_descriptor_heap(&normalized, "main")
        .map_err(|error| shader_error(&input.spec.program_key, stage_name, error))?;
    fs::write(&native_path, &lowered.source).map_err(|error| {
        shader_error(
            &input.spec.program_key,
            stage_name,
            format!("failed to stage native Slang: {error}"),
        )
    })?;
    input
        .compiler
        .compile(&ShaderCompileRequest {
            source: native_path,
            entry_point: "main".to_owned(),
            stage: input.stage,
            output: spirv_path.clone(),
            contract: ShaderContract::descriptor_heap(u64::from(lowered.push_constant_bytes)),
        })
        .map_err(|error| shader_error(&input.spec.program_key, stage_name, error))?;
    let spirv = read_spirv_words(&spirv_path, &input.spec.program_key, stage_name)?;
    let bindings = lowered
        .bindings
        .into_iter()
        .map(|binding| WeIrShaderBinding {
            kind: lower_binding_kind(binding.kind),
            register: binding.register,
            descriptor_count: 1,
            push_offset: binding.push_offset,
        })
        .collect();
    Ok(WeIrShaderProgram {
        program_key: input.spec.program_key.clone(),
        stage: input.ir_stage,
        entry_point: "main".to_owned(),
        push_constant_bytes: lowered.push_constant_bytes,
        bindings,
        stage_io: interface
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

fn lower_binding_kind(kind: DescriptorHeapBindingKind) -> WeIrShaderBindingKind {
    match kind {
        DescriptorHeapBindingKind::SampledImage => WeIrShaderBindingKind::SampledImage,
        DescriptorHeapBindingKind::StorageImage => WeIrShaderBindingKind::StorageImage,
        DescriptorHeapBindingKind::Sampler => WeIrShaderBindingKind::Sampler,
        DescriptorHeapBindingKind::UniformBuffer => WeIrShaderBindingKind::UniformBuffer,
        DescriptorHeapBindingKind::StorageBuffer => WeIrShaderBindingKind::StorageBuffer,
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
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("gilder-we-shaders-{}-{id}", std::process::id()));
        fs::create_dir(&path).map_err(|error| WeIngestError::Io {
            path: path.clone(),
            source: error,
        })?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryShaderDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deduplicates_pipeline_contracts_by_owned_program_identity() {
        let mut ir = WeSceneIr {
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
        };
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
}
