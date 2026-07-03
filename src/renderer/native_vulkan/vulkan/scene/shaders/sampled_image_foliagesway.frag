#version 450

// reverse-engineered reference:
// extracted/3742497499/workshop/2790231929/effects/foliagesway MODE=0 is a
// fragment UV pass. Geometry stays fixed; a noise texture drives time-varying
// UV offsets before resampling g_Texture0. MODE=1 vertex displacement remains
// a separate route.

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
    layout(offset = 96) float foliage_strength;
    layout(offset = 100) float foliage_speed;
    layout(offset = 104) float foliage_phase;
    layout(offset = 108) float foliage_power;
    layout(offset = 112) float foliage_noise_scale;
    layout(offset = 116) float foliage_ratio;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 124) float foliage_direction;
    layout(offset = 128) uint foliage_flags;
    layout(offset = 228) uint output_flags;
} pc;

const float PI = 3.14159265358979323846;
const uint FOLIAGE_SWAY_FLAG_MASK = 1u;
const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;

vec2 rotate_vec2(vec2 value, float radians) {
    float s = sin(radians);
    float c = cos(radians);
    return vec2(c * value.x - s * value.y, s * value.x + c * value.y);
}

vec3 fallback_noise(vec2 uv) {
    vec3 p = fract(vec3(uv.xyx) * vec3(123.34, 456.21, 345.45));
    p += dot(p, p.yzx + 45.32);
    return fract(vec3(p.x * p.y, p.y * p.z, p.z * p.x));
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
    vec2 base_resolution = pc.texture_resolution[0];
    if ((pc.texture_resolution_mask & 1u) == 0u) {
        base_resolution = max(pc.extent, vec2(1.0));
    }
    // reverse-engineered reference: foliagesway.vert uses
    // g_Texture0Resolution.z / g_Texture0Resolution.w. WE resolution.zw are
    // the logical texture width/height.
    float aspect = max(base_resolution.x, 1.0) / max(base_resolution.y, 1.0) * pc.foliage_ratio;
    aspect = max(aspect, 0.0001);

    vec2 noise_uv = v_uv * pc.foliage_noise_scale;
    vec3 noise = ((pc.texture_resolution_mask & (1u << 2)) != 0u)
        ? texture(g_Texture2, noise_uv).rgb
        : fallback_noise(noise_uv);

    float amp = pc.foliage_strength * pc.foliage_strength * 0.005;
    if ((pc.foliage_flags & FOLIAGE_SWAY_FLAG_MASK) != 0u) {
        amp *= texture(g_Texture1, v_effect_uv).r;
    }

    vec2 params = rotate_vec2(v_uv, pc.foliage_direction);
    float phase = (noise.g * PI * 2.0 + params.x * 10.0 + params.y * 5.0) * pc.foliage_phase;
    vec4 sines = sin(phase + pc.foliage_speed * pc.time_seconds
        * vec4(1.0, -0.16161616, 0.0083333, -0.00019841));
    vec4 csines = sin(0.4 + phase + pc.foliage_speed * pc.time_seconds
        * vec4(-0.5, 0.041666666, -0.0013888889, 0.000024801587));

    sines = pow(abs(sines), vec4(pc.foliage_power)) * sign(sines);
    csines = pow(abs(csines), vec4(pc.foliage_power)) * sign(csines);

    vec2 noise_direction = rotate_vec2(vec2(1.0 / aspect, aspect), pc.foliage_direction);
    vec2 tex_coord_offset = vec2(
        noise_direction.x * dot(sines, vec4(amp)),
        noise_direction.y * dot(csines, vec4(amp))
    );
    out_color = finalize_output(apply_vertex_color(texture(g_Texture0, v_uv + tex_coord_offset)));
}
