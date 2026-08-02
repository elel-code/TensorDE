//! Cold-path native descriptor-heap shaders for decoded video presentation.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use vulkan_renderer_build::{
    DescriptorHeapBinding, DescriptorHeapBindingKind, ShaderCompileRequest, ShaderContract,
    ShaderStage, SlangCompiler, lower_slang_bindings_to_descriptor_heap_at_offset,
};

struct CompiledStage {
    spirv: PathBuf,
    push_bytes: u32,
    bindings: Vec<DescriptorHeapBinding>,
}

pub(super) fn build_video_present_shaders() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set"));
    let shader_dir = out_dir.join("gilder_video_present_shaders");
    fs::create_dir_all(&shader_dir).expect("create video-present shader output directory");

    let fullscreen_vertex = compile_descriptor_free(
        &shader_dir,
        "fullscreen",
        ShaderStage::Vertex,
        Path::new("shaders/video_present/fullscreen.vert.slang"),
        0,
    );
    let fullscreen_fragment = compile_native_fragment(
        &shader_dir,
        "fullscreen",
        Path::new("shaders/video_present/fullscreen.frag.slang"),
        0,
    );
    let scene_vertex = compile_descriptor_free(
        &shader_dir,
        "scene_video_layer",
        ShaderStage::Vertex,
        Path::new("shaders/video_present/scene_video_layer.vert.slang"),
        8,
    );
    let scene_fragment = compile_native_fragment(
        &shader_dir,
        "scene_video_layer",
        Path::new("shaders/video_present/scene_video_layer.frag.slang"),
        8,
    );

    assert_video_bindings("fullscreen", &fullscreen_fragment, 0);
    assert_video_bindings("scene video layer", &scene_fragment, 8);
    let generated = format!(
        "static DECODED_PRESENT_VERTEX_SPIRV: &[u32] = vulkan_renderer::include_spirv!({:?});\n\
         static DECODED_PRESENT_FRAGMENT_SPIRV: &[u32] = vulkan_renderer::include_spirv!({:?});\n\
         const DECODED_PRESENT_PUSH_BYTES: u32 = {};\n\
         static DECODED_PRESENT_BINDINGS: &[DecodedImageDescriptorBinding] = &[{}];\n\
         static DECODED_SCENE_VIDEO_VERTEX_SPIRV: &[u32] = vulkan_renderer::include_spirv!({:?});\n\
         static DECODED_SCENE_VIDEO_FRAGMENT_SPIRV: &[u32] = vulkan_renderer::include_spirv!({:?});\n\
         const DECODED_SCENE_VIDEO_PUSH_BYTES: u32 = {};\n\
         static DECODED_SCENE_VIDEO_BINDINGS: &[DecodedImageDescriptorBinding] = &[{}];\n",
        fullscreen_vertex.spirv,
        fullscreen_fragment.spirv,
        fullscreen_fragment.push_bytes,
        binding_expressions(&fullscreen_fragment.bindings),
        scene_vertex.spirv,
        scene_fragment.spirv,
        scene_fragment.push_bytes,
        binding_expressions(&scene_fragment.bindings),
    );
    fs::write(out_dir.join("gilder_video_present_shaders.rs"), generated)
        .expect("write generated video-present shader catalog");
    let scene_generated = format!(
        "static SCENE_VIDEO_LAYER_VERTEX_SPIRV: &[u32] = vulkan_renderer::include_spirv!({:?});\n\
         static SCENE_VIDEO_LAYER_FRAGMENT_SPIRV: &[u32] = vulkan_renderer::include_spirv!({:?});\n\
         const SCENE_VIDEO_LAYER_PUSH_BYTES: u32 = {};\n",
        scene_vertex.spirv, scene_fragment.spirv, scene_fragment.push_bytes,
    );
    fs::write(
        out_dir.join("gilder_scene_video_shaders.rs"),
        scene_generated,
    )
    .expect("write generated scene-video shader catalog");
    let shared_present_generated = format!(
        "static SHARED_VIDEO_PRESENT_VERTEX_SPIRV: &[u32] = vulkan_renderer::include_spirv!({:?});\n\
         static SHARED_VIDEO_PRESENT_FRAGMENT_SPIRV: &[u32] = vulkan_renderer::include_spirv!({:?});\n\
         const SHARED_VIDEO_PRESENT_PUSH_BYTES: u32 = {};\n",
        fullscreen_vertex.spirv, fullscreen_fragment.spirv, fullscreen_fragment.push_bytes,
    );
    fs::write(
        out_dir.join("gilder_shared_video_present_shaders.rs"),
        shared_present_generated,
    )
    .expect("write shared video-present shader catalog");
}

fn compile_descriptor_free(
    shader_dir: &Path,
    key: &str,
    stage: ShaderStage,
    source: &Path,
    push_bytes: u64,
) -> CompiledStage {
    let extension = match stage {
        ShaderStage::Vertex => "vert",
        ShaderStage::Fragment => "frag",
        ShaderStage::Compute => unreachable!("video present has no compute stage"),
    };
    let spirv = shader_dir.join(format!("{key}.{extension}.spv"));
    SlangCompiler::from_environment()
        .compile(&ShaderCompileRequest {
            source: source.to_owned(),
            entry_point: "main".to_owned(),
            stage,
            output: spirv.clone(),
            contract: ShaderContract::descriptor_free(push_bytes),
        })
        .unwrap_or_else(|error| panic!("compile descriptor-free video shader {key}: {error}"));
    CompiledStage {
        spirv,
        push_bytes: push_bytes as u32,
        bindings: Vec::new(),
    }
}

fn compile_native_fragment(
    shader_dir: &Path,
    key: &str,
    source_path: &Path,
    push_base: u32,
) -> CompiledStage {
    let source = fs::read_to_string(source_path).expect("read video-present fragment source");
    let lowered = lower_slang_bindings_to_descriptor_heap_at_offset(&source, "main", push_base)
        .unwrap_or_else(|error| panic!("lower video-present fragment {key}: {error}"));
    let native = shader_dir.join(format!("{key}.frag.slang"));
    let spirv = shader_dir.join(format!("{key}.frag.spv"));
    fs::write(&native, lowered.source).expect("write native video-present fragment source");
    SlangCompiler::from_environment()
        .compile(&ShaderCompileRequest {
            source: native,
            entry_point: "main".to_owned(),
            stage: ShaderStage::Fragment,
            output: spirv.clone(),
            contract: ShaderContract::descriptor_heap(u64::from(lowered.push_constant_bytes)),
        })
        .unwrap_or_else(|error| panic!("compile native video-present fragment {key}: {error}"));
    CompiledStage {
        spirv,
        push_bytes: lowered.push_constant_bytes,
        bindings: lowered.bindings,
    }
}

fn assert_video_bindings(label: &str, stage: &CompiledStage, push_base: u32) {
    let expected = [
        (DescriptorHeapBindingKind::SampledImage, 0, push_base),
        (DescriptorHeapBindingKind::SampledImage, 1, push_base + 4),
        (DescriptorHeapBindingKind::Sampler, 0, push_base + 8),
        (DescriptorHeapBindingKind::Sampler, 1, push_base + 12),
    ];
    let found = stage
        .bindings
        .iter()
        .map(|binding| (binding.kind, binding.register, binding.push_offset))
        .collect::<Vec<_>>();
    assert_eq!(found, expected, "{label} descriptor push ABI changed");
    assert_eq!(stage.push_bytes, push_base + 16);
}

fn binding_expressions(bindings: &[DescriptorHeapBinding]) -> String {
    bindings
        .iter()
        .map(|binding| {
            let kind = match binding.kind {
                DescriptorHeapBindingKind::SampledImage => "SampledImage",
                DescriptorHeapBindingKind::Sampler => "Sampler",
                other => panic!("video-present shader exposes unsupported {other:?}"),
            };
            format!(
                "DecodedImageDescriptorBinding {{ kind: DecodedImageDescriptorKind::{kind}, register: {}, push_offset: {} }}",
                binding.register, binding.push_offset
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}
