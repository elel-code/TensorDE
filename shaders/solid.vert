#version 450

layout(location = 0) out vec4 v_color;

layout(push_constant, std430) uniform SolidData {
    vec4 destination;
    vec4 color;
} solid;

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
    vec2 position = solid.destination.xy + local * solid.destination.zw;
    gl_Position = vec4(position, 0.0, 1.0);
    v_color = solid.color;
}
