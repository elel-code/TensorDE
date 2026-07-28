use super::super::*;
use super::effect_program::effect_combo_value_for_key;

pub(super) fn blend_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    let samplers = effect_sampler_declarations(texture_slot_mask);
    let repeat = effect_combo_value_for_key(key, "TRANSFORMREPEAT", 0);
    let write_alpha = effect_combo_value_for_key(key, "WRITEALPHA", 0) != 0;
    let uv_policy = match repeat {
        1 => "blend_uv = fract(blend_uv);",
        2 => "blend_uv = clamp(blend_uv, vec2(0.0), vec2(1.0));",
        _ => {
            "inside = float(all(greaterThanEqual(blend_uv, vec2(0.0))) && all(lessThanEqual(blend_uv, vec2(1.0))));"
        }
    };
    let alpha = if write_alpha {
        "albedo.a = blend_color.a * u_Effect.g_BlendAlphaAngleScale.y;"
    } else {
        ""
    };
    assert_eq!(effect_combo_value_for_key(key, "BLENDMODE", 2), 0);
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 0) out vec4 o_Color;
{samplers}layout(set = 0, binding = 3) uniform BlendUniform {{
    vec4 g_BlendAlphaAngleScale;
    vec4 g_OffsetTransformRepeat;
    vec4 g_Texture0Resolution;
    vec4 g_Texture1Resolution;
}} u_Effect;
vec2 rotateVec2(vec2 value, float angle) {{
    vec2 cs = vec2(cos(angle), sin(angle));
    return vec2(value.x * cs.x - value.y * cs.y, value.x * cs.y + value.y * cs.x);
}}
void main() {{
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    vec2 blend_uv = v_ObjectTexCoord;
    if (u_Effect.g_OffsetTransformRepeat.z > 0.5) {{
        vec2 source_size = max(u_Effect.g_Texture0Resolution.zw, vec2(1.0));
        vec2 blend_size = max(u_Effect.g_Texture1Resolution.zw, vec2(1.0));
        blend_uv -= (u_Effect.g_OffsetTransformRepeat.xy
            - (source_size - blend_size) * 0.5) / source_size;
        blend_uv -= 0.5;
        blend_uv = rotateVec2(blend_uv, u_Effect.g_BlendAlphaAngleScale.z);
        blend_uv *= source_size / blend_size
            / max(u_Effect.g_BlendAlphaAngleScale.w, 0.0001);
        blend_uv += 0.5;
    }}
    float inside = 1.0;
    {uv_policy}
    vec4 blend_color = texture(g_Texture1, blend_uv);
    float amount = inside * u_Effect.g_BlendAlphaAngleScale.x * blend_color.a;
    albedo.rgb = mix(albedo.rgb, blend_color.rgb, amount);
    {alpha}
    o_Color = albedo;
}}
"#
    )
}

pub(super) fn blend_gradient_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(effect_combo_value_for_key(key, "BLENDMODE", 0), 0);
    let samplers = effect_sampler_declarations(texture_slot_mask);
    let write_alpha = effect_combo_value_for_key(key, "WRITEALPHA", 0) != 0;
    let alpha = if write_alpha {
        "albedo.a = blend_color.a * u_Effect.g_BlendAlphaAngleScale.y;"
    } else {
        ""
    };
    let gradient = if texture_slot_mask & (1 << 2) != 0 {
        "texture(g_Texture2, blend_uv).r"
    } else {
        "0.5"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 0) out vec4 o_Color;
{samplers}layout(set = 0, binding = 3) uniform BlendGradientUniform {{
    vec4 g_BlendAlphaAngleScale;
    vec4 g_OffsetTransformRepeat;
    vec4 g_Texture0Resolution;
    vec4 g_Texture1Resolution;
    vec4 g_GradientScaleEdgeBrightness;
    vec4 g_EdgeColor;
}} u_Effect;
void main() {{
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    vec2 blend_uv = v_ObjectTexCoord;
    vec4 blend_color = texture(g_Texture1, blend_uv);
    float gradient = {gradient};
    float width = u_Effect.g_GradientScaleEdgeBrightness.x;
    float amount = smoothstep(
        clamp(gradient - width, 0.0, 1.0),
        clamp(gradient + width, 0.0, 1.0),
        u_Effect.g_BlendAlphaAngleScale.x) * blend_color.a;
    albedo.rgb = mix(albedo.rgb, blend_color.rgb, amount);
    {alpha}
    o_Color = albedo;
}}
"#
    )
}
