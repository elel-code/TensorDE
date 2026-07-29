//! Typed rounded-mask shader variants.

use super::effect_program::effect_combo_value_for_key;

pub(super) fn rounded_mask_fragment_source(key: &str) -> String {
    let square = effect_combo_value_for_key(key, "B_SQUARE", 1) != 0;
    let alpha_only = effect_combo_value_for_key(key, "C_ALPHA_ONLY", 1) != 0;
    let soft = effect_combo_value_for_key(key, "SOFT", 0) != 0;
    let size_expression = if square {
        "u_Effect.g_SizeSoftnessAlpha.xy"
    } else {
        "u_Effect.g_SizeSoftnessAlpha.xy * aspect_scale"
    };
    let edge_expression = if soft {
        "float edge_softness = u_Effect.g_SizeSoftnessAlpha.z\n        / max(v_ObjectPixelExtent.z, 1.0) * 2.0;\n    float mask_alpha = smoothstep(edge_softness, 0.0, distance);"
    } else {
        "float mask_alpha = 1.0 - step(0.0, distance);"
    };
    let output_expression = if alpha_only {
        "o_Color = vec4(source.rgb, source.a * mask_alpha * u_Effect.g_SizeSoftnessAlpha.w);"
    } else {
        "float alpha = source.a * mask_alpha * u_Effect.g_SizeSoftnessAlpha.w;\n    vec3 tint = u_Effect.g_ColorRadius.rgb;\n    vec3 blended = mix(tint, source.rgb, source.a);\n    o_Color = vec4(mix(tint, blended, alpha), mask_alpha);"
    };
    rounded_mask_source(
        size_expression,
        edge_expression,
        output_expression,
        "RoundedMaskUniform",
    )
}

pub(super) fn rounded_mask_edit_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    assert_eq!(effect_combo_value_for_key(key, "B_SQUARE", 1), 0);
    assert_eq!(effect_combo_value_for_key(key, "SOFT", 0), 1);
    assert_eq!(effect_combo_value_for_key(key, "HOLLOW", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "SEDIRECTION", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "TRANSPARENCY", 4), 4);
    assert_eq!(effect_combo_value_for_key(key, "C_ALPHA_ONLY", 1), 1);
    assert_eq!(effect_combo_value_for_key(key, "INVERT", 0), 0);
    rounded_mask_source(
        "u_Effect.g_SizeSoftnessAlpha.xy * aspect_scale",
        "float edge_softness = u_Effect.g_SizeSoftnessAlpha.z\n        / max(v_ObjectPixelExtent.z, 1.0) * 2.0;\n    float mask_alpha = smoothstep(edge_softness, 0.0, distance);",
        "o_Color = vec4(source.rgb, source.a * mask_alpha * u_Effect.g_SizeSoftnessAlpha.w);",
        "RoundedMaskEditUniform",
    )
}

fn rounded_mask_source(
    size_expression: &str,
    edge_expression: &str,
    output_expression: &str,
    uniform_name: &str,
) -> String {
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec3 v_ObjectPixelExtent;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform {uniform_name} {{
    vec4 g_ColorRadius;
    vec4 g_SizeSoftnessAlpha;
    vec4 g_BorderWidth;
    vec4 g_Unused;
}} u_Effect;
float roundedBoxSdf(vec2 point, vec2 size, float radius) {{
    vec2 half_size = size * 0.5;
    float half_min = min(half_size.x, half_size.y);
    float r = clamp(radius * half_min, 0.001, half_min);
    vec2 delta = abs(point) - (half_size - r);
    return length(max(delta, 0.0)) - r;
}}
void main() {{
    vec4 source = texture(g_Texture0, v_TexCoord);
    float width_pixels = max(v_ObjectPixelExtent.x, 1.0);
    float height_pixels = max(v_ObjectPixelExtent.y, 1.0);
    vec2 aspect_scale = vec2(
        max(1.0, width_pixels / height_pixels),
        max(1.0, height_pixels / width_pixels));
    vec2 mask_uv = (v_ObjectTexCoord - 0.5) * aspect_scale + 0.5;
    vec2 mask_size = {size_expression};
    float distance = roundedBoxSdf(
        mask_uv - vec2(0.5),
        mask_size,
        u_Effect.g_ColorRadius.w);
    {edge_expression}
    {output_expression}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_variant_keeps_authored_intersect_alpha_and_inward_softness() {
        let source = rounded_mask_edit_fragment_source(
            "effects/rounded_mask_effect_edit__SLOTS_1__B_SQUARE_0__SOFT_1",
            1,
        );
        assert!(source.contains("RoundedMaskEditUniform"));
        assert!(source.contains("source.a * mask_alpha"));
        assert!(source.contains("smoothstep(edge_softness, 0.0, distance)"));
    }
}
