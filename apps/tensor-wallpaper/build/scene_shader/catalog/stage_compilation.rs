//! Cold-path Slang compilation and descriptor-heap lowering for built-in stages.

use std::fs;
use std::path::{Path, PathBuf};

use vulkan_renderer_build::{
    DescriptorHeapBinding, DescriptorHeapBindingKind, ShaderCompileRequest, ShaderContract,
    ShaderStage, SlangCompiler, lower_generated_stage_to_slang,
    lower_slang_bindings_to_descriptor_heap_at_offset,
    lower_slang_input_attachment_to_descriptor_heap_at_offset,
};

pub(super) struct CompiledSceneStage {
    pub(super) spirv: PathBuf,
    pub(super) source: PathBuf,
    pub(super) push_constant_bytes: u32,
    pub(super) bindings: Vec<DescriptorHeapBinding>,
}

pub(super) fn compile_slang_scene_fragment(
    shader_dir: &Path,
    key: &str,
    source: &str,
) -> CompiledSceneStage {
    compile_slang_scene_stage(shader_dir, key, source, "frag", ShaderStage::Fragment, 0)
}

/// Cold-lowers a generated engine stage directly to Slang before the
/// descriptor-heap pass. This deliberately has no GLSL/HLSL compiler route.
pub(super) fn compile_generated_scene_fragment(
    shader_dir: &Path,
    key: &str,
    source: &str,
) -> CompiledSceneStage {
    compile_generated_scene_stage(shader_dir, key, source, "frag", ShaderStage::Fragment, 0)
}

pub(super) fn compile_slang_scene_vertex(
    shader_dir: &Path,
    key: &str,
    source: &str,
    push_base_bytes: u32,
) -> CompiledSceneStage {
    compile_slang_scene_stage(
        shader_dir,
        key,
        source,
        "vert",
        ShaderStage::Vertex,
        push_base_bytes,
    )
}

pub(super) fn compile_slang_scene_input_attachment(
    shader_dir: &Path,
    key: &str,
    source: &str,
    push_base_bytes: u32,
) -> CompiledSceneStage {
    let safe_name = key
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let source_path = shader_dir.join(format!("{safe_name}.input.frag.source.slang"));
    let slang_path = shader_dir.join(format!("{safe_name}.input.frag.slang"));
    let spirv_path = shader_dir.join(format!("{safe_name}.input.frag.spv"));
    fs::write(&source_path, source).expect("write exact Slang input-attachment source");
    let lowered =
        lower_slang_input_attachment_to_descriptor_heap_at_offset(source, "main", push_base_bytes)
            .unwrap_or_else(|error| {
                panic!("lower exact Slang input-attachment shader {key}: {error}")
            });
    assert_eq!(
        lowered.bindings.as_slice(),
        [DescriptorHeapBinding {
            kind: DescriptorHeapBindingKind::InputAttachment,
            register: 64,
            push_offset: push_base_bytes,
            source_name: "sourceInput".to_owned(),
            source_type: "SubpassInput<float4>".to_owned(),
        }],
        "built-in local-read binding must remain the exact typed input attachment"
    );
    fs::write(&slang_path, &lowered.source)
        .expect("write Slang input-attachment heap proxy");
    SlangCompiler::from_environment()
        .compile_input_attachment(&ShaderCompileRequest {
            source: slang_path,
            entry_point: "main".to_owned(),
            stage: ShaderStage::Fragment,
            output: spirv_path.clone(),
            contract: ShaderContract::descriptor_heap(u64::from(lowered.push_constant_bytes)),
        })
        .unwrap_or_else(|error| {
            panic!("compile exact Slang input-attachment shader {key}: {error}")
        });
    CompiledSceneStage {
        spirv: spirv_path,
        source: source_path,
        push_constant_bytes: lowered.push_constant_bytes,
        bindings: lowered.bindings,
    }
}

pub(super) fn compile_generated_scene_vertex(
    shader_dir: &Path,
    key: &str,
    source: &str,
    push_base_bytes: u32,
) -> CompiledSceneStage {
    compile_generated_scene_stage(
        shader_dir,
        key,
        source,
        "vert",
        ShaderStage::Vertex,
        push_base_bytes,
    )
}

pub(super) fn compile_particle_compute(
    shader_dir: &Path,
    key: &str,
    source: &str,
) -> CompiledSceneStage {
    compile_slang_scene_stage(shader_dir, key, source, "comp", ShaderStage::Compute, 0)
}

fn compile_slang_scene_stage(
    shader_dir: &Path,
    key: &str,
    source: &str,
    extension: &str,
    stage: ShaderStage,
    push_base_bytes: u32,
) -> CompiledSceneStage {
    let safe_name = key
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let source_path = shader_dir.join(format!("{safe_name}.{extension}.source.slang"));
    let slang_path = shader_dir.join(format!("{safe_name}.{extension}.slang"));
    let spirv_path = shader_dir.join(format!("{safe_name}.{extension}.spv"));
    fs::write(&source_path, source).expect("write exact Slang scene source");
    let (slang_source, push_constant_bytes, bindings, contract) =
        if slang_exposes_resources(source) {
            let lowered =
                lower_slang_bindings_to_descriptor_heap_at_offset(source, "main", push_base_bytes)
                    .unwrap_or_else(|error| {
                        panic!("lower exact Slang scene shader {key} {extension}: {error}")
                    });
            (
                lowered.source,
                lowered.push_constant_bytes,
                lowered.bindings,
                ShaderContract::descriptor_heap(u64::from(lowered.push_constant_bytes)),
            )
        } else {
            (
                source.to_owned(),
                0,
                Vec::new(),
                ShaderContract::descriptor_free(0),
            )
        };
    fs::write(&slang_path, slang_source)
        .expect("write descriptor-heap Slang scene source");
    SlangCompiler::from_environment()
        .compile(&ShaderCompileRequest {
            source: slang_path,
            entry_point: "main".to_owned(),
            stage,
            output: spirv_path.clone(),
            contract,
        })
        .unwrap_or_else(|error| {
            panic!("compile exact Slang scene shader {key} {extension}: {error}")
        });
    CompiledSceneStage {
        spirv: spirv_path,
        source: source_path,
        push_constant_bytes,
        bindings,
    }
}

fn compile_generated_scene_stage(
    shader_dir: &Path,
    key: &str,
    source: &str,
    extension: &str,
    stage: ShaderStage,
    push_base_bytes: u32,
) -> CompiledSceneStage {
    let slang = lower_generated_stage_to_slang(source, stage).unwrap_or_else(|error| {
        panic!("lower generated scene shader {key} {extension} to Slang: {error}")
    });
    compile_slang_scene_stage(shader_dir, key, &slang, extension, stage, push_base_bytes)
}

fn slang_exposes_resources(source: &str) -> bool {
    source.lines().any(|line| line.contains(": register("))
}

pub(super) fn builtin_binding_expressions(bindings: &[DescriptorHeapBinding]) -> String {
    bindings
        .iter()
        .map(|binding| {
            let kind = match binding.kind {
                vulkan_renderer_build::DescriptorHeapBindingKind::InputAttachment => {
                    "InputAttachment"
                }
                vulkan_renderer_build::DescriptorHeapBindingKind::SampledImage => "SampledImage",
                vulkan_renderer_build::DescriptorHeapBindingKind::StorageImage => "StorageImage",
                vulkan_renderer_build::DescriptorHeapBindingKind::Sampler => "Sampler",
                vulkan_renderer_build::DescriptorHeapBindingKind::UniformBuffer => "UniformBuffer",
                vulkan_renderer_build::DescriptorHeapBindingKind::StorageBuffer => "StorageBuffer",
            };
            format!(
                "BuiltinSceneDescriptorBinding {{ kind: BuiltinSceneDescriptorBindingKind::{kind}, register: {}, push_offset: {} }}",
                binding.register, binding.push_offset
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}
