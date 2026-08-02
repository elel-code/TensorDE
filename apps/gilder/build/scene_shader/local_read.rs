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
    source: &'static str,
    inputs: &'static [(u32, u32, u32)],
    color_output_locations: &'static [u32],
}

impl InputAttachmentFragmentSource {
    pub(crate) const fn source(&self) -> &'static str {
        self.source
    }

    pub(crate) fn catalog_expression(
        &self,
        path: &Path,
        push_constant_bytes: u32,
        bindings: &str,
    ) -> String {
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
            "Some(BuiltinSceneLocalReadShader {{ fragment_spirv: vulkan_renderer::include_spirv!({:?}), push_constant_bytes: {push_constant_bytes}, bindings: &[{bindings}], input_attachments: &[{inputs}], color_output_locations: &[{outputs}] }})",
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
    pub push_constant_bytes: u32,
    pub bindings: &'static [BuiltinSceneDescriptorBinding],
    pub input_attachments: &'static [BuiltinSceneInputAttachment],
    pub color_output_locations: &'static [u32],
}

"#
}

pub(crate) const fn flat_passthrough_input_attachment_source() -> InputAttachmentFragmentSource {
    InputAttachmentFragmentSource {
        source: include_str!("../../shaders/scene/passthrough_local_read.frag.slang"),
        inputs: FLAT_PASSTHROUGH_INPUTS,
        color_output_locations: FLAT_PASSTHROUGH_COLOR_OUTPUTS,
    }
}
