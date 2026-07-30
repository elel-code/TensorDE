//! Cold-path Slang normalization and native descriptor-heap lowering for built-in fragments.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use vulkan_renderer_build::{
    DescriptorHeapBinding, DescriptorHeapBindingKind, ShaderCompileRequest, ShaderContract,
    ShaderStage, SlangCompiler, lower_slang_bindings_to_descriptor_heap,
};

pub(super) struct NativeSceneFragment {
    pub(super) spirv: PathBuf,
    pub(super) source: PathBuf,
    pub(super) push_constant_bytes: u32,
    pub(super) bindings: Vec<DescriptorHeapBinding>,
}

pub(super) fn compile_native_scene_fragment(
    shader_dir: &Path,
    key: &str,
    source: &str,
) -> NativeSceneFragment {
    let safe_name = key
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let source_path = shader_dir.join(format!("{safe_name}.frag.glsl"));
    let frontend_path = shader_dir.join(format!("{safe_name}.frag.frontend.glsl"));
    let normalized_path = shader_dir.join(format!("{safe_name}.frag.normalized.slang"));
    let native_path = shader_dir.join(format!("{safe_name}.frag.slang"));
    let spirv_path = shader_dir.join(format!("{safe_name}.frag.spv"));
    fs::write(&source_path, source).expect("write build-time scene fragment source");
    let frontend = flatten_glsl_uniform_blocks(source);
    let frontend = prune_unreferenced_stage_inputs(&frontend);
    fs::write(&frontend_path, frontend)
        .expect("write normalized GLSL scene fragment frontend source");
    let compiler = SlangCompiler::from_environment();
    transpile_generated_glsl(&frontend_path, &normalized_path, source, key);
    let normalized = fs::read_to_string(&normalized_path)
        .expect("read normalized built-in scene fragment source");
    let lowered = lower_slang_bindings_to_descriptor_heap(&normalized, "main")
        .unwrap_or_else(|error| panic!("lower built-in scene shader {key} fragment: {error}"));
    fs::write(&native_path, &lowered.source).expect("write native built-in scene fragment source");
    compiler
        .compile(&ShaderCompileRequest {
            source: native_path,
            entry_point: "main".to_owned(),
            stage: ShaderStage::Fragment,
            output: spirv_path.clone(),
            contract: ShaderContract::descriptor_heap(u64::from(lowered.push_constant_bytes)),
        })
        .unwrap_or_else(|error| panic!("compile built-in scene shader {key} fragment: {error}"));
    NativeSceneFragment {
        spirv: spirv_path,
        source: source_path,
        push_constant_bytes: lowered.push_constant_bytes,
        bindings: lowered.bindings,
    }
}

fn flatten_glsl_uniform_blocks(source: &str) -> String {
    let mut output = Vec::new();
    let mut instances = Vec::new();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with("layout(set = 0, binding = ")
            && trimmed.contains(") uniform ")
            && trimmed.ends_with('{')
        {
            let mut members = Vec::new();
            let instance = loop {
                let member = lines
                    .next()
                    .expect("built-in scene uniform block must be closed");
                let member = member.trim();
                if let Some(instance) = member
                    .strip_prefix("} ")
                    .and_then(|line| line.strip_suffix(';'))
                {
                    break instance.to_owned();
                }
                if !member.is_empty() {
                    members.push(member.to_owned());
                }
            };
            assert!(
                !members.is_empty(),
                "built-in scene uniform block {instance:?} must contain members"
            );
            output.extend(
                members
                    .into_iter()
                    .map(|member| format!("uniform {member}")),
            );
            instances.push(instance);
            continue;
        }
        output.push(line.to_owned());
    }
    let mut output = output.join("\n");
    for instance in instances {
        output = output.replace(&format!("{instance}."), "");
    }
    output.replace("binding = 35)", "binding = 3)")
}

fn prune_unreferenced_stage_inputs(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("layout(location = ")
                || (!trimmed.contains(") in ") && !trimmed.contains(") flat in "))
            {
                return true;
            }
            let Some(name) = trimmed.trim_end_matches(';').split_whitespace().last() else {
                return true;
            };
            identifier_occurrences(source, name) != 1
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn identifier_occurrences(source: &str, identifier: &str) -> usize {
    source
        .match_indices(identifier)
        .filter(|(offset, _)| {
            let before = source[..*offset].chars().next_back();
            let after = source[*offset + identifier.len()..].chars().next();
            !before.is_some_and(|ch| ch == '_' || ch.is_ascii_alphanumeric())
                && !after.is_some_and(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        })
        .count()
}

fn transpile_generated_glsl(source: &Path, output: &Path, glsl: &str, key: &str) {
    let slangc = env::var_os("SLANGC").unwrap_or_else(|| "slangc".into());
    let result = Command::new(slangc)
        .args(["-lang", "glsl"])
        .arg(source)
        .args([
            "-entry",
            "main",
            "-stage",
            "fragment",
            "-target",
            "hlsl",
            "-profile",
            "glsl_450",
            "-matrix-layout-row-major",
            "-no-mangle",
            "-O2",
            "-warnings-as-errors",
            "all",
            "-restrictive-capability-check",
            "-o",
        ])
        .arg(output)
        .output()
        .unwrap_or_else(|error| panic!("run Slang GLSL frontend for {key:?}: {error}"));
    if !result.status.success() {
        panic!(
            "normalize built-in scene shader {key} fragment failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let normalized =
        fs::read_to_string(output).expect("read built-in scene fragment frontend output");
    let normalized = repair_combined_sampler_intrinsics(&normalized);
    let normalized = inject_fragment_input_locations(&normalized, glsl);
    fs::write(output, normalized).expect("write repaired built-in scene fragment frontend output");
}

fn repair_combined_sampler_intrinsics(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let call = line
                .find(".GetDimensions(")
                .map(|offset| (offset, ".GetDimensions("))
                .or_else(|| line.find(".Load(").map(|offset| (offset, ".Load(")));
            let Some((call, operation)) = call else {
                return line.to_owned();
            };
            let arguments = call + operation.len();
            let Some(comma) = line[arguments..].find(',').map(|comma| arguments + comma) else {
                return line.to_owned();
            };
            if !line[arguments..comma].contains("_sampler_") {
                return line.to_owned();
            }
            format!("{}{}", &line[..arguments], line[comma + 1..].trim_start())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn inject_fragment_input_locations(hlsl: &str, glsl: &str) -> String {
    let mut hlsl = hlsl.to_owned();
    for line in glsl.lines() {
        let trimmed = line.trim();
        let Some(location) = trimmed
            .strip_prefix("layout(location = ")
            .and_then(|line| line.split_once(')'))
        else {
            continue;
        };
        if !matches!(
            location.1.trim_start(),
            declaration if declaration.starts_with("in ") || declaration.starts_with("flat in ")
        ) {
            continue;
        }
        let Some(declaration) = location
            .1
            .trim_start()
            .strip_prefix("in ")
            .or_else(|| location.1.trim_start().strip_prefix("flat in "))
        else {
            continue;
        };
        let mut words = declaration.trim_end_matches(';').split_whitespace();
        let Some((glsl_type, name)) = words.next().zip(words.next()) else {
            continue;
        };
        let hlsl_type = match glsl_type {
            "float" => "float",
            "vec2" => "float2",
            "vec3" => "float3",
            "vec4" => "float4",
            "int" => "int",
            "ivec2" => "int2",
            "uint" => "uint",
            "uvec4" => "uint4",
            _ => continue,
        };
        let needle = format!("{hlsl_type} {name} : COLOR");
        let replacement = format!("[[vk::location({})]] {needle}", location.0);
        hlsl = hlsl.replace(&needle, &replacement);
    }
    hlsl
}

pub(super) fn builtin_binding_expressions(bindings: &[DescriptorHeapBinding]) -> String {
    bindings
        .iter()
        .map(|binding| {
            let kind = match binding.kind {
                DescriptorHeapBindingKind::SampledImage => "SampledImage",
                DescriptorHeapBindingKind::StorageImage => "StorageImage",
                DescriptorHeapBindingKind::Sampler => "Sampler",
                DescriptorHeapBindingKind::UniformBuffer => "UniformBuffer",
                DescriptorHeapBindingKind::StorageBuffer => "StorageBuffer",
            };
            format!(
                "BuiltinSceneDescriptorBinding {{ kind: BuiltinSceneDescriptorBindingKind::{kind}, register: {}, push_offset: {} }}",
                binding.register, binding.push_offset
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}
