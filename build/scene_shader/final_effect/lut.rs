pub(super) fn framebuffer_lut_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 2) uniform FramebufferLutDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(location = 0) out vec2 v_FramebufferCoord;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    vec2 screen_uv = position * 0.5 + 0.5;
    vec2 object_uv = vec2(
        dot(u_Draw.g_ScreenUvToObjectUvRow0.xyz, vec3(screen_uv, 1.0)),
        dot(u_Draw.g_ScreenUvToObjectUvRow1.xyz, vec3(screen_uv, 1.0)));
    v_FramebufferCoord = vec2(
        dot(u_Draw.g_ObjectUvToScreenUvRow0.xyz, vec3(object_uv, 1.0)),
        dot(u_Draw.g_ObjectUvToScreenUvRow1.xyz, vec3(object_uv, 1.0)));
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_owned()
}

pub(super) fn framebuffer_lut_fragment_source(cube_size: u32) -> String {
    assert!(cube_size == 16 || cube_size == 64);
    let grid_size = (cube_size as f32).sqrt() as u32;
    let atlas_size = cube_size * grid_size;
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_FramebufferCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SceneSnapshot;
layout(set = 0, binding = 1) uniform sampler2D g_Lut;
layout(set = 0, binding = 3) uniform FinalFramebufferLutProgram {{
    vec4 g_ResolvedColorAlpha;
    vec4 g_MultiplyTranslucentClampFlip;
}} u_Effect;
void main() {{
    vec4 texture_color = texture(g_SceneSnapshot, v_FramebufferCoord);
    if (u_Effect.g_MultiplyTranslucentClampFlip.z > 0.5) {{
        texture_color = clamp(texture_color, vec4(0.0), vec4(1.0));
    }}
    const float cube_size = {cube_size}.0;
    const float grid_size = {grid_size}.0;
    const float atlas_size = {atlas_size}.0;
    float blue = texture_color.b * (cube_size - 1.0);
    float slice_1 = floor(blue);
    float slice_2 = ceil(blue);
    vec2 cell_1 = vec2(mod(slice_1, grid_size), floor(slice_1 / grid_size));
    vec2 cell_2 = vec2(mod(slice_2, grid_size), floor(slice_2 / grid_size));
    vec2 inset = vec2(0.5 / atlas_size);
    vec2 span = vec2((cube_size - 1.0) / atlas_size);
    vec2 tex_pos_1 = cell_1 / grid_size + inset + span * texture_color.rg;
    vec2 tex_pos_2 = cell_2 / grid_size + inset + span * texture_color.rg;
    if (u_Effect.g_MultiplyTranslucentClampFlip.w > 0.5) {{
        tex_pos_1.y = 1.0 - tex_pos_1.y;
        tex_pos_2.y = 1.0 - tex_pos_2.y;
    }}
    vec3 lut_color = mix(
        textureLod(g_Lut, tex_pos_1, 0.0).rgb,
        textureLod(g_Lut, tex_pos_2, 0.0).rgb,
        fract(blue));
    float amount = u_Effect.g_MultiplyTranslucentClampFlip.x
        + u_Effect.g_MultiplyTranslucentClampFlip.y * (1.0 - texture_color.a);
    vec4 color = vec4(
        mix(texture_color.rgb, lut_color, amount),
        texture_color.a);
    o_Color = color * u_Effect.g_ResolvedColorAlpha;
}}
"#
    )
}
