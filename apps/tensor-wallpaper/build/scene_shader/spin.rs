//! Typed WE elliptical spin shader.

use super::effect_program::effect_combo_value_for_key;

pub(crate) fn spin_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 1, "spin requires authored source slot 0");
    let repeat = effect_combo_value_for_key(key, "REPEAT", 1) != 0;
    let repeat_statement = if repeat {
        "    rotated_uv = fract(rotated_uv);\n"
    } else {
        ""
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform SpinUniform {{
    vec4 g_TimeSpeedRatioAxis;
    vec4 g_PhaseCenter;
    vec4 g_SizeFeather;
}} u_Effect;
vec2 rotate2(vec2 value, float angle) {{
    float cosine = cos(angle);
    float sine = sin(angle);
    return vec2(
        value.x * cosine - value.y * sine,
        value.x * sine + value.y * cosine);
}}
void main() {{
    vec2 source_size = vec2(textureSize(g_Texture0, 0));
    float aspect = source_size.x / max(source_size.y, 1.0);
    vec2 center = u_Effect.g_PhaseCenter.yz;
    float ratio = max(u_Effect.g_TimeSpeedRatioAxis.z, 0.000001);
    float axis = u_Effect.g_TimeSpeedRatioAxis.w;
    vec2 local = v_TexCoord - center;
    local.x *= aspect;
    local = rotate2(local, axis);
    local.x *= ratio;
    vec2 soft_mask = rotate2(local, -axis) + center;
    float phase = u_Effect.g_PhaseCenter.x * 6.28318530718;
    local = rotate2(
        local,
        u_Effect.g_TimeSpeedRatioAxis.x * u_Effect.g_TimeSpeedRatioAxis.y + phase);
    local.x /= ratio;
    local = rotate2(local, -axis);
    local.x /= aspect;
    vec2 rotated_uv = local + center;
{repeat_statement}    vec4 rotated = texture(g_Texture0, rotated_uv);
    vec2 mask_delta = soft_mask - center;
    float size = u_Effect.g_SizeFeather.x;
    float feather = u_Effect.g_SizeFeather.y;
    float mask = smoothstep(
        size + feather + 0.00001,
        size - feather,
        length(mask_delta));
    o_Color = mix(texture(g_Texture0, v_TexCoord), rotated, mask);
}}
"#
    )
}
