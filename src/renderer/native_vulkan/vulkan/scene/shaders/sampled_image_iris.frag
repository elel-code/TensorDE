#version 450

// reverse-engineered reference: WallpaperEngine effects/iris.frag keeps the
// original source sample and mixes in the UV-offset iris sample by mask. The
// mesh/quad geometry stays fixed; the iris mask only gates the animated source
// resample.

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
    layout(offset = 96) vec2 iris_scale;
    layout(offset = 104) float iris_speed;
    layout(offset = 108) float iris_rough;
    layout(offset = 112) float iris_noise_amount;
    layout(offset = 116) float iris_phase_offset;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 228) uint output_flags;
} pc;

const float M_PI = 3.14159265359;
const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;

vec2 iris_motion() {
    float time = pc.time_seconds * pc.iris_speed + pc.iris_phase_offset;
    float low_dt = floor(time);
    vec2 motion2 = sin(1.9 * (low_dt + vec2(0.0, 1.0)));
    vec4 motion4 = sin(2.5 * (low_dt + vec4(0.0, 0.0, 1.0, 1.0))
        + vec4(1.0, 2.0, 1.0, 2.0));
    vec2 move_start = motion2.xx + motion4.xy;
    vec2 move_end = motion2.yy + motion4.zw;
    float phase = cos(fract(time) * M_PI) * -0.5 + 0.5;
    vec2 da = mix(move_start, move_end, smoothstep(1.0 - pc.iris_rough, 1.0, phase));
    da.x += sin(time) * pc.iris_noise_amount;
    da.y += cos(time) * pc.iris_noise_amount;
    return da * pc.iris_scale * 0.001;
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

void main() {
    float mask = 1.0;
    if ((pc.texture_resolution_mask & (1u << 1)) != 0u) {
        mask = texture(g_Texture1, v_effect_uv).r;
    }
    vec4 albedo = texture(g_Texture0, v_uv);
    vec4 iris = texture(g_Texture0, v_uv + iris_motion() * mask);
    out_color = finalize_output(apply_vertex_color(mix(albedo, iris, mask)));
}
