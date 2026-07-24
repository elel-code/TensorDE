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
    vec4 uv_axis_y_surface_size;
} draw;

// The GLSL default gl_PerVertex block also declares ClipDistance and
// CullDistance. Tensor does not require those optional Vulkan features, so
// expose only the builtin this fullscreen primitive writes.
out gl_PerVertex {
    vec4 gl_Position;
};

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
    // Tensor converts its top-left physical surface coordinates to Vulkan NDC
    // before recording the draw. Keep this shader free of a second Y flip.
    vec2 position = draw.destination.xy + local * draw.destination.zw;
    gl_Position = vec4(position, 0.0, 1.0);
    v_local = local;
    v_tex_coord = draw.uv_origin_axis_x.xy
        + local.x * draw.uv_origin_axis_x.zw
        + local.y * draw.uv_axis_y_surface_size.xy;
}
