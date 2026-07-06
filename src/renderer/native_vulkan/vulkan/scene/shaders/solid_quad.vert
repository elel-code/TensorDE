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
layout(location = 1) in vec4 in_rgba;
layout(location = 2) in uvec4 in_layer_pose_ref;
layout(location = 3) in uvec4 in_frame_time_ref;

layout(location = 0) out vec4 v_rgba;

layout(push_constant) uniform ScenePush {
    vec2 extent;
} pc;

FrameTimeBuffer frame_time_buffer_from_ref(uvec4 ref_words) {
    uint64_t address = uint64_t(ref_words.x) | (uint64_t(ref_words.y) << 32);
    return FrameTimeBuffer(address);
}

LayerPoseBuffer layer_pose_buffer_from_ref(uvec4 ref_words) {
    uint64_t address = uint64_t(ref_words.x) | (uint64_t(ref_words.y) << 32);
    return LayerPoseBuffer(address);
}

bool has_layer_pose_ref() {
    return in_layer_pose_ref.x != 0u || in_layer_pose_ref.y != 0u;
}

float timeline_time_seconds() {
    if (in_frame_time_ref.x == 0u && in_frame_time_ref.y == 0u) {
        return 0.0;
    }
    return frame_time_buffer_from_ref(in_frame_time_ref).frame.constants.y;
}

LayerPose layer_pose_at_frame(uint frame_index) {
    LayerPoseBuffer timeline = layer_pose_buffer_from_ref(in_layer_pose_ref);
    return timeline.poses[frame_index];
}

LayerPose layer_pose_at_time() {
    LayerPoseBuffer timeline = layer_pose_buffer_from_ref(in_layer_pose_ref);
    uint frame_count = max(in_layer_pose_ref.z, 1u);
    float frame_rate = max(float(in_layer_pose_ref.w), 1.0);
    float frame = max(timeline_time_seconds() * frame_rate, 0.0);
    if (frame_count > 1u) {
        frame = mod(frame, float(frame_count));
    } else {
        frame = 0.0;
    }
    uint frame0 = min(uint(floor(frame)), frame_count - 1u);
    uint frame1 = frame_count > 1u ? (frame0 + 1u) % frame_count : frame0;
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

vec2 layer_delta_position(vec2 position, LayerPose base, LayerPose current) {
    float a = base.position_transform_x.x;
    float b = base.position_transform_x.y;
    float tx = base.position_transform_x.z;
    float c = base.position_transform_y.x;
    float d = base.position_transform_y.y;
    float ty = base.position_transform_y.z;
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

float layer_delta_alpha(float base_alpha, LayerPose base, LayerPose current) {
    float base_opacity = clamp(base.constants.x, 0.0, 1.0);
    float current_opacity = clamp(current.constants.x, 0.0, 1.0);
    if (base_opacity <= 0.000001) {
        return current_opacity;
    }
    return clamp(base_alpha * current_opacity / base_opacity, 0.0, 1.0);
}

void main() {
    vec2 position = in_position;
    vec4 rgba = in_rgba;
    if (has_layer_pose_ref()) {
        LayerPose base_pose = layer_pose_at_frame(0u);
        LayerPose current_pose = layer_pose_at_time();
        position = layer_delta_position(position, base_pose, current_pose);
        rgba.a = layer_delta_alpha(rgba.a, base_pose, current_pose);
    }
    vec2 normalized = position / pc.extent;
    gl_Position = vec4(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
    v_rgba = rgba;
}
