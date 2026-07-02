#version 450

// CWE reference: WallpaperEngine effects/scroll computes a squared signed
// scroll velocity in vertex shader and samples g_Texture0 at
// frac((v_TexCoord + scroll * g_Time) * g_Scale). Geometry stays fixed.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec2 v_effect_uv;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D g_Texture0;

layout(push_constant) uniform ScenePush {
    layout(offset = 0) vec2 extent;
    layout(offset = 8) uint alpha_texture_slot;
    layout(offset = 12) uint alpha_texture_mode;
    layout(offset = 16) float time_seconds;
    layout(offset = 20) uint texture_resolution_mask;
    layout(offset = 24) uint system_uniform_count;
    layout(offset = 28) uint constant_uniform_count;
    layout(offset = 32) vec2 texture_resolution[8];
    layout(offset = 96) float scroll_speed_x;
    layout(offset = 100) float scroll_speed_y;
    layout(offset = 104) float scroll_repeat_x;
    layout(offset = 108) float scroll_repeat_y;
    layout(offset = 120) uint effect_shader_code;
} pc;

float signed_square(float value) {
    return sign(value) * value * value;
}

void main() {
    vec2 scroll = vec2(signed_square(pc.scroll_speed_x), signed_square(pc.scroll_speed_y))
        * pc.time_seconds;
    vec2 repeat = vec2(pc.scroll_repeat_x, pc.scroll_repeat_y);
    vec2 uv = fract((v_uv + scroll) * repeat);
    out_color = texture(g_Texture0, uv);
}
