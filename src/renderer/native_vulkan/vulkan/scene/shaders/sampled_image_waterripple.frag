#version 450

// reverse-engineered reference: WallpaperEngine effects/waterripple samples
// g_Texture2 twice as a scrolling normal map, offsets g_Texture0 UVs, and
// writes the sampled source color without changing geometry.

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
    layout(offset = 96) float waterripple_strength;
    layout(offset = 100) float waterripple_animation_speed;
    layout(offset = 104) float waterripple_scale;
    layout(offset = 108) float waterripple_scroll_speed;
    layout(offset = 112) float waterripple_direction;
    layout(offset = 116) float waterripple_ratio;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 228) uint output_flags;
} pc;

const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;

vec2 rotate_up(float radians) {
    return vec2(-sin(radians), cos(radians));
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
    vec2 coords = v_uv;
    vec2 coords2 = v_uv * 1.333;
    vec2 scroll = rotate_up(pc.waterripple_direction)
        * pc.waterripple_scroll_speed
        * pc.waterripple_scroll_speed
        * pc.time_seconds;
    float animation = pc.time_seconds
        * pc.waterripple_animation_speed
        * pc.waterripple_animation_speed;

    vec4 ripple_coords = vec4(coords + animation + scroll, coords2 - animation + scroll)
        * pc.waterripple_scale;
    vec2 source_resolution = pc.texture_resolution[0];
    if ((pc.texture_resolution_mask & 1u) != 0u && source_resolution.y > 0.0) {
        ripple_coords.xz *= source_resolution.x / source_resolution.y;
    }
    ripple_coords.yw *= pc.waterripple_ratio;

    vec3 n1 = texture(g_Texture2, ripple_coords.xy).xyz * 2.0 - 1.0;
    vec3 n2 = texture(g_Texture2, ripple_coords.zw).xyz * 2.0 - 1.0;
    vec3 normal = normalize(vec3(n1.xy + n2.xy, n1.z));
    float mask = 1.0;
    if ((pc.texture_resolution_mask & (1u << 1)) != 0u) {
        mask = texture(g_Texture1, v_effect_uv).r;
    }
    vec2 uv = v_uv + normal.xy
        * pc.waterripple_strength
        * pc.waterripple_strength
        * mask;

    out_color = finalize_output(apply_vertex_color(texture(g_Texture0, uv)));
}
