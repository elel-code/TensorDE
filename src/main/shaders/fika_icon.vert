#version 460

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec4 in_rounding_bounds;
layout(location = 3) in vec2 in_radius_alpha;

layout(location = 0) out vec2 out_uv;
layout(location = 1) flat out vec4 out_rounding_bounds;
layout(location = 2) flat out vec2 out_radius_alpha;

void main() {
    gl_Position = vec4(in_position, 0.0, 1.0);
    out_uv = in_uv;
    out_rounding_bounds = in_rounding_bounds;
    out_radius_alpha = in_radius_alpha;
}
