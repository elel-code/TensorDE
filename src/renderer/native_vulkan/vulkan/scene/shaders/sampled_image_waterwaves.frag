#version 450

// WaterWaves effect-family shader. Normal WaterWaves runs one WE pass; graph
// lowering may mark adjacent WaterWaves passes as WaterWaves2, which executes
// the previous-texture UV dependency in one fragment invocation.

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
    layout(offset = 144) float waterwaves2_strength;
    layout(offset = 148) float waterwaves2_speed;
    layout(offset = 152) float waterwaves2_scale;
    layout(offset = 156) float waterwaves2_exponent;
    layout(offset = 160) float waterwaves2_direction;
    layout(offset = 164) float waterwaves2_speed2;
    layout(offset = 168) float waterwaves2_scale2;
    layout(offset = 172) float waterwaves2_offset2;
    layout(offset = 176) float waterwaves2_exponent2;
    layout(offset = 180) float waterwaves2_direction2;
    layout(offset = 184) uint waterwaves2_flags;
    layout(offset = 228) uint output_flags;
} pc;

const uint WATERWAVES_FLAG_MASK = 1u;
const uint WATERWAVES_FLAG_DUAL = 2u;
const uint WATERWAVES_FLAG_TIMEOFFSET = 4u;
const uint EFFECT_SHADER_CODE_WATERWAVES2 = 14u;
const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;
const float M_PI_2 = 6.28318530718;

struct WaterWavesParams {
    float strength;
    float speed;
    float scale;
    float exponent;
    float direction_angle;
    float speed2;
    float scale2;
    float offset2;
    float exponent2;
    float direction2_angle;
    uint flags;
};

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

float pass1_mask_sample(vec2 uv, vec2 source_coord, inout float cached_source_alpha) {
    if (effect_uv_inside(uv)) {
        return texture(g_Texture1, uv).r;
    }
    if (source_alpha_at(source_coord, cached_source_alpha) <= 0.001) {
        return 0.0;
    }
    return texture(g_Texture1, clamp(uv, vec2(0.0), vec2(1.0))).r;
}

float pass1_timeoffset_sample(vec2 uv, vec2 source_coord, inout float cached_source_alpha) {
    if (effect_uv_inside(uv)) {
        return texture(g_Texture2, uv).r;
    }
    if (source_alpha_at(source_coord, cached_source_alpha) <= 0.001) {
        return 0.0;
    }
    return texture(g_Texture2, clamp(uv, vec2(0.0), vec2(1.0))).r;
}

float pass2_mask_sample(vec2 uv, float cached_temp_alpha) {
    if (effect_uv_inside(uv)) {
        return texture(g_Texture3, uv).r;
    }
    if (cached_temp_alpha <= 0.001) {
        return 0.0;
    }
    return texture(g_Texture3, clamp(uv, vec2(0.0), vec2(1.0))).r;
}

float pass2_timeoffset_sample(vec2 uv, float cached_temp_alpha) {
    if (effect_uv_inside(uv)) {
        return texture(g_Texture4, uv).r;
    }
    if (cached_temp_alpha <= 0.001) {
        return 0.0;
    }
    return texture(g_Texture4, clamp(uv, vec2(0.0), vec2(1.0))).r;
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

WaterWavesParams pass1_params() {
    WaterWavesParams params;
    params.strength = pc.waterwaves_strength;
    params.speed = pc.waterwaves_speed;
    params.scale = pc.waterwaves_scale;
    params.exponent = pc.waterwaves_exponent;
    params.direction_angle = pc.waterwaves_direction;
    params.speed2 = pc.waterwaves_speed2;
    params.scale2 = pc.waterwaves_scale2;
    params.offset2 = pc.waterwaves_offset2;
    params.exponent2 = pc.waterwaves_exponent2;
    params.direction2_angle = pc.waterwaves_direction2;
    params.flags = pc.waterwaves_flags;
    return params;
}

WaterWavesParams pass2_params() {
    WaterWavesParams params;
    params.strength = pc.waterwaves2_strength;
    params.speed = pc.waterwaves2_speed;
    params.scale = pc.waterwaves2_scale;
    params.exponent = pc.waterwaves2_exponent;
    params.direction_angle = pc.waterwaves2_direction;
    params.speed2 = pc.waterwaves2_speed2;
    params.scale2 = pc.waterwaves2_scale2;
    params.offset2 = pc.waterwaves2_offset2;
    params.exponent2 = pc.waterwaves2_exponent2;
    params.direction2_angle = pc.waterwaves2_direction2;
    params.flags = pc.waterwaves2_flags;
    return params;
}

vec2 waterwaves_layer_offset(vec2 tex_coord_motion, WaterWavesParams params, float mask, float time_offset) {
    vec2 direction = rotate_up(params.direction_angle);
    float distance = v_time_seconds * params.speed
        + dot(tex_coord_motion, direction) * params.scale
        + time_offset;
    float strength = params.strength * params.strength;
    vec2 offset = vec2(direction.y, -direction.x);
    float val = signed_pow_sin(distance, params.exponent);

    if ((params.flags & WATERWAVES_FLAG_DUAL) != 0u) {
        vec2 direction2 = rotate_up(params.direction2_angle);
        float distance2 = (v_time_seconds + params.offset2)
            * params.speed2
            + dot(tex_coord_motion, direction2) * params.scale2
            + time_offset;
        val *= signed_pow_sin(distance2, params.exponent2);
    }

    return val * offset * strength * mask;
}

vec2 pass1_layer_offset(vec2 effect_uv, vec2 source_coord, inout float cached_source_alpha) {
    WaterWavesParams params = pass1_params();
    float mask = 1.0;
    if ((params.flags & WATERWAVES_FLAG_MASK) != 0u) {
        mask = pass1_mask_sample(effect_uv, source_coord, cached_source_alpha);
    }
    float time_offset = 0.0;
    if ((params.flags & WATERWAVES_FLAG_TIMEOFFSET) != 0u) {
        time_offset = pass1_timeoffset_sample(effect_uv, source_coord, cached_source_alpha) * M_PI_2;
    }
    return waterwaves_layer_offset(effect_uv, params, mask, time_offset);
}

vec2 pass2_layer_offset(vec2 effect_uv, float cached_temp_alpha) {
    WaterWavesParams params = pass2_params();
    float mask = 1.0;
    if ((params.flags & WATERWAVES_FLAG_MASK) != 0u) {
        mask = pass2_mask_sample(effect_uv, cached_temp_alpha);
    }
    float time_offset = 0.0;
    if ((params.flags & WATERWAVES_FLAG_TIMEOFFSET) != 0u) {
        time_offset = pass2_timeoffset_sample(effect_uv, cached_temp_alpha) * M_PI_2;
    }
    return waterwaves_layer_offset(effect_uv, params, mask, time_offset);
}

vec4 sample_after_pass1(vec2 target_uv, vec2 effect_uv, vec2 target_scale) {
    float source_alpha = -1.0;
    vec2 offset = pass1_layer_offset(effect_uv, target_uv, source_alpha);
    vec2 source_coord = target_uv + offset * target_scale;
    return texture(g_Texture0, source_coord);
}

vec4 sample_after_pass2(vec2 target_uv, vec2 effect_uv, vec2 target_scale) {
    vec4 pass1_at_output = sample_after_pass1(target_uv, effect_uv, target_scale);
    vec2 pass2_offset = pass2_layer_offset(effect_uv, pass1_at_output.a);
    vec2 pass2_input_uv = target_uv + pass2_offset * target_scale;
    return sample_after_pass1(pass2_input_uv, effect_uv + pass2_offset, target_scale);
}

vec4 sample_single_waterwaves() {
    return sample_after_pass1(v_uv, v_effect_uv, target_uv_per_layer_uv());
}

vec4 sample_fused_waterwaves2() {
    return sample_after_pass2(v_uv, v_effect_uv, target_uv_per_layer_uv());
}

void main() {
    vec4 color = pc.effect_shader_code == EFFECT_SHADER_CODE_WATERWAVES2
        ? sample_fused_waterwaves2()
        : sample_single_waterwaves();
    out_color = finalize_output(apply_vertex_color(color));
}
