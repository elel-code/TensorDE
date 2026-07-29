//! Typed clipping-mask shader variants and their authored cover mapping.

pub(super) fn clipping_mask_vertex_source() -> String {
    r#"#version 450
layout(location = 0) out vec2 v_SourceCoord;
layout(location = 1) out vec2 v_ClipBaseCoord;
layout(location = 2) out vec2 v_ScreenCoord;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    v_SourceCoord = uv;
    v_ClipBaseCoord = uv;
    v_ScreenCoord = uv;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_owned()
}

pub(super) fn clipping_mask_object_mesh_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(set = 0, binding = 2) uniform SceneDrawTransform {
    vec4 g_ModelViewProjectionMatrix[4];
} g_Draw;
layout(location = 0) out vec2 v_SourceCoord;
layout(location = 1) out vec2 v_ClipBaseCoord;
layout(location = 2) out vec2 v_ScreenCoord;
void main() {
    vec4 local_position = vec4(a_Position.xy, 0.0, 1.0);
    vec4 projected = vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], local_position));
    v_SourceCoord = a_TexCoord;
    v_ClipBaseCoord = a_TexCoord;
    v_ScreenCoord = projected.xy / projected.w * 0.5 + 0.5;
    gl_Position = projected;
}
"#
    .to_owned()
}

pub(super) fn clipping_mask_fragment_source(texture_slot_mask: u32) -> String {
    assert!(matches!(texture_slot_mask, 0x9 | 0xb | 0xf));
    let texture1 = texture_slot_mask & 0x2 != 0;
    let texture2 = texture_slot_mask & 0x4 != 0;
    if !texture1 {
        return r#"#version 450
layout(location = 0) in vec2 v_SourceCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
void main() {
    o_Color = texture(g_Texture0, v_SourceCoord);
}
"#
        .to_owned();
    }
    let mask_declaration = if texture2 {
        "layout(set = 0, binding = 2) uniform sampler2D g_Texture2;\n"
    } else {
        ""
    };
    let mask_expression = if texture2 {
        "texture(g_Texture2, v_SourceCoord).r"
    } else {
        "1.0"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_SourceCoord;
layout(location = 1) in vec2 v_ClipBaseCoord;
layout(location = 2) in vec2 v_ScreenCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
{mask_declaration}layout(set = 0, binding = 35) uniform sampler2D g_Texture3;
layout(set = 0, binding = 3) uniform ClippingMaskUniform {{
    vec4 g_AlphaFuzzinessToleranceOffset;
    vec4 g_BaseColor;
    vec4 g_SourceResolution;
    vec4 g_ClipResolution;
}} u_Effect;
void main() {{
    vec4 albedo = texture(g_Texture0, v_SourceCoord);
    vec2 source_resolution = max(u_Effect.g_SourceResolution.xy, vec2(1.0));
    vec2 clip_resolution = max(u_Effect.g_ClipResolution.xy, vec2(1.0));
    float source_aspect = source_resolution.x / source_resolution.y;
    float clip_aspect = clip_resolution.x / clip_resolution.y;
    float horizontal = step(source_aspect, clip_aspect);
    vec2 cover_scale = mix(
        vec2(1.0, clip_aspect / source_aspect),
        vec2(source_aspect / clip_aspect, 1.0),
        horizontal);
    vec2 clip_coord = (v_ClipBaseCoord - 0.5) * cover_scale + 0.5;
    vec2 source_to_clip = source_resolution / clip_resolution;
    vec2 excess = (clip_resolution * max(source_to_clip.x, source_to_clip.y)
        - source_resolution) * 0.5 / source_resolution;
    clip_coord += excess * u_Effect.g_AlphaFuzzinessToleranceOffset.w
        * vec2(horizontal, 1.0 - horizontal) * cover_scale;
    vec2 inside = step(abs(floor(clip_coord)) + 0.001, vec2(1.0));
    vec4 clip_color = texture(g_Texture1, clip_coord);
    float mask = {mask_expression};
    float blend = mask * u_Effect.g_AlphaFuzzinessToleranceOffset.x
        * inside.x * inside.y;
    vec4 background = texture(g_Texture3, v_ScreenCoord);
    albedo.rgb = mix(background.rgb, albedo.rgb, albedo.a);
    albedo.rgb = mix(albedo.rgb, clip_color.rgb, blend * clip_color.a);
    o_Color = albedo;
}}
"#
    )
}
