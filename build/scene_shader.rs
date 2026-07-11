pub(super) fn object_composite_sources() -> (String, String) {
    let vertex = r#"#version 450
layout(location = 0) out vec2 v_TexCoord;
void main() {
    vec2 positions[3] = vec2[](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    vec2 position = positions[gl_VertexIndex];
    v_TexCoord = position * 0.5 + 0.5;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#;
    let fragment = r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ObjectCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
} g_Material;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord);
    color.rgb *= g_Material.g_Color4.rgb;
    color.a *= g_Material.g_Color4.a;
    o_Color = color;
}
"#;
    (vertex.to_owned(), fragment.to_owned())
}

pub(super) fn puppet_effect_source_sources() -> (String, String) {
    image_effect_source_sources()
}

pub(super) fn image_effect_source_sources() -> (String, String) {
    let vertex = r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
void main() {
    v_TexCoord = a_TexCoord;
    v_VertexAlpha = a_Opacity;
    gl_Position = vec4(a_TexCoord * 2.0 - 1.0, 0.0, 1.0);
}
"#;
    let fragment = r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord);
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#;
    (vertex.to_owned(), fragment.to_owned())
}

pub(super) fn image_effect_composite_sources() -> (String, String) {
    let vertex = super::scene_mesh_vertex_source();
    let fragment = r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ImageEffectCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
} g_Material;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord) * g_Material.g_Color4;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#;
    (vertex, fragment.to_owned())
}

pub(super) fn puppet_effect_composite_sources() -> (String, String) {
    (
        puppet_effect_composite_vertex(),
        puppet_effect_composite_fragment(),
    )
}

fn puppet_effect_composite_vertex() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 3) in uvec4 a_BlendIndices;
layout(location = 4) in vec4 a_BlendWeights;
layout(location = 0) out vec2 v_EffectTexCoord;
layout(location = 1) out float v_BoneAlpha;
layout(set = 0, binding = 2) uniform SceneDrawTransform {
    vec4 g_ModelViewProjectionMatrix[4];
} g_Draw;
struct GilderPuppetBonePalette {
    vec4 row0;
    vec4 row1;
    vec4 row2;
    vec4 row3;
    vec4 alpha;
};
layout(std430, set = 0, binding = 4) readonly buffer ScenePuppetBones {
    GilderPuppetBonePalette g_Bones[];
} g_Puppet;
vec4 projectPosition(vec4 position) {
    return vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], position));
}
void main() {
    vec4 raw_position = vec4(a_Position.xy, 0.0, 1.0);
    v_EffectTexCoord = a_TexCoord;
    vec4 skinned_position = vec4(0.0);
    float skinned_alpha = 0.0;
    float total_weight = 0.0;
    for (uint slot = 0u; slot < 4u; slot++) {
        float weight = a_BlendWeights[slot];
        if (weight <= 0.0000001) {
            continue;
        }
        GilderPuppetBonePalette bone = g_Puppet.g_Bones[a_BlendIndices[slot]];
        skinned_position += vec4(
            dot(bone.row0, raw_position),
            dot(bone.row1, raw_position),
            dot(bone.row2, raw_position),
            dot(bone.row3, raw_position)) * weight;
        skinned_alpha += bone.alpha.x * weight;
        total_weight += weight;
    }
    vec4 local_position = raw_position;
    v_BoneAlpha = 1.0;
    if (total_weight > 0.0000001) {
        local_position = skinned_position / total_weight;
        v_BoneAlpha = skinned_alpha / total_weight;
    }
    gl_Position = projectPosition(local_position);
}
"#
    .to_owned()
}

fn puppet_effect_composite_fragment() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_EffectTexCoord;
layout(location = 1) in float v_BoneAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform PuppetEffectCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
} g_Material;
void main() {
    vec4 color = texture(g_Texture0, v_EffectTexCoord) * g_Material.g_Color4;
    color.a *= v_BoneAlpha;
    o_Color = color;
}
"#
    .to_owned()
}
