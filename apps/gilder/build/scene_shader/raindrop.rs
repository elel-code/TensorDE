pub(super) fn raindrop_fragment_source(texture_slot_mask: u32) -> String {
    assert_ne!(texture_slot_mask & 1, 0);
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform RaindropUniform {
    vec4 g_TimeAmountBlurSpeed;
    vec4 g_DensityFogVignette;
    vec4 g_Texture0Resolution;
    vec4 g_ShadowColor;
    vec4 g_HighlightColor;
} u_Effect;
float s(float a, float b, float t) { return smoothstep(a, b, t); }
float n(float t) { return fract(sin(t * 12345.564) * 7658.76); }
vec3 n13(float p) {
    vec3 p3 = fract(vec3(p) * vec3(0.1031, 0.11369, 0.13787));
    p3 += vec3(dot(p3, p3.yzx + vec3(19.19)));
    return fract(vec3(
        (p3.x + p3.y) * p3.z,
        (p3.x + p3.z) * p3.y,
        (p3.y + p3.z) * p3.x));
}
float saw(float b, float t) { return s(0.0, b, t) * s(1.0, b, t); }
vec2 dropLayer(vec2 uv_in, float time) {
    vec2 uv_base = uv_in;
    vec2 uv = uv_in;
    uv.y += time * 0.75;
    vec2 aspect = vec2(u_Effect.g_DensityFogVignette.x, 1.0);
    vec2 grid = aspect * 2.0;
    vec2 id = floor(uv * grid);
    uv.y += n(id.x);
    id = floor(uv * grid);
    vec3 noise = n13(id.x * 35.2 + id.y * 2376.1);
    vec2 st = fract(uv * grid) - vec2(0.5, 0.0);
    float x = noise.x - 0.5;
    float y = uv_base.y * 20.0;
    x += sin(y + sin(y)) * (0.5 - abs(x)) * (noise.z - 0.5);
    x *= 0.7;
    float timer = fract(time + noise.z);
    y = (saw(0.85, timer) - 0.5) * 0.9 + 0.5;
    float distance_to_drop = length((st - vec2(x, y)) * aspect.yx);
    float main_drop = s(0.4, 0.0, distance_to_drop);
    float radius = sqrt(s(1.0, y, st.y));
    float column_distance = abs(st.x - x);
    float trail = s(0.23 * radius, 0.15 * radius * radius, column_distance);
    float trail_front = s(-0.02, 0.02, st.y - y);
    trail *= trail_front * radius * radius;
    y = fract(uv_base.y * 10.0) + (st.y - 0.5);
    float droplet_distance = length(st - vec2(x, y));
    float droplets = s(0.3, 0.0, droplet_distance);
    return vec2(main_drop + droplets * radius * trail_front, trail);
}
float staticDrops(vec2 uv_in, float time) {
    vec2 uv = uv_in * 40.0;
    vec2 id = floor(uv);
    uv = fract(uv) - 0.5;
    vec3 noise = n13(id.x * 107.45 + id.y * 3543.654);
    vec2 p = (noise.xy - 0.5) * 0.7;
    return s(0.3, 0.0, length(uv - p)) * fract(noise.z * 10.0)
        * saw(0.025, fract(time + noise.z));
}
vec2 drops(vec2 uv, float time, float l0, float l1, float l2) {
    float static_drop = staticDrops(uv, time) * l0;
    vec2 moving_1 = dropLayer(uv, time) * l1;
    vec2 moving_2 = dropLayer(uv * 1.85, time) * l2;
    float coverage = s(0.3, 1.0, static_drop + moving_1.x + moving_2.x);
    return vec2(coverage, max(moving_1.y * l0, moving_2.y * l1));
}
vec3 sampleBackground(vec2 uv) {
    return texture(g_Texture0, clamp(uv, vec2(0.0), vec2(1.0))).rgb;
}
vec3 frostedGlass(vec2 uv, vec2 normal, float blur) {
    vec2 base = clamp(uv + normal, vec2(0.0), vec2(1.0));
    vec2 px = blur / max(u_Effect.g_Texture0Resolution.xy, vec2(1.0));
    vec3 color = sampleBackground(base) * 0.20;
    color += sampleBackground(base + vec2(px.x, 0.0)) * 0.13;
    color += sampleBackground(base - vec2(px.x, 0.0)) * 0.13;
    color += sampleBackground(base + vec2(0.0, px.y)) * 0.13;
    color += sampleBackground(base - vec2(0.0, px.y)) * 0.13;
    color += sampleBackground(base + px) * 0.07;
    color += sampleBackground(base - px) * 0.07;
    color += sampleBackground(base + vec2(px.x, -px.y)) * 0.07;
    color += sampleBackground(base + vec2(-px.x, px.y)) * 0.07;
    return color;
}
void main() {
    vec2 uv = v_TexCoord;
    vec2 flipped_uv = vec2(uv.x, 1.0 - uv.y);
    vec2 resolution = max(u_Effect.g_Texture0Resolution.xy, vec2(1.0));
    vec2 rain_uv = (flipped_uv * resolution - resolution * 0.5) / resolution.y;
    float amount = clamp(u_Effect.g_TimeAmountBlurSpeed.y, 0.0, 1.0);
    float time = u_Effect.g_TimeAmountBlurSpeed.x * u_Effect.g_TimeAmountBlurSpeed.w;
    float static_level = s(-0.5, 1.0, amount) * 2.0;
    float layer_1 = s(0.25, 0.75, amount);
    float layer_2 = s(0.0, 0.5, amount);
    vec2 coverage = drops(rain_uv, time, static_level, layer_1, layer_2);
    vec2 epsilon = vec2(0.001, 0.0);
    float x = drops(rain_uv + epsilon, time, static_level, layer_1, layer_2).x;
    float y = drops(rain_uv + epsilon.yx, time, static_level, layer_1, layer_2).x;
    vec2 normal = vec2(x - coverage.x, y - coverage.x);
    float minimum_blur = 2.5;
    float maximum_blur = mix(4.0, 7.0, amount);
    float focus = mix(maximum_blur - coverage.y, minimum_blur, s(0.08, 0.22, coverage.x));
    vec2 texture_normal = vec2(normal.x, -normal.y) * 1.2;
    float blur = max(0.0, focus * u_Effect.g_TimeAmountBlurSpeed.z);
    vec3 color = frostedGlass(uv, texture_normal, blur);
    float fog = smoothstep(minimum_blur, maximum_blur, focus);
    color = mix(color, vec3(0.72, 0.74, 0.78), fog * u_Effect.g_DensityFogVignette.y);
    color += u_Effect.g_HighlightColor.rgb * coverage.y;
    color += u_Effect.g_ShadowColor.rgb * coverage.x * 0.12;
    vec2 vignette_uv = flipped_uv - 0.5;
    color *= 1.0 - dot(vignette_uv, vignette_uv) * u_Effect.g_DensityFogVignette.z;
    o_Color = vec4(color, 1.0);
}
"#
    .to_owned()
}
