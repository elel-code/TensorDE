pub(super) fn framebuffer_water_opacity_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 2) uniform FramebufferWaterDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(location = 0) out vec2 v_TexCoord;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    vec2 position = positions[gl_VertexIndex];
    v_TexCoord = position * 0.5 + 0.5;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_owned()
}

pub(super) fn framebuffer_water_shake_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(set = 0, binding = 2) uniform FramebufferWaterDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(location = 0) out vec2 v_TexCoord;
void main() {
    v_TexCoord = a_TexCoord;
    vec2 screen_uv = vec2(
        dot(u_Draw.g_ObjectUvToScreenUvRow0.xyz, vec3(a_TexCoord, 1.0)),
        dot(u_Draw.g_ObjectUvToScreenUvRow1.xyz, vec3(a_TexCoord, 1.0)));
    gl_Position = vec4(screen_uv * 2.0 - 1.0, 0.0, 1.0);
}
"#
    .to_owned()
}

pub(super) fn framebuffer_water_opacity_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_CausticsPrepass;
layout(set = 0, binding = 3) uniform FramebufferWaterOpacityProgram {
    vec4 g_WavesTimeSpeedScaleStrength;
    vec4 g_WavesDirectionExponentOpacityUnused;
    vec4 g_StageEnabled;
} u_Effect;
vec4 quantizeUnorm8(vec4 value) {
    return roundEven(clamp(value, 0.0, 1.0) * 255.0) / 255.0;
}
vec2 rotateVec2(vec2 value, float angle) {
    vec2 cs = vec2(cos(angle), sin(angle));
    return vec2(value.x * cs.x - value.y * cs.y,
        value.x * cs.y + value.y * cs.x);
}
float shapedSine(float phase, float exponent) {
    float wave = sin(phase);
    return pow(abs(wave), exponent) * sign(wave);
}
vec2 waterOffset(vec2 output_uv) {
    if (u_Effect.g_StageEnabled.x <= 0.5) {
        return vec2(0.0);
    }
    vec2 direction = rotateVec2(
        vec2(0.0, 1.0), u_Effect.g_WavesDirectionExponentOpacityUnused.x);
    float distance = u_Effect.g_WavesTimeSpeedScaleStrength.x
        * u_Effect.g_WavesTimeSpeedScaleStrength.y
        + dot(output_uv, direction) * u_Effect.g_WavesTimeSpeedScaleStrength.z;
    float displacement = shapedSine(
        distance, u_Effect.g_WavesDirectionExponentOpacityUnused.y);
    float strength = u_Effect.g_WavesTimeSpeedScaleStrength.w;
    return vec2(direction.y, -direction.x)
        * displacement * strength * strength;
}
void main() {
    vec4 color = texture(g_CausticsPrepass, v_TexCoord + waterOffset(v_TexCoord));
    color = quantizeUnorm8(color);
    if (u_Effect.g_StageEnabled.y > 0.5) {
        color.a *= u_Effect.g_WavesDirectionExponentOpacityUnused.z;
    }
    o_Color = color;
}
"#
    .to_owned()
}

pub(super) fn framebuffer_water_shake_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_OpacityTarget;
layout(set = 0, binding = 1) uniform sampler2D g_ShakeFlow;
layout(set = 0, binding = 3) uniform FramebufferWaterShakeProgram {
    vec4 g_ShakeTimeSpeedStrengthUnused;
    vec4 g_ShakeBoundsFriction;
    vec4 g_FlowResolution;
    vec4 g_StageEnabled;
} u_Effect;
float shakeOffset() {
    float time = u_Effect.g_ShakeTimeSpeedStrengthUnused.x
        * u_Effect.g_ShakeTimeSpeedStrengthUnused.y;
    float sine = sin(fract(time * 0.159155) * 6.283185) * 0.498 + 0.5;
    float positive_half = step(0.0, cos(time));
    float shaped = mix(
        1.0 - pow(1.0 - sine, u_Effect.g_ShakeBoundsFriction.z),
        pow(sine, u_Effect.g_ShakeBoundsFriction.w),
        positive_half);
    float bounded = clamp(
        (shaped - u_Effect.g_ShakeBoundsFriction.x)
            / (u_Effect.g_ShakeBoundsFriction.y - u_Effect.g_ShakeBoundsFriction.x),
        0.0,
        1.0);
    return bounded * 2.0 - 1.0;
}
void main() {
    vec2 shake_uv = v_TexCoord;
    if (u_Effect.g_StageEnabled.x > 0.5) {
        vec2 flow_uv = v_TexCoord * u_Effect.g_FlowResolution.zw
            / u_Effect.g_FlowResolution.xy;
        vec2 flow = (texture(g_ShakeFlow, flow_uv).rg - vec2(0.498)) * 2.0;
        float strength = u_Effect.g_ShakeTimeSpeedStrengthUnused.z;
        shake_uv += shakeOffset() * strength * strength * flow;
    }
    o_Color = texture(g_OpacityTarget, shake_uv);
}
"#
    .to_owned()
}
