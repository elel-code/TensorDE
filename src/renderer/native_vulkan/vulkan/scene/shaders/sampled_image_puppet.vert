#version 450
#extension GL_EXT_buffer_reference : require
#extension GL_EXT_buffer_reference2 : require
#extension GL_EXT_shader_explicit_arithmetic_types_int64 : require

struct PuppetPoseBone {
    mat4 skin_matrix;
    vec4 opacity;
};

struct FrameTime {
    vec4 constants;
};

struct LayerPose {
    vec4 position_transform_x;
    vec4 position_transform_y;
    vec4 constants;
};

layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer PuppetPoseBuffer {
    PuppetPoseBone bones[];
};

layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer FrameTimeBuffer {
    FrameTime frame;
};

layout(buffer_reference, std430, buffer_reference_align = 16) readonly buffer LayerPoseBuffer {
    LayerPose poses[];
};

layout(location = 0) in vec2 in_position;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec4 in_bone_weights;
layout(location = 3) in uvec4 in_bone_indices;
layout(location = 4) in vec4 in_vertex_opacity;
layout(location = 5) in vec4 in_position_transform_x;
layout(location = 6) in vec4 in_position_transform_y;
layout(location = 7) in vec4 in_frame_constants;
layout(location = 8) in vec4 in_effect_uv_x;
layout(location = 9) in vec4 in_effect_uv_y;
layout(location = 10) in uvec4 in_puppet_pose_ref;
layout(location = 11) in vec4 in_tint;
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

PuppetPoseBuffer pose_buffer_from_ref(uvec4 ref_words) {
    uint64_t address = uint64_t(ref_words.x) | (uint64_t(ref_words.y) << 32);
    return PuppetPoseBuffer(address);
}

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

LayerPose layer_pose_at_time(float time_seconds) {
    LayerPoseBuffer timeline = layer_pose_buffer_from_ref(in_layer_pose_ref);
    uint frame_count = max(in_layer_pose_ref.z, 1u);
    float frame_rate = max(float(in_layer_pose_ref.w), 1.0);
    float frame = min(max(time_seconds * frame_rate, 0.0), float(frame_count - 1u));
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

void main() {
    PuppetPoseBuffer pose = pose_buffer_from_ref(in_puppet_pose_ref);
    uint pose_frame_count = max(in_puppet_pose_ref.z, 1u);
    uint pose_frame_bone_count = max(in_puppet_pose_ref.w, 1u);
    float pose_fps = max(in_frame_constants.z, 1.0);
    bool pose_loops = in_frame_constants.w > 0.5;
    float time_seconds = frame_time_seconds();
    float pose_frame = max(time_seconds * pose_fps, 0.0);
    if (pose_loops && pose_frame_count > 1u) {
        pose_frame = mod(pose_frame, float(pose_frame_count));
    } else {
        pose_frame = min(pose_frame, float(pose_frame_count - 1u));
    }
    uint pose_frame0 = min(uint(floor(pose_frame)), pose_frame_count - 1u);
    uint pose_frame1 = pose_frame0 + 1u;
    if (pose_loops && pose_frame_count > 1u) {
        pose_frame1 = pose_frame1 % pose_frame_count;
    } else {
        pose_frame1 = min(pose_frame1, pose_frame_count - 1u);
    }
    float pose_mix = fract(pose_frame);

    vec4 local = vec4(0.0);
    float total_weight = 0.0;
    float bone_opacity = 0.0;
    for (uint slot = 0u; slot < 4u; slot++) {
        float weight = in_bone_weights[slot];
        if (weight <= 0.000001) {
            continue;
        }
        uint bone_index = min(in_bone_indices[slot], pose_frame_bone_count - 1u);
        uint bone0 = pose_frame0 * pose_frame_bone_count + bone_index;
        uint bone1 = pose_frame1 * pose_frame_bone_count + bone_index;
        mat4 skin_matrix =
            pose.bones[bone0].skin_matrix * (1.0 - pose_mix) +
            pose.bones[bone1].skin_matrix * pose_mix;
        float sampled_bone_opacity = mix(
            pose.bones[bone0].opacity.x,
            pose.bones[bone1].opacity.x,
            pose_mix
        );
        local += (skin_matrix * vec4(in_position, 0.0, 1.0)) * weight;
        bone_opacity += sampled_bone_opacity * weight;
        total_weight += weight;
    }
    if (total_weight > 0.000001) {
        local /= total_weight;
        bone_opacity /= total_weight;
    } else {
        local = vec4(in_position, 0.0, 1.0);
        bone_opacity = 1.0;
    }

    vec4 layer_transform_x = in_position_transform_x;
    vec4 layer_transform_y = in_position_transform_y;
    float layer_opacity = in_frame_constants.y;
    if (has_layer_pose_ref()) {
        LayerPose layer_pose = layer_pose_at_time(time_seconds);
        layer_transform_x = layer_pose.position_transform_x;
        layer_transform_y = layer_pose.position_transform_y;
        layer_opacity = layer_pose.constants.x;
    }
    vec2 gpu_position = vec2(
        local.x * layer_transform_x.x + local.y * layer_transform_x.y + layer_transform_x.z,
        local.x * layer_transform_y.x + local.y * layer_transform_y.y + layer_transform_y.z
    );
    vec2 normalized = gpu_position / pc.vertex_extent;
    gl_Position = vec4(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
    v_uv = in_uv;
    v_effect_uv = vec2(
        local.x * in_effect_uv_x.x + local.y * in_effect_uv_x.y + in_effect_uv_x.z,
        local.x * in_effect_uv_y.x + local.y * in_effect_uv_y.y + in_effect_uv_y.z
    );
    v_opacity = in_vertex_opacity.x * bone_opacity * layer_opacity;
    v_tint = in_tint;
    v_time_seconds = time_seconds;
}
