#version 450

// reverse-engineered reference: WallpaperEngine effects/waterwaves keeps
// geometry fixed, computes one or two sine-wave UV offsets in fragment space,
// optionally gates them by mask/time-offset textures, then samples g_Texture0.
// Gilder may enlarge a graph target to retain puppet overhang. v_uv samples
// that enlarged target. v_effect_uv carries WE's logical a_TexCoord basis after
// Gilder has applied the texture-resolution transform for authored masks, so it
// drives phase/mask and must be converted back before offsetting the target UV.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec2 v_effect_uv;
layout(location = 2) in float v_opacity;
layout(location = 3) in vec4 v_tint;
layout(location = 4) flat in float v_time_seconds;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;

layout(push_constant) uniform ScenePush {
    layout(offset = 0) vec2 extent;
    layout(offset = 8) uint alpha_texture_slot;
    layout(offset = 12) uint alpha_texture_mode;
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
    layout(offset = 228) uint output_flags;
} pc;

const uint WATERWAVES_FLAG_MASK = 1u;
const uint WATERWAVES_FLAG_DUAL = 2u;
const uint WATERWAVES_FLAG_TIMEOFFSET = 4u;
const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;
const float M_PI_2 = 6.28318530718;

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

vec4 finalize_output(vec4 color) {
    if ((pc.output_flags & OUTPUT_FLAG_PREMULTIPLY_RGB) != 0u) {
        color.rgb *= color.a;
    }
    return color;
}

bool effect_uv_inside(vec2 uv) {
    return all(greaterThanEqual(uv, vec2(0.0))) && all(lessThanEqual(uv, vec2(1.0)));
}

float source_alpha_at(vec2 source_coord, inout float cached_alpha) {
    if (cached_alpha < 0.0) {
        cached_alpha = texture(g_Texture0, source_coord).a;
    }
    return cached_alpha;
}

float waterwaves_mask_sample(vec2 uv, vec2 source_coord, inout float cached_source_alpha) {
    if (effect_uv_inside(uv)) {
        return texture(g_Texture1, uv).r;
    }
    if (source_alpha_at(source_coord, cached_source_alpha) <= 0.001) {
        return 0.0;
    }
    return texture(g_Texture1, clamp(uv, vec2(0.0), vec2(1.0))).r;
}

float waterwaves_timeoffset_sample(vec2 uv, vec2 source_coord, inout float cached_source_alpha) {
    if (effect_uv_inside(uv)) {
        return texture(g_Texture2, uv).r;
    }
    if (source_alpha_at(source_coord, cached_source_alpha) <= 0.001) {
        return 0.0;
    }
    return texture(g_Texture2, clamp(uv, vec2(0.0), vec2(1.0))).r;
}

float uv_axis_scale(float target_dx, float target_dy, float effect_dx, float effect_dy) {
    float effect_len = length(vec2(effect_dx, effect_dy));
    if (effect_len <= 0.000001) {
        return 1.0;
    }
    return length(vec2(target_dx, target_dy)) / effect_len;
}

vec2 target_uv_per_layer_uv() {
    return vec2(
        uv_axis_scale(dFdx(v_uv.x), dFdy(v_uv.x), dFdx(v_effect_uv.x), dFdy(v_effect_uv.x)),
        uv_axis_scale(dFdx(v_uv.y), dFdy(v_uv.y), dFdx(v_effect_uv.y), dFdy(v_effect_uv.y))
    );
}

void main() {
    vec2 source_coord = v_uv;
    vec2 tex_coord_motion = v_effect_uv;
    vec2 mask_uv = v_effect_uv;
    float cached_source_alpha = -1.0;

    float mask = 1.0;
    if ((pc.waterwaves_flags & WATERWAVES_FLAG_MASK) != 0u
        && (pc.texture_resolution_mask & (1u << 1)) != 0u) {
        // Gilder can enlarge image-local EffectTargets to hold retained puppet
        // overhang. Empty synthetic padding must not smear a nonzero border
        // mask, but real overhanging mesh pixels still need WE's clamp-to-edge
        // sampler behavior; source alpha separates those two cases.
        mask = waterwaves_mask_sample(mask_uv, source_coord, cached_source_alpha);
    }

    float time_offset = 0.0;
    if ((pc.waterwaves_flags & WATERWAVES_FLAG_TIMEOFFSET) != 0u
        && (pc.texture_resolution_mask & (1u << 2)) != 0u) {
        time_offset =
            waterwaves_timeoffset_sample(mask_uv, source_coord, cached_source_alpha) * M_PI_2;
    }

    vec2 direction = rotate_up(pc.waterwaves_direction);
    float distance = v_time_seconds * pc.waterwaves_speed
        + dot(tex_coord_motion, direction) * pc.waterwaves_scale
        + time_offset;
    float strength = pc.waterwaves_strength * pc.waterwaves_strength;
    vec2 offset = vec2(direction.y, -direction.x);
    float val = signed_pow_sin(distance, pc.waterwaves_exponent);

    if ((pc.waterwaves_flags & WATERWAVES_FLAG_DUAL) != 0u) {
        vec2 direction2 = rotate_up(pc.waterwaves_direction2);
        float distance2 = (v_time_seconds + pc.waterwaves_offset2)
            * pc.waterwaves_speed2
            + dot(tex_coord_motion, direction2) * pc.waterwaves_scale2
            + time_offset;
        val *= signed_pow_sin(distance2, pc.waterwaves_exponent2);
    }

    vec2 layer_uv_offset = val * offset * strength * mask;
    source_coord += layer_uv_offset * target_uv_per_layer_uv();
    out_color = finalize_output(apply_vertex_color(texture(g_Texture0, source_coord)));
}
