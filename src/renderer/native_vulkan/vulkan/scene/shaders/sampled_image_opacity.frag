#version 450

// reverse-engineered reference: WallpaperEngine effects/opacity keeps the
// source color and only gates alpha by the opacity mask and g_UserAlpha.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec2 v_effect_uv;
layout(location = 2) in float v_opacity;
layout(location = 3) in vec4 v_tint;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;

layout(push_constant) uniform ScenePush {
    layout(offset = 0) vec2 extent;
    layout(offset = 8) uint alpha_texture_slot;
    layout(offset = 12) uint alpha_texture_mode;
    layout(offset = 16) float time_seconds;
    layout(offset = 20) uint texture_resolution_mask;
    layout(offset = 24) uint system_uniform_count;
    layout(offset = 28) uint constant_uniform_count;
    layout(offset = 32) vec2 texture_resolution[8];
    layout(offset = 96) float user_alpha;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 228) uint output_flags;
} pc;

const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;
const uint ALPHA_TEXTURE_MODE_COVERAGE = 3u;

vec4 apply_vertex_color(vec4 color) {
    color *= v_tint;
    color.a *= v_opacity;
    return color;
}

vec4 finalize_output(vec4 color) {
    if (pc.alpha_texture_mode == ALPHA_TEXTURE_MODE_COVERAGE) {
        color.a = clamp((color.a - 0.5) / max(fwidth(color.a), 0.0001) + 0.5, 0.0, 1.0);
    }
    if ((pc.output_flags & OUTPUT_FLAG_PREMULTIPLY_RGB) != 0u) {
        color.rgb *= color.a;
    }
    return color;
}

void main() {
    vec4 albedo = texture(g_Texture0, v_uv);
    float mask = 1.0;
    if ((pc.texture_resolution_mask & (1u << 1)) != 0u) {
        mask = texture(g_Texture1, v_effect_uv).r;
    }
    albedo.a *= mask * pc.user_alpha;
    out_color = finalize_output(apply_vertex_color(albedo));
}
