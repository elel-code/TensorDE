//! Built-in fragment programs for foundational scene material families.

pub(crate) fn color_fragment_source() -> String {
    r#"#version 450
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 3) uniform ColorMaterial {
    vec4 g_Color4;
    vec4 g_Unused0;
    vec4 g_Unused1;
} g_Material;
void main() {
    vec4 color = g_Material.g_Color4;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

pub(crate) fn text_fragment_source() -> String {
    r#"#version 450
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 3) uniform TextMaterial {
    vec4 g_Color4;
    vec4 g_Unused0;
    vec4 g_Unused1;
} g_Material;
void main() {
    vec4 color = g_Material.g_Color4;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

pub(crate) fn generic_particle_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 2) in vec3 v_ParticleColor;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ParticleMaterial {
    vec4 g_Color4;
    vec4 g_Unused0;
    vec4 g_Unused1;
} g_Material;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord) * g_Material.g_Color4;
    color.rgb *= v_ParticleColor;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

pub(crate) fn minimal_alpha_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out float o_Alpha;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
void main() {
    o_Alpha = texture(g_Texture0, v_TexCoord).a;
}
"#
    .to_owned()
}

pub(crate) fn passthrough_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
void main() {
    o_Color = texture(g_Texture0, v_TexCoord);
}
"#
    .to_owned()
}
