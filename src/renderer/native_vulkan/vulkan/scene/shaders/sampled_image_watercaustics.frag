#version 450

// reverse-engineered reference: WallpaperEngine effects/caustics.frag layers
// voronoi/perlin textures, then applies common_blending.h ApplyBlending().

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
layout(set = 0, binding = 5) uniform sampler2D g_Texture5;

layout(push_constant) uniform ScenePush {
    layout(offset = 0) vec2 extent;
    layout(offset = 8) uint alpha_texture_slot;
    layout(offset = 12) uint alpha_texture_mode;
    layout(offset = 20) uint texture_resolution_mask;
    layout(offset = 24) uint system_uniform_count;
    layout(offset = 28) uint constant_uniform_count;
    layout(offset = 32) vec2 texture_resolution[8];
    layout(offset = 96) float caustics_brightness;
    layout(offset = 100) float caustics_glow;
    layout(offset = 104) float caustics_scale;
    layout(offset = 108) float caustics_speed;
    layout(offset = 112) float caustics_time_offset;
    layout(offset = 116) float caustics_distortion;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 124) float caustics_chromatic;
    layout(offset = 128) float caustics_blur;
    layout(offset = 132) float caustics_color1_r;
    layout(offset = 136) float caustics_color1_g;
    layout(offset = 140) float caustics_color1_b;
    layout(offset = 144) float caustics_color2_r;
    layout(offset = 148) float caustics_color2_g;
    layout(offset = 152) float caustics_color2_b;
    layout(offset = 156) uint caustics_flags;
    layout(offset = 228) uint output_flags;
} pc;

const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;

bool has_texture(uint slot) {
    return (pc.texture_resolution_mask & (1u << slot)) != 0u;
}

float saturate(float value) {
    return clamp(value, 0.0, 1.0);
}

vec3 saturate3(vec3 value) {
    return clamp(value, vec3(0.0), vec3(1.0));
}

float hash_noise(vec2 coords) {
    vec2 p = floor(coords * 256.0);
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

vec4 sample_noise3(vec2 coords) {
    if (has_texture(3u)) {
        return texture(g_Texture3, coords);
    }
    float n = hash_noise(coords);
    return vec4(n, hash_noise(coords + 17.0), hash_noise(coords + 37.0), 1.0);
}

vec4 sample_offset4(vec2 coords) {
    if (has_texture(4u)) {
        return texture(g_Texture4, coords);
    }
    return vec4(0.5, 0.5, 0.5, 1.0);
}

float sample_pattern2(vec2 coords) {
    if (has_texture(2u)) {
        return texture(g_Texture2, coords).r;
    }
    return hash_noise(coords);
}

float sample_pattern5(vec2 coords) {
    if (has_texture(5u)) {
        return texture(g_Texture5, coords).r;
    }
    return sample_pattern2(coords);
}

vec3 blend_color_burn(vec3 base, vec3 blend) {
    return vec3(
        blend.r == 0.0 ? blend.r : max(1.0 - ((1.0 - base.r) / blend.r), 0.0),
        blend.g == 0.0 ? blend.g : max(1.0 - ((1.0 - base.g) / blend.g), 0.0),
        blend.b == 0.0 ? blend.b : max(1.0 - ((1.0 - base.b) / blend.b), 0.0)
    );
}

vec3 blend_color_dodge(vec3 base, vec3 blend) {
    return vec3(
        blend.r == 1.0 ? blend.r : min(base.r / (1.0 - blend.r), 1.0),
        blend.g == 1.0 ? blend.g : min(base.g / (1.0 - blend.g), 1.0),
        blend.b == 1.0 ? blend.b : min(base.b / (1.0 - blend.b), 1.0)
    );
}

vec3 apply_blending(uint mode, vec3 base, vec3 blend, float opacity) {
    if (mode == 2u) {
        return mix(base, base * blend, opacity);
    }
    if (mode == 3u) {
        return mix(base, blend_color_burn(base, blend), opacity);
    }
    if (mode == 6u) {
        return mix(base, max(base, blend), opacity);
    }
    if (mode == 7u) {
        return mix(base, 1.0 - (1.0 - base) * (1.0 - blend), opacity);
    }
    if (mode == 8u) {
        return mix(base, blend_color_dodge(base, blend), opacity);
    }
    if (mode == 31u) {
        return base + blend * opacity;
    }
    if (mode == 32u) {
        return mix(base, base + base * blend, opacity);
    }
    return blend;
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
    vec4 albedo = texture(g_Texture0, v_uv);
    bool framebuffer_overlay = (pc.caustics_flags & (1u << 16)) != 0u;
    float mask = 1.0;
    if (has_texture(1u)) {
        mask *= texture(g_Texture1, v_effect_uv).r;
    }

    vec2 source_resolution = pc.texture_resolution[0];
    float ratio = pc.extent.y > 0.0 ? pc.extent.x / pc.extent.y : 1.0;
    if (!framebuffer_overlay && has_texture(0u) && source_resolution.y > 0.0) {
        ratio = source_resolution.x / source_resolution.y;
    }

    vec2 caustics_coords = v_uv;
    caustics_coords.x *= ratio;
    caustics_coords *= pc.caustics_scale;

    vec2 noise_coords = caustics_coords * 0.02;
    vec2 noise_coords2 = caustics_coords * 0.0333;
    vec2 blend_coords = caustics_coords * 0.01333;
    vec2 shift_coords = caustics_coords * 0.05;

    float time = v_time_seconds * pc.caustics_speed + pc.caustics_time_offset;
    noise_coords.x += time * 0.005;
    noise_coords2.y += time * 0.004111;
    blend_coords += time * 0.003777;
    shift_coords += time * 0.01;

    vec4 shift_color = sample_offset4(shift_coords) * 2.0 - 1.0;
    vec4 noise_color = sample_noise3(noise_coords) * 2.0 - 1.0;
    vec4 noise_color2 = sample_noise3(noise_coords2) * 2.0 - 1.0;

    caustics_coords += noise_color.xy * 0.025 * pc.caustics_distortion;
    caustics_coords += noise_color2.xy * 0.025 * pc.caustics_distortion;
    caustics_coords += shift_color.rg * pc.caustics_distortion;

    vec2 caustics_left = caustics_coords;
    vec2 caustics_right = caustics_coords;
    caustics_left.x -= 0.01 * pc.caustics_chromatic;
    caustics_right.x += 0.01 * pc.caustics_chromatic;

    vec3 caustics = vec3(
        sample_pattern2(caustics_left),
        sample_pattern2(caustics_coords),
        sample_pattern2(caustics_right)
    );
    float glow_sample = sample_pattern5(caustics_coords);
    vec4 blend_color = sample_noise3(blend_coords);
    caustics = mix(caustics, vec3(glow_sample), pc.caustics_blur);

    float caustics_sample;
    vec3 color1 = vec3(pc.caustics_color1_r, pc.caustics_color1_g, pc.caustics_color1_b);
    vec3 color2 = vec3(pc.caustics_color2_r, pc.caustics_color2_g, pc.caustics_color2_b);
    vec3 caustics_color;
    uint style_mode = (pc.caustics_flags >> 8u) & 0xffu;
    if (style_mode == 1u) {
        caustics_sample = caustics.y;
        float blend_threshold = max(0.3, blend_color.x - shift_color.x);
        float particle_noise = sample_noise3(shift_coords).r;
        float particle_sample = smoothstep(blend_threshold, blend_threshold - 0.001, caustics_sample)
            * step(0.3, particle_noise * caustics_sample);
        caustics_sample = smoothstep(blend_threshold, blend_threshold + 0.001, caustics_sample)
            + particle_sample;
        caustics_sample = saturate(caustics_sample + glow_sample * pc.caustics_glow);
        caustics_color = pc.caustics_brightness
            * mix(color1, color2, smoothstep(0.0, 0.5, blend_color.x));
    } else {
        caustics_sample = dot(caustics, vec3(0.33333));
        caustics_sample = smoothstep(
            blend_color.x * 0.8,
            1.0 - blend_color.y * 0.2,
            caustics_sample + glow_sample * pc.caustics_glow
        );
        caustics_color = pc.caustics_brightness * mix(color1, color2, blend_color.x);
        caustics_color.rgb *= caustics;
    }

    uint blend_mode = pc.caustics_flags & 0xffu;
    if (framebuffer_overlay) {
        float opacity = mask * caustics_sample;
        out_color = finalize_output(apply_vertex_color(vec4(caustics_color * opacity, opacity)));
        return;
    }
    albedo.rgb = apply_blending(blend_mode, albedo.rgb, caustics_color, mask * caustics_sample);
    out_color = finalize_output(apply_vertex_color(albedo));
}
