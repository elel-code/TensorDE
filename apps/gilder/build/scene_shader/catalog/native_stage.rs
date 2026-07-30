//! Cold-path Slang normalization and native descriptor-heap lowering for built-in stages.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use vulkan_renderer_build::{
    DescriptorHeapBinding, DescriptorHeapBindingKind, ShaderCompileRequest, ShaderContract,
    ShaderStage, SlangCompiler, lower_slang_bindings_to_descriptor_heap_at_offset,
    lower_slang_input_attachment_to_descriptor_heap_at_offset,
};

pub(super) struct NativeSceneStage {
    pub(super) spirv: PathBuf,
    pub(super) source: PathBuf,
    pub(super) push_constant_bytes: u32,
    pub(super) bindings: Vec<DescriptorHeapBinding>,
}

pub(super) fn compile_native_scene_fragment(
    shader_dir: &Path,
    key: &str,
    source: &str,
) -> NativeSceneStage {
    compile_native_scene_stage(shader_dir, key, source, "frag", ShaderStage::Fragment, 0)
}

pub(super) fn compile_native_scene_vertex(
    shader_dir: &Path,
    key: &str,
    source: &str,
    push_base_bytes: u32,
) -> NativeSceneStage {
    compile_native_scene_stage(
        shader_dir,
        key,
        source,
        "vert",
        ShaderStage::Vertex,
        push_base_bytes,
    )
}

pub(super) fn compile_native_scene_input_attachment(
    shader_dir: &Path,
    key: &str,
    source: &str,
    push_base_bytes: u32,
) -> NativeSceneStage {
    let safe_name = key
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let source_path = shader_dir.join(format!("{safe_name}.input.frag.source.slang"));
    let native_path = shader_dir.join(format!("{safe_name}.input.frag.slang"));
    let spirv_path = shader_dir.join(format!("{safe_name}.input.frag.spv"));
    fs::write(&source_path, source).expect("write built-in input-attachment source");
    let lowered =
        lower_slang_input_attachment_to_descriptor_heap_at_offset(source, "main", push_base_bytes)
            .unwrap_or_else(|error| {
                panic!("lower built-in input-attachment shader {key}: {error}")
            });
    assert_eq!(
        lowered.bindings.len(),
        1,
        "built-in input-attachment shader must expose one native binding"
    );
    assert_eq!(
        lowered.bindings[0].kind,
        DescriptorHeapBindingKind::InputAttachment,
        "built-in local-read binding must remain typed as an input attachment"
    );
    fs::write(&native_path, &lowered.source)
        .expect("write native built-in input-attachment proxy source");
    SlangCompiler::from_environment()
        .compile_input_attachment(&ShaderCompileRequest {
            source: native_path,
            entry_point: "main".to_owned(),
            stage: ShaderStage::Fragment,
            output: spirv_path.clone(),
            contract: ShaderContract::descriptor_heap(u64::from(lowered.push_constant_bytes)),
        })
        .unwrap_or_else(|error| panic!("compile built-in input-attachment shader {key}: {error}"));
    NativeSceneStage {
        spirv: spirv_path,
        source: source_path,
        push_constant_bytes: lowered.push_constant_bytes,
        bindings: lowered.bindings,
    }
}

fn compile_native_scene_stage(
    shader_dir: &Path,
    key: &str,
    source: &str,
    extension: &str,
    stage: ShaderStage,
    push_base_bytes: u32,
) -> NativeSceneStage {
    let safe_name = key
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let source_path = shader_dir.join(format!("{safe_name}.{extension}.glsl"));
    let frontend_path = shader_dir.join(format!("{safe_name}.{extension}.frontend.glsl"));
    let normalized_path = shader_dir.join(format!("{safe_name}.{extension}.normalized.slang"));
    let native_path = shader_dir.join(format!("{safe_name}.{extension}.slang"));
    let spirv_path = shader_dir.join(format!("{safe_name}.{extension}.spv"));
    fs::write(&source_path, source).expect("write build-time scene stage source");
    let frontend = flatten_glsl_uniform_blocks(source).replace("layout(std430, set", "layout(set");
    let frontend = prune_unreferenced_stage_inputs(&frontend);
    fs::write(&frontend_path, frontend).expect("write normalized GLSL scene stage frontend source");
    let compiler = SlangCompiler::from_environment();
    transpile_generated_glsl(&frontend_path, &normalized_path, source, key, stage);
    let normalized =
        fs::read_to_string(&normalized_path).expect("read normalized built-in scene stage source");
    let normalized = normalized
        .lines()
        .filter(|line| !line.trim_start().starts_with("#pragma"))
        .collect::<Vec<_>>()
        .join("\n");
    let (native_source, push_constant_bytes, bindings, contract) =
        if normalized_exposes_resources(&normalized) {
            let lowered = lower_slang_bindings_to_descriptor_heap_at_offset(
                &normalized,
                "main",
                push_base_bytes,
            )
            .unwrap_or_else(|error| {
                panic!("lower built-in scene shader {key} {extension}: {error}")
            });
            let contract = ShaderContract::descriptor_heap(u64::from(lowered.push_constant_bytes));
            (
                lowered.source,
                lowered.push_constant_bytes,
                lowered.bindings,
                contract,
            )
        } else {
            (
                normalized,
                0,
                Vec::new(),
                ShaderContract::descriptor_free(0),
            )
        };
    fs::write(&native_path, native_source).expect("write native built-in scene stage source");
    compiler
        .compile(&ShaderCompileRequest {
            source: native_path,
            entry_point: "main".to_owned(),
            stage,
            output: spirv_path.clone(),
            contract,
        })
        .unwrap_or_else(|error| panic!("compile built-in scene shader {key} {extension}: {error}"));
    NativeSceneStage {
        spirv: spirv_path,
        source: source_path,
        push_constant_bytes,
        bindings,
    }
}

fn normalized_exposes_resources(source: &str) -> bool {
    source.lines().any(|line| {
        let line = line.trim();
        line.starts_with("cbuffer ") || line.contains(": register(")
    })
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

fn transpile_generated_glsl(
    source: &Path,
    output: &Path,
    glsl: &str,
    key: &str,
    stage: ShaderStage,
) {
    let stage_name = match stage {
        ShaderStage::Vertex => "vertex",
        ShaderStage::Fragment => "fragment",
        ShaderStage::Compute => "compute",
    };
    let slangc = env::var_os("SLANGC").unwrap_or_else(|| "slangc".into());
    let result = Command::new(slangc)
        .args(["-lang", "glsl"])
        .arg(source)
        .args([
            "-entry",
            "main",
            "-stage",
            stage_name,
            "-target",
            "hlsl",
            "-profile",
            "glsl_450",
            "-matrix-layout-row-major",
            "-no-mangle",
            "-O2",
            "-warnings-as-errors",
            "all",
            "-o",
        ])
        .arg(output)
        .output()
        .unwrap_or_else(|error| panic!("run Slang GLSL frontend for {key:?}: {error}"));
    if !result.status.success() {
        panic!(
            "normalize built-in scene shader {key} {stage_name} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let normalized = fs::read_to_string(output).expect("read built-in scene stage frontend output");
    let normalized = repair_combined_sampler_intrinsics(&normalized);
    let normalized = inject_stage_io_locations(&normalized, glsl);
    let normalized = restore_glsl_resource_bindings(&normalized, glsl);
    fs::write(output, normalized).expect("write repaired built-in scene stage frontend output");
}

#[derive(Debug)]
struct GlslUniformBlock {
    binding: u32,
    members: Vec<String>,
}

fn restore_glsl_resource_bindings(hlsl: &str, glsl: &str) -> String {
    let mut hlsl = restore_glsl_uniform_blocks(hlsl, &glsl_uniform_blocks(glsl));
    for (binding, instance) in glsl_storage_blocks(glsl) {
        let marker = format!(" {instance} : register(u");
        let start = hlsl
            .find(&marker)
            .unwrap_or_else(|| panic!("normalized storage block {instance:?} is missing"));
        let register_start = start + marker.len();
        let register_end = hlsl[register_start..]
            .find(')')
            .map(|offset| register_start + offset)
            .expect("normalized storage register must be closed");
        hlsl.replace_range(register_start..register_end, &binding.to_string());
    }
    hlsl
}

fn restore_glsl_uniform_blocks(hlsl: &str, blocks: &[GlslUniformBlock]) -> String {
    if blocks.is_empty() {
        return hlsl.to_owned();
    }
    let Some(struct_range) = source_block_range(hlsl, "struct GlobalParams_0") else {
        return hlsl.to_owned();
    };
    let cbuffer_range = source_block_range(hlsl, "cbuffer globalParams_0")
        .expect("normalized GLSL uniforms must expose globalParams_0 cbuffer");
    let member_lines = hlsl[struct_range.clone()]
        .lines()
        .map(str::trim)
        .filter(|line| line.ends_with(';') && !line.starts_with('}'))
        .collect::<Vec<_>>();
    let mut replacement = String::new();
    let mut replacements = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let active_members = block
            .members
            .iter()
            .filter_map(|member| {
                member_lines
                    .iter()
                    .find(|line| identifier_occurrences(line, member) == 1)
                    .map(|line| (member, *line))
            })
            .collect::<Vec<_>>();
        if active_members.is_empty() {
            continue;
        }
        replacement.push_str(&format!("struct GilderBuiltinUniformBlock{index}\n{{\n"));
        for (_, line) in &active_members {
            replacement.push_str("    ");
            replacement.push_str(line);
            replacement.push('\n');
        }
        replacement.push_str("};\n");
        replacement.push_str(&format!(
            "cbuffer gilderUniformBlock{index} : register(b{})\n{{\n    GilderBuiltinUniformBlock{index} gilderUniform{index};\n}}\n\n",
            block.binding
        ));
        replacements.extend(active_members.into_iter().map(|(member, _)| {
            (
                format!("globalParams_0.{member}"),
                format!("gilderUniform{index}.{member}"),
            )
        }));
    }
    assert_eq!(
        replacements.len(),
        member_lines.len(),
        "normalized GLSL uniform members must map to exactly one authored block"
    );
    let mut hlsl = hlsl.to_owned();
    for range in [struct_range, cbuffer_range].into_iter().rev() {
        hlsl.replace_range(range, "");
    }
    let insertion = hlsl
        .find("static ")
        .or_else(|| hlsl.find("struct _S"))
        .unwrap_or(hlsl.len());
    hlsl.insert_str(insertion, &replacement);
    for (from, to) in replacements {
        hlsl = hlsl.replace(&from, &to);
    }
    hlsl
}

fn source_block_range(source: &str, header: &str) -> Option<std::ops::Range<usize>> {
    let start = source.find(header)?;
    let body = source[start..].find('{').map(|offset| start + offset)?;
    let end = source[body..].find("\n}").map(|offset| body + offset + 2)?;
    let end = if source.as_bytes().get(end) == Some(&b';') {
        end + 1
    } else {
        end
    };
    Some(start..end)
}

fn glsl_uniform_blocks(source: &str) -> Vec<GlslUniformBlock> {
    let mut blocks = Vec::new();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if !trimmed.contains(") uniform ") || !trimmed.ends_with('{') {
            continue;
        }
        let binding = glsl_layout_binding(trimmed)
            .expect("built-in scene uniform block must declare a binding");
        let mut members = Vec::new();
        for member in lines.by_ref() {
            let member = member.trim();
            if member.starts_with('}') {
                break;
            }
            if member.is_empty() {
                continue;
            }
            let declaration = member
                .strip_suffix(';')
                .expect("built-in scene uniform member must end in a semicolon");
            let declarator = declaration
                .split_whitespace()
                .next_back()
                .expect("built-in scene uniform member must have a name");
            let name = declarator.split_once('[').map_or(declarator, |item| item.0);
            members.push(name.to_owned());
        }
        blocks.push(GlslUniformBlock { binding, members });
    }
    blocks
}

fn glsl_storage_blocks(source: &str) -> Vec<(u32, String)> {
    let mut blocks = Vec::new();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if !trimmed.contains(" buffer ") || !trimmed.ends_with('{') {
            continue;
        }
        let binding = glsl_layout_binding(trimmed)
            .expect("built-in scene storage block must declare a binding");
        for end in lines.by_ref() {
            let end = end.trim();
            if let Some(instance) = end
                .strip_prefix("} ")
                .and_then(|line| line.strip_suffix(';'))
            {
                blocks.push((binding, instance.to_owned()));
                break;
            }
        }
    }
    blocks
}

fn glsl_layout_binding(line: &str) -> Option<u32> {
    let (_, binding) = line.split_once("binding = ")?;
    binding
        .split(|character: char| !character.is_ascii_digit())
        .next()
        .and_then(|binding| binding.parse().ok())
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

fn inject_stage_io_locations(hlsl: &str, glsl: &str) -> String {
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
            declaration if declaration.starts_with("in ")
                || declaration.starts_with("flat in ")
                || declaration.starts_with("out ")
                || declaration.starts_with("flat out ")
        ) {
            continue;
        }
        let Some(declaration) = location
            .1
            .trim_start()
            .strip_prefix("in ")
            .or_else(|| location.1.trim_start().strip_prefix("flat in "))
            .or_else(|| location.1.trim_start().strip_prefix("out "))
            .or_else(|| location.1.trim_start().strip_prefix("flat out "))
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
            "ivec3" => "int3",
            "ivec4" => "int4",
            "uint" => "uint",
            "uvec2" => "uint2",
            "uvec3" => "uint3",
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
                DescriptorHeapBindingKind::InputAttachment => "InputAttachment",
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
