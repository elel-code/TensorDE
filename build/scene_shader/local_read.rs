//! Build-time shader sources for the explicitly typed local-read lane.
//!
//! This module only emits variants whose access semantics are intrinsically
//! exact-pixel.  A `subpassInput` variant is never inferred from a texture
//! slot; the catalog must opt into the family and the runtime contract must
//! still request the matching typed input-attachment access.

pub(crate) fn input_attachment_fragment_source(flat_passthrough_family: bool) -> Option<String> {
    flat_passthrough_family.then(|| {
        r#"#version 450
layout(input_attachment_index = 0, set = 0, binding = 64) uniform subpassInput g_Input0;
layout(location = 0) out vec4 o_Color;
void main() {
    o_Color = subpassLoad(g_Input0);
}
"#
        .to_owned()
    })
}
