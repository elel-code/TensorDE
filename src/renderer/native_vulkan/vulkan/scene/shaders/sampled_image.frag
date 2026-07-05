#version 450

// Generic WE image shader path.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec2 v_effect_uv;
layout(location = 2) in float v_opacity;
layout(location = 3) in vec4 v_tint;
layout(location = 4) flat in float v_time_seconds;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;
layout(set = 0, binding = 3) uniform sampler2D g_Texture3;
layout(set = 0, binding = 4) uniform sampler2D g_Texture4;
layout(set = 0, binding = 5) uniform sampler2D g_Texture5;
layout(set = 0, binding = 6) uniform sampler2D g_Texture6;
layout(set = 0, binding = 7) uniform sampler2D g_Texture7;

layout(push_constant) uniform ScenePush {
    layout(offset = 0) vec2 extent;
    layout(offset = 8) uint alpha_texture_slot;
    layout(offset = 12) uint alpha_texture_mode;
    layout(offset = 20) uint texture_resolution_mask;
    layout(offset = 24) uint system_uniform_count;
    layout(offset = 28) uint constant_uniform_count;
    layout(offset = 32) vec2 texture_resolution[8];
    layout(offset = 96) float user_alpha;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 228) uint output_flags;
} pc;

const uint ALPHA_TEXTURE_SLOT_DISABLED = 0xffffffffu;
const uint ALPHA_TEXTURE_MODE_MULTIPLY = 0u;
const uint ALPHA_TEXTURE_MODE_INVERSE = 1u;
const uint ALPHA_TEXTURE_MODE_COVERAGE = 3u;
const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;

vec4 apply_vertex_color(vec4 color) {
    color *= v_tint;
    color.a *= v_opacity;
    return color;
}

float alpha_mask(vec2 mask_uv, uint slot) {
    switch (slot) {
        case 1u:
            return texture(g_Texture1, mask_uv).r;
        case 2u:
            return texture(g_Texture2, mask_uv).r;
        case 3u:
            return texture(g_Texture3, mask_uv).r;
        case 4u:
            return texture(g_Texture4, mask_uv).r;
        case 5u:
            return texture(g_Texture5, mask_uv).r;
        case 6u:
            return texture(g_Texture6, mask_uv).r;
        case 7u:
            return texture(g_Texture7, mask_uv).r;
        default:
            return 1.0;
    }
}

vec4 apply_alpha_texture(vec4 color) {
    if (pc.alpha_texture_slot == ALPHA_TEXTURE_SLOT_DISABLED) {
        return color;
    }
    float mask = alpha_mask(v_effect_uv, pc.alpha_texture_slot);
    if (pc.alpha_texture_mode == ALPHA_TEXTURE_MODE_INVERSE) {
        color.a *= 1.0 - mask;
    } else {
        color.a *= mask;
    }
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
    vec4 color = texture(g_Texture0, v_uv);
    color = apply_alpha_texture(color);
    out_color = finalize_output(apply_vertex_color(color));
}
