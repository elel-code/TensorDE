use super::effect_program::effect_combo_value_for_key;

pub(super) fn oscilloscope_vertex_source() -> String {
    [
        oscilloscope_uniform_source(),
        r#"
layout(set = 0, binding = 2) uniform OscilloscopeDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(location = 0) out vec4 v_AudioValue0;
layout(location = 1) out vec4 v_AudioValue1;
layout(location = 2) out vec4 v_AudioValue2;
layout(location = 3) out vec4 v_AudioValue3;
layout(location = 4) out vec2 v_TexCoord;
layout(location = 5) out vec3 v_PerspCoord;
layout(location = 6) out vec3 v_ViewCoord;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    vec2 screen_uv = vec2(
        dot(u_Draw.g_ObjectUvToScreenUvRow0.xyz, vec3(uv, 1.0)),
        dot(u_Draw.g_ObjectUvToScreenUvRow1.xyz, vec3(uv, 1.0)));
    vec4 exponent = vec4(u_Effect.g_OffsetAngleAmplitudeExponent.z + 0.01);
    v_AudioValue0 = pow(u_Effect.g_Spectrum[0] * 2.0, exponent);
    v_AudioValue1 = pow(u_Effect.g_Spectrum[1] * 2.0, exponent);
    v_AudioValue2 = pow(u_Effect.g_Spectrum[2] * 2.0, exponent);
    v_AudioValue3 = pow(u_Effect.g_Spectrum[3] * 2.0, exponent);
    v_TexCoord = uv;
    v_PerspCoord = vec3(uv, 1.0);
    v_ViewCoord = vec3(screen_uv * 2.0 - 1.0, 1.0);
    gl_Position = vec4(position, 0.0, 1.0);
}
"#,
    ]
    .concat()
}

pub(super) fn oscilloscope_object_mesh_vertex_source() -> String {
    [
        oscilloscope_uniform_source(),
        r#"
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(set = 0, binding = 2) uniform OscilloscopeDrawUniform {
    vec4 g_ModelViewProjectionMatrix[4];
} u_Draw;
layout(location = 0) out vec4 v_AudioValue0;
layout(location = 1) out vec4 v_AudioValue1;
layout(location = 2) out vec4 v_AudioValue2;
layout(location = 3) out vec4 v_AudioValue3;
layout(location = 4) out vec2 v_TexCoord;
layout(location = 5) out vec3 v_PerspCoord;
layout(location = 6) out vec3 v_ViewCoord;
void main() {
    vec4 local_position = vec4(a_Position, 0.0, 1.0);
    vec4 projected = vec4(
        dot(u_Draw.g_ModelViewProjectionMatrix[0], local_position),
        dot(u_Draw.g_ModelViewProjectionMatrix[1], local_position),
        dot(u_Draw.g_ModelViewProjectionMatrix[2], local_position),
        dot(u_Draw.g_ModelViewProjectionMatrix[3], local_position));
    vec4 exponent = vec4(u_Effect.g_OffsetAngleAmplitudeExponent.z + 0.01);
    v_AudioValue0 = pow(u_Effect.g_Spectrum[0] * 2.0, exponent);
    v_AudioValue1 = pow(u_Effect.g_Spectrum[1] * 2.0, exponent);
    v_AudioValue2 = pow(u_Effect.g_Spectrum[2] * 2.0, exponent);
    v_AudioValue3 = pow(u_Effect.g_Spectrum[3] * 2.0, exponent);
    v_TexCoord = a_TexCoord;
    v_PerspCoord = vec3(a_TexCoord, 1.0);
    v_ViewCoord = vec3(projected.xy, projected.w);
    gl_Position = projected;
}
"#,
    ]
    .concat()
}

pub(super) fn oscilloscope_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask & 0x5, 0x5);
    assert_eq!(effect_combo_value_for_key(key, "RESOLUTION", 32), 16);
    [
        oscilloscope_uniform_source(),
        r#"
layout(location = 0) in vec4 v_AudioValue0;
layout(location = 1) in vec4 v_AudioValue1;
layout(location = 2) in vec4 v_AudioValue2;
layout(location = 3) in vec4 v_AudioValue3;
layout(location = 4) in vec2 v_TexCoord;
layout(location = 5) in vec3 v_PerspCoord;
layout(location = 6) in vec3 v_ViewCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;
float audioValue(int band) {
    int vector_index = band / 4;
    int lane = band % 4;
    if (vector_index == 0) {
        return v_AudioValue0[lane];
    }
    if (vector_index == 1) {
        return v_AudioValue1[lane];
    }
    if (vector_index == 2) {
        return v_AudioValue2[lane];
    }
    return v_AudioValue3[lane];
}
void main() {
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float opacity = u_Effect.g_ColorOpacity.a;
    if (opacity > 0.001) {
        vec2 ratio = vec2(
            u_Effect.g_Texture0Resolution.x
                / u_Effect.g_Texture0Resolution.y,
            1.0);
        float angle = u_Effect.g_OffsetAngleAmplitudeExponent.y;
        vec2 rotation = vec2(sin(angle), cos(angle));
        vec2 position =
            (u_Effect.g_BrightnessAmplitudeHeightThickness.z - 0.5)
                * rotation
                + 0.5;
        vec2 persp_coord = v_PerspCoord.xy / max(0.001, v_PerspCoord.z);
        vec2 coord = (persp_coord - position) * ratio;
        coord = vec2(
            coord.x * rotation.y - coord.y * rotation.x,
            coord.x * rotation.x + coord.y * rotation.y);
        float x = coord.x
            + u_Effect.g_OffsetAngleAmplitudeExponent.x * 3.14159265359;
        float scope = round(u_Effect.g_SmoothnessFrequencyScopeFlow.z * 2.0);
        float value = 0.0;
        for (int i = 0; i < 16; ++i) {
            float amplitude = audioValue(i);
            float frequency = exp(
                float(i) * u_Effect.g_SmoothnessFrequencyScopeFlow.y / 16.0);
            float flow = u_Effect.g_SmoothnessFrequencyScopeFlow.w * amplitude;
            value += sin((x + float(i) + flow) * frequency * scope)
                * amplitude;
        }
        value *= u_Effect.g_BrightnessAmplitudeHeightThickness.y * 0.03125;
        float distance_to_wave = abs(coord.y + value);
        float outside = step(
            0.5,
            max(abs(persp_coord.x - 0.5), abs(persp_coord.y - 0.5)));
        distance_to_wave += outside * 1.0e9;
        float thickness = u_Effect.g_BrightnessAmplitudeHeightThickness.w
            * 0.015;
        float coverage = clamp(
            (thickness - distance_to_wave) / thickness,
            0.0,
            1.0);
        coverage = coverage * coverage * (3.0 - 2.0 * coverage);
        float smoothed = 1.0 - exp(
            -coverage / max(1.0e-6, u_Effect.g_SmoothnessFrequencyScopeFlow.x));
        vec3 framebuffer_coord = v_ViewCoord / v_ViewCoord.z;
        vec3 background = texture(
            g_Texture2,
            framebuffer_coord.xy * 0.5 + 0.5).rgb;
        albedo.rgb = mix(background, albedo.rgb, albedo.a);
        vec3 wave_color = u_Effect.g_ColorOpacity.rgb
            * u_Effect.g_BrightnessAmplitudeHeightThickness.x;
        albedo.rgb = mix(albedo.rgb, wave_color, opacity * smoothed);
        float blended_alpha = clamp(albedo.a + smoothed, 0.0, 1.0);
        albedo.a = mix(
            albedo.a,
            blended_alpha,
            opacity * step(0.0, v_PerspCoord.z));
    }
    o_Color = albedo;
}
"#,
    ]
    .concat()
}

fn oscilloscope_uniform_source() -> &'static str {
    r#"#version 450
layout(set = 0, binding = 3) uniform OscilloscopeUniform {
    vec4 g_ColorOpacity;
    vec4 g_BrightnessAmplitudeHeightThickness;
    vec4 g_SmoothnessFrequencyScopeFlow;
    vec4 g_OffsetAngleAmplitudeExponent;
    vec4 g_Spectrum[4];
    vec4 g_Texture0Resolution;
} u_Effect;
"#
}
