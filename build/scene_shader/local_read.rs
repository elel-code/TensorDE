//! Build-time shader sources for the explicitly typed local-read lane.
//!
//! This module only emits variants whose access semantics are intrinsically
//! exact-pixel.  A `subpassInput` variant is never inferred from a texture
//! slot; the catalog must opt into the family and the runtime contract must
//! still request the matching typed input-attachment access.

use std::path::Path;

const FLAT_PASSTHROUGH_INPUTS: &[(u32, u32, u32)] = &[(0, 0, 64)];
const FLAT_PASSTHROUGH_COLOR_OUTPUTS: &[u32] = &[0];

pub(crate) struct InputAttachmentFragmentSource {
    source: String,
    inputs: &'static [(u32, u32, u32)],
    color_output_locations: &'static [u32],
}

impl InputAttachmentFragmentSource {
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn catalog_expression(&self, path: &Path) -> String {
        let inputs = self
            .inputs
            .iter()
            .map(|(slot, index, binding)| {
                format!(
                    "BuiltinSceneInputAttachment {{ slot: {slot}, input_attachment_index: {index}, binding: {binding} }}"
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let outputs = self
            .color_output_locations
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Some(BuiltinSceneLocalReadShader {{ fragment_spirv: vulkanalia::include_shader_code!({:?}), input_attachments: &[{inputs}], color_output_locations: &[{outputs}] }})",
            path.to_str()
                .expect("built-in input-attachment fragment shader path must be UTF-8")
        )
    }
}

pub(crate) const fn input_attachment_catalog_type_source() -> &'static str {
    r#"#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSceneInputAttachment {
    pub slot: u32,
    pub input_attachment_index: u32,
    pub binding: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct BuiltinSceneLocalReadShader {
    pub fragment_spirv: &'static [u32],
    pub input_attachments: &'static [BuiltinSceneInputAttachment],
    pub color_output_locations: &'static [u32],
}

"#
}

pub(crate) fn input_attachment_fragment_source(
    flat_passthrough_family: bool,
) -> Option<InputAttachmentFragmentSource> {
    flat_passthrough_family.then(|| {
        let source = r#"#version 450
layout(input_attachment_index = 0, set = 0, binding = 64) uniform subpassInput g_Input0;
layout(location = 0) out vec4 o_Color;
void main() {
    o_Color = subpassLoad(g_Input0);
}
"#
        .to_owned();
        InputAttachmentFragmentSource {
            source,
            inputs: FLAT_PASSTHROUGH_INPUTS,
            color_output_locations: FLAT_PASSTHROUGH_COLOR_OUTPUTS,
        }
    })
}
