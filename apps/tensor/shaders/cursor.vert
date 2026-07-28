#version 450

layout(location = 0) out vec2 v_local;

layout(push_constant, std430) uniform CursorData {
    vec4 destination;
} cursor;

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
    vec2 position = cursor.destination.xy + local * cursor.destination.zw;
    gl_Position = vec4(position, 0.0, 1.0);
    v_local = local;
}
