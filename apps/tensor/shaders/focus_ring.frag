#version 450

layout(location = 0) in vec2 v_pixel;
layout(location = 0) out vec4 o_color;

layout(push_constant, std430) uniform FocusRingData {
    vec4 destination;
    vec4 color;
    vec4 inner_rect;
    vec4 shape;
} ring;

float rounded_rect_distance(vec2 point, vec2 origin, vec2 size, float radius) {
    vec2 half_size = max(size * 0.5, vec2(0.5));
    float fitted_radius = clamp(radius, 0.0, min(half_size.x, half_size.y));
    vec2 distance = abs(point - origin - half_size) - half_size + fitted_radius;
    return min(max(distance.x, distance.y), 0.0)
        + length(max(distance, vec2(0.0)))
        - fitted_radius;
}

float filled_coverage(float distance) {
    float aa = max(fwidth(distance), 0.75);
    return 1.0 - smoothstep(-aa, aa, distance);
}

void main() {
    float outer = filled_coverage(
        rounded_rect_distance(v_pixel, vec2(0.0), ring.shape.zw, ring.shape.x)
    );
    float inner = filled_coverage(
        rounded_rect_distance(v_pixel, ring.inner_rect.xy, ring.inner_rect.zw, ring.shape.y)
    );
    float coverage = outer * (1.0 - inner);
    if (coverage <= 0.0) {
        discard;
    }

    float alpha = ring.color.a * coverage;
    o_color = vec4(ring.color.rgb * alpha, alpha);
}
