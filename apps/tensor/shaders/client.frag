#version 450

layout(location = 0) in vec2 v_tex_coord;
layout(location = 1) in vec2 v_local;
layout(location = 0) out vec4 o_color;

layout(set = 0, binding = 0) uniform sampler2D client_image;

layout(push_constant, std430) uniform DrawData {
    uint descriptor_index;
    uint corner_radius;
    float opacity;
    float padding;
    vec4 destination;
    vec4 uv_origin_axis_x;
    vec4 uv_axis_y_surface_size;
} draw;

void main() {
    vec4 color = texture(client_image, v_tex_coord);
    float coverage = draw.opacity;
    vec2 surface_size = draw.uv_axis_y_surface_size.zw;
    float radius = min(float(draw.corner_radius), min(surface_size.x, surface_size.y) * 0.5);
    if (radius > 0.0) {
        vec2 distance_to_edge = min(v_local, 1.0 - v_local) * surface_size;
        vec2 corner_distance = max(radius - distance_to_edge, vec2(0.0));
        float outside = length(corner_distance);
        float edge = 1.0 - smoothstep(max(radius - 1.0, 0.0), radius, outside);
        coverage *= edge;
    }
    o_color = color * coverage;
}
