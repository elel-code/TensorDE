use super::effect_program::effect_combo_value_for_key;

pub(super) fn procedural_noise_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    assert_eq!(effect_combo_value_for_key(key, "AA_CATEGORY", 0), 1);
    assert_eq!(effect_combo_value_for_key(key, "AB_TYPEUV", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "BLENDMODE", 0), 20);
    assert_eq!(effect_combo_value_for_key(key, "STEPANIM", 0), 1);
    assert_eq!(effect_combo_value_for_key(key, "PERSPSWITCH", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "TILE", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "LAYERED", 0), 0);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ProceduralNoiseUniform {
    vec4 g_TimeSpeedDirectionDirectionSpeed;
    vec4 g_OffsetScale;
    vec4 g_MagnitudeSeedFps;
    vec4 g_Opacity;
} u_Effect;
vec2 hash23(vec3 p) {
    p = fract(p * vec3(0.1031, 0.1030, 437.195));
    p += dot(p, p.yzx + 19.19);
    return fract((p.xx + p.yz) * p.zy);
}
void main() {
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float opacity = u_Effect.g_Opacity.x;
    if (opacity <= 0.001) {
        o_Color = albedo;
        return;
    }
    vec2 resolution = vec2(textureSize(g_Texture0, 0));
    vec2 aspect = vec2(1.0, resolution.y / resolution.x);
    vec2 scale = max(
        vec2(0.000001),
        u_Effect.g_OffsetScale.zw * resolution / 3.0);
    float fps = u_Effect.g_MagnitudeSeedFps.w;
    float animate = floor(u_Effect.g_TimeSpeedDirectionDirectionSpeed.x * fps)
        / max(0.000001, fps)
        * u_Effect.g_TimeSpeedDirectionDirectionSpeed.y * 0.01
        + u_Effect.g_MagnitudeSeedFps.z;
    float direction = u_Effect.g_TimeSpeedDirectionDirectionSpeed.z;
    vec2 scroll = vec2(-sin(direction), cos(direction))
        * u_Effect.g_TimeSpeedDirectionDirectionSpeed.x
        * u_Effect.g_TimeSpeedDirectionDirectionSpeed.w * 10.0;
    vec2 transformed_offset = (u_Effect.g_OffsetScale.xy + scroll) * aspect * scale;
    vec2 coord = v_TexCoord * scale + transformed_offset;
    vec2 magnitude = u_Effect.g_MagnitudeSeedFps.xy * 0.05;
    vec2 noise_offset = (hash23(vec3(floor(coord * 0.5), animate)) - 0.5)
        * magnitude;
    vec4 displaced = texture(g_Texture0, v_TexCoord + noise_offset);
    vec3 subtract = max(albedo.rgb + displaced.rgb - vec3(1.0), vec3(0.0));
    o_Color = vec4(mix(albedo.rgb, subtract, opacity), albedo.a);
}
"#
    .to_owned()
}
