//! Typed descriptor-heap lowering for Slang-compatible source.

use std::collections::BTreeSet;

use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DescriptorHeapBindingKind {
    InputAttachment,
    SampledImage,
    StorageImage,
    Sampler,
    UniformBuffer,
    StorageBuffer,
}

/// Lowers one exact-pixel fragment input into a native resource-heap proxy.
///
/// Slang 2026.14 does not allow `SubpassInput<T>` as a `DescriptorHandle<T>`
/// argument. The returned source therefore uses a storage-image proxy with
/// the same `OpImageRead` operation. [`crate::SlangCompiler::compile_input_attachment`]
/// legalizes and strictly validates the final SPIR-V `SubpassData` type.
pub fn lower_slang_input_attachment_to_descriptor_heap_at_offset(
    source: &str,
    entry_point: &str,
    push_base_bytes: u32,
) -> Result<DescriptorHeapSlang> {
    if !push_base_bytes.is_multiple_of(4) {
        return Err(Error::SourceLowering(
            "descriptor push base must be a multiple of four bytes".to_owned(),
        ));
    }
    let mut retained = Vec::new();
    let mut input = None;
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some((declaration, register)) = trimmed.split_once(" : register(t")
            && declaration.starts_with("SubpassInput<")
        {
            if input.is_some() {
                return Err(Error::SourceLowering(
                    "native input-attachment shader exposes more than one input".to_owned(),
                ));
            }
            let register = register.strip_suffix(");").ok_or_else(|| {
                Error::SourceLowering(format!(
                    "invalid input-attachment register declaration `{trimmed}`"
                ))
            })?;
            let register = register.parse::<u32>().map_err(|error| {
                Error::SourceLowering(format!(
                    "invalid input-attachment register `{register}`: {error}"
                ))
            })?;
            let split = declaration.rfind(char::is_whitespace).ok_or_else(|| {
                Error::SourceLowering(format!(
                    "input-attachment declaration has no name: {trimmed}"
                ))
            })?;
            let source_type = declaration[..split].trim();
            let source_name = declaration[split..].trim();
            if source_type != "SubpassInput<float4>" || source_name.is_empty() {
                return Err(Error::SourceLowering(format!(
                    "unsupported native input-attachment declaration `{trimmed}`"
                )));
            }
            input = Some((register, source_name.to_owned(), source_type.to_owned()));
            continue;
        }
        if trimmed.starts_with("SubpassInput") || trimmed.contains(": register(") {
            return Err(Error::SourceLowering(format!(
                "native input-attachment shader exposes an unsupported resource `{trimmed}`"
            )));
        }
        retained.push(line.to_owned());
    }
    let Some((register, source_name, source_type)) = input else {
        return Err(Error::SourceLowering(
            "native input-attachment shader exposes no SubpassInput<float4>".to_owned(),
        ));
    };
    let load = format!("{source_name}.SubpassLoad()");
    let load_count = retained
        .iter()
        .map(|line| line.matches(&load).count())
        .sum::<usize>();
    if load_count != 1 {
        return Err(Error::SourceLowering(format!(
            "native input attachment `{source_name}` must have exactly one SubpassLoad, found {load_count}"
        )));
    }
    let mut body = retained
        .join("\n")
        .replace(&load, &format!("gilderHeap_{source_name}()[int2(0, 0)]"));
    if body.contains("SubpassInput") || body.contains("SubpassLoad") {
        return Err(Error::SourceLowering(
            "native input-attachment lowering left an opaque subpass operation".to_owned(),
        ));
    }
    let entry_offset = entry_definition_offset(&body, entry_point)?;
    let push_offset = push_base_bytes;
    let push_constant_bytes = push_base_bytes
        .checked_add(4)
        .ok_or_else(|| Error::SourceLowering("push layout exceeds u32".to_owned()))?;
    let mut prelude = String::from("struct GilderDescriptorHeapPush\n{\n");
    if push_base_bytes != 0 {
        prelude.push_str(&format!(
            "    uint reservedPipelineWords[{}];\n",
            push_base_bytes / 4
        ));
    }
    prelude.push_str("    uint binding0Index;\n};\n");
    prelude.push_str(
        "[[vk::push_constant]] ConstantBuffer<GilderDescriptorHeapPush> gilderHeapPush;\n\n",
    );
    prelude.push_str(&format!(
        "RWTexture2D<float4> gilderHeap_{source_name}()\n{{\n    return DescriptorHandle<RWTexture2D<float4>>(gilderHeapPush.binding0Index);\n}}\n\n"
    ));
    body.insert_str(entry_offset, &prelude);
    Ok(DescriptorHeapSlang {
        source: body,
        push_constant_bytes,
        bindings: vec![DescriptorHeapBinding {
            kind: DescriptorHeapBindingKind::InputAttachment,
            register,
            push_offset,
            source_name,
            source_type,
        }],
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptorHeapBinding {
    pub kind: DescriptorHeapBindingKind,
    pub register: u32,
    pub push_offset: u32,
    pub source_name: String,
    pub source_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptorHeapSlang {
    pub source: String,
    pub push_constant_bytes: u32,
    pub bindings: Vec<DescriptorHeapBinding>,
}

pub fn lower_slang_bindings_to_descriptor_heap(
    source: &str,
    entry_point: &str,
) -> Result<DescriptorHeapSlang> {
    lower_slang_bindings_to_descriptor_heap_at_offset(source, entry_point, 0)
}

/// Lowers shader resources into the pipeline-global descriptor push-data
/// address space beginning at `push_base_bytes`.
///
/// Graphics stages share the bytes written by `vkCmdPushDataEXT`; callers
/// compiling a multi-stage program must therefore assign non-overlapping
/// ranges instead of restarting every stage at byte zero.
pub fn lower_slang_bindings_to_descriptor_heap_at_offset(
    source: &str,
    entry_point: &str,
    push_base_bytes: u32,
) -> Result<DescriptorHeapSlang> {
    if !push_base_bytes.is_multiple_of(4) {
        return Err(Error::SourceLowering(
            "descriptor push base must be a multiple of four bytes".to_owned(),
        ));
    }
    let mut retained = Vec::new();
    let mut bindings = Vec::new();
    let mut lines = source.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("#pragma") {
            continue;
        }
        if let Some(binding) = resource_declaration(line)? {
            bindings.push(binding);
            continue;
        }
        if let Some(register) = cbuffer_register(line)? {
            let member = parse_cbuffer_member(&mut lines)?;
            bindings.push(ParsedBinding {
                kind: DescriptorHeapBindingKind::UniformBuffer,
                register,
                source_name: member.1,
                source_type: format!("ConstantBuffer<{}>", member.0),
            });
            continue;
        }
        retained.push(line.to_owned());
    }
    if bindings.is_empty() {
        return Err(Error::SourceLowering(
            "descriptor-heap shader exposes no resource declarations".to_owned(),
        ));
    }
    bindings.sort_by(|left, right| {
        (left.kind, left.register, &left.source_name).cmp(&(
            right.kind,
            right.register,
            &right.source_name,
        ))
    });
    let mut identities = BTreeSet::new();
    for binding in &bindings {
        if !identities.insert((binding.kind, binding.register)) {
            return Err(Error::SourceLowering(format!(
                "duplicate {:?} register {}",
                binding.kind, binding.register
            )));
        }
    }
    let mut body = retained.join("\n");
    for binding in &bindings {
        body = replace_identifier(
            &body,
            &binding.source_name,
            &format!("gilderHeap_{}()", binding.source_name),
        );
    }
    let entry_offset = entry_definition_offset(&body, entry_point)?;
    let mut prelude = String::new();
    prelude.push_str("struct GilderDescriptorHeapPush\n{\n");
    if push_base_bytes != 0 {
        prelude.push_str(&format!(
            "    uint reservedPipelineWords[{}];\n",
            push_base_bytes / 4
        ));
    }
    let bindings = bindings
        .into_iter()
        .enumerate()
        .map(|(index, binding)| {
            let local_offset = u32::try_from(index)
                .ok()
                .and_then(|index| index.checked_mul(4))
                .ok_or_else(|| Error::SourceLowering("push layout exceeds u32".to_owned()))?;
            let push_offset = push_base_bytes
                .checked_add(local_offset)
                .ok_or_else(|| Error::SourceLowering("push layout exceeds u32".to_owned()))?;
            prelude.push_str(&format!("    uint binding{index}Index;\n"));
            Ok(DescriptorHeapBinding {
                kind: binding.kind,
                register: binding.register,
                push_offset,
                source_name: binding.source_name,
                source_type: binding.source_type,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    prelude.push_str("};\n");
    prelude.push_str(
        "[[vk::push_constant]] ConstantBuffer<GilderDescriptorHeapPush> gilderHeapPush;\n\n",
    );
    for (index, binding) in bindings.iter().enumerate() {
        prelude.push_str(&format!(
            "{} gilderHeap_{}()\n{{\n    return DescriptorHandle<{}>(gilderHeapPush.binding{}Index);\n}}\n\n",
            binding.source_type, binding.source_name, binding.source_type, index
        ));
    }
    body.insert_str(entry_offset, &prelude);
    let local_bytes = u32::try_from(bindings.len())
        .ok()
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(|| Error::SourceLowering("push layout exceeds u32".to_owned()))?;
    let push_constant_bytes = push_base_bytes
        .checked_add(local_bytes)
        .ok_or_else(|| Error::SourceLowering("push layout exceeds u32".to_owned()))?;
    Ok(DescriptorHeapSlang {
        source: body,
        push_constant_bytes,
        bindings,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedBinding {
    kind: DescriptorHeapBindingKind,
    register: u32,
    source_name: String,
    source_type: String,
}

fn resource_declaration(line: &str) -> Result<Option<ParsedBinding>> {
    let trimmed = line.trim();
    let Some((declaration, register)) = trimmed.split_once(": register(") else {
        return Ok(None);
    };
    let Some(register) = register.strip_suffix(");") else {
        return Ok(None);
    };
    let Some((class, number)) = register.split_at_checked(1) else {
        return Ok(None);
    };
    if !matches!(class, "t" | "s" | "u") {
        return Ok(None);
    }
    let register = number.parse::<u32>().map_err(|error| {
        Error::SourceLowering(format!("invalid resource register `{register}`: {error}"))
    })?;
    let declaration = declaration.trim_end();
    let split = declaration.rfind(char::is_whitespace).ok_or_else(|| {
        Error::SourceLowering(format!("resource declaration has no name: {trimmed}"))
    })?;
    let source_type = declaration[..split].trim().to_owned();
    let source_name = declaration[split..].trim().to_owned();
    if source_name.is_empty() {
        return Err(Error::SourceLowering(format!(
            "resource declaration has an empty name: {trimmed}"
        )));
    }
    let kind = match class {
        "s" if source_type.starts_with("Sampler") => DescriptorHeapBindingKind::Sampler,
        "t" if source_type.starts_with("Texture") => DescriptorHeapBindingKind::SampledImage,
        "t" if source_type.contains("Buffer") => DescriptorHeapBindingKind::StorageBuffer,
        "u" if source_type.starts_with("RWTexture") => DescriptorHeapBindingKind::StorageImage,
        "u" if source_type.contains("Buffer") => DescriptorHeapBindingKind::StorageBuffer,
        _ => {
            return Err(Error::SourceLowering(format!(
                "unsupported resource declaration `{trimmed}`"
            )));
        }
    };
    Ok(Some(ParsedBinding {
        kind,
        register,
        source_name,
        source_type,
    }))
}

fn cbuffer_register(line: &str) -> Result<Option<u32>> {
    let trimmed = line.trim();
    if !trimmed.starts_with("cbuffer ") {
        return Ok(None);
    }
    let (_, register) = trimmed.split_once(": register(b").ok_or_else(|| {
        Error::SourceLowering(format!("cbuffer has no explicit register: {trimmed}"))
    })?;
    let register = register
        .strip_suffix(')')
        .ok_or_else(|| Error::SourceLowering(format!("invalid cbuffer register: {trimmed}")))?;
    register.parse::<u32>().map(Some).map_err(|error| {
        Error::SourceLowering(format!("invalid cbuffer register `{register}`: {error}"))
    })
}

fn parse_cbuffer_member<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Result<(String, String)> {
    let Some(open) = lines.next() else {
        return Err(Error::SourceLowering("unterminated cbuffer".to_owned()));
    };
    if open.trim() != "{" {
        return Err(Error::SourceLowering(
            "cbuffer opening brace must be on its own line".to_owned(),
        ));
    }
    let mut members = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "}" {
            break;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let declaration = trimmed
            .strip_suffix(';')
            .ok_or_else(|| Error::SourceLowering(format!("invalid cbuffer member `{trimmed}`")))?;
        let split = declaration.rfind(char::is_whitespace).ok_or_else(|| {
            Error::SourceLowering(format!("cbuffer member has no name: {trimmed}"))
        })?;
        members.push((
            declaration[..split].trim().to_owned(),
            declaration[split..].trim().to_owned(),
        ));
    }
    let [member] = members.as_slice() else {
        return Err(Error::SourceLowering(format!(
            "expected one typed cbuffer member, found {}",
            members.len()
        )));
    };
    Ok(member.clone())
}

fn entry_definition_offset(source: &str, entry_point: &str) -> Result<usize> {
    let needle = format!(" {entry_point}(");
    let found = source
        .find(&needle)
        .ok_or_else(|| Error::SourceLowering(format!("entry point `{entry_point}` is missing")))?;
    let mut offset = source[..found].rfind('\n').map_or(0, |offset| offset + 1);
    loop {
        let prefix = source[..offset].trim_end_matches(['\r', '\n']);
        let line_offset = prefix.rfind('\n').map_or(0, |line| line + 1);
        let line = prefix[line_offset..].trim();
        if !line.starts_with('[') || !line.ends_with(']') {
            break;
        }
        offset = line_offset;
    }
    Ok(offset)
}

fn replace_identifier(source: &str, name: &str, replacement: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut start = 0;
    while let Some(relative) = source[start..].find(name) {
        let found = start + relative;
        let end = found + name.len();
        let left = source[..found].chars().next_back();
        let right = source[end..].chars().next();
        let boundary = |character: Option<char>| {
            character
                .is_none_or(|character| !(character.is_ascii_alphanumeric() || character == '_'))
        };
        output.push_str(&source[start..found]);
        if boundary(left) && boundary(right) {
            output.push_str(replacement);
        } else {
            output.push_str(name);
        }
        start = end;
    }
    output.push_str(&source[start..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_glsl_frontend_resources_to_typed_heap_accessors() {
        let source = r#"Texture2D<float4 > image_0 : register(t0);
SamplerState imageSampler_0 : register(s0);
struct GlobalParams_0
{
    float4 tint_0;
};
cbuffer globals_0 : register(b0)
{
    GlobalParams_0 globals_0;
}
struct FragmentOutput { float4 color : SV_TARGET0; };
FragmentOutput main(float2 uv : COLOR0)
{
    float4 color = image_0.Sample(imageSampler_0, uv) * globals_0.tint_0;
    FragmentOutput output = { color };
    return output;
}"#;
        let lowered = lower_slang_bindings_to_descriptor_heap(source, "main").unwrap();
        assert_eq!(lowered.push_constant_bytes, 12);
        assert_eq!(
            lowered
                .bindings
                .iter()
                .map(|binding| (binding.kind, binding.register, binding.push_offset))
                .collect::<Vec<_>>(),
            vec![
                (DescriptorHeapBindingKind::SampledImage, 0, 0),
                (DescriptorHeapBindingKind::Sampler, 0, 4),
                (DescriptorHeapBindingKind::UniformBuffer, 0, 8),
            ]
        );
        assert!(
            lowered
                .source
                .contains("DescriptorHandle<Texture2D<float4 >>")
        );
        assert!(lowered.source.contains(
            "DescriptorHandle<ConstantBuffer<GlobalParams_0>>(gilderHeapPush.binding2Index)"
        ));
        assert!(
            lowered
                .source
                .contains("gilderHeap_image_0().Sample(gilderHeap_imageSampler_0(), uv)")
        );
        assert!(!lowered.source.contains("register(t0)"));
        assert!(!lowered.source.contains("register(s0)"));
        assert!(!lowered.source.contains("register(b0)"));
    }

    #[test]
    fn rejects_duplicate_registers_instead_of_aliasing_them() {
        let source = "Texture2D<float4> a : register(t0);\nTexture2D<float4> b : register(t0);\nfloat4 main() { return a.Load(0) + b.Load(0); }";
        assert!(lower_slang_bindings_to_descriptor_heap(source, "main").is_err());
    }

    #[test]
    fn pipeline_global_base_pads_source_and_rebases_binding_offsets() {
        let source = r#"Texture2D<float4> image : register(t0);
SamplerState samplerState : register(s0);
float4 main(float2 uv : COLOR0) : SV_TARGET0
{
    return image.Sample(samplerState, uv);
}"#;

        let lowered = lower_slang_bindings_to_descriptor_heap_at_offset(source, "main", 4).unwrap();

        assert_eq!(lowered.push_constant_bytes, 12);
        assert_eq!(
            lowered
                .bindings
                .iter()
                .map(|binding| binding.push_offset)
                .collect::<Vec<_>>(),
            vec![4, 8]
        );
        assert!(lowered.source.contains("uint reservedPipelineWords[1];"));
    }

    #[test]
    fn pipeline_global_base_must_be_word_aligned() {
        let source = "Texture2D<float4> image : register(t0);\nfloat4 main() : SV_TARGET0 { return image.Load(0); }";
        assert!(lower_slang_bindings_to_descriptor_heap_at_offset(source, "main", 2).is_err());
    }

    #[test]
    fn compute_entry_attributes_remain_attached_after_heap_lowering() {
        let source = r#"StructuredBuffer<float> values : register(t0);
[[shader("compute")]]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID)
{
    float value = values[id.x];
}"#;

        let lowered = lower_slang_bindings_to_descriptor_heap(source, "main").unwrap();

        let prelude = lowered
            .source
            .find("struct GilderDescriptorHeapPush")
            .unwrap();
        let shader_attribute = lowered.source.find("[[shader(\"compute\")]]").unwrap();
        let entry = lowered.source.find("void main(").unwrap();
        assert!(prelude < shader_attribute);
        assert!(shader_attribute < entry);
        assert!(
            lowered
                .source
                .contains("[[shader(\"compute\")]]\n[numthreads(64, 1, 1)]\nvoid main(")
        );
    }
}
