pub(super) fn caustics_effect_fragment_source(
    texture_slot_mask: u32,
    chromatic_zero: bool,
    pattern_glow_shared: bool,
    color_equal: bool,
) -> String {
    assert_eq!(
        texture_slot_mask & 0x3d,
        0x3d,
        "current caustics shader contract requires slots 0, 2, 3, 4, and 5"
    );
    let source = r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;
// Logical WE texture slot 3 uses binding 35 because binding 3 is the
// fragment material uniform ABI.
layout(set = 0, binding = 35) uniform sampler2D g_Texture3;
layout(set = 0, binding = 4) uniform sampler2D g_Texture4;
layout(set = 0, binding = 5) uniform sampler2D g_Texture5;
layout(set = 0, binding = 3) uniform CausticsUniform {
    vec4 g_TimeSpeedScaleBrightness;
    vec4 g_GlowDistortionChromaticBlur;
    vec4 g_ColorStart;
    vec4 g_ColorEnd;
} u_Effect;
void main() {
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float ratio = max(u_Effect.g_ColorEnd.w, 0.0001);
    vec2 causticsCoords = v_TexCoord;
    causticsCoords.x *= ratio;
    causticsCoords *= u_Effect.g_TimeSpeedScaleBrightness.z;

    vec2 noiseCoords = causticsCoords * 0.02;
    vec2 noiseCoords2 = causticsCoords * 0.0333;
    vec2 blendCoords = causticsCoords * 0.01333;
    vec2 shiftCoords = causticsCoords * 0.05;
    float time = u_Effect.g_TimeSpeedScaleBrightness.x
        * u_Effect.g_TimeSpeedScaleBrightness.y
        + u_Effect.g_ColorStart.w;
    noiseCoords.x += time * 0.005;
    noiseCoords2.y += time * 0.004111;
    blendCoords += time * 0.003777;
    shiftCoords += time * 0.01;

    vec2 shiftOffset = texture(g_Texture4, shiftCoords).ba * 2.0 - 1.0;
    vec2 noiseOffset = texture(g_Texture3, noiseCoords).ba * 2.0 - 1.0;
    vec2 noiseOffset2 = texture(g_Texture3, noiseCoords2).rg * 2.0 - 1.0;
    float distortion = u_Effect.g_GlowDistortionChromaticBlur.y;
    causticsCoords += noiseOffset * 0.025 * distortion;
    causticsCoords += noiseOffset2 * 0.025 * distortion;
    causticsCoords += shiftOffset * distortion;

    float chromatic = u_Effect.g_GlowDistortionChromaticBlur.z;
    vec2 leftCoords = causticsCoords - vec2(0.01 * chromatic, 0.0);
    vec2 rightCoords = causticsCoords + vec2(0.01 * chromatic, 0.0);
    vec3 caustics = vec3(
        texture(g_Texture2, leftCoords).r,
        texture(g_Texture2, causticsCoords).r,
        texture(g_Texture2, rightCoords).r);
    float glowSample = texture(g_Texture5, causticsCoords).r;
    vec4 blendColor = texture(g_Texture3, blendCoords);
    caustics = mix(
        caustics,
        vec3(glowSample),
        u_Effect.g_GlowDistortionChromaticBlur.w);
    float causticsSample = dot(caustics, vec3(0.33333));
    causticsSample = smoothstep(
        blendColor.x * 0.8,
        1.0 - blendColor.y * 0.2,
        causticsSample
            + glowSample * u_Effect.g_GlowDistortionChromaticBlur.x);
    vec3 causticsColor = u_Effect.g_TimeSpeedScaleBrightness.w
        * mix(u_Effect.g_ColorStart.rgb, u_Effect.g_ColorEnd.rgb, blendColor.x);
    causticsColor *= caustics;

    vec3 lightened = max(albedo.rgb, causticsColor);
    albedo.rgb = mix(albedo.rgb, lightened, clamp(causticsSample, 0.0, 1.0));
    o_Color = albedo;
}
"#
    .to_owned();
    let mut source = if chromatic_zero {
        source.replace(
            r#"    float chromatic = u_Effect.g_GlowDistortionChromaticBlur.z;
    vec2 leftCoords = causticsCoords - vec2(0.01 * chromatic, 0.0);
    vec2 rightCoords = causticsCoords + vec2(0.01 * chromatic, 0.0);
    vec3 caustics = vec3(
        texture(g_Texture2, leftCoords).r,
        texture(g_Texture2, causticsCoords).r,
        texture(g_Texture2, rightCoords).r);"#,
            r#"    float causticsPattern = texture(g_Texture2, causticsCoords).r;
    vec3 caustics = vec3(causticsPattern);"#,
        )
    } else {
        source
    };
    if pattern_glow_shared {
        assert!(
            chromatic_zero,
            "shared caustics pattern/glow requires the chromatic-zero variant"
        );
        // glowSample := causticsPattern and caustics is a broadcast of that
        // pattern, so mix(caustics, glow, blur) is algebraically caustics for
        // any blur weight. Drop the dead mix; keep glow for the smoothstep term.
        source = source
            .replace(
                "    float glowSample = texture(g_Texture5, causticsCoords).r;",
                "    float glowSample = causticsPattern;",
            )
            .replace(
                r#"    caustics = mix(
        caustics,
        vec3(glowSample),
        u_Effect.g_GlowDistortionChromaticBlur.w);
"#,
                "",
            );
    }
    if color_equal {
        // Static equal color ramps: mix(start, end, t) == start for all t.
        source = source.replace(
            r#"    vec3 causticsColor = u_Effect.g_TimeSpeedScaleBrightness.w
        * mix(u_Effect.g_ColorStart.rgb, u_Effect.g_ColorEnd.rgb, blendColor.x);
    causticsColor *= caustics;"#,
            r#"    vec3 causticsColor = u_Effect.g_TimeSpeedScaleBrightness.w
        * u_Effect.g_ColorStart.rgb
        * caustics;"#,
        );
    }
    source
}
