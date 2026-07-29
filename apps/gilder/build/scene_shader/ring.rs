//! Authored partial rounded ring effect (`effects/huan`).

pub(super) fn ring_fragment_source(texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 3) uniform RingUniform {
    vec4 g_ColorRingSize;
    vec4 g_Color2RingWidth;
    vec4 g_GradientRotationAaGap;
    vec4 g_CornerRadiusOpacity;
} u_Effect;
void main() {
    const float pi = 3.14159265359;
    vec2 uv = v_TexCoord - 0.5;
    float radius = length(uv);
    float angle = atan(uv.y, uv.x);
    float inner_radius = u_Effect.g_ColorRingSize.w - u_Effect.g_Color2RingWidth.w;
    float outer_radius = u_Effect.g_ColorRingSize.w + u_Effect.g_Color2RingWidth.w;
    float antialias = u_Effect.g_GradientRotationAaGap.y;
    float ring = smoothstep(inner_radius, inner_radius + antialias, radius)
        * (1.0 - smoothstep(outer_radius - antialias, outer_radius, radius));
    float display_angle = u_Effect.g_GradientRotationAaGap.z * pi / 180.0;
    float display_start = -pi * 0.5;
    float display_end = display_start + display_angle;
    float normalized_end = display_end > pi ? display_end - 2.0 * pi : display_end;
    bool in_display = display_angle >= 2.0 * pi
        || (normalized_end > display_start
            ? angle >= display_start && angle <= normalized_end
            : angle >= display_start || angle <= normalized_end);
    float display_mask = in_display ? 1.0 : 0.0;
    float corner_radius = u_Effect.g_Color2RingWidth.w
        * u_Effect.g_CornerRadiusOpacity.x;
    if (display_angle < 2.0 * pi) {
        vec2 left_center = vec2(cos(normalized_end), sin(normalized_end))
            * u_Effect.g_ColorRingSize.w;
        vec2 right_center = vec2(cos(display_start), sin(display_start))
            * u_Effect.g_ColorRingSize.w;
        bool in_ring_range = radius >= inner_radius && radius <= outer_radius;
        float left_distance = length(uv - left_center);
        float right_distance = length(uv - right_center);
        if (!in_display && in_ring_range && left_distance <= corner_radius) {
            display_mask = 1.0 - smoothstep(
                corner_radius - antialias * 0.5, corner_radius, left_distance);
        }
        if (!in_display && in_ring_range && right_distance <= corner_radius) {
            display_mask = 1.0 - smoothstep(
                corner_radius - antialias * 0.5, corner_radius, right_distance);
        }
    }
    ring *= display_mask;
    float rotation = u_Effect.g_GradientRotationAaGap.x * pi / 180.0;
    vec2 rotated = vec2(
        uv.x * cos(rotation) - uv.y * sin(rotation),
        uv.x * sin(rotation) + uv.y * cos(rotation));
    float gradient = clamp(rotated.x + 0.5, 0.0, 1.0);
    vec3 color = mix(
        u_Effect.g_ColorRingSize.rgb,
        u_Effect.g_Color2RingWidth.rgb,
        gradient) * ring;
    float alpha = ring * u_Effect.g_CornerRadiusOpacity.y;
    if (alpha <= 0.0) {
        discard;
    }
    o_Color = vec4(color, alpha);
}
"#
    .to_owned()
}
