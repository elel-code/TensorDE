//! Cold-path Slang shader for the live SceneColor terminal present pass.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use vulkan_renderer_build::{
    DescriptorHeapBindingKind, ShaderCompileRequest, ShaderContract, ShaderStage, SlangCompiler,
    lower_slang_bindings_to_descriptor_heap,
};

pub(super) fn build_scene_present_shaders() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set"));
    let shader_dir = out_dir.join("tensor_wallpaper_scene_present_shaders");
    fs::create_dir_all(&shader_dir).expect("create scene-present shader output directory");

    let vertex_spirv = compile_vertex(&shader_dir);
    let (fragment_spirv, push_bytes) = compile_fragment(&shader_dir);
    let generated = format!(
        "static SCENE_TERMINAL_PRESENT_VERTEX_SPIRV: &[u32] = vulkan_renderer::include_spirv!({vertex_spirv:?});\n\
         static SCENE_TERMINAL_PRESENT_FRAGMENT_SPIRV: &[u32] = vulkan_renderer::include_spirv!({fragment_spirv:?});\n\
         const SCENE_TERMINAL_PRESENT_PUSH_BYTES: u32 = {push_bytes};\n"
    );
    fs::write(
        out_dir.join("tensor_wallpaper_scene_present_shaders.rs"),
        generated,
    )
    .expect("write generated scene-present shader catalog");
}

fn compile_vertex(shader_dir: &Path) -> PathBuf {
    let spirv = shader_dir.join("terminal_present.vert.spv");
    SlangCompiler::from_environment()
        .compile(&ShaderCompileRequest {
            source: Path::new("shaders/scene_present/fullscreen.vert.slang").to_owned(),
            entry_point: "main".to_owned(),
            stage: ShaderStage::Vertex,
            output: spirv.clone(),
            contract: ShaderContract::descriptor_free(0),
        })
        .unwrap_or_else(|error| panic!("compile scene terminal-present vertex shader: {error}"));
    spirv
}

fn compile_fragment(shader_dir: &Path) -> (PathBuf, u32) {
    let source_path = Path::new("shaders/scene_present/fullscreen.frag.slang");
    let source = fs::read_to_string(source_path).expect("read scene-present fragment source");
    let lowered = lower_slang_bindings_to_descriptor_heap(&source, "main")
        .unwrap_or_else(|error| panic!("lower scene terminal-present fragment shader: {error}"));
    let bindings = lowered
        .bindings
        .iter()
        .map(|binding| (binding.kind, binding.register, binding.push_offset))
        .collect::<Vec<_>>();
    assert_eq!(
        bindings,
        vec![
            (DescriptorHeapBindingKind::SampledImage, 0, 0),
            (DescriptorHeapBindingKind::Sampler, 0, 4),
        ],
        "scene terminal-present descriptor push ABI changed"
    );
    assert_eq!(
        lowered.push_constant_bytes, 8,
        "scene terminal-present push size changed"
    );

    let slang = shader_dir.join("terminal_present.frag.slang");
    let spirv = shader_dir.join("terminal_present.frag.spv");
    fs::write(&slang, lowered.source).expect("write scene-present descriptor-heap source");
    SlangCompiler::from_environment()
        .compile(&ShaderCompileRequest {
            source: slang,
            entry_point: "main".to_owned(),
            stage: ShaderStage::Fragment,
            output: spirv.clone(),
            contract: ShaderContract::descriptor_heap(u64::from(lowered.push_constant_bytes)),
        })
        .unwrap_or_else(|error| panic!("compile scene terminal-present fragment shader: {error}"));
    (spirv, lowered.push_constant_bytes)
}
