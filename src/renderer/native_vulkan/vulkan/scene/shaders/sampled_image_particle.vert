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

layout(location = 0) in vec2 in_corner;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec2 in_spawn;
layout(location = 3) in vec2 in_velocity;
layout(location = 4) in vec4 in_particle_constants;
layout(location = 5) in vec4 in_position_transform_x;
layout(location = 6) in vec4 in_position_transform_y;
layout(location = 7) in vec4 in_frame_constants;
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

FrameTimeBuffer frame_time_buffer_from_ref(uvec4 ref_words) {
    uint64_t address = uint64_t(ref_words.x) | (uint64_t(ref_words.y) << 32);
    return FrameTimeBuffer(address);
}

LayerPoseBuffer layer_pose_buffer_from_ref(uvec4 ref_words) {
    uint64_t address = uint64_t(ref_words.x) | (uint64_t(ref_words.y) << 32);
    return LayerPoseBuffer(address);
}

float scene_time_seconds() {
    if (in_frame_time_ref.x == 0u && in_frame_time_ref.y == 0u) {
        return in_frame_constants.x;
    }
    return frame_time_buffer_from_ref(in_frame_time_ref).frame.constants.x;
}

float timeline_time_seconds() {
    if (in_frame_time_ref.x == 0u && in_frame_time_ref.y == 0u) {
        return in_frame_constants.x;
    }
    return frame_time_buffer_from_ref(in_frame_time_ref).frame.constants.y;
}

bool has_layer_pose_ref() {
    return in_layer_pose_ref.x != 0u || in_layer_pose_ref.y != 0u;
}

LayerPose layer_pose_at_time(float time_seconds) {
    LayerPoseBuffer timeline = layer_pose_buffer_from_ref(in_layer_pose_ref);
    uint frame_count = max(in_layer_pose_ref.z, 1u);
    float frame_rate = max(float(in_layer_pose_ref.w), 1.0);
    float frame = max(time_seconds * frame_rate, 0.0);
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

vec2 rotated_corner(vec2 corner, vec2 velocity) {
    float speed = length(velocity);
    if (speed <= 0.000001) {
        return corner;
    }
    vec2 dir = velocity / speed;
    return vec2(
        corner.x * dir.x - corner.y * dir.y,
        corner.x * dir.y + corner.y * dir.x
    );
}

void main() {
    float scene_seconds = scene_time_seconds();
    float timeline_seconds = timeline_time_seconds();
    float lifetime = max(in_particle_constants.y, 0.001);
    bool loop_playback = in_frame_constants.z > 0.5;
    bool fade = in_frame_constants.w > 0.5;
    float age = 0.0;
    float alive = 1.0;
    if (loop_playback) {
        age = mod(max(scene_seconds + in_particle_constants.x, 0.0), lifetime);
    } else {
        float raw_age = max(scene_seconds - in_particle_constants.x, 0.0);
        alive = raw_age <= lifetime ? 1.0 : 0.0;
        age = min(raw_age, lifetime);
    }
    float fade_opacity = fade ? clamp(1.0 - age / lifetime, 0.0, 1.0) : 1.0;
    vec2 gravity = in_particle_constants.zw;
    vec2 local = in_spawn + in_velocity * age + 0.5 * gravity * age * age +
        rotated_corner(in_corner, in_velocity);

    vec4 layer_transform_x = in_position_transform_x;
    vec4 layer_transform_y = in_position_transform_y;
    float layer_opacity = in_frame_constants.y;
    if (has_layer_pose_ref()) {
        LayerPose layer_pose = layer_pose_at_time(timeline_seconds);
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
    v_effect_uv = in_uv;
    v_opacity = layer_opacity * fade_opacity * alive;
    v_tint = in_tint;
    v_time_seconds = scene_seconds;
}
