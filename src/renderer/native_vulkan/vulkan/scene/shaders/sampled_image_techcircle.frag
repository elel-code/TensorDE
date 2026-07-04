#version 450

// reverse-engineered reference:
// reverse-engineered/extracted/3742497499/shaders/workshop/2123274886/effects/tech_circle.frag
// The WE pass computes ring/sector coverage in fragment space. For compositor
// correctness we emit the generated ring as an alpha overlay; normal alpha
// blending yields the same visible color mix without a CPU framebuffer readback.

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
    layout(offset = 96) float color_r;
    layout(offset = 100) float color_g;
    layout(offset = 104) float color_b;
    layout(offset = 108) float alpha;
    layout(offset = 112) float speed;
    layout(offset = 116) float skew;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 124) float ring_radius;
    layout(offset = 128) float ring_width;
    layout(offset = 132) float ring_segment_count;
    layout(offset = 136) float ring_segment_width;
    layout(offset = 140) float sector_offset;
    layout(offset = 144) float sector_width;
    layout(offset = 148) float sector_segment_count;
    layout(offset = 152) float sector_segment_width;
    layout(offset = 156) uint tech_flags;
    layout(offset = 228) uint output_flags;
} pc;

const float PI = 3.1415926535897932384626433832795;
const uint TECH_COORD_CARTESIAN = 0u;
const uint TECH_COORD_POLAR = 1u;
const uint TECH_RING_SEGMENTS = 1u << 4;
const uint TECH_RATIO_CORRECTION = 1u << 8;
const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;

float saturate(float value) {
    return clamp(value, 0.0, 1.0);
}

float saw(float x) {
    return abs(fract(x + 0.5) * 2.0 - 1.0);
}

float simple_stripes(float count, float width, float x) {
    float threshold = 1.0 - saturate(width);
    return step(threshold, saw(x * max(count, 1.0)))
        * step(0.0, x)
        * step(x, 1.0);
}

float ring(float dist, float width, float stripe_count, float stripe_width, vec2 puv) {
    float safe_width = max(width, 0.0001);
    return simple_stripes(stripe_count, stripe_width, (puv.x - dist + safe_width * 0.5) / safe_width);
}

float sector(float pos, float width, float stripe_count, float stripe_width, vec2 puv) {
    float safe_width = max(width, 0.0001);
    float sector_pos = fract(puv.y - fract(pos - safe_width * 0.5));
    return simple_stripes(stripe_count, stripe_width, sector_pos / safe_width);
}

vec4 finalize_output(vec4 color) {
    if ((pc.output_flags & OUTPUT_FLAG_PREMULTIPLY_RGB) != 0u) {
        color.rgb *= color.a;
    }
    return color;
}

void main() {
    uint coord_sys = pc.tech_flags & 0x3u;
    uint sector_segments = (pc.tech_flags >> 5u) & 0x3u;
    vec2 uv = v_uv;
    if (coord_sys == TECH_COORD_POLAR) {
        vec2 centered = v_uv - vec2(0.5);
        if ((pc.tech_flags & TECH_RATIO_CORRECTION) != 0u && pc.extent.y > 0.0) {
            centered.y /= pc.extent.x / pc.extent.y;
        }
        uv = vec2(length(centered) * 2.0, atan(centered.x, centered.y) / (PI * 2.0) + 0.5);
    } else if ((pc.tech_flags & TECH_RATIO_CORRECTION) != 0u && pc.extent.y > 0.0) {
        uv.y /= pc.extent.x / pc.extent.y;
    }

    float center_perimeter = max(pc.ring_radius * 2.0 * PI, 0.0001);
    float current_perimeter = max(uv.x * 2.0 * PI, 0.0001);
    float perimeter_ratio = current_perimeter / center_perimeter;
    uv.y += ((uv.x - pc.ring_radius) / max(pc.ring_width, 0.0001) / perimeter_ratio) * pc.skew;

    float ring_coverage = ((pc.tech_flags & TECH_RING_SEGMENTS) != 0u)
        ? ring(pc.ring_radius, pc.ring_width, pc.ring_segment_count, pc.ring_segment_width, uv)
        : ring(pc.ring_radius, pc.ring_width, 1.0, 1.0, uv);
    float sector_pos = pc.sector_offset + pc.time_seconds * pc.speed;
    float sector_coverage = sector(sector_pos, pc.sector_width, 1.0, 1.0, uv);
    if (sector_segments == 1u) {
        sector_coverage = sector(
            sector_pos,
            pc.sector_width,
            pc.sector_segment_count,
            pc.sector_segment_width,
            uv
        );
    } else if (sector_segments == 2u) {
        sector_coverage = sector(
            sector_pos,
            pc.sector_width,
            pc.sector_segment_count,
            pc.sector_segment_width / max(perimeter_ratio, 0.0001),
            uv
        );
    }

    float final_alpha = saturate(ring_coverage * sector_coverage * pc.alpha) * v_opacity * v_tint.a;
    vec3 color = vec3(pc.color_r, pc.color_g, pc.color_b) * v_tint.rgb;
    out_color = finalize_output(vec4(color, final_alpha));
}
