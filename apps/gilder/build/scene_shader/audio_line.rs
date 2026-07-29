//! Authored audio-line effect.

pub(super) fn audio_line_fragment_source(texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform AudioLineUniform {
    vec4 g_ColorOpacity;
    vec4 g_AmplitudeBandEnvelopeThickness;
    vec4 g_SmoothnessVerticalOffset;
    vec4 g_Unused;
    vec4 g_Spectrum64Left[16];
    vec4 g_Spectrum64Right[16];
} u_Effect;
float spectrumLeft(int band) {
    int index = clamp(band, 0, 63);
    return u_Effect.g_Spectrum64Left[index / 4][index % 4];
}
float spectrumRight(int band) {
    int index = clamp(band, 0, 63);
    return u_Effect.g_Spectrum64Right[index / 4][index % 4];
}
float spectrum(int band) {
    return 0.5 * (spectrumLeft(band) + spectrumRight(band));
}
float mirroredAudioValue(int index, int maximum_band) {
    index = abs(index);
    if (index > maximum_band) {
        index = maximum_band - (index - maximum_band);
    }
    return spectrum(clamp(index, 0, 63));
}
float cubicSpline(float p0, float p1, float p2, float p3, float t) {
    float t2 = t * t;
    float t3 = t2 * t;
    return 0.5 * ((2.0 * p1) + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3);
}
void main() {
    ivec2 resolution = textureSize(g_Texture0, 0);
    float aspect = float(resolution.x) / max(float(resolution.y), 1.0);
    vec2 uv = v_TexCoord - 0.5;
    uv.x *= aspect;
    float x_normalized = abs(uv.x) / max(0.5 * aspect, 0.000001);
    float frequency_normalized = 1.0 - x_normalized;
    float maximum_band = clamp(
        u_Effect.g_AmplitudeBandEnvelopeThickness.y, 1.0, 63.0);
    float audio_index = frequency_normalized * maximum_band;
    int index = int(floor(audio_index));
    float t = fract(audio_index);
    int maximum_band_index = int(maximum_band);
    float raw_audio = cubicSpline(
        mirroredAudioValue(index - 1, maximum_band_index),
        mirroredAudioValue(index, maximum_band_index),
        mirroredAudioValue(index + 1, maximum_band_index),
        mirroredAudioValue(index + 2, maximum_band_index),
        t);
    raw_audio = max(0.0, raw_audio);
    float envelope = pow(
        max(0.0, 1.0 - x_normalized),
        u_Effect.g_AmplitudeBandEnvelopeThickness.z);
    float audio = raw_audio * envelope
        * u_Effect.g_AmplitudeBandEnvelopeThickness.x;
    float curve_y = u_Effect.g_SmoothnessVerticalOffset.y - audio;
    float distance_to_curve = abs(uv.y - curve_y);
    float half_thickness = u_Effect.g_AmplitudeBandEnvelopeThickness.w * 0.5;
    float line = 1.0 - smoothstep(
        half_thickness,
        half_thickness + u_Effect.g_SmoothnessVerticalOffset.x,
        distance_to_curve);
    vec4 original = texture(g_Texture0, v_TexCoord);
    float curve_alpha = line * u_Effect.g_ColorOpacity.a;
    o_Color = vec4(
        mix(original.rgb, u_Effect.g_ColorOpacity.rgb, curve_alpha),
        max(original.a, curve_alpha));
}
"#
    .to_owned()
}
