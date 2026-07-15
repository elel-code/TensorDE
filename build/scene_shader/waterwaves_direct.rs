//! Direct typed evaluation of a complete authored WaterWaves chain.

pub(crate) fn waterwaves_direct_sources(
    puppet_skinning: bool,
    premultiply_output: bool,
) -> (String, String) {
    let vertex = if puppet_skinning {
        super::puppet_effect_composite_vertex()
    } else {
        super::super::scene_mesh_vertex_source()
    };
    (vertex, waterwaves_direct_fragment(premultiply_output))
}

fn waterwaves_direct_fragment(premultiply_output: bool) -> String {
    let premultiply = premultiply_output
        .then_some("    color.rgb *= color.a;\n")
        .unwrap_or_default();
    [
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;
layout(set = 0, binding = 4) uniform sampler2D g_Texture4;
layout(set = 0, binding = 5) uniform sampler2D g_Texture5;
layout(set = 0, binding = 6) uniform sampler2D g_Texture6;
layout(set = 0, binding = 7) uniform sampler2D g_Texture7;
layout(set = 0, binding = 35) uniform sampler2D g_Texture3;
layout(set = 0, binding = 3) uniform WaterWavesDirectUniform {
    vec4 g_ResolvedColorAlpha;
    vec4 g_Chain;
    vec4 g_Stage[28];
} u_Effect;
vec2 rotateVec2(vec2 value, float angle) {
    vec2 cs = vec2(cos(angle), sin(angle));
    return vec2(value.x * cs.x - value.y * cs.y,
        value.x * cs.y + value.y * cs.x);
}
float shapedSine(float phase, float exponent) {
    float wave = sin(phase);
    return pow(abs(wave), max(exponent, 0.0001)) * sign(wave);
}
float stageMask(int stage, vec2 uv) {
    if (stage == 0) return texture(g_Texture1, uv).r;
    if (stage == 1) return texture(g_Texture2, uv).r;
    if (stage == 2) return texture(g_Texture3, uv).r;
    if (stage == 3) return texture(g_Texture4, uv).r;
    if (stage == 4) return texture(g_Texture5, uv).r;
    if (stage == 5) return texture(g_Texture6, uv).r;
    return texture(g_Texture7, uv).r;
}
vec2 stageOffset(int stage, vec2 uv) {
    int base = stage * 4;
    vec4 speed_scale_strength_mask = u_Effect.g_Stage[base];
    vec4 direction_speed2_scale2_direction2 = u_Effect.g_Stage[base + 1];
    vec4 offset2_dual_exponents = u_Effect.g_Stage[base + 2];
    vec4 mask_resolution = u_Effect.g_Stage[base + 3];
    float mask = 1.0;
    if (speed_scale_strength_mask.w > 0.5) {
        vec2 mask_uv = uv * mask_resolution.zw
            / max(mask_resolution.xy, vec2(1.0));
        mask = stageMask(stage, mask_uv);
    }
    vec2 direction = rotateVec2(
        vec2(0.0, 1.0), direction_speed2_scale2_direction2.x);
    float phase = u_Effect.g_Chain.y * speed_scale_strength_mask.x
        + dot(uv, direction) * speed_scale_strength_mask.y;
    float displacement = shapedSine(phase, offset2_dual_exponents.z);
    if (offset2_dual_exponents.y > 0.5) {
        vec2 direction2 = rotateVec2(
            vec2(0.0, 1.0), direction_speed2_scale2_direction2.w);
        float phase2 = (u_Effect.g_Chain.y + offset2_dual_exponents.x)
            * direction_speed2_scale2_direction2.y
            + dot(uv, direction2) * direction_speed2_scale2_direction2.z;
        displacement *= shapedSine(phase2, offset2_dual_exponents.w);
    }
    float strength = speed_scale_strength_mask.z;
    return vec2(direction.y, -direction.x)
        * displacement * strength * strength * mask;
}
void main() {
    int stage_count = clamp(int(u_Effect.g_Chain.x + 0.5), 0, 7);
    vec2 source_uv = v_TexCoord;
    for (int stage = 6; stage >= 0; --stage) {
        if (stage < stage_count) {
            source_uv += stageOffset(stage, source_uv);
        }
    }
    vec2 source_texel = 1.0 / vec2(textureSize(g_Texture0, 0));
    float authored_filter_radius = 0.17 * sqrt(max(float(stage_count - 1), 0.0));
    vec2 filter_offset = source_texel * authored_filter_radius;
    vec4 source_color = (
        texture(g_Texture0, source_uv + vec2(-filter_offset.x, -filter_offset.y))
        + texture(g_Texture0, source_uv + vec2(filter_offset.x, -filter_offset.y))
        + texture(g_Texture0, source_uv + vec2(-filter_offset.x, filter_offset.y))
        + texture(g_Texture0, source_uv + vec2(filter_offset.x, filter_offset.y))) * 0.25;
    vec4 color = source_color * u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
"#,
        premultiply,
        r#"    o_Color = color;
}
"#,
    ]
    .concat()
}
