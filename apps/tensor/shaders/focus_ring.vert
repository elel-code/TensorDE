#version 450

layout(location = 0) out vec2 v_pixel;

layout(push_constant, std430) uniform FocusRingData {
    vec4 destination;
    vec4 color;
    vec4 inner_rect;
    vec4 shape;
} ring;

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
    vec2 position = ring.destination.xy + local * ring.destination.zw;
    gl_Position = vec4(position, 0.0, 1.0);
    v_pixel = local * ring.shape.zw;
}
