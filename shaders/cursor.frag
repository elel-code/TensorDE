#version 450

layout(location = 0) in vec2 v_local;
layout(location = 0) out vec4 o_color;

bool contains(vec2 point, const vec2 vertices[7]) {
    bool inside = false;
    for (int index = 0, previous = 6; index < 7; previous = index++) {
        vec2 left = vertices[index];
        vec2 right = vertices[previous];
        bool crosses = (left.y > point.y) != (right.y > point.y);
        if (crosses) {
            float x = (right.x - left.x) * (point.y - left.y) / (right.y - left.y) + left.x;
            if (point.x < x) {
                inside = !inside;
            }
        }
    }
    return inside;
}

void main() {
    const vec2 outline[7] = vec2[](
        vec2(0.04, 0.00),
        vec2(0.04, 0.83),
        vec2(0.25, 0.61),
        vec2(0.45, 0.98),
        vec2(0.63, 0.89),
        vec2(0.43, 0.52),
        vec2(0.84, 0.52)
    );
    const vec2 fill[7] = vec2[](
        vec2(0.13, 0.15),
        vec2(0.13, 0.62),
        vec2(0.27, 0.48),
        vec2(0.46, 0.84),
        vec2(0.50, 0.82),
        vec2(0.31, 0.45),
        vec2(0.62, 0.45)
    );

    if (!contains(v_local, outline)) {
        discard;
    }
    o_color = contains(v_local, fill)
        ? vec4(1.0, 1.0, 1.0, 1.0)
        : vec4(0.0, 0.0, 0.0, 1.0);
}
