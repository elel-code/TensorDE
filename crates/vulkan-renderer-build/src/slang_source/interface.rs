use std::collections::BTreeMap;

use crate::ShaderStage;

use super::{Declarations, LoweredEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Declarator {
    name: String,
    array_count: Option<u32>,
}

impl Declarator {
    pub(super) fn parse(source: &str) -> Result<Self, String> {
        let (name, array_count) = if let Some((name, suffix)) = source.split_once('[') {
            let count = suffix
                .strip_suffix(']')
                .filter(|count| {
                    !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit())
                })
                .and_then(|count| count.parse::<u32>().ok())
                .filter(|count| *count != 0)
                .ok_or_else(|| format!("invalid generated interface array declarator {source}"))?;
            (name, Some(count))
        } else {
            (source, None)
        };
        if !is_identifier(name) {
            return Err(format!("invalid generated interface declarator {source}"));
        }
        Ok(Self {
            name: name.to_owned(),
            array_count,
        })
    }

    pub(super) fn name(&self) -> &str {
        &self.name
    }

    pub(super) fn is_array(&self) -> bool {
        self.array_count.is_some()
    }

    fn declaration(&self) -> String {
        self.array_count.map_or_else(
            || self.name.clone(),
            |count| format!("{}[{count}]", self.name),
        )
    }

    fn element_count(&self) -> u32 {
        self.array_count.unwrap_or(1)
    }

    fn emit_copy(&self, output: &mut String, destination: &str, source: &str) {
        if let Some(count) = self.array_count {
            for index in 0..count {
                output.push_str(&format!(
                    "    {destination}{}[{index}] = {source}{}[{index}];\n",
                    self.name, self.name
                ));
            }
        } else {
            output.push_str(&format!(
                "    {destination}{} = {source}{};\n",
                self.name, self.name
            ));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Item {
    pub(super) location: u32,
    pub(super) ty: String,
    pub(super) declarator: Declarator,
    pub(super) flat: bool,
}

impl Item {
    fn location_count(&self) -> Result<u32, String> {
        let columns: u32 = match self.ty.as_str() {
            "mat2" | "float2x2" => 2,
            "mat3" | "float3x3" => 3,
            "mat4" | "float4x4" => 4,
            _ => 1,
        };
        columns
            .checked_mul(self.declarator.element_count())
            .ok_or_else(|| {
                format!(
                    "generated interface location span overflows: {} {}",
                    self.ty,
                    self.declarator.declaration()
                )
            })
    }
}

pub(super) fn validate_locations(items: &[Item], direction: &str) -> Result<(), String> {
    let mut locations = BTreeMap::new();
    for item in items {
        let count = item.location_count()?;
        let end = item
            .location
            .checked_add(count)
            .ok_or_else(|| format!("generated {direction} location range overflows"))?;
        for location in item.location..end {
            if locations
                .insert(location, item.declarator.declaration())
                .is_some()
            {
                return Err(format!(
                    "duplicate generated {direction} location {location}"
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn emit_statics(
    declarations: &Declarations,
    stage: ShaderStage,
    entry: &LoweredEntry,
) -> String {
    let input_name = if stage == ShaderStage::Vertex {
        "VulkanRendererVertexInput"
    } else {
        "VulkanRendererFragmentInput"
    };
    let output_name = if stage == ShaderStage::Vertex {
        "VulkanRendererVertexOutput"
    } else {
        "VulkanRendererFragmentOutput"
    };
    let mut output = String::new();
    for input in &declarations.inputs {
        output.push_str(&format!(
            "static {} {};\n",
            input.ty,
            input.declarator.declaration()
        ));
    }
    if entry.uses_vertex_index {
        output.push_str("static uint gl_VertexIndex;\n");
    }
    if entry.uses_instance_index {
        output.push_str("static uint gl_InstanceIndex;\n");
    }
    if entry.uses_frag_coord {
        output.push_str("static float4 gl_FragCoord;\n");
    }
    if entry.uses_front_facing {
        output.push_str("static bool gl_FrontFacing;\n");
    }
    for item in &declarations.outputs {
        output.push_str(&format!(
            "static {} {};\n",
            item.ty,
            item.declarator.declaration()
        ));
    }
    if stage == ShaderStage::Vertex {
        output.push_str("static float4 vulkanRendererPosition;\n");
    }
    output.push_str(&format!("struct {input_name}\n{{\n"));
    for item in &declarations.inputs {
        output.push_str(&field(item, false, stage));
    }
    if entry.uses_vertex_index {
        output.push_str("    uint vulkanRendererVertexIndex : SV_VertexID;\n");
    }
    if entry.uses_instance_index {
        output.push_str("    uint vulkanRendererInstanceIndex : SV_InstanceID;\n");
    }
    if entry.uses_frag_coord {
        output.push_str("    float4 vulkanRendererFragCoord : SV_Position;\n");
    }
    if entry.uses_front_facing {
        output.push_str("    bool vulkanRendererFrontFacing : SV_IsFrontFace;\n");
    }
    output.push_str("};\n");
    output.push_str(&format!("struct {output_name}\n{{\n"));
    if stage == ShaderStage::Vertex {
        output.push_str("    float4 position : SV_Position;\n");
    }
    for item in &declarations.outputs {
        output.push_str(&field(item, true, stage));
    }
    output.push_str("};\n");
    output
}

pub(super) fn emit_input_copy(item: &Item, output: &mut String) {
    item.declarator.emit_copy(output, "", "input.");
}

pub(super) fn emit_output_copy(item: &Item, output: &mut String) {
    item.declarator.emit_copy(output, "output.", "");
}

fn field(item: &Item, output: bool, stage: ShaderStage) -> String {
    let interpolation = if item.flat { "nointerpolation " } else { "" };
    let semantic = if output && stage == ShaderStage::Fragment {
        format!("SV_Target{}", item.location)
    } else {
        format!("TEXCOORD{}", item.location)
    };
    format!(
        "    {interpolation}[[vk::location({})]] {} {} : {semantic};\n",
        item.location,
        item.ty,
        item.declarator.declaration()
    )
}

fn is_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}
