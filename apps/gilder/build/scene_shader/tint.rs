//! Typed WE tint shader variants.

use super::effect_program::effect_combo_value_for_key;

pub(crate) fn tint_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 1, "tint requires authored source slot 0");
    let blend_mode = effect_combo_value_for_key(key, "BLENDMODE", 30);
    let blend_expression = match blend_mode {
        0 => "mix(albedo.rgb, u_Effect.g_AlphaColor.yzw, alpha)",
        30 => {
            "mix(albedo.rgb, vec3(max(albedo.r, max(albedo.g, albedo.b))) * u_Effect.g_AlphaColor.yzw, alpha)"
        }
        _ => panic!("tint shader {key:?} has no typed blend-mode contract"),
    };
    let alpha_expression = if blend_mode == 0 {
        "    albedo.a = 1.0;\n"
    } else {
        ""
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform TintUniform {{
    vec4 g_AlphaColor;
}} u_Effect;
void main() {{
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float alpha = clamp(u_Effect.g_AlphaColor.x, 0.0, 1.0);
    albedo.rgb = {blend_expression};
{alpha_expression}    o_Color = albedo;
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tint_and_normal_replace_are_distinct() {
        let tint = tint_fragment_source("effects/tint__SLOTS_1", 1);
        let normal = tint_fragment_source("effects/tint__SLOTS_1__BLENDMODE_0", 1);
        assert!(tint.contains("max(albedo.r"));
        assert!(!tint.contains("albedo.a = 1.0"));
        assert!(normal.contains("albedo.a = 1.0"));
    }
}
