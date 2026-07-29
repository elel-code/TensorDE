//! Authored ray-marched fluid sphere (`effects/qiu`).

use super::effect_program::effect_combo_value_for_key;

pub(super) fn sphere_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    assert_eq!(effect_combo_value_for_key(key, "CUSTOMCOLOR", 0), 1);
    assert_eq!(effect_combo_value_for_key(key, "RAINBOW", 1), 0);
    assert_eq!(effect_combo_value_for_key(key, "SPHERE_SOLID_COLOR", 0), 1);
    assert_eq!(effect_combo_value_for_key(key, "TRANSPARENT_BG", 1), 1);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform SphereUniform {
    vec4 g_TimeFlowSizeEffectAlpha;
    vec4 g_Color1;
    vec4 g_Color2;
    vec4 g_SphereColorAlpha;
} u_Effect;
vec3 authoredColor(float x, float time) {
    float blend = sin(x * 3.14159 + time * 0.5) * 0.5 + 0.5;
    return mix(u_Effect.g_Color1.rgb, u_Effect.g_Color2.rgb, blend);
}
void main() {
    vec2 resolution = vec2(textureSize(g_Texture0, 0));
    vec2 fragment_coord = v_TexCoord * resolution;
    vec2 uv = (2.0 * fragment_coord - resolution) / resolution.y;
    vec3 ray_origin = vec3(0.0, 0.0, 6.0);
    vec3 ray_direction = normalize(vec3(uv, -2.0));
    float authored_time = u_Effect.g_TimeFlowSizeEffectAlpha.x;
    // The authored transparent-background permutation overwrites its initial
    // flow-scaled value with g_Time before ray marching.
    float time = authored_time;
    vec3 color = vec3(0.0);
    bool inside_sphere = false;
    float t = dot(-ray_origin, ray_direction);
    vec3 center_point = t * ray_direction + ray_origin;
    float center_distance_squared = dot(center_point, center_point);
    float sphere_radius_squared = u_Effect.g_TimeFlowSizeEffectAlpha.z
        * u_Effect.g_TimeFlowSizeEffectAlpha.z;
    float intersection = sphere_radius_squared - center_distance_squared;
    if (center_distance_squared <= sphere_radius_squared) {
        inside_sphere = true;
        float near_t = t - sqrt(intersection);
        float far_t = t + sqrt(intersection);
        color *= exp(-(far_t - near_t));
        t = near_t + texture(g_Texture0, fract(fragment_coord / 1024.0)).a * 0.01;
        for (int step_index = 0; step_index < 99 && t < far_t; ++step_index) {
            vec3 point = t * ray_direction + ray_origin;
            float rotation_time = (t + time) / 5.0;
            float cosine = cos(rotation_time);
            float sine = sin(rotation_time);
            point.xy = mat2(cosine, -sine, sine, cosine) * point.xy;
            for (int octave = 0; octave < 9; ++octave) {
                float frequency = exp(float(octave)) / exp2(float(octave));
                point += cos(point.yzx * frequency + time) / frequency;
            }
            float distance_step = 0.01
                + abs((ray_origin - point - vec3(0.0, 1.0, 0.0)).y - 1.0) / 10.0;
            color += authoredColor(t, authored_time) * 0.001 / distance_step;
            t += distance_step * 0.25;
        }
        float fresnel_base = 0.04;
        vec3 normal = normalize(near_t * ray_direction + ray_origin);
        float cosine_theta = dot(-ray_direction, normal);
        float fresnel = fresnel_base
            + (1.0 - fresnel_base) * pow(1.0 - cosine_theta, 5.0);
        color *= 1.0 - fresnel;
        vec3 background_reflection = pow(
            texture(g_Texture0, reflect(ray_direction, normal).xy).rgb,
            vec3(2.2));
        color += fresnel * mix(
            background_reflection,
            u_Effect.g_SphereColorAlpha.rgb,
            u_Effect.g_SphereColorAlpha.a);
    }
    color = vec3(1.0) - exp(-color);
    color = pow(color, vec3(1.0 / 2.2));
    color = clamp(color, vec3(0.0), vec3(1.0));
    float alpha = inside_sphere ? u_Effect.g_TimeFlowSizeEffectAlpha.w : 0.0;
    o_Color = vec4(color, alpha);
}
"#
    .to_owned()
}
