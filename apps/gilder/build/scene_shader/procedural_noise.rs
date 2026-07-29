use super::effect_program::effect_combo_value_for_key;

pub(super) fn procedural_noise_fragment_source(key: &str, texture_slot_mask: u32) -> String {
    assert_eq!(texture_slot_mask, 0x1);
    assert_eq!(effect_combo_value_for_key(key, "AA_CATEGORY", 0), 1);
    if effect_combo_value_for_key(key, "AB_TYPEUV", 0) == 4 {
        return procedural_curl_fragment_source(key);
    }
    assert_eq!(effect_combo_value_for_key(key, "AB_TYPEUV", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "BLENDMODE", 0), 20);
    assert_eq!(effect_combo_value_for_key(key, "STEPANIM", 0), 1);
    assert_eq!(effect_combo_value_for_key(key, "PERSPSWITCH", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "TILE", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "LAYERED", 0), 0);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ProceduralNoiseUniform {
    vec4 g_TimeSpeedDirectionDirectionSpeed;
    vec4 g_OffsetScale;
    vec4 g_MagnitudeSeedFps;
    vec4 g_Opacity;
} u_Effect;
vec2 hash23(vec3 p) {
    p = fract(p * vec3(0.1031, 0.1030, 437.195));
    p += dot(p, p.yzx + 19.19);
    return fract((p.xx + p.yz) * p.zy);
}
void main() {
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float opacity = u_Effect.g_Opacity.x;
    if (opacity <= 0.001) {
        o_Color = albedo;
        return;
    }
    vec2 resolution = vec2(textureSize(g_Texture0, 0));
    vec2 aspect = vec2(1.0, resolution.y / resolution.x);
    vec2 scale = max(
        vec2(0.000001),
        u_Effect.g_OffsetScale.zw * resolution / 3.0);
    float fps = u_Effect.g_MagnitudeSeedFps.w;
    float animate = floor(u_Effect.g_TimeSpeedDirectionDirectionSpeed.x * fps)
        / max(0.000001, fps)
        * u_Effect.g_TimeSpeedDirectionDirectionSpeed.y * 0.01
        + u_Effect.g_MagnitudeSeedFps.z;
    float direction = u_Effect.g_TimeSpeedDirectionDirectionSpeed.z;
    vec2 scroll = vec2(-sin(direction), cos(direction))
        * u_Effect.g_TimeSpeedDirectionDirectionSpeed.x
        * u_Effect.g_TimeSpeedDirectionDirectionSpeed.w * 10.0;
    vec2 transformed_offset = (u_Effect.g_OffsetScale.xy + scroll) * aspect * scale;
    vec2 coord = v_TexCoord * scale + transformed_offset;
    vec2 magnitude = u_Effect.g_MagnitudeSeedFps.xy * 0.05;
    vec2 noise_offset = (hash23(vec3(floor(coord * 0.5), animate)) - 0.5)
        * magnitude;
    vec4 displaced = texture(g_Texture0, v_TexCoord + noise_offset);
    vec3 subtract = max(albedo.rgb + displaced.rgb - vec3(1.0), vec3(0.0));
    o_Color = vec4(mix(albedo.rgb, subtract, opacity), albedo.a);
}
"#
    .to_owned()
}

fn procedural_curl_fragment_source(key: &str) -> String {
    assert_eq!(effect_combo_value_for_key(key, "AA_CATEGORY", 0), 1);
    assert_eq!(effect_combo_value_for_key(key, "AB_TYPEUV", 0), 4);
    assert_eq!(effect_combo_value_for_key(key, "BLENDMODE", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "PERSPSWITCH", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "TILE", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "LAYERED", 0), 0);
    assert_eq!(effect_combo_value_for_key(key, "STEPANIM", 0), 0);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ProceduralNoiseUniform {
    vec4 g_TimeSpeedDirectionDirectionSpeed;
    vec4 g_OffsetScale;
    vec4 g_MagnitudeSeedFps;
    vec4 g_OpacityFractalsScaleInfluence;
    vec4 g_ThresholdExponent;
    vec4 g_Gradient;
} u_Effect;
vec4 hash44(vec4 p) {
    p = fract(p * vec4(0.1031, 0.1030, 0.0973, 444.129));
    p += dot(p, p.wzxy + 19.19);
    return fract((p.xxyz + p.yzzw) * p.zywx);
}
vec4 simplex(vec3 uv, float seed) {
    const vec2 c = vec2(0.166666666667, 0.333333333333);
    vec3 lattice = floor(uv + dot(uv, c.yyy));
    vec3 x = uv - lattice + dot(lattice, c.xxx);
    vec3 ordering = step(x.yzx, x);
    vec3 first_corner = ordering * (1.0 - ordering.zxy);
    vec3 second_corner = 1.0 - ordering.zxy * (1.0 - ordering);
    vec3 x1 = x - first_corner + c.x;
    vec3 x2 = x - second_corner + c.y;
    vec3 x3 = x - 0.5;
    vec4 weight = max(0.6 - vec4(
        dot(x, x), dot(x1, x1), dot(x2, x2), dot(x3, x3)), vec4(0.0));
    weight *= weight;
    weight *= weight;
    vec4 coordinates0 = vec4(lattice, seed);
    vec4 coordinates1 = vec4(lattice + first_corner, seed);
    vec4 coordinates2 = vec4(lattice + second_corner, seed);
    vec4 coordinates3 = vec4(lattice + 1.0, seed);
    vec4 result;
    for (int channel = 0; channel < 4; ++channel) {
        float offset = float(channel);
        vec4 distance_value = vec4(
            dot(hash44(coordinates0 + offset).xyz - 0.5, x),
            dot(hash44(coordinates1 + offset).xyz - 0.5, x1),
            dot(hash44(coordinates2 + offset).xyz - 0.5, x2),
            dot(hash44(coordinates3 + offset).xyz - 0.5, x3));
        result[channel] = dot(distance_value * weight, vec4(40.0));
    }
    return result;
}
vec2 curlNoise(vec2 uv, float speed, vec2 scale) {
    vec2 epsilon = vec2(0.0, scale.y);
    vec2 result = vec2(0.0);
    float influence = 0.5;
    float octave_scale = 1.0;
    int fractals = clamp(int(u_Effect.g_OpacityFractalsScaleInfluence.y), 1, 5);
    for (int octave = 0; octave < fractals; ++octave) {
        vec4 noise = simplex(vec3((uv + epsilon) * octave_scale, speed), octave_scale);
        result += influence * vec2(noise.x - noise.y, noise.w - noise.z);
        octave_scale *= u_Effect.g_OpacityFractalsScaleInfluence.z;
        scale *= u_Effect.g_OpacityFractalsScaleInfluence.z;
        speed += speed;
        influence *= u_Effect.g_OpacityFractalsScaleInfluence.w;
    }
    return result;
}
void main() {
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float opacity = u_Effect.g_OpacityFractalsScaleInfluence.x;
    if (opacity <= 0.001) {
        o_Color = albedo;
        return;
    }
    vec2 resolution = vec2(textureSize(g_Texture0, 0));
    vec2 aspect = vec2(1.0, resolution.y / resolution.x);
    vec2 scale = max(vec2(0.000001), u_Effect.g_OffsetScale.zw * 10.0) * aspect;
    float time = u_Effect.g_TimeSpeedDirectionDirectionSpeed.x;
    float direction = u_Effect.g_TimeSpeedDirectionDirectionSpeed.z;
    vec2 scroll = vec2(-sin(direction), cos(direction)) * time
        * u_Effect.g_TimeSpeedDirectionDirectionSpeed.w * 10.0;
    vec2 transformed_offset = (u_Effect.g_OffsetScale.xy + scroll) * aspect * scale;
    vec2 coord = v_TexCoord * scale + transformed_offset;
    float animate = time * u_Effect.g_TimeSpeedDirectionDirectionSpeed.y
        + u_Effect.g_MagnitudeSeedFps.z;
    vec2 noised_coord = curlNoise(coord, animate, scale)
        * u_Effect.g_MagnitudeSeedFps.xy * 0.15 * scale;
    float magnitude = length(noised_coord);
    float scaled_magnitude = mix(
        u_Effect.g_ThresholdExponent.x,
        u_Effect.g_ThresholdExponent.y,
        pow(magnitude, u_Effect.g_ThresholdExponent.w))
        + u_Effect.g_ThresholdExponent.z;
    scaled_magnitude = (scaled_magnitude - 0.5)
        * max(0.000001, u_Effect.g_Gradient.x) + 0.5;
    noised_coord = noised_coord / max(0.000001, magnitude) * scaled_magnitude / scale;
    vec4 displaced = texture(g_Texture0, v_TexCoord + noised_coord);
    o_Color = vec4(mix(albedo.rgb, displaced.rgb, opacity), albedo.a);
}
"#
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_variant_is_not_the_white_noise_displacement_program() {
        let source = procedural_noise_fragment_source(
            "effects/procedural_noise__SLOTS_1__AA_CATEGORY_1__AB_TYPEUV_4",
            1,
        );
        assert!(source.contains("curlNoise"));
        assert!(source.contains("simplex"));
        assert!(!source.contains("floor(coord * 0.5)"));
    }
}
