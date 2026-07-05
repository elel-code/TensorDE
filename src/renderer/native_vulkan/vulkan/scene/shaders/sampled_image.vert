#version 450

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec2 in_effect_uv;
layout(location = 3) in float in_opacity;
layout(location = 4) in vec4 in_tint;
layout(location = 5) in vec4 in_position_transform_x;
layout(location = 6) in vec4 in_position_transform_y;
layout(location = 7) in vec4 in_frame_constants;

layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec2 v_effect_uv;
layout(location = 2) out float v_opacity;
layout(location = 3) out vec4 v_tint;
layout(location = 4) flat out float v_time_seconds;

layout(push_constant) uniform ScenePush {
    layout(offset = 0) vec2 extent;
    layout(offset = 8) uint alpha_texture_slot;
    layout(offset = 12) uint alpha_texture_mode;
    layout(offset = 232) vec2 vertex_extent;
} pc;

void main() {
    vec2 gpu_position = vec2(
        in_position.x * in_position_transform_x.x + in_position.y * in_position_transform_x.y + in_position_transform_x.z,
        in_position.x * in_position_transform_y.x + in_position.y * in_position_transform_y.y + in_position_transform_y.z
    );
    vec2 normalized = gpu_position / pc.vertex_extent;
    gl_Position = vec4(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
    v_uv = in_uv;
    v_effect_uv = in_effect_uv;
    v_opacity = in_opacity;
    v_tint = in_tint;
    v_time_seconds = in_frame_constants.x;
}
