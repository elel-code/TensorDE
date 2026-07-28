pub(super) fn audio_bars_fragment_source(key: &str) -> String {
    let shape = super::effect_combo_value_for_key(key, "SHAPE", 0);
    assert_eq!(shape, 7, "current audio-bars variant must be CENTER_V");
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform AudioBarsUniform {
    vec4 g_ColorOpacity;
    vec4 g_CountSpacingBounds;
    vec4 g_MinHeightRadiusVolumeAaX;
    vec4 g_AaY;
    vec4 g_SpectrumLeft[8];
    vec4 g_SpectrumRight[8];
} u_Effect;
float spectrumLeft(int band) {
    int index = clamp(band, 0, 31);
    return u_Effect.g_SpectrumLeft[index / 4][index % 4];
}
float spectrumRight(int band) {
    int index = clamp(band, 0, 31);
    return u_Effect.g_SpectrumRight[index / 4][index % 4];
}
float roundedBoxSdf(vec2 point, vec2 half_size, float radius) {
    float r = clamp(radius, 0.0, min(half_size.x, half_size.y));
    vec2 delta = abs(point) - (half_size - r);
    return length(max(delta, 0.0)) - r;
}
void main() {
    if (any(lessThan(v_ObjectTexCoord, vec2(0.0)))
        || any(greaterThan(v_ObjectTexCoord, vec2(1.0)))) {
        o_Color = vec4(0.0);
        return;
    }
    float count = max(u_Effect.g_CountSpacingBounds.x, 1.0);
    float cell_width = 1.0 / count;
    float bar_width = cell_width
        * clamp(1.0 - u_Effect.g_CountSpacingBounds.y, 0.0, 1.0);
    float minimum_height = u_Effect.g_MinHeightRadiusVolumeAaX.x * bar_width;
    float lower_bound = u_Effect.g_CountSpacingBounds.z;
    float upper_bound = u_Effect.g_CountSpacingBounds.w;
    float frequency = floor(v_ObjectTexCoord.x * count) / count * 32.0;
    int frequency0 = int(mod(frequency, 32.0));
    int frequency1 = (frequency0 + 1) % 32;
    float frequency_blend = smoothstep(0.0, 1.0, fract(frequency));
    float left = mix(spectrumLeft(frequency0), spectrumLeft(frequency1), frequency_blend);
    float right = mix(spectrumRight(frequency0), spectrumRight(frequency1), frequency_blend);
    float volume = mix(left, right, step(0.5, v_ObjectTexCoord.y))
        * u_Effect.g_MinHeightRadiusVolumeAaX.z;
    float half_height = 0.5 * mix(
        max(lower_bound, minimum_height) * 2.0,
        upper_bound,
        volume);
    vec2 gradient_x = vec2(dFdx(v_ObjectTexCoord.x), dFdy(v_ObjectTexCoord.x));
    vec2 gradient_y = vec2(dFdx(v_ObjectTexCoord.y), dFdy(v_ObjectTexCoord.y));
    float width_pixels = 1.0 / max(length(gradient_x), 0.000001);
    float height_pixels = 1.0 / max(length(gradient_y), 0.000001);
    float x_correction = width_pixels / max(height_pixels, 0.000001);
    float cell_position = fract(v_ObjectTexCoord.x * count) - 0.5;
    vec2 point = vec2(
        cell_position * cell_width * x_correction,
        v_ObjectTexCoord.y - 0.5);
    vec2 half_size = vec2(bar_width * 0.5 * x_correction, half_height);
    float radius = u_Effect.g_MinHeightRadiusVolumeAaX.y
        * min(half_size.x, half_size.y);
    float distance = roundedBoxSdf(point, half_size, radius);
    float authored_aa = max(
        u_Effect.g_MinHeightRadiusVolumeAaX.w,
        u_Effect.g_AaY.x) * 15.0
        / float(max(textureSize(g_Texture0, 0).x, textureSize(g_Texture0, 0).y));
    float antialias = max(fwidth(distance), authored_aa);
    float bar = 1.0 - smoothstep(-antialias, antialias, distance);
    vec4 scene = texture(g_Texture0, v_TexCoord);
    float opacity = bar * u_Effect.g_ColorOpacity.a;
    vec3 base = mix(u_Effect.g_ColorOpacity.rgb, scene.rgb, scene.a);
    vec3 final_color = mix(base, u_Effect.g_ColorOpacity.rgb, opacity);
    o_Color = vec4(final_color, opacity);
}
"#
    .to_owned()
}
