#version 450

// CWE reference: WallpaperEngine effects/iris is a fragment-space source
// resample. The mesh/quad geometry stays fixed; the iris mask only gates the
// animated UV offset used to sample g_Texture0.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec2 v_effect_uv;

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
} pc;

const float M_PI = 3.14159265359;

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

void main() {
    float mask = 1.0;
    if ((pc.texture_resolution_mask & (1u << 1)) != 0u) {
        mask = texture(g_Texture1, v_effect_uv).r;
    }
    out_color = texture(g_Texture0, v_uv + iris_motion() * mask);
}
