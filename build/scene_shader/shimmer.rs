use super::super::*;
use super::effect_program::effect_combo_value_for_key;

pub(super) fn shimmer_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    let samplers = effect_sampler_declarations(texture_slot_mask);
    let mask = if texture_slot_mask & (1 << 1) != 0 {
        "texture(g_Texture1, v_TexCoord).r"
    } else {
        "1.0"
    };
    let time_offset = if texture_slot_mask & (1 << 2) != 0 {
        "texture(g_Texture2, v_TexCoord).r * u_Effect.g_TimeOffsetColor.x"
    } else {
        "0.0"
    };
    let gradient = if texture_slot_mask & (1 << 3) != 0 {
        "texture(g_Texture3, fract(shimmer_coord)).rgb"
    } else {
        // Compatibility for artifacts converted before shader-declared defaults
        // became explicit IR bindings. Fresh conversions always bind slot 3.
        "vec3(smoothstep(0.0, 0.35, shimmer_coord.x) * (1.0 - smoothstep(0.65, 1.0, shimmer_coord.x)))"
    };
    let mode = effect_combo_value_for_key(key, "MODE", 0);
    let movement = if mode == 1 {
        "u_Effect.g_DelayWidthAmountOffset.w + u_Effect.g_DelayWidthAmountOffset.y * sin(u_Effect.g_TimeDirectionScaleSpeed.w * u_Effect.g_TimeDirectionScaleSpeed.x + time_offset)"
    } else {
        "u_Effect.g_DelayWidthAmountOffset.w + u_Effect.g_TimeDirectionScaleSpeed.w * (u_Effect.g_TimeDirectionScaleSpeed.x + time_offset)"
    };
    let blend_mode = effect_combo_value_for_key(key, "BLENDMODE", 32);
    let blend = match blend_mode {
        0 => "shimmer_color * u_Effect.g_TimeOffsetColor.yzw".to_owned(),
        7 => "vec3(1.0) - (vec3(1.0) - albedo.rgb) * (vec3(1.0) - shimmer_color * u_Effect.g_TimeOffsetColor.yzw)".to_owned(),
        31 => "albedo.rgb + shimmer_color * u_Effect.g_TimeOffsetColor.yzw".to_owned(),
        32 => "albedo.rgb + albedo.rgb * shimmer_color * u_Effect.g_TimeOffsetColor.yzw".to_owned(),
        other => panic!("unsupported shimmer BLENDMODE {other} for {key}"),
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
{samplers}layout(set = 0, binding = 3) uniform ShimmerUniform {{
    vec4 g_TimeDirectionScaleSpeed;
    vec4 g_DelayWidthAmountOffset;
    vec4 g_TimeOffsetColor;
    vec4 g_Reserved;
}} u_Effect;
vec2 rotateVec2(vec2 value, float angle) {{
    vec2 cs = vec2(cos(angle), sin(angle));
    return vec2(value.x * cs.x - value.y * cs.y, value.x * cs.y + value.y * cs.x);
}}
void main() {{
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float mask = {mask};
    float time_offset = {time_offset};
    float scale = max(u_Effect.g_TimeDirectionScaleSpeed.z, 0.0001);
    float delay = max(u_Effect.g_DelayWidthAmountOffset.x, 0.0001);
    vec2 shimmer_coord = rotateVec2(
        v_TexCoord,
        -u_Effect.g_TimeDirectionScaleSpeed.y + 1.57079632679) * scale;
    shimmer_coord.x += {movement};
    shimmer_coord.x = clamp(fract(shimmer_coord.x / (scale * delay)) * scale * delay, 0.0, 1.0);
    vec3 shimmer_color = {gradient};
    vec3 effect_color = {blend};
    albedo.rgb = mix(
        albedo.rgb,
        effect_color,
        mask * shimmer_color * u_Effect.g_DelayWidthAmountOffset.z);
    o_Color = albedo;
}}
"#
    )
}
