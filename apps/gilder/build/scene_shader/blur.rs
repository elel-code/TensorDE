use super::effect_program::effect_combo_value_for_key;

pub(super) fn blur_downsample4_fragment_source(texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
void main() {
    vec2 offset = 1.0 / vec2(textureSize(g_Texture0, 0));
    vec4 samples[4] = vec4[](
        texture(g_Texture0, v_TexCoord - offset),
        texture(g_Texture0, v_TexCoord + vec2(offset.x, -offset.y)),
        texture(g_Texture0, v_TexCoord + vec2(-offset.x, offset.y)),
        texture(g_Texture0, v_TexCoord + offset));
    float alpha_weight = 0.0;
    vec4 result = vec4(0.0);
    for (int index = 0; index < 4; index++) {
        result += samples[index] * samples[index].a;
        alpha_weight += samples[index].a;
    }
    o_Color = vec4(
        result.rgb / max(0.001, alpha_weight),
        result.a * 0.25);
}
"#
    .to_owned()
}

pub(super) fn blur_gaussian_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    assert_eq!(effect_combo_value_for_key(key, "KERNEL", 0), 0);
    let axis = if effect_combo_value_for_key(key, "VERTICAL", 0) != 0 {
        "vec2(0.0, u_Effect.g_Scale.y / source_size.y)"
    } else {
        "vec2(u_Effect.g_Scale.x / source_size.x, 0.0)"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform BlurGaussianUniform {{
    vec4 g_Scale;
}} u_Effect;
void main() {{
    vec2 source_size = vec2(textureSize(g_Texture0, 0));
    vec2 offset = {axis};
    vec4 albedo =
        texture(g_Texture0, v_TexCoord - offset * 6.0) * 0.006299
        + texture(g_Texture0, v_TexCoord - offset * 5.0) * 0.017298
        + texture(g_Texture0, v_TexCoord - offset * 4.0) * 0.039533
        + texture(g_Texture0, v_TexCoord - offset * 3.0) * 0.075189
        + texture(g_Texture0, v_TexCoord - offset * 2.0) * 0.119007
        + texture(g_Texture0, v_TexCoord - offset) * 0.156756
        + texture(g_Texture0, v_TexCoord) * 0.171834
        + texture(g_Texture0, v_TexCoord + offset) * 0.156756
        + texture(g_Texture0, v_TexCoord + offset * 2.0) * 0.119007
        + texture(g_Texture0, v_TexCoord + offset * 3.0) * 0.075189
        + texture(g_Texture0, v_TexCoord + offset * 4.0) * 0.039533
        + texture(g_Texture0, v_TexCoord + offset * 5.0) * 0.017298
        + texture(g_Texture0, v_TexCoord + offset * 6.0) * 0.006299;
    o_Color = albedo;
}}
"#
    )
}

pub(super) fn blur_combine_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x5);
    let composite = effect_combo_value_for_key(key, "COMPOSITE", 0);
    let blend_mode = effect_combo_value_for_key(key, "BLENDMODE", 0);
    assert_eq!(effect_combo_value_for_key(key, "COMPOSITEMONO", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "BLURALPHA", 1), 1);
    let composite_expression = match (composite, blend_mode) {
        (0, 0) => "    o_Color = effect;",
        (1, 1) => {
            r#"    float opacity = effect.a * u_Effect.g_CompositeAlphaOffset.x;
    effect.rgb = mix(original.rgb, min(original.rgb, effect.rgb), opacity);
    effect.a = max(
        effect.a * clamp(u_Effect.g_CompositeAlphaOffset.x, 0.0, 1.0),
        original.a);
    o_Color = effect;"#
        }
        (1, 2) => {
            r#"    float opacity = effect.a * u_Effect.g_CompositeAlphaOffset.x;
    effect.rgb = mix(original.rgb, original.rgb * effect.rgb, opacity);
    effect.a = max(
        effect.a * clamp(u_Effect.g_CompositeAlphaOffset.x, 0.0, 1.0),
        original.a);
    o_Color = effect;"#
        }
        (1, 5) => {
            r#"    effect.rgb = min(original.rgb, effect.rgb);
    effect.a = max(
        effect.a * clamp(u_Effect.g_CompositeAlphaOffset.x, 0.0, 1.0),
        original.a);
    o_Color = effect;"#
        }
        _ => panic!("blur combine shader {key:?} has no typed composite contract"),
    };
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;
layout(set = 0, binding = 3) uniform BlurCombineUniform {
    vec4 g_CompositeAlphaOffset;
    vec4 g_CompositeColor;
} u_Effect;
void main() {
    vec2 source_size = vec2(textureSize(g_Texture0, 0));
    vec2 blurred_uv = v_TexCoord + u_Effect.g_CompositeAlphaOffset.yz / source_size;
    vec4 blurred = texture(g_Texture0, blurred_uv);
    vec4 original = texture(g_Texture2, v_TexCoord);
    float divisor = mix(blurred.a, 1.0, step(blurred.a, 0.0));
    vec4 effect = vec4(
        blurred.rgb / divisor * u_Effect.g_CompositeColor.rgb,
        blurred.a);
__COMPOSITE_EXPRESSION__
}
"#
    .replace("__COMPOSITE_EXPRESSION__", composite_expression)
}
