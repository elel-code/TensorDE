//! Strict cold lowering from the generated Wallpaper Engine stage dialect into
//! native Slang source.
//!
//! This module intentionally does not invoke a GLSL or HLSL compiler frontend.
//! It accepts only the small, explicit generated-stage grammar, emits native
//! Slang, and rejects anything outside that grammar instead of falling back.

use std::collections::BTreeMap;

use crate::ShaderStage;

mod implicit_sampler;
mod interface;
mod intrinsics;
mod storage;
#[cfg(test)]
mod tests;

use interface::Item as InterfaceItem;

#[derive(Debug, Clone, PartialEq, Eq)]
struct UniformItem {
    ty: String,
    declarator: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UniformBlock {
    name: String,
    instance: String,
    binding: u32,
    body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SampledImage {
    name: String,
    sampler_type: String,
    binding: u32,
    implicit_binding: bool,
}

#[derive(Default)]
struct Declarations {
    inputs: Vec<InterfaceItem>,
    outputs: Vec<InterfaceItem>,
    uniform_blocks: Vec<UniformBlock>,
    uniforms_by_binding: BTreeMap<u32, Vec<UniformItem>>,
    sampled_images: Vec<SampledImage>,
    storage_buffers: Vec<storage::StorageBuffer>,
}

/// Lowers the generated scene-stage source language into directly compilable
/// native Slang. The output deliberately has no `layout`, `#version`, GLSL
/// sampler declaration, or GLSL entry-point ABI.
pub fn lower_generated_stage_to_native_slang(
    source: &str,
    stage: ShaderStage,
) -> Result<String, String> {
    if stage == ShaderStage::Compute {
        return Err("generated compute stages must provide native Slang directly".to_owned());
    }
    let (declarations, body) = collect_declarations(source, stage)?;
    let body = intrinsics::lower_generated_intrinsics(body, &declarations)?;
    let entry = lower_entry_point(&body, &declarations, stage)?;
    Ok([
        native_prelude(),
        emit_resources(&declarations),
        interface::emit_statics(&declarations, stage, &entry),
        entry.source,
    ]
    .join("\n"))
}

fn collect_declarations(
    source: &str,
    stage: ShaderStage,
) -> Result<(Declarations, String), String> {
    let mut declarations = Declarations::default();
    let mut body = Vec::new();
    let lines = source.lines().collect::<Vec<_>>();
    let mut line_index = 0;

    while line_index < lines.len() {
        let line = lines[line_index];
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("#version")
            || trimmed.starts_with("precision ")
            || trimmed.starts_with("#extension")
        {
            line_index += 1;
            continue;
        }
        if let Some((layout, rest)) = split_layout(trimmed)? {
            if let Some(interface) = parse_interface(rest, layout)? {
                match interface.0 {
                    InterfaceDirection::Input => declarations.inputs.push(interface.1),
                    InterfaceDirection::Output => declarations.outputs.push(interface.1),
                }
                line_index += 1;
                continue;
            }
            if rest.starts_with("uniform ") && rest.contains('{') {
                let (block, consumed) = parse_uniform_block(&lines[line_index..], layout)?;
                declarations.uniform_blocks.push(block);
                line_index += consumed;
                continue;
            }
            if let Some(sampler) = parse_sampler(rest, layout)? {
                declarations.sampled_images.push(sampler);
                line_index += 1;
                continue;
            }
            if let Some((storage, consumed)) = storage::parse_storage_buffer(&lines[line_index..])?
            {
                declarations.storage_buffers.push(storage);
                line_index += consumed;
                continue;
            }
            if let Some(uniform) = parse_uniform(rest)? {
                let binding = layout_binding(layout)?;
                declarations
                    .uniforms_by_binding
                    .entry(binding)
                    .or_default()
                    .push(uniform);
                line_index += 1;
                continue;
            }
            if rest == "in;" && layout.contains("local_size_") {
                return Err(
                    "generated compute workgroup layout reached graphics lowering".to_owned(),
                );
            }
            return Err(format!(
                "unsupported generated stage layout declaration: {trimmed}"
            ));
        }
        if let Some(sampler) = implicit_sampler::parse(trimmed)? {
            declarations.sampled_images.push(sampler);
            line_index += 1;
            continue;
        }
        if let Some(uniform) = parse_uniform(trimmed)? {
            declarations
                .uniforms_by_binding
                .entry(0)
                .or_default()
                .push(uniform);
            line_index += 1;
            continue;
        }
        if trimmed.starts_with("attribute ") || trimmed.starts_with("varying ") {
            return Err(format!(
                "legacy stage-I/O declaration reached native lowering: {trimmed}"
            ));
        }
        body.push(line);
        line_index += 1;
    }

    let body = storage::rewrite_member_accesses(body.join("\n"), &declarations.storage_buffers)?;
    prune_unreferenced_resources(&mut declarations, &body);
    validate_declarations(&declarations, stage)?;
    Ok((declarations, body))
}

fn prune_unreferenced_resources(declarations: &mut Declarations, source: &str) {
    declarations
        .sampled_images
        .retain(|sampled| has_identifier(source, &sampled.name));
    declarations
        .uniform_blocks
        .retain(|block| has_identifier(source, &block.instance));
    declarations
        .storage_buffers
        .retain(|buffer| has_identifier(source, buffer.instance()));
    for uniforms in declarations.uniforms_by_binding.values_mut() {
        uniforms.retain(|uniform| {
            let name = uniform
                .declarator
                .split_once('[')
                .map_or(uniform.declarator.as_str(), |(name, _)| name);
            has_identifier(source, name)
        });
    }
    declarations
        .uniforms_by_binding
        .retain(|_, uniforms| !uniforms.is_empty());
    implicit_sampler::reindex(&mut declarations.sampled_images);
}

fn validate_declarations(declarations: &Declarations, stage: ShaderStage) -> Result<(), String> {
    interface::validate_locations(&declarations.inputs, "input")?;
    interface::validate_locations(&declarations.outputs, "output")?;
    if stage == ShaderStage::Fragment && declarations.outputs.is_empty() {
        return Err("fragment stage has no explicit color output".to_owned());
    }
    Ok(())
}

enum InterfaceDirection {
    Input,
    Output,
}

fn split_layout(line: &str) -> Result<Option<(&str, &str)>, String> {
    let Some(rest) = line.strip_prefix("layout(") else {
        return Ok(None);
    };
    let Some(end) = rest.find(')') else {
        return Err(format!("unterminated generated layout declaration: {line}"));
    };
    Ok(Some((&rest[..end], rest[end + 1..].trim())))
}

fn parse_interface(
    rest: &str,
    layout: &str,
) -> Result<Option<(InterfaceDirection, InterfaceItem)>, String> {
    let location = match layout_location(layout) {
        Some(location) => location,
        None => return Ok(None),
    };
    let (flat, rest) = rest
        .strip_prefix("flat ")
        .map_or((false, rest), |rest| (true, rest));
    let (direction, declaration) = if let Some(declaration) = rest.strip_prefix("in ") {
        (InterfaceDirection::Input, declaration)
    } else if let Some(declaration) = rest.strip_prefix("out ") {
        (InterfaceDirection::Output, declaration)
    } else {
        return Ok(None);
    };
    let (ty, declarator) = declaration_parts(declaration)?;
    Ok(Some((
        direction,
        InterfaceItem {
            location,
            ty,
            declarator: interface::Declarator::parse(&declarator)?,
            flat,
        },
    )))
}

fn parse_sampler(rest: &str, layout: &str) -> Result<Option<SampledImage>, String> {
    let Some(declaration) = rest.strip_prefix("uniform ") else {
        return Ok(None);
    };
    let (sampler_type, name) = declaration_parts(declaration)?;
    if !is_sampled_image_type(&sampler_type) {
        return Ok(None);
    }
    if name.contains('[') {
        return Err(format!("sampled-image arrays are not supported: {name}"));
    }
    Ok(Some(SampledImage {
        name,
        sampler_type,
        binding: layout_binding(layout)?,
        implicit_binding: false,
    }))
}

fn is_sampled_image_type(sampler_type: &str) -> bool {
    sampler_type.starts_with("sampler")
        || sampler_type.starts_with("isampler")
        || sampler_type.starts_with("usampler")
}

fn parse_uniform(rest: &str) -> Result<Option<UniformItem>, String> {
    let Some(declaration) = rest.strip_prefix("uniform ") else {
        return Ok(None);
    };
    if declaration.contains('{') {
        return Ok(None);
    }
    let (ty, declarator) = declaration_parts(declaration)?;
    if ty.starts_with("sampler") || ty.starts_with("isampler") || ty.starts_with("usampler") {
        return Ok(None);
    }
    Ok(Some(UniformItem { ty, declarator }))
}

fn parse_uniform_block(lines: &[&str], layout: &str) -> Result<(UniformBlock, usize), String> {
    let header = lines
        .first()
        .ok_or_else(|| "missing generated uniform block header".to_owned())?
        .trim();
    let (_, rest) =
        split_layout(header)?.ok_or_else(|| "uniform block header lacks layout".to_owned())?;
    let name = rest
        .strip_prefix("uniform ")
        .and_then(|rest| rest.split_once('{'))
        .map(|(name, _)| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("invalid generated uniform block header: {header}"))?;
    if let Some((_, tail)) = rest.split_once('{')
        && let Some((body, instance)) = tail.split_once('}')
    {
        let instance = instance
            .trim()
            .strip_suffix(';')
            .map(str::trim)
            .ok_or_else(|| format!("invalid generated uniform block terminator: {header}"))?;
        return Ok((
            UniformBlock {
                instance: if instance.is_empty() {
                    name.clone()
                } else {
                    instance.to_owned()
                },
                name,
                binding: layout_binding(layout)?,
                body: body.trim().to_owned(),
            },
            1,
        ));
    }
    let mut body = Vec::new();
    for (offset, line) in lines.iter().enumerate().skip(1) {
        let trimmed = line.trim();
        if let Some(instance) = trimmed
            .strip_prefix('}')
            .map(str::trim)
            .and_then(|line| line.strip_suffix(';'))
        {
            let instance = if instance.is_empty() {
                name.clone()
            } else {
                instance.to_owned()
            };
            return Ok((
                UniformBlock {
                    name,
                    instance,
                    binding: layout_binding(layout)?,
                    body: body.join("\n"),
                },
                offset + 1,
            ));
        }
        body.push(*line);
    }
    Err("unterminated generated uniform block".to_owned())
}

fn layout_location(layout: &str) -> Option<u32> {
    layout
        .split(',')
        .map(str::trim)
        .find_map(|item| item.strip_prefix("location"))
        .and_then(|item| item.trim_start().strip_prefix('='))
        .and_then(|item| item.trim().parse().ok())
}

fn layout_binding(layout: &str) -> Result<u32, String> {
    let binding = layout
        .split(',')
        .map(str::trim)
        .find_map(|item| item.strip_prefix("binding"))
        .and_then(|item| item.trim_start().strip_prefix('='))
        .and_then(|item| item.trim().parse().ok())
        .ok_or_else(|| format!("generated resource layout lacks binding: {layout}"))?;
    Ok(binding)
}

fn declaration_parts(declaration: &str) -> Result<(String, String), String> {
    let declaration = declaration
        .split_once("//")
        .map_or(declaration, |(declaration, _)| declaration)
        .trim()
        .strip_suffix(';')
        .map(str::trim)
        .ok_or_else(|| format!("generated declaration lacks semicolon: {declaration}"))?;
    let mut parts = declaration.split_ascii_whitespace();
    let ty = parts
        .next()
        .ok_or_else(|| format!("generated declaration lacks type: {declaration}"))?;
    let declarator = parts
        .next()
        .ok_or_else(|| format!("generated declaration lacks name: {declaration}"))?;
    if parts.next().is_some() {
        return Err(format!(
            "generated declaration has unexpected tokens: {declaration}"
        ));
    }
    Ok((ty.to_owned(), declarator.to_owned()))
}

struct LoweredEntry {
    source: String,
    uses_vertex_index: bool,
    uses_instance_index: bool,
    uses_frag_coord: bool,
    uses_front_facing: bool,
}

fn lower_entry_point(
    source: &str,
    declarations: &Declarations,
    stage: ShaderStage,
) -> Result<LoweredEntry, String> {
    let marker = "void main";
    let main_start = source
        .find(marker)
        .ok_or_else(|| "generated stage has no void main entry point".to_owned())?;
    let arguments_start = source[main_start..]
        .find('(')
        .map(|offset| main_start + offset)
        .ok_or_else(|| "generated main lacks opening parenthesis".to_owned())?;
    let arguments_end = matching_delimiter(source, arguments_start, '(', ')')?;
    if !source[arguments_start + 1..arguments_end].trim().is_empty() {
        return Err("generated main entry point must not take parameters".to_owned());
    }
    let body_start = source[arguments_end + 1..]
        .find('{')
        .map(|offset| arguments_end + 1 + offset)
        .ok_or_else(|| "generated main lacks opening body".to_owned())?;
    let body_end = matching_delimiter(source, body_start, '{', '}')?;
    let prefix = &source[..main_start];
    let mut body = source[body_start + 1..body_end].to_owned();

    let uses_vertex_index = has_identifier(&body, "gl_VertexIndex");
    let uses_instance_index = has_identifier(&body, "gl_InstanceIndex");
    let uses_frag_coord = has_identifier(&body, "gl_FragCoord");
    let uses_front_facing = has_identifier(&body, "gl_FrontFacing");

    if stage == ShaderStage::Vertex {
        body = replace_identifier(&body, "gl_Position", "gilderPosition");
    } else {
        if has_identifier(&body, "gl_FragColor") {
            let output = declarations
                .outputs
                .first()
                .ok_or_else(|| "gl_FragColor has no declared color output".to_owned())?;
            if output.declarator.is_array() {
                return Err("gl_FragColor cannot target an interface array".to_owned());
            }
            body = replace_identifier(&body, "gl_FragColor", output.declarator.name());
        }
        if has_identifier(&body, "gl_FragData") {
            return Err("gl_FragData is not supported by native stage lowering".to_owned());
        }
    }
    let output_name = if stage == ShaderStage::Vertex {
        "GilderVertexOutput"
    } else {
        "GilderFragmentOutput"
    };
    let input_name = if stage == ShaderStage::Vertex {
        "GilderVertexInput"
    } else {
        "GilderFragmentInput"
    };
    let mut setup = String::new();
    for input in &declarations.inputs {
        interface::emit_input_copy(input, &mut setup);
    }
    if uses_vertex_index {
        setup.push_str("    gl_VertexIndex = input.gilderVertexIndex;\n");
    }
    if uses_instance_index {
        setup.push_str("    gl_InstanceIndex = input.gilderInstanceIndex;\n");
    }
    if uses_frag_coord {
        setup.push_str("    gl_FragCoord = input.gilderFragCoord;\n");
    }
    if uses_front_facing {
        setup.push_str("    gl_FrontFacing = input.gilderFrontFacing;\n");
    }
    setup.push_str(&format!("    {output_name} output;\n"));
    let mut finish = String::new();
    if stage == ShaderStage::Vertex {
        finish.push_str("    output.position = gilderPosition;\n");
    }
    for output in &declarations.outputs {
        interface::emit_output_copy(output, &mut finish);
    }
    finish.push_str("    return output;\n");
    body = replace_void_returns(&body, &finish);
    let source = format!(
        "{prefix}[[shader(\"{}\")]]\n{output_name} main({input_name} input)\n{{\n{setup}{body}\n{finish}}}\n{}",
        stage.slang_name(),
        &source[body_end + 1..]
    );
    Ok(LoweredEntry {
        source,
        uses_vertex_index,
        uses_instance_index,
        uses_frag_coord,
        uses_front_facing,
    })
}

fn emit_resources(declarations: &Declarations) -> String {
    let mut output = String::new();
    for sampled in &declarations.sampled_images {
        let texture =
            sampled_texture_type(&sampled.sampler_type).expect("validated sampled image type");
        output.push_str(&format!(
            "{texture} {}_texture : register(t{});\nSamplerState {}_sampler : register(s{});\n",
            sampled.name, sampled.binding, sampled.name, sampled.binding
        ));
        output.push_str(&sampled_texture_size_helper(sampled));
    }
    for storage in &declarations.storage_buffers {
        output.push_str(&storage::emit_storage_buffer(storage));
    }
    for block in &declarations.uniform_blocks {
        output.push_str(&format!(
            "struct {}\n{{\n{}\n}};\ncbuffer {}_Buffer : register(b{})\n{{\n    {} {};\n}}\n",
            block.name, block.body, block.name, block.binding, block.name, block.instance
        ));
    }
    for (binding, uniforms) in &declarations.uniforms_by_binding {
        if uniforms.is_empty() {
            continue;
        }
        let struct_name = format!("GilderUniforms{binding}Data");
        let instance = format!("gilderUniforms{binding}");
        output.push_str(&format!("struct {struct_name}\n{{\n"));
        for uniform in uniforms {
            output.push_str(&format!("    {} {};\n", uniform.ty, uniform.declarator));
        }
        output.push_str(&format!(
            "}};\ncbuffer GilderUniforms{binding} : register(b{binding})\n{{\n    {struct_name} {instance};\n}}\n"
        ));
        for uniform in uniforms {
            let name = uniform
                .declarator
                .split_once('[')
                .map_or(uniform.declarator.as_str(), |(name, _)| name);
            output.push_str(&format!("#define {name} {instance}.{name}\n"));
        }
    }
    output
}

fn sampled_texture_size_helper(sampled: &SampledImage) -> String {
    let texture = format!("{}_texture", sampled.name);
    match sampled.sampler_type.as_str() {
        "sampler2D" | "isampler2D" | "usampler2D" | "samplerCube" => format!(
            "int2 gilderTextureSize_{}(uint mip)\n{{\n    uint width;\n    uint height;\n    uint levels;\n    {texture}.GetDimensions(mip, width, height, levels);\n    return int2(width, height);\n}}\n",
            sampled.name
        ),
        "sampler3D" => format!(
            "int3 gilderTextureSize_{}(uint mip)\n{{\n    uint width;\n    uint height;\n    uint depth;\n    uint levels;\n    {texture}.GetDimensions(mip, width, height, depth, levels);\n    return int3(width, height, depth);\n}}\n",
            sampled.name
        ),
        "sampler2DArray" => format!(
            "int3 gilderTextureSize_{}(uint mip)\n{{\n    uint width;\n    uint height;\n    uint elements;\n    uint levels;\n    {texture}.GetDimensions(mip, width, height, elements, levels);\n    return int3(width, height, elements);\n}}\n",
            sampled.name
        ),
        unsupported => panic!("validated unsupported sampled image type {unsupported}"),
    }
}

fn sampled_texture_type(sampler_type: &str) -> Option<&'static str> {
    match sampler_type {
        "sampler2D" => Some("Texture2D<float4>"),
        "sampler3D" => Some("Texture3D<float4>"),
        "samplerCube" => Some("TextureCube<float4>"),
        "sampler2DArray" => Some("Texture2DArray<float4>"),
        "isampler2D" => Some("Texture2D<int4>"),
        "usampler2D" => Some("Texture2D<uint4>"),
        _ => None,
    }
}

fn matching_delimiter(
    source: &str,
    start: usize,
    open: char,
    close: char,
) -> Result<usize, String> {
    let mut depth = 0u32;
    for (offset, character) in source[start..].char_indices() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth = depth
                .checked_sub(1)
                .ok_or_else(|| "generated delimiter underflow".to_owned())?;
            if depth == 0 {
                return Ok(start + offset);
            }
        }
    }
    Err(format!("unterminated generated delimiter {open}"))
}

fn has_identifier(source: &str, identifier: &str) -> bool {
    source.match_indices(identifier).any(|(offset, _)| {
        let before = source[..offset].chars().next_back();
        let after = source[offset + identifier.len()..].chars().next();
        !before.is_some_and(is_identifier_character) && !after.is_some_and(is_identifier_character)
    })
}

fn replace_identifier(source: &str, identifier: &str, replacement: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for (offset, _) in source.match_indices(identifier) {
        let before = source[..offset].chars().next_back();
        let after = source[offset + identifier.len()..].chars().next();
        if before.is_some_and(is_identifier_character) || after.is_some_and(is_identifier_character)
        {
            continue;
        }
        output.push_str(&source[cursor..offset]);
        output.push_str(replacement);
        cursor = offset + identifier.len();
    }
    output.push_str(&source[cursor..]);
    output
}

fn is_identifier_character(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

fn replace_void_returns(source: &str, replacement: &str) -> String {
    source
        .lines()
        .map(|line| {
            if line.trim() == "return;" {
                let indent = &line[..line.len() - line.trim_start().len()];
                replacement
                    .lines()
                    .map(|line| format!("{indent}{}", line.trim_start()))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn native_prelude() -> String {
    r#"#define vec2 float2
#define vec3 float3
#define vec4 float4
#define ivec2 int2
#define ivec3 int3
#define ivec4 int4
#define uvec2 uint2
#define uvec3 uint3
#define uvec4 uint4
#define bvec2 bool2
#define bvec3 bool3
#define bvec4 bool4
#define mat2 float2x2
#define mat3 float3x3
#define mat4 float4x4
#define fract frac
#define mix lerp
#define inversesqrt rsqrt
#define roundEven round
#define texture2D(S, UV) S ## _texture.Sample(S ## _sampler, UV)
#define texture3D(S, UVW) S ## _texture.Sample(S ## _sampler, UVW)
#define texture(S, UV) S ## _texture.Sample(S ## _sampler, UV)
#define texture2DLod(S, UV, LOD) S ## _texture.SampleLevel(S ## _sampler, UV, LOD)
#define textureLod(S, UV, LOD) S ## _texture.SampleLevel(S ## _sampler, UV, LOD)
#define texelFetch(S, COORD, LOD) S ## _texture.Load(int3(COORD, LOD))
#define textureSize(S, LOD) gilderTextureSize_ ## S(LOD)
#define greaterThan(A, B) ((A) > (B))
#define greaterThanEqual(A, B) ((A) >= (B))
#define lessThan(A, B) ((A) < (B))
#define lessThanEqual(A, B) ((A) <= (B))
#define equal(A, B) ((A) == (B))
#define notEqual(A, B) ((A) != (B))
#define mod(A, B) ((A) - (B) * floor((A) / (B)))
"#
    .to_owned()
}
