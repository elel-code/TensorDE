#version 450

layout(location = 0) out vec2 v_tex_coord;
layout(location = 1) out vec2 v_local;

layout(push_constant, std430) uniform DrawData {
    uint descriptor_index;
    uint corner_radius;
    float opacity;
    float padding;
    vec4 destination;
    vec4 uv_origin_axis_x;
    vec4 uv_axis_y_viewport;
} draw;

void main() {
    const vec2 positions[6] = vec2[](
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(1.0, 1.0),
        vec2(0.0, 0.0),
        vec2(1.0, 1.0),
        vec2(0.0, 1.0)
    );
    vec2 local = positions[gl_VertexIndex];
    vec2 position = draw.destination.xy + local * draw.destination.zw;
    gl_Position = vec4(
        position.x / draw.uv_axis_y_viewport.z * 2.0 - 1.0,
        1.0 - position.y / draw.uv_axis_y_viewport.w * 2.0,
        0.0,
        1.0
    );
    v_local = local;
    v_tex_coord = draw.uv_origin_axis_x.xy
        + local.x * draw.uv_origin_axis_x.zw
        + local.y * draw.uv_axis_y_viewport.xy;
}
