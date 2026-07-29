//! Authored two-color gradient effect.

use super::effect_program::effect_combo_value_for_key;

pub(super) fn gradient_color_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    assert_eq!(effect_combo_value_for_key(key, "AXIS", 0), 1);
    assert_eq!(effect_combo_value_for_key(key, "BLENDMODE", 31), 0);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform GradientColorUniform {
    vec4 g_TimeAmountSpeedOscillate;
    vec4 g_Color1Opacity;
    vec4 g_Color2;
} u_Effect;
vec3 rgbToHsv(vec3 c) {
    vec4 k = vec4(0.0, -0.3333333333, 0.6666666667, -1.0);
    vec4 p = mix(vec4(c.bg, k.wz), vec4(c.gb, k.xy), step(c.b, c.g));
    vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));
    float d = q.x - min(q.w, q.y);
    float e = 1.0e-10;
    return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}
vec3 hsvToRgb(vec3 c) {
    vec3 p = abs(fract(c.xxx + vec3(0.0, 0.6666666667, 0.3333333333)) * 6.0 - 3.0);
    return c.z * mix(vec3(1.0), clamp(p - 1.0, 0.0, 1.0), c.y);
}
void main() {
    vec4 scene = texture(g_Texture0, v_TexCoord);
    float timer = sin(u_Effect.g_TimeAmountSpeedOscillate.x
        * u_Effect.g_TimeAmountSpeedOscillate.w);
    float distance_blend = pow(
        v_TexCoord.y,
        u_Effect.g_TimeAmountSpeedOscillate.y) + timer;
    vec3 result = mix(
        u_Effect.g_Color1Opacity.rgb,
        u_Effect.g_Color2.rgb,
        distance_blend);
    vec3 hsv = rgbToHsv(result);
    hsv.x = fract(hsv.x + u_Effect.g_TimeAmountSpeedOscillate.x
        * u_Effect.g_TimeAmountSpeedOscillate.z);
    result = hsvToRgb(hsv);
    vec3 base = mix(result, scene.rgb, scene.a);
    vec3 final_color = mix(base, result, u_Effect.g_Color1Opacity.a);
    o_Color = vec4(final_color, scene.a);
}
"#
    .to_owned()
}
