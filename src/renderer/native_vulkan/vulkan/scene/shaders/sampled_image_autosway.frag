#version 450

// reverse-engineered reference:
// extracted/3742497499/shaders/workshop/3392386920/effects/auto_sway.vert/.frag.
// The scene uses AA_VERSION=2, NODE_COUNT=4, DEBUG=0. WE computes only linear
// per-vertex intermediates before fragment-space UV rotation, so evaluating the
// same formulas per fragment is equivalent after interpolation and keeps the
// work on the GPU instead of using native CPU sine mesh motion.

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec2 v_effect_uv;
layout(location = 2) in float v_opacity;
layout(location = 3) in vec4 v_tint;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;

layout(push_constant) uniform ScenePush {
    layout(offset = 0) vec2 extent;
    layout(offset = 8) uint alpha_texture_slot;
    layout(offset = 12) uint alpha_texture_mode;
    layout(offset = 16) float time_seconds;
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
    layout(offset = 132) float auto_weight_center_offset;
    layout(offset = 136) float auto_smooth_distance;
    layout(offset = 140) float auto_directional_compensation;
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
    layout(offset = 224) uint auto_flags;
    layout(offset = 228) uint output_flags;
} pc;

const uint AUTO_SWAY_FLAG_MASK = 1u;
const uint OUTPUT_FLAG_PREMULTIPLY_RGB = 1u;
const float PI = 3.14159265358979323846;
const float PI_HALF = 1.57079632679489661923;
const float TAU = 6.28318530717958647692;

struct Node {
    vec2 direction;
    vec2 endpoint_direction;
    float len;
    float endpoint_len;
    float pos_x;
    float endpoint_pos_x;
    float motion_radian;
};

vec2 rotate_vec2(vec2 value, float radians) {
    float s = sin(radians);
    float c = cos(radians);
    return vec2(c * value.x - s * value.y, s * value.x + c * value.y);
}

vec2 safe_normalize(vec2 value) {
    float len = length(value);
    return len > 0.000001 ? value / len : vec2(1.0, 0.0);
}

float sine_step(float lower, float upper, float value) {
    float denom = max(upper - lower, 0.000001);
    return sin(clamp((value - lower) / denom, 0.0, 1.0) * PI_HALF);
}

float auto_time_offset(float node_num) {
    float denom = max(4.0 - 2.0, 0.000001);
    return clamp((node_num - 2.0) / denom, 0.0, 1.0);
}

Node pre_calc_node(
    float node_num,
    vec2 tex_coord,
    float aspect,
    float motion_offset,
    vec2 endpoint_center,
    vec2 this_center,
    vec2 next_center,
    float this_wind_direction,
    float next_wind_direction,
    float this_offset,
    float next_offset
) {
    this_center.x *= aspect;
    next_center.x *= aspect;

    vec2 node_vec = this_center - next_center;
    vec2 endpoint_node_vec = endpoint_center - next_center;
    vec2 direction = safe_normalize(node_vec);
    vec2 endpoint_direction = mix(
        safe_normalize(endpoint_node_vec),
        direction,
        pc.auto_directional_compensation
    );
    float len = dot(node_vec, direction);
    float endpoint_len = mix(
        len,
        dot(endpoint_node_vec, endpoint_direction),
        pc.auto_smooth_distance
    );
    vec2 relative_tex_coord = tex_coord - next_center;
    float endpoint_pos_x = dot(relative_tex_coord, endpoint_direction)
        - pc.auto_weight_center_offset * len;
    float pos_x = dot(relative_tex_coord, endpoint_direction);

    float this_motion_time = pc.auto_global_time_offset + pc.time_seconds * pc.auto_speed;
    float prev_motion_time = this_motion_time;
    this_motion_time += motion_offset * auto_time_offset(node_num);
    prev_motion_time += motion_offset * auto_time_offset(node_num + 1.0);

    float motion_radian = sin(this_motion_time * TAU);
    float prev_motion_radian = sin(prev_motion_time * TAU) * pc.auto_inertia;
    motion_radian += sin(this_wind_direction + PI_HALF) + sin(pc.auto_global_wind_offset);
    prev_motion_radian += sin(next_wind_direction + PI_HALF);
    prev_motion_radian *= step(0.5, node_num);
    motion_radian -= prev_motion_radian;

    return Node(
        direction,
        endpoint_direction,
        len,
        endpoint_len,
        pos_x,
        endpoint_pos_x,
        motion_radian
    );
}

void apply_node(
    inout vec4 tex_coord,
    float aspect,
    vec2 root_center,
    float this_width,
    float next_width,
    Node node,
    float mask,
    inout float auto_mask
) {
    root_center.x *= aspect;
    vec4 relative_tex_coord = tex_coord - root_center.xyxy;
    float width_mix = node.pos_x / max(node.len, 0.000001);
    float h_boundary = mix(next_width, this_width, width_mix);
    float pos_y = abs(dot(relative_tex_coord.zw, vec2(node.direction.y, -node.direction.x)));
    auto_mask = max(
        auto_mask,
        1.0 - smoothstep(h_boundary, h_boundary + pc.auto_x_feather * node.len * 2.0, pos_y)
    );

    float weight = sine_step(0.0, node.endpoint_len, node.endpoint_pos_x);
    weight *= (1.0 - weight * pc.auto_damping) * auto_mask * mask;
    tex_coord.xy = rotate_vec2(
        relative_tex_coord.xy,
        node.motion_radian * pc.auto_strength * weight
    ) + root_center;
}

vec4 apply_vertex_color(vec4 color) {
    color *= v_tint;
    color.a *= v_opacity;
    return color;
}

vec4 finalize_output(vec4 color) {
    if ((pc.output_flags & OUTPUT_FLAG_PREMULTIPLY_RGB) != 0u) {
        color.rgb *= color.a;
    }
    return color;
}

void main() {
    vec2 base_resolution = pc.texture_resolution[0];
    if ((pc.texture_resolution_mask & 1u) == 0u) {
        base_resolution = max(pc.extent, vec2(1.0));
    }
    // reverse-engineered reference: auto_sway.vert computes v_aspect as
    // g_Texture0Resolution.z / g_Texture0Resolution.w. WE resolution.zw are
    // the logical texture width/height, so the shader uses width / height.
    float aspect = max(base_resolution.x, 1.0) / max(base_resolution.y, 1.0);
    float reciprocal_aspect = 1.0 / max(aspect, 0.000001);

    vec4 tex_coord = vec4(v_uv * vec2(aspect, 1.0), v_uv * vec2(aspect, 1.0));
    float mask = 1.0;
    if ((pc.auto_flags & AUTO_SWAY_FLAG_MASK) != 0u
        && (pc.texture_resolution_mask & (1u << 1)) != 0u) {
        mask = texture(g_Texture1, v_effect_uv).r;
    }

    float motion_offset = pc.auto_inertia * pc.auto_segment_count;
    vec2 endpoint_center = pc.auto_center1;
    // WE AA_VERSION=2 scales endpointSpinCenter.x once in main; preCalcNode
    // only aspect-scales this/next centers before building the endpoint vector.
    endpoint_center.x *= aspect;
    Node node1 = pre_calc_node(
        2.0,
        tex_coord.zw,
        aspect,
        motion_offset,
        endpoint_center,
        pc.auto_center1,
        pc.auto_center2,
        pc.auto_angle2,
        pc.auto_angle3,
        pc.auto_time_offset1,
        pc.auto_time_offset2
    );
    Node node2 = pre_calc_node(
        3.0,
        tex_coord.zw,
        aspect,
        motion_offset,
        endpoint_center,
        pc.auto_center2,
        pc.auto_center3,
        pc.auto_angle3,
        pc.auto_angle4,
        pc.auto_time_offset2,
        pc.auto_time_offset3
    );
    Node node3 = pre_calc_node(
        4.0,
        tex_coord.zw,
        aspect,
        motion_offset,
        endpoint_center,
        pc.auto_center3,
        pc.auto_center4,
        pc.auto_angle4,
        pc.auto_angle5,
        pc.auto_time_offset3,
        pc.auto_time_offset4
    );

    float auto_mask = 0.0;
    apply_node(tex_coord, aspect, pc.auto_center2, pc.auto_size1, pc.auto_size2, node1, mask, auto_mask);
    apply_node(tex_coord, aspect, pc.auto_center3, pc.auto_size2, pc.auto_size3, node2, mask, auto_mask);
    apply_node(tex_coord, aspect, pc.auto_center4, pc.auto_size3, pc.auto_size4, node3, mask, auto_mask);

    tex_coord.xz *= reciprocal_aspect;
    out_color = finalize_output(apply_vertex_color(texture(g_Texture0, tex_coord.xy)));
}
