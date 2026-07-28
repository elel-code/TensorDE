#version 460

layout(set = 0, binding = 0) uniform texture2D icon_texture;
layout(set = 0, binding = 1) uniform sampler icon_sampler;

layout(location = 0) in vec2 in_uv;
layout(location = 1) flat in vec4 in_rounding_bounds;
layout(location = 2) flat in vec2 in_radius_alpha;

layout(location = 0) out vec4 out_color;

void main() {
    vec4 color = texture(sampler2D(icon_texture, icon_sampler), in_uv);
    float coverage = 1.0;
    vec4 bounds = in_rounding_bounds;
    if (in_radius_alpha.x > 0.0 && bounds.z > bounds.x && bounds.w > bounds.y) {
        vec2 extent = bounds.zw - bounds.xy;
        float radius = min(extent.x, extent.y) * in_radius_alpha.x;
        vec2 center = clamp(in_uv, bounds.xy + radius, bounds.zw - radius);
        float distance = length(in_uv - center) - radius;
        float antialias = max(fwidth(distance) * 0.5, 0.00001);
        coverage = 1.0 - smoothstep(-antialias, antialias, distance);
    }
    out_color = color * (coverage * in_radius_alpha.y);
}
