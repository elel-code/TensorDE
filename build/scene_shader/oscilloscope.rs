use super::effect_program::effect_combo_value_for_key;

pub(super) fn oscilloscope_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_ne!(texture_slot_mask & 1, 0);
    assert_eq!(effect_combo_value_for_key(key, "RESOLUTION", 32), 16);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform OscilloscopeUniform {
    vec4 g_ColorOpacity;
    vec4 g_BrightnessAmplitudeHeightThickness;
    vec4 g_SmoothnessFrequencyScopeFlow;
    vec4 g_OffsetAngleAmplitudeExponent;
    vec4 g_Spectrum[4];
} u_Effect;
float spectrum(int band) {
    int index = clamp(band, 0, 15);
    return u_Effect.g_Spectrum[index / 4][index % 4];
}
void main() {
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float angle = u_Effect.g_OffsetAngleAmplitudeExponent.y;
    vec2 direction = vec2(sin(angle), cos(angle));
    vec2 position = (u_Effect.g_BrightnessAmplitudeHeightThickness.z - 0.5)
        * direction + 0.5;
    vec2 coord = v_ObjectTexCoord - position;
    coord = vec2(
        coord.x * direction.y - coord.y * direction.x,
        coord.x * direction.x + coord.y * direction.y);
    float x = coord.x + u_Effect.g_OffsetAngleAmplitudeExponent.x * 3.14159265359;
    float value = 0.0;
    for (int i = 0; i < 16; ++i) {
        float amplitude = pow(
            max(spectrum(i) * 2.0, 0.0),
            u_Effect.g_OffsetAngleAmplitudeExponent.z + 0.01);
        float frequency = exp(
            float(i) * u_Effect.g_SmoothnessFrequencyScopeFlow.y / 16.0);
        float flow = u_Effect.g_SmoothnessFrequencyScopeFlow.w * amplitude;
        value += sin((x + float(i) + flow) * frequency
            * u_Effect.g_SmoothnessFrequencyScopeFlow.z) * amplitude;
    }
    value *= u_Effect.g_BrightnessAmplitudeHeightThickness.y * 0.03125;
    float distance_to_wave = abs(coord.y + value);
    float width = u_Effect.g_BrightnessAmplitudeHeightThickness.w * 0.015;
    float aa = fwidth(distance_to_wave);
    float coverage = 1.0 - smoothstep(max(width - aa, 0.0), width + aa, distance_to_wave);
    float opacity = coverage * u_Effect.g_ColorOpacity.a;
    vec3 wave_color = u_Effect.g_ColorOpacity.rgb
        * u_Effect.g_BrightnessAmplitudeHeightThickness.x;
    albedo.rgb = mix(albedo.rgb, wave_color, opacity);
    albedo.a = mix(albedo.a, clamp(albedo.a + coverage, 0.0, 1.0), opacity);
    o_Color = albedo;
}
"#
    .to_owned()
}
