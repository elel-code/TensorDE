//! Typed descriptor-heap lowering for Slang-compatible source.

use std::collections::BTreeSet;

use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DescriptorHeapBindingKind {
    SampledImage,
    StorageImage,
    Sampler,
    UniformBuffer,
    StorageBuffer,
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
    let bindings = bindings
        .into_iter()
        .enumerate()
        .map(|(index, binding)| {
            let push_offset = u32::try_from(index)
                .ok()
                .and_then(|index| index.checked_mul(4))
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
    let push_constant_bytes = u32::try_from(bindings.len())
        .ok()
        .and_then(|count| count.checked_mul(4))
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
    Ok(source[..found].rfind('\n').map_or(0, |offset| offset + 1))
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
}
