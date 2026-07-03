#version 450

// reverse-engineered reference: extracted/3742497499 effects/waterflow keeps
// geometry fixed, samples a flow map from g_Texture1 and a phase texture from
// g_Texture2, then blends two pairs of time-cycled g_Texture0 UV offsets.

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
    layout(offset = 96) float waterflow_strength;
    layout(offset = 100) float waterflow_speed;
    layout(offset = 104) float waterflow_feather;
    layout(offset = 108) float waterflow_phase_scale;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 228) uint output_flags;
} pc;

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

float phase_blend(float cycle, float feather) {
    float blend = 2.0 * abs(cycle - 0.5);
    float safe_feather = clamp(feather, 0.0001, 0.5);
    return smoothstep(0.5 - safe_feather, 0.5 + safe_feather, blend);
}

void main() {
    float flow_phase = 0.0;
    if ((pc.texture_resolution_mask & (1u << 2)) != 0u) {
        flow_phase = texture(g_Texture2, v_uv * pc.waterflow_phase_scale).r;
    }

    vec2 flow_colors = vec2(0.498);
    if ((pc.texture_resolution_mask & (1u << 1)) != 0u) {
        flow_colors = texture(g_Texture1, v_effect_uv).rg;
    }
    vec2 flow_mask = (flow_colors - vec2(0.498)) * 2.0;
    float flow_amount = length(flow_mask);

    float phase = fract(pc.time_seconds * pc.waterflow_speed);
    vec4 cycles = vec4(
        phase,
        fract(phase + 0.5),
        fract(phase + 0.25),
        fract(phase + 0.75)
    ) - vec4(0.5);
    vec2 blend = vec2(
        phase_blend(fract(phase), pc.waterflow_feather),
        phase_blend(fract(phase + 0.25), pc.waterflow_feather)
    );

    vec4 offset = flow_mask.xyxy * pc.waterflow_strength * 0.1 * cycles.xxyy;
    vec4 offset2 = flow_mask.xyxy * pc.waterflow_strength * 0.1 * cycles.zzww;

    vec4 albedo = texture(g_Texture0, v_uv);
    vec4 flow_albedo = mix(
        texture(g_Texture0, v_uv + offset.xy),
        texture(g_Texture0, v_uv + offset.zw),
        blend.x
    );
    vec4 flow_albedo2 = mix(
        texture(g_Texture0, v_uv + offset2.xy),
        texture(g_Texture0, v_uv + offset2.zw),
        blend.y
    );
    flow_albedo = mix(flow_albedo, flow_albedo2, smoothstep(0.2, 0.8, flow_phase));

    out_color = finalize_output(apply_vertex_color(mix(albedo, flow_albedo, flow_amount)));
}
