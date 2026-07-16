pub(super) fn swing_fragment_source(texture_slot_mask: u32) -> String {
    assert_ne!(texture_slot_mask & 1, 0);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform SwingUniform {
    vec4 g_TimeAmountSpeedPhase;
    vec4 g_SizeCenterFeather;
    vec4 g_Point0Point1;
    vec4 g_Texture0Resolution;
} u_Effect;
void main() {
    vec2 tex_coord = v_TexCoord;
    float aspect = u_Effect.g_Texture0Resolution.x
        / max(u_Effect.g_Texture0Resolution.y, 1.0);
    vec2 p0 = u_Effect.g_Point0Point1.xy * vec2(aspect, 1.0);
    vec2 p1 = u_Effect.g_Point0Point1.zw * vec2(aspect, 1.0);
    tex_coord.x *= aspect;
    vec2 axis = normalize(p1 - p0);
    vec2 center = mix(p0, p1, u_Effect.g_SizeCenterFeather.y);
    vec2 axis_ortho = vec2(-axis.y, axis.x);
    vec2 delta = tex_coord - center;
    float along = dot(axis, delta);
    float ortho = dot(axis_ortho, delta);
    float anim = sin(
        u_Effect.g_TimeAmountSpeedPhase.x * u_Effect.g_TimeAmountSpeedPhase.z
        + u_Effect.g_TimeAmountSpeedPhase.w * 6.28318530718)
        * u_Effect.g_TimeAmountSpeedPhase.y;
    tex_coord += axis * anim * ortho * along;
    tex_coord += axis_ortho * anim * anim * ortho;
    float feather = max(u_Effect.g_SizeCenterFeather.z, 0.00001);
    float mask = smoothstep(feather, 0.0, dot(tex_coord - p1, axis));
    mask *= smoothstep(-feather, 0.0, dot(tex_coord - p0, axis));
    float size = u_Effect.g_SizeCenterFeather.x
        * (1.0 - abs(anim) * u_Effect.g_TimeAmountSpeedPhase.y * 0.5);
    mask *= smoothstep(size + feather, size - feather, ortho);
    mask *= step(0.0, ortho);
    tex_coord.x /= aspect;
    o_Color = texture(g_Texture0, mix(v_TexCoord, tex_coord, mask));
}
"#
    .to_owned()
}
