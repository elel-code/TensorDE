#version 450

// CWE reference: WallpaperEngine effects/waterwaves keeps geometry fixed,
// computes one or two sine-wave UV offsets in fragment space, optionally
// gates them by mask/time-offset textures, then samples g_Texture0.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec2 v_effect_uv;
layout(location = 2) in float v_opacity;
layout(location = 3) in vec4 v_tint;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;

layout(push_constant) uniform ScenePush {
    layout(offset = 0) vec2 extent;
    layout(offset = 8) uint alpha_texture_slot;
    layout(offset = 12) uint alpha_texture_mode;
    layout(offset = 16) float time_seconds;
    layout(offset = 20) uint texture_resolution_mask;
    layout(offset = 24) uint system_uniform_count;
    layout(offset = 28) uint constant_uniform_count;
    layout(offset = 32) vec2 texture_resolution[8];
    layout(offset = 96) float waterwaves_strength;
    layout(offset = 100) float waterwaves_speed;
    layout(offset = 104) float waterwaves_scale;
    layout(offset = 108) float waterwaves_exponent;
    layout(offset = 112) float waterwaves_direction;
    layout(offset = 116) float waterwaves_speed2;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 124) float waterwaves_scale2;
    layout(offset = 128) float waterwaves_offset2;
    layout(offset = 132) float waterwaves_exponent2;
    layout(offset = 136) float waterwaves_direction2;
    layout(offset = 140) uint waterwaves_flags;
} pc;

const uint WATERWAVES_FLAG_MASK = 1u;
const uint WATERWAVES_FLAG_DUAL = 2u;
const uint WATERWAVES_FLAG_TIMEOFFSET = 4u;
const float M_PI_2 = 1.57079632679;

vec2 rotate_up(float radians) {
    return vec2(-sin(radians), cos(radians));
}

float signed_pow_sin(float value, float exponent) {
    float wave = sin(value);
    return sign(wave) * pow(abs(wave), max(exponent, 0.001));
}

vec4 apply_vertex_color(vec4 color) {
    color *= v_tint;
    color.a *= v_opacity;
    return color;
}

void main() {
    vec2 tex_coord = v_uv;
    vec2 tex_coord_motion = tex_coord;
    vec2 mask_uv = v_effect_uv;

    float mask = 1.0;
    if ((pc.waterwaves_flags & WATERWAVES_FLAG_MASK) != 0u
        && (pc.texture_resolution_mask & (1u << 1)) != 0u) {
        mask = texture(g_Texture1, mask_uv).r;
    }

    float time_offset = 0.0;
    if ((pc.waterwaves_flags & WATERWAVES_FLAG_TIMEOFFSET) != 0u
        && (pc.texture_resolution_mask & (1u << 2)) != 0u) {
        time_offset = texture(g_Texture2, mask_uv).r * M_PI_2;
    }

    vec2 direction = rotate_up(pc.waterwaves_direction);
    float distance = pc.time_seconds * pc.waterwaves_speed
        + dot(tex_coord_motion, direction) * pc.waterwaves_scale
        + time_offset;
    float strength = pc.waterwaves_strength * pc.waterwaves_strength;
    vec2 offset = vec2(direction.y, -direction.x);
    float val = signed_pow_sin(distance, pc.waterwaves_exponent);

    if ((pc.waterwaves_flags & WATERWAVES_FLAG_DUAL) != 0u) {
        vec2 direction2 = rotate_up(pc.waterwaves_direction2);
        float distance2 = (pc.time_seconds + pc.waterwaves_offset2)
            * pc.waterwaves_speed2
            + dot(tex_coord_motion, direction2) * pc.waterwaves_scale2
            + time_offset;
        val *= signed_pow_sin(distance2, pc.waterwaves_exponent2);
    }

    tex_coord += val * offset * strength * mask;
    out_color = apply_vertex_color(texture(g_Texture0, tex_coord));
}
