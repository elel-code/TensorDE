#version 450

// reverse-engineered reference:
// reverse-engineered/extracted/3742497499/shaders/workshop/3082978660/effects/Simple_Audio_Bars.frag
// This is the first-class GPU path for effect-only composelayers. When the
// native FFmpeg/PipeWire audio clock has decoded PCM, a packed 32-band
// spectrum is pushed into this shader; otherwise it falls back to a decoded
// RMS signal or a deterministic preview signal so non-audio tests and static
// previews still draw visible bars.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec2 v_effect_uv;
layout(location = 2) in float v_opacity;
layout(location = 3) in vec4 v_tint;
layout(location = 4) flat in float v_time_seconds;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D g_Texture0;

layout(push_constant) uniform ScenePush {
    layout(offset = 0) vec2 extent;
    layout(offset = 8) uint alpha_texture_slot;
    layout(offset = 12) uint alpha_texture_mode;
    layout(offset = 20) uint texture_resolution_mask;
    layout(offset = 24) uint system_uniform_count;
    layout(offset = 28) uint constant_uniform_count;
    layout(offset = 32) vec2 texture_resolution[8];
    layout(offset = 96) float color_r;
    layout(offset = 100) float color_g;
    layout(offset = 104) float color_b;
    layout(offset = 108) float opacity;
    layout(offset = 112) float bar_count;
    layout(offset = 116) float volume_factor;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 124) float bar_spacing;
    layout(offset = 128) float min_height;
    layout(offset = 132) float bounds_low;
    layout(offset = 136) float bounds_high;
    layout(offset = 140) uint audio_flags;
    layout(offset = 144) float aa_x;
    layout(offset = 148) float aa_y;
    layout(offset = 152) float radius;
    layout(offset = 156) float signal_level;
    layout(offset = 160) uint spectrum32_packed[16];
    layout(offset = 228) uint output_flags;
} pc;

const uint AUDIO_SHAPE_BOTTOM = 0u;
const uint AUDIO_SHAPE_TOP = 1u;
const uint AUDIO_SHAPE_LEFT = 2u;
const uint AUDIO_SHAPE_RIGHT = 3u;
const uint AUDIO_SHAPE_CENTER_H = 6u;
const uint AUDIO_SHAPE_CENTER_V = 7u;
const uint AUDIO_FLAG_HAS_SIGNAL = 1u << 16;
const uint AUDIO_FLAG_HAS_SPECTRUM32 = 1u << 17;
const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;

float saturate(float value) {
    return clamp(value, 0.0, 1.0);
}

float hash01(float value) {
    return fract(sin(value * 12.9898) * 43758.5453);
}

float pseudo_audio(float index) {
    float slow = sin(v_time_seconds * 2.2 + index * 0.73) * 0.5 + 0.5;
    float fast = sin(v_time_seconds * 5.1 + index * 1.91) * 0.5 + 0.5;
    float noise = hash01(index + floor(v_time_seconds * 8.0) * 0.13);
    return saturate(slow * 0.50 + fast * 0.30 + noise * 0.20);
}

float remap_volume(float volume) {
    return volume;
}

float audio_spectrum32(float index) {
    uint band = uint(clamp(floor(index), 0.0, 31.0));
    uint packed = pc.spectrum32_packed[band >> 1u];
    uint raw = ((band & 1u) == 0u) ? (packed & 0xffffu) : (packed >> 16);
    return float(raw & 0xffffu) / 65535.0;
}

float audio_signal(float index) {
    if ((pc.audio_flags & AUDIO_FLAG_HAS_SPECTRUM32) != 0u) {
        return audio_spectrum32(index);
    }
    if ((pc.audio_flags & AUDIO_FLAG_HAS_SIGNAL) == 0u) {
        return pseudo_audio(index);
    }
    float profile = 0.68 + 0.32 * hash01(index + 19.0);
    return saturate(pc.signal_level * profile);
}

float rounded_box_sdf(vec2 cur_position, vec3 size, float deformity) {
    size *= 0.5;
    size.x *= deformity;
    cur_position.y -= size.y + size.z;
    size.y -= size.z;
    float radius = clamp(pc.radius, 0.1, 1.0) * min(size.x, size.y);
    cur_position.x *= deformity;
    return length(max(abs(cur_position) - size.xy + radius, 0.0)) - radius;
}

vec4 finalize_output(vec4 color) {
    if ((pc.output_flags & OUTPUT_FLAG_PREMULTIPLY_RGB) != 0u) {
        color.rgb *= color.a;
    }
    return color;
}

void main() {
    uint shape = pc.audio_flags & 0xffu;
    vec2 uv = v_uv;
    vec2 source_uv = v_uv;
    if (shape == AUDIO_SHAPE_TOP) {
        uv.y = 1.0 - uv.y;
    } else if (shape == AUDIO_SHAPE_LEFT) {
        uv = vec2(v_uv.y, 1.0 - v_uv.x);
    } else if (shape == AUDIO_SHAPE_RIGHT) {
        uv = v_uv.yx;
    } else if (shape == AUDIO_SHAPE_CENTER_H) {
        uv = v_uv.yx;
        uv.y = fract(0.5 - uv.y) + floor(uv.y);
    } else if (shape == AUDIO_SHAPE_CENTER_V) {
        uv.y = fract(0.5 - uv.y) + floor(uv.y);
    }

    float count = clamp(pc.bar_count, 1.0, 256.0);
    float audio_resolution = 32.0;
    float scaled_x = clamp(uv.x, 0.0, 0.999999) * count;
    float frequency = floor(scaled_x) / count * audio_resolution;
    float bar_freq1 = mod(frequency, audio_resolution);
    float bar_freq2 = mod(bar_freq1 + 1.0, audio_resolution);
    float audio_mix = smoothstep(0.0, 1.0, fract(frequency));
    float bar_dist = abs(fract(scaled_x) * 2.0 - 1.0);
    float spacing = clamp(pc.bar_spacing, 0.0, 0.95);
    float r_width = (1.0 - spacing) / count;
    vec2 tex0_resolution = pc.extent;
    if ((pc.texture_resolution_mask & 1u) != 0u) {
        tex0_resolution = pc.texture_resolution[0];
    }
    float deformity = 1.0;
    if (tex0_resolution.x > 0.0 && tex0_resolution.y > 0.0) {
        if (
            shape == AUDIO_SHAPE_LEFT
            || shape == AUDIO_SHAPE_RIGHT
            || shape == AUDIO_SHAPE_CENTER_H
        ) {
            deformity = tex0_resolution.y / tex0_resolution.x;
        } else {
            deformity = tex0_resolution.x / tex0_resolution.y;
        }
    }

    float right_freq1 = ((pc.audio_flags & AUDIO_FLAG_HAS_SPECTRUM32) != 0u) ? bar_freq1 : bar_freq1 + 17.0;
    float right_freq2 = ((pc.audio_flags & AUDIO_FLAG_HAS_SPECTRUM32) != 0u) ? bar_freq2 : bar_freq2 + 17.0;
    float left_audio = remap_volume(mix(audio_signal(bar_freq1), audio_signal(bar_freq2), audio_mix))
        * max(pc.volume_factor, 0.0);
    float right_audio = remap_volume(mix(audio_signal(right_freq1), audio_signal(right_freq2), audio_mix))
        * max(pc.volume_factor, 0.0);
    float min_height = pc.min_height * r_width * deformity;
    float aa_factor = 15.0 / max(1.0, min(tex0_resolution.x, tex0_resolution.y));
    float aa_min = -max(pc.aa_x, 0.0) * aa_factor;
    float aa_max = max(pc.aa_y, 0.0) * aa_factor;
    if (aa_max <= aa_min) {
        aa_max = aa_min + aa_factor * 0.04;
    }

    float bar;
    if (shape == AUDIO_SHAPE_CENTER_H || shape == AUDIO_SHAPE_CENTER_V) {
        float left_height = 0.5 * mix(max(pc.bounds_low, min_height) * 2.0, pc.bounds_high, left_audio);
        float right_height = 0.5 * mix(max(pc.bounds_low, min_height) * 2.0, pc.bounds_high, right_audio);
        float lower_left = 0.0;
        float lower_right = 0.0;
        vec2 center_left = vec2(bar_dist / count * 0.5, uv.y);
        vec2 center_right = vec2(bar_dist / count * 0.5, 1.0 - uv.y);
        vec3 size_left = vec3(r_width, left_height, lower_left);
        vec3 size_right = vec3(r_width, right_height, lower_right);
        float offset = min(r_width, max(size_left.y, size_right.y)) * 0.5;
        if (shape == AUDIO_SHAPE_CENTER_V) {
            offset *= deformity;
        }
        center_left.y += offset;
        center_right.y += offset;
        float d_left = rounded_box_sdf(center_left, size_left, deformity);
        float d_right = rounded_box_sdf(center_right, size_right, deformity);
        bar = 1.0 - min(smoothstep(aa_min, aa_max, d_left), smoothstep(aa_min, aa_max, d_right));
    } else {
        float height = mix(max(pc.bounds_low, min_height), pc.bounds_high, (left_audio + right_audio) * 0.5);
        vec2 center = vec2(bar_dist / count * 0.5, 1.0 - uv.y);
        float d = rounded_box_sdf(center, vec3(r_width, height, 0.0), deformity);
        bar = 1.0 - smoothstep(aa_min, aa_max, d);
    }

    vec4 scene = texture(g_Texture0, source_uv);
    float bar_opacity = saturate(bar * pc.opacity);
    vec3 color = vec3(pc.color_r, pc.color_g, pc.color_b) * v_tint.rgb;
    vec3 base_color = mix(color, scene.rgb, scene.a);
    vec3 final_color = mix(base_color, color, bar_opacity);
    float alpha = bar_opacity * v_opacity * v_tint.a;
    out_color = finalize_output(vec4(final_color, alpha));
}
