//! Authored user-texture alpha blend used by launcher tiles.

use super::effect_program::effect_combo_value_for_key;

pub(super) fn custom_user_texture_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x3);
    assert_eq!(effect_combo_value_for_key(key, "WRITEALPHA", 0), 1);
    assert_eq!(effect_combo_value_for_key(key, "NUMBLENDTEXTURES", 1), 1);
    assert_eq!(effect_combo_value_for_key(key, "TRANSFORMUV", 0), 0);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 3) uniform CustomUserTextureUniform {
    vec4 g_Multiply;
} u_Effect;
void main() {
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    vec4 blend_color = texture(g_Texture1, v_TexCoord);
    float blend_alpha = u_Effect.g_Multiply.x;
    float new_alpha = albedo.a * (1.0 - blend_alpha)
        + blend_color.a * blend_alpha;
    vec4 input_color = albedo;
    albedo.rgb = albedo.rgb * albedo.a * (1.0 - blend_alpha)
        + blend_color.rgb * blend_color.a * blend_alpha;
    vec3 source_rgb = mix(
        blend_color.rgb,
        input_color.rgb,
        step(0.01, input_color.a) * (1.0 - blend_color.a * blend_alpha));
    vec3 destination_rgb = mix(
        input_color.rgb,
        blend_color.rgb,
        step(0.01, blend_color.a * (1.0 - input_color.a * (1.0 - blend_alpha))));
    albedo.rgb += mix(source_rgb, destination_rgb, blend_alpha) * (1.0 - new_alpha);
    albedo.a = new_alpha;
    o_Color = albedo;
}
"#
    .to_owned()
}
