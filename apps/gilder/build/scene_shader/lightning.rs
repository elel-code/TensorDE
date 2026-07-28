use super::effect_program::effect_combo_value_for_key;

pub(super) fn lightning_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 3) uniform LightningUniform {
    vec4 g_TimeSpeedErraticAmount;
    vec4 g_PowerBrightness;
    vec4 g_Color;
} u_Effect;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) flat out float v_LightningIntensity;
float hash11(float p) { return fract(sin(p * 127.1 + 311.7) * 43758.5453123); }
float noise1D(float p) {
    float i = floor(p);
    float f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(hash11(i), hash11(i + 1.0), f);
}
float lightningTiming(float time, float speed, float erratic, float amount) {
    float t = time * speed;
    float gate = noise1D(t * 0.10);
    float cluster = noise1D(t * 0.35 + 3.0) * noise1D(t * 0.25 + 7.0) * gate;
    float burst = smoothstep(0.10, 0.35, cluster + erratic * 0.03);
    float dt = 0.04;
    float d1 = max(0.0, noise1D(t * 4.0) - noise1D((t - dt) * 4.0)) * 6.0;
    float d2 = max(0.0, noise1D(t * 6.0 + 5.0) - noise1D((t - dt) * 6.0 + 5.0)) * 6.0;
    float d3 = max(0.0, noise1D(t * 9.0 + 11.0) - noise1D((t - dt) * 9.0 + 11.0)) * 6.0;
    return clamp(max(d1, max(d2, d3)) * burst * amount, 0.0, 1.0);
}
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    v_TexCoord = position * 0.5 + 0.5;
    v_LightningIntensity = lightningTiming(
        u_Effect.g_TimeSpeedErraticAmount.x,
        u_Effect.g_TimeSpeedErraticAmount.y,
        u_Effect.g_TimeSpeedErraticAmount.z,
        u_Effect.g_TimeSpeedErraticAmount.w);
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_owned()
}

pub(super) fn lightning_object_mesh_vertex_source() -> String {
    lightning_vertex_source()
        .replacen(
            r#"layout(location = 0) out vec2 v_TexCoord;"#,
            r#"layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 0) out vec2 v_TexCoord;
layout(set = 0, binding = 2) uniform SceneDrawTransform {
    vec4 g_ModelViewProjectionMatrix[4];
} g_Draw;"#,
            1,
        )
        .replacen(
            r#"    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    v_TexCoord = position * 0.5 + 0.5;"#,
            r#"    v_TexCoord = a_TexCoord;
    vec4 local_position = vec4(a_Position, 0.0, 1.0);"#,
            1,
        )
        .replacen(
            "    gl_Position = vec4(position, 0.0, 1.0);",
            r#"    gl_Position = vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], local_position));"#,
            1,
        )
}

pub(super) fn lightning_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_ne!(texture_slot_mask & 1, 0);
    let blend_mode = effect_combo_value_for_key(key, "BLENDMODE", 9);
    let blended = match blend_mode {
        7 => "vec3(1.0) - (vec3(1.0) - albedo.rgb) * (vec3(1.0) - lit)",
        31 => "albedo.rgb + lit * flash",
        other => panic!("unsupported lightning BLENDMODE {other} for {key}"),
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) flat in float v_LightningIntensity;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform LightningUniform {{
    vec4 g_TimeSpeedErraticAmount;
    vec4 g_PowerBrightness;
    vec4 g_Color;
}} u_Effect;
void main() {{
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float flash = pow(v_LightningIntensity, max(u_Effect.g_PowerBrightness.x, 0.0001))
        * u_Effect.g_PowerBrightness.y;
    vec3 lit = albedo.rgb + u_Effect.g_Color.rgb * flash * 1.5;
    albedo.rgb = mix(albedo.rgb, {blended}, flash);
    o_Color = vec4(max(vec3(0.0), albedo.rgb), albedo.a);
}}
"#
    )
}
