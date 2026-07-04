#version 450

// reverse-engineered reference: WallpaperEngine effects/skew has two variants.
// MODE=0 shears only source UVs. MODE=1 moves vertices by
// g_Texture0Resolution.zw * top/bottom/left/right. In Gilder's graph-local
// full-target route, MODE=1 is rendered by the mathematically equivalent inverse
// source-coordinate transform plus transparent output outside the sheared quad.
// For graph-local full targets the vertex-mode equivalent samples in original
// layer UV space (`v_effect_uv`) rather than normalized target UV space. That
// lets the renderer expand the target bounds for the moved vertices without
// scaling the skew amount. With bottom=-0.39, lower rows sample source x+0.39,
// so the visible lower silhouette moves left instead of being cut off.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec2 v_effect_uv;
layout(location = 2) in float v_opacity;
layout(location = 3) in vec4 v_tint;

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
    layout(offset = 96) float skew_top;
    layout(offset = 100) float skew_bottom;
    layout(offset = 104) float skew_left;
    layout(offset = 108) float skew_right;
    layout(offset = 112) uint skew_flags;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 228) uint output_flags;
} pc;

const uint SKEW_FLAG_REPEAT = 1u;
const uint SKEW_FLAG_VERTEX_MODE = 2u;
const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;

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
    if ((pc.skew_flags & SKEW_FLAG_VERTEX_MODE) != 0u) {
        vec2 layer_uv = v_effect_uv;
        float source_y = 1.0 - layer_uv.y;
        vec2 uv = layer_uv;
        uv.x -= mix(pc.skew_top, pc.skew_bottom, source_y);
        uv.y += mix(pc.skew_left, pc.skew_right, clamp(layer_uv.x, 0.0, 1.0));
        if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) {
            out_color = vec4(0.0);
            return;
        }
        out_color = finalize_output(apply_vertex_color(texture(g_Texture0, uv)));
        return;
    } else {
        vec2 pass_uv = vec2(v_uv.x, 1.0 - v_uv.y);
        vec2 uv = pass_uv;
        uv.x -= step(pass_uv.y, 0.5) * pc.skew_top
            + step(0.5, pass_uv.y) * pc.skew_bottom;
        uv.y += step(pass_uv.x, 0.5) * pc.skew_left
            + step(0.5, pass_uv.x) * pc.skew_right;
        if ((pc.skew_flags & SKEW_FLAG_REPEAT) != 0u) {
            uv = fract(uv);
        }
        vec2 sample_uv = vec2(uv.x, 1.0 - uv.y);
        out_color = finalize_output(apply_vertex_color(texture(g_Texture0, sample_uv)));
    }
}
