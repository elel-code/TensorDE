use super::effect_program::effect_combo_value_for_key;

pub(super) fn lut_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask & 0x3, 0x3);
    let quad_size = effect_combo_value_for_key(key, "QUAD_SIZE", 16);
    assert!(quad_size == 16 || quad_size == 64);
    let clamp_source = if effect_combo_value_for_key(key, "CLAMP", 1) != 0 {
        "texture_color = clamp(texture_color, vec4(0.0), vec4(1.0));"
    } else {
        ""
    };
    let flip = if effect_combo_value_for_key(key, "LUT_FLIP_Y", 0) != 0 {
        "tex_pos_1.y = 1.0 - tex_pos_1.y; tex_pos_2.y = 1.0 - tex_pos_2.y;"
    } else {
        ""
    };
    let grid = (quad_size as f32).sqrt() as i64;
    let atlas_size = quad_size * grid;
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 3) uniform LutUniform {{
    vec4 g_MultiplyTranslucentCompensation;
}} u_Effect;
void main() {{
    vec4 texture_color = texture(g_Texture0, v_TexCoord);
    {clamp_source}
    const float cube_size = {quad_size}.0;
    const float grid_size = {grid}.0;
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
    {flip}
    vec3 lut_color = mix(
        textureLod(g_Texture1, tex_pos_1, 0.0).rgb,
        textureLod(g_Texture1, tex_pos_2, 0.0).rgb,
        fract(blue));
    float amount = u_Effect.g_MultiplyTranslucentCompensation.x
        + u_Effect.g_MultiplyTranslucentCompensation.y * (1.0 - texture_color.a);
    o_Color = vec4(mix(texture_color.rgb, lut_color, amount), texture_color.a);
}}
"#
    )
}
