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
    layout(offset = 20) uint texture_resolution_mask;
    layout(offset = 24) uint system_uniform_count;
    layout(offset = 28) uint constant_uniform_count;
    layout(offset = 32) vec2 texture_resolution[8];
    layout(offset = 96) float auto_strength;
    layout(offset = 100) float auto_damping;
    layout(offset = 104) float auto_x_feather;
    layout(offset = 108) float auto_speed;
    layout(offset = 112) float auto_inertia;
    layout(offset = 116) float auto_segment_count;
    layout(offset = 120) uint effect_shader_code;
    layout(offset = 124) float auto_global_time_offset;
    layout(offset = 128) float auto_global_wind_offset;
    layout(offset = 132) uint auto_weight_center_offset_or_foliage_mode;
    layout(offset = 136) vec2 auto_smooth_directional_or_foliage_direction_weights;
    layout(offset = 144) vec2 auto_center1;
    layout(offset = 152) vec2 auto_center2;
    layout(offset = 160) vec2 auto_center3;
    layout(offset = 168) vec2 auto_center4;
    layout(offset = 176) float auto_size1;
    layout(offset = 180) float auto_size2;
    layout(offset = 184) float auto_size3;
    layout(offset = 188) float auto_size4;
    layout(offset = 192) float auto_angle2;
    layout(offset = 196) float auto_angle3;
    layout(offset = 200) float auto_angle4;
    layout(offset = 204) float auto_angle5;
    layout(offset = 208) float auto_time_offset1;
    layout(offset = 212) float auto_time_offset2;
    layout(offset = 216) float auto_time_offset3;
    layout(offset = 220) float auto_time_offset4;
    layout(offset = 224) uint effect_flags_or_foliage_vertex_strength_multiplier_bits;
    layout(offset = 228) uint output_flags;
    layout(offset = 232) vec2 vertex_extent;
} pc;

const uint EFFECT_SHADER_CODE_FOLIAGE_SWAY = 7u;
const uint EFFECT_SHADER_CODE_AUTO_SWAY = 8u;
const float PI = 3.14159265358979323846;
const float PI_HALF = 1.57079632679489661923;
const float TAU = 6.28318530717958647692;

struct AutoSwayNode {
    vec2 direction;
    vec2 endpoint_direction;
    float len;
    float endpoint_len;
    float pos_x;
    float endpoint_pos_x;
    float motion_radian;
};

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
        return in_frame_constants.w > 0.0 ? in_frame_constants.w : in_frame_constants.x;
    }
    return frame_time_buffer_from_ref(in_frame_time_ref).frame.constants.y;
}

bool has_layer_pose_ref() {
    return in_layer_pose_ref.x != 0u || in_layer_pose_ref.y != 0u;
}

vec2 rotate_vec2(vec2 value, float radians) {
    float s = sin(radians);
    float c = cos(radians);
    return vec2(c * value.x - s * value.y, s * value.x + c * value.y);
}

vec2 safe_normalize(vec2 value) {
    float len = length(value);
    return len > 0.000001 ? value / len : vec2(1.0, 0.0);
}

float auto_weight_center_offset() {
    return uintBitsToFloat(pc.auto_weight_center_offset_or_foliage_mode);
}

float auto_smooth_distance() {
    return pc.auto_smooth_directional_or_foliage_direction_weights.x;
}

float auto_directional_compensation() {
    return pc.auto_smooth_directional_or_foliage_direction_weights.y;
}

uint foliage_mode() {
    return pc.auto_weight_center_offset_or_foliage_mode;
}

vec2 foliage_direction_weights() {
    return pc.auto_smooth_directional_or_foliage_direction_weights;
}

vec4 foliage_corner_weights() {
    return vec4(pc.auto_center1, pc.auto_center2);
}

float foliage_vertex_strength_multiplier() {
    return uintBitsToFloat(pc.effect_flags_or_foliage_vertex_strength_multiplier_bits);
}

vec3 fallback_noise(vec2 uv) {
    vec3 p = fract(vec3(uv.xyx) * vec3(123.34, 456.21, 345.45));
    p += dot(p, p.yzx + 45.32);
    return fract(vec3(p.x * p.y, p.y * p.z, p.z * p.x));
}

float sine_step(float lower, float upper, float value) {
    float denom = max(upper - lower, 0.000001);
    return sin(clamp((value - lower) / denom, 0.0, 1.0) * PI_HALF);
}

float auto_time_offset(float node_num) {
    float denom = max(4.0 - 2.0, 0.000001);
    return clamp((node_num - 2.0) / denom, 0.0, 1.0);
}

AutoSwayNode pre_calc_auto_sway_node(
    float node_num,
    vec2 tex_coord,
    float aspect,
    float motion_offset,
    vec2 endpoint_center,
    vec2 this_center,
    vec2 next_center,
    float this_wind_direction,
    float next_wind_direction
) {
    this_center.x *= aspect;
    next_center.x *= aspect;

    vec2 node_vec = this_center - next_center;
    vec2 endpoint_node_vec = endpoint_center - next_center;
    vec2 direction = safe_normalize(node_vec);
    vec2 endpoint_direction = mix(
        safe_normalize(endpoint_node_vec),
        direction,
        auto_directional_compensation()
    );
    float len = dot(node_vec, direction);
    float endpoint_len = mix(
        len,
        dot(endpoint_node_vec, endpoint_direction),
        auto_smooth_distance()
    );
    vec2 relative_tex_coord = tex_coord - next_center;
    float endpoint_pos_x = dot(relative_tex_coord, endpoint_direction)
        - auto_weight_center_offset() * len;
    float pos_x = dot(relative_tex_coord, endpoint_direction);

    float this_motion_time = pc.auto_global_time_offset + scene_time_seconds() * pc.auto_speed;
    float prev_motion_time = this_motion_time;
    this_motion_time += motion_offset * auto_time_offset(node_num);
    prev_motion_time += motion_offset * auto_time_offset(node_num + 1.0);

    float motion_radian = sin(this_motion_time * TAU);
    float prev_motion_radian = sin(prev_motion_time * TAU) * pc.auto_inertia;
    motion_radian += sin(this_wind_direction + PI_HALF) + sin(pc.auto_global_wind_offset);
    prev_motion_radian += sin(next_wind_direction + PI_HALF);
    prev_motion_radian *= step(0.5, node_num);
    motion_radian -= prev_motion_radian;

    return AutoSwayNode(
        direction,
        endpoint_direction,
        len,
        endpoint_len,
        pos_x,
        endpoint_pos_x,
        motion_radian
    );
}

void apply_auto_sway_node(
    inout vec2 tex_coord,
    float aspect,
    vec2 root_center,
    float this_width,
    float next_width,
    AutoSwayNode node,
    inout float auto_mask
) {
    root_center.x *= aspect;
    vec2 relative_tex_coord = tex_coord - root_center;
    float width_mix = node.pos_x / max(node.len, 0.000001);
    float h_boundary = mix(next_width, this_width, width_mix);
    float pos_y = abs(dot(relative_tex_coord, vec2(node.direction.y, -node.direction.x)));
    auto_mask = max(
        auto_mask,
        1.0 - smoothstep(h_boundary, h_boundary + pc.auto_x_feather * node.len * 2.0, pos_y)
    );

    float weight = sine_step(0.0, node.endpoint_len, node.endpoint_pos_x);
    weight *= (1.0 - weight * pc.auto_damping) * auto_mask;
    tex_coord = rotate_vec2(
        relative_tex_coord,
        node.motion_radian * pc.auto_strength * weight
    ) + root_center;
}

vec2 auto_sway_uv(vec2 uv) {
    if (pc.effect_shader_code != EFFECT_SHADER_CODE_AUTO_SWAY) {
        return uv;
    }
    vec2 base_resolution = pc.texture_resolution[0];
    if ((pc.texture_resolution_mask & 1u) == 0u) {
        base_resolution = max(pc.extent, vec2(1.0));
    }
    float aspect = max(base_resolution.x, 1.0) / max(base_resolution.y, 1.0);
    float reciprocal_aspect = 1.0 / max(aspect, 0.000001);
    vec2 tex_coord = uv * vec2(aspect, 1.0);

    float motion_offset = pc.auto_inertia * pc.auto_segment_count;
    vec2 endpoint_center = pc.auto_center1;
    endpoint_center.x *= aspect;
    AutoSwayNode node1 = pre_calc_auto_sway_node(
        2.0, tex_coord, aspect, motion_offset, endpoint_center,
        pc.auto_center1, pc.auto_center2, pc.auto_angle2, pc.auto_angle3
    );
    AutoSwayNode node2 = pre_calc_auto_sway_node(
        3.0, tex_coord, aspect, motion_offset, endpoint_center,
        pc.auto_center2, pc.auto_center3, pc.auto_angle3, pc.auto_angle4
    );
    AutoSwayNode node3 = pre_calc_auto_sway_node(
        4.0, tex_coord, aspect, motion_offset, endpoint_center,
        pc.auto_center3, pc.auto_center4, pc.auto_angle4, pc.auto_angle5
    );

    float auto_mask = 0.0;
    apply_auto_sway_node(tex_coord, aspect, pc.auto_center2, pc.auto_size1, pc.auto_size2, node1, auto_mask);
    apply_auto_sway_node(tex_coord, aspect, pc.auto_center3, pc.auto_size2, pc.auto_size3, node2, auto_mask);
    apply_auto_sway_node(tex_coord, aspect, pc.auto_center4, pc.auto_size3, pc.auto_size4, node3, auto_mask);
    tex_coord.x *= reciprocal_aspect;
    return tex_coord;
}

vec2 auto_sway_vertex_offset(vec2 uv) {
    if (pc.effect_shader_code != EFFECT_SHADER_CODE_AUTO_SWAY) {
        return vec2(0.0);
    }
    vec2 base_resolution = pc.texture_resolution[0];
    if ((pc.texture_resolution_mask & 1u) == 0u) {
        base_resolution = max(pc.extent, vec2(1.0));
    }
    return (auto_sway_uv(uv) - uv) * max(base_resolution, vec2(1.0));
}

vec2 foliage_sway_vertex_offset(vec2 uv) {
    if (pc.effect_shader_code != EFFECT_SHADER_CODE_FOLIAGE_SWAY) {
        return vec2(0.0);
    }
    if (foliage_mode() == 0u) {
        return vec2(0.0);
    }

    float phase = pc.auto_x_feather;
    vec4 sines = sin(phase + pc.auto_damping * scene_time_seconds()
        * vec4(1.0, -0.16161616, 0.0083333, -0.00019841));
    vec4 csines = sin(0.4 + phase + pc.auto_damping * scene_time_seconds()
        * vec4(-0.5, 0.041666666, -0.0013888889, 0.000024801587));
    sines = pow(abs(sines), vec4(pc.auto_speed)) * sign(sines);
    csines = pow(abs(csines), vec4(pc.auto_speed)) * sign(csines);

    vec4 corners = foliage_corner_weights();
    float weight = clamp(
        corners.x * (1.0 - uv.x) * (1.0 - uv.y) +
        corners.y * uv.x * (1.0 - uv.y) +
        corners.z * uv.x * uv.y +
        corners.w * (1.0 - uv.x) * uv.y,
        0.0,
        1.0
    );
    vec2 direction_weights = foliage_direction_weights();
    return vec2(
        dot(sines, vec4(1.0)) * pc.auto_strength * foliage_vertex_strength_multiplier() * weight * direction_weights.x,
        dot(csines, vec4(1.0)) * pc.auto_strength * foliage_vertex_strength_multiplier() * weight * direction_weights.y
    );
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
    float frame_delta = floor(max(timeline_time_seconds(), 0.0) * fps);
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
    vec2 uv = animated_uv(in_uv);
    vec2 local_position = in_position
        + auto_sway_vertex_offset(uv)
        + foliage_sway_vertex_offset(uv);
    vec2 gpu_position = gpu_layer_position(local_position);
    vec2 normalized = gpu_position / pc.vertex_extent;
    gl_Position = vec4(normalized.x * 2.0 - 1.0, 1.0 - normalized.y * 2.0, 0.0, 1.0);
    v_uv = uv;
    v_effect_uv = in_effect_uv;
    v_opacity = in_opacity;
    v_tint = in_tint;
    v_time_seconds = scene_time_seconds();
}
