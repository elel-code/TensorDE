#version 450
#extension GL_EXT_buffer_reference : require
#extension GL_EXT_buffer_reference2 : require
#extension GL_EXT_shader_explicit_arithmetic_types_int64 : require

struct FrameTime {
    vec4 constants;
};

struct LayerPose {
    vec4 position_transform_x;
    vec4 position_transform_y;
    vec4 constants;
};

layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer FrameTimeBuffer {
    FrameTime frame;
};

layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer LayerPoseBuffer {
    LayerPose poses[];
};

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec2 in_effect_uv;
layout(location = 3) in float in_opacity;
layout(location = 4) in vec4 in_tint;
layout(location = 5) in vec4 in_position_transform_x;
layout(location = 6) in vec4 in_position_transform_y;
layout(location = 7) in vec4 in_frame_constants;
layout(location = 8) in vec4 in_uv_animation_x;
layout(location = 9) in vec4 in_uv_animation_y;
layout(location = 12) in uvec4 in_frame_time_ref;
layout(location = 13) in uvec4 in_layer_pose_ref;

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

FrameTimeBuffer frame_time_buffer_from_ref(uvec4 ref_words) {
    uint64_t address = uint64_t(ref_words.x) | (uint64_t(ref_words.y) << 32);
    return FrameTimeBuffer(address);
}

LayerPoseBuffer layer_pose_buffer_from_ref(uvec4 ref_words) {
    uint64_t address = uint64_t(ref_words.x) | (uint64_t(ref_words.y) << 32);
    return LayerPoseBuffer(address);
}

float frame_time_seconds() {
    if (in_frame_time_ref.x == 0u && in_frame_time_ref.y == 0u) {
        return in_frame_constants.x;
    }
    return frame_time_buffer_from_ref(in_frame_time_ref).frame.constants.x;
}

bool has_layer_pose_ref() {
    return in_layer_pose_ref.x != 0u || in_layer_pose_ref.y != 0u;
}

LayerPose layer_pose_at_time() {
    LayerPoseBuffer timeline = layer_pose_buffer_from_ref(in_layer_pose_ref);
    uint frame_count = max(in_layer_pose_ref.z, 1u);
    float frame_rate = max(float(in_layer_pose_ref.w), 1.0);
    float frame = min(max(frame_time_seconds() * frame_rate, 0.0), float(frame_count - 1u));
    uint frame0 = min(uint(floor(frame)), frame_count - 1u);
    uint frame1 = min(frame0 + 1u, frame_count - 1u);
    float frame_mix = fract(frame);
    LayerPose a = timeline.poses[frame0];
    LayerPose b = timeline.poses[frame1];
    LayerPose pose;
    pose.position_transform_x = mix(a.position_transform_x, b.position_transform_x, frame_mix);
    pose.position_transform_y = mix(a.position_transform_y, b.position_transform_y, frame_mix);
    pose.constants = mix(a.constants, b.constants, frame_mix);
    return pose;
}

vec2 transform_position(vec2 position, vec4 row_x, vec4 row_y) {
    return vec2(
        position.x * row_x.x + position.y * row_x.y + row_x.z,
        position.x * row_y.x + position.y * row_y.y + row_y.z
    );
}

vec2 gpu_layer_position(vec2 position) {
    if (!has_layer_pose_ref()) {
        return transform_position(position, in_position_transform_x, in_position_transform_y);
    }
    LayerPose current = layer_pose_at_time();
    float a = in_position_transform_x.x;
    float b = in_position_transform_x.y;
    float tx = in_position_transform_x.z;
    float c = in_position_transform_y.x;
    float d = in_position_transform_y.y;
    float ty = in_position_transform_y.z;
    float det = a * d - b * c;
    if (abs(det) <= 0.0000001) {
        return position;
    }
    float inv_det = 1.0 / det;
    float inv_a = d * inv_det;
    float inv_b = -b * inv_det;
    float inv_tx = (b * ty - d * tx) * inv_det;
    float inv_c = -c * inv_det;
    float inv_d = a * inv_det;
    float inv_ty = (c * tx - a * ty) * inv_det;
    vec4 row_x = vec4(
        current.position_transform_x.x * inv_a + current.position_transform_x.y * inv_c,
        current.position_transform_x.x * inv_b + current.position_transform_x.y * inv_d,
        current.position_transform_x.x * inv_tx + current.position_transform_x.y * inv_ty + current.position_transform_x.z,
        0.0
    );
    vec4 row_y = vec4(
        current.position_transform_y.x * inv_a + current.position_transform_y.y * inv_c,
        current.position_transform_y.x * inv_b + current.position_transform_y.y * inv_d,
        current.position_transform_y.x * inv_tx + current.position_transform_y.y * inv_ty + current.position_transform_y.z,
        0.0
    );
    return transform_position(position, row_x, row_y);
}

vec2 animated_uv(vec2 base_uv) {
    if (in_frame_constants.y < 0.5) {
        return base_uv;
    }
    float u_span = in_uv_animation_x.y;
    float v_span = in_uv_animation_y.y;
    if (abs(u_span) <= 0.0000001 || abs(v_span) <= 0.0000001) {
        return base_uv;
    }
    float frame_count = max(in_uv_animation_x.z, 1.0);
    float columns = max(in_uv_animation_y.z, 1.0);
    float fps = max(in_uv_animation_x.w, 0.0);
    float frame_delta = floor(max(frame_time_seconds(), 0.0) * fps);
    float frame_index = in_uv_animation_y.w + frame_delta;
    if (in_frame_constants.z > 0.5) {
        frame_index = mod(frame_index, frame_count);
    } else {
        frame_index = min(frame_index, frame_count - 1.0);
    }
    float column = mod(frame_index, columns);
    float row = floor(frame_index / columns);
    float u_t = clamp((base_uv.x - in_uv_animation_x.x) / u_span, 0.0, 1.0);
    float v_t = clamp((base_uv.y - in_uv_animation_y.x) / v_span, 0.0, 1.0);
    return vec2((column + u_t) * u_span, (row + v_t) * v_span);
}

void main() {
    vec2 gpu_position = gpu_layer_position(in_position);
    vec2 normalized = gpu_position / pc.vertex_extent;
    gl_Position = vec4(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
    v_uv = animated_uv(in_uv);
    v_effect_uv = in_effect_uv;
    v_opacity = in_opacity;
    v_tint = in_tint;
    v_time_seconds = frame_time_seconds();
}
