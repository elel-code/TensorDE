use super::effect_program::effect_combo_value_for_key;

pub(super) fn auto_sway_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    assert_eq!(effect_combo_value_for_key(key, "AA_VERSION", 2), 2);
    assert_eq!(effect_combo_value_for_key(key, "NODE_COUNT", 2), 4);
    assert_eq!(effect_combo_value_for_key(key, "DEBUG", 1), 0);
    assert_eq!(effect_combo_value_for_key(key, "INTERPOLATION", 5), 5);
    assert_eq!(effect_combo_value_for_key(key, "AUTO_TIMEOFFSET", 1), 1);
    assert_eq!(
        effect_combo_value_for_key(key, "AUTO_TIMEOFFSET_INTERPOLATION", 0),
        0
    );
    assert_eq!(effect_combo_value_for_key(key, "VERTICAL_AUTO_MASK", 1), 1);
    assert_eq!(
        effect_combo_value_for_key(key, "VERTICAL_RELATIVE_WIDTH", 1),
        1
    );
    assert_eq!(effect_combo_value_for_key(key, "EXPONENT", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "NOISE", 0), 0);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform AutoSwayUniform {
    vec4 g_TimeGlobalOffsetSpeedInertia;
    vec4 g_SegmentWeightSmoothDirectional;
    vec4 g_StrengthDampingFeatherWind;
    vec4 g_Center1Center2;
    vec4 g_Center3Center4;
    vec4 g_Size1Size4;
    vec4 g_Angle2Angle5;
} u_Effect;
const float PI_HALF = 1.5707963267948966;
const float TWO_PI = 6.283185307179586;
vec2 rotateVec2(vec2 value, float angle) {
    vec2 cs = vec2(cos(angle), sin(angle));
    return vec2(
        value.x * cs.x - value.y * cs.y,
        value.x * cs.y + value.y * cs.x);
}
void preCalcNode(
    int node_number,
    vec2 original_uv,
    float aspect,
    vec2 endpoint_center,
    vec2 current_center,
    vec2 next_center,
    float current_wind,
    float next_wind,
    out vec2 direction,
    out float length_to_next,
    out float endpoint_length,
    out float position_x,
    out float endpoint_position_x,
    out float motion_radians)
{
    endpoint_center.x *= aspect;
    current_center.x *= aspect;
    next_center.x *= aspect;
    vec2 node_vector = current_center - next_center;
    vec2 endpoint_vector = endpoint_center - next_center;
    direction = normalize(node_vector);
    vec2 endpoint_direction = mix(
        normalize(endpoint_vector),
        direction,
        u_Effect.g_SegmentWeightSmoothDirectional.w);
    length_to_next = dot(node_vector, direction);
    endpoint_length = mix(
        length_to_next,
        dot(endpoint_vector, endpoint_direction),
        u_Effect.g_SegmentWeightSmoothDirectional.z);
    vec2 relative_uv = original_uv - next_center;
    endpoint_position_x = dot(relative_uv, endpoint_direction)
        - u_Effect.g_SegmentWeightSmoothDirectional.y * length_to_next;
    position_x = dot(relative_uv, endpoint_direction);

    float base_time = u_Effect.g_TimeGlobalOffsetSpeedInertia.y
        + u_Effect.g_TimeGlobalOffsetSpeedInertia.x
            * u_Effect.g_TimeGlobalOffsetSpeedInertia.z;
    float motion_offset = u_Effect.g_TimeGlobalOffsetSpeedInertia.w
        * u_Effect.g_SegmentWeightSmoothDirectional.x;
    float current_phase = clamp(float(node_number - 2) * 0.5, 0.0, 1.0);
    float previous_phase = clamp(float(node_number - 1) * 0.5, 0.0, 1.0);
    float current_motion = sin((base_time + motion_offset * current_phase) * TWO_PI);
    float previous_motion = sin((base_time + motion_offset * previous_phase) * TWO_PI)
        * u_Effect.g_TimeGlobalOffsetSpeedInertia.w;
    current_motion += sin(current_wind + PI_HALF)
        + sin(u_Effect.g_StrengthDampingFeatherWind.w);
    previous_motion += sin(next_wind + PI_HALF);
    motion_radians = current_motion - previous_motion;
}
void applyNode(
    inout vec4 tex_coord,
    float aspect,
    vec2 root_center,
    float current_width,
    float next_width,
    vec2 direction,
    float length_to_next,
    float endpoint_length,
    float position_x,
    float endpoint_position_x,
    float motion_radians,
    inout float auto_mask)
{
    root_center.x *= aspect;
    vec4 relative_uv = tex_coord - root_center.xyxy;
    float horizontal_boundary = mix(
        next_width,
        current_width,
        position_x / length_to_next);
    float position_y = abs(dot(
        relative_uv.zw,
        vec2(direction.y, -direction.x)));
    auto_mask = max(
        auto_mask,
        1.0 - smoothstep(
            horizontal_boundary,
            horizontal_boundary
                + u_Effect.g_StrengthDampingFeatherWind.z
                    * length_to_next * 2.0,
            position_y));
    float weight = sin(
        clamp(endpoint_position_x / endpoint_length, 0.0, 1.0) * PI_HALF);
    weight *= (1.0 - weight * u_Effect.g_StrengthDampingFeatherWind.y)
        * auto_mask;
    tex_coord.xy = rotateVec2(
        relative_uv.xy,
        motion_radians * u_Effect.g_StrengthDampingFeatherWind.x * weight)
        + root_center;
}
void main() {
    vec2 source_size = vec2(textureSize(g_Texture0, 0));
    float aspect = source_size.x / source_size.y;
    vec4 tex_coord = v_TexCoord.xyxy;
    tex_coord.xz *= aspect;
    vec2 centers[4] = vec2[](
        u_Effect.g_Center1Center2.xy,
        u_Effect.g_Center1Center2.zw,
        u_Effect.g_Center3Center4.xy,
        u_Effect.g_Center3Center4.zw);
    float sizes[4] = float[](
        u_Effect.g_Size1Size4.x,
        u_Effect.g_Size1Size4.y,
        u_Effect.g_Size1Size4.z,
        u_Effect.g_Size1Size4.w);
    float winds[4] = float[](
        u_Effect.g_Angle2Angle5.x,
        u_Effect.g_Angle2Angle5.y,
        u_Effect.g_Angle2Angle5.z,
        u_Effect.g_Angle2Angle5.w);
    float auto_mask = 0.0;
    for (int node = 0; node < 3; node++) {
        vec2 direction;
        float length_to_next;
        float endpoint_length;
        float position_x;
        float endpoint_position_x;
        float motion_radians;
        preCalcNode(
            node + 2,
            tex_coord.zw,
            aspect,
            centers[0],
            centers[node],
            centers[node + 1],
            winds[node],
            winds[node + 1],
            direction,
            length_to_next,
            endpoint_length,
            position_x,
            endpoint_position_x,
            motion_radians);
        applyNode(
            tex_coord,
            aspect,
            centers[node + 1],
            sizes[node],
            sizes[node + 1],
            direction,
            length_to_next,
            endpoint_length,
            position_x,
            endpoint_position_x,
            motion_radians,
            auto_mask);
    }
    tex_coord.x /= aspect;
    o_Color = texture(g_Texture0, tex_coord.xy);
}
"#
    .to_owned()
}
