use super::flat_rounded_mask_support_vertex_source;

#[path = "final_effect/framebuffer_water.rs"]
mod framebuffer_water;

use framebuffer_water::{
    framebuffer_water_opacity_fragment_source, framebuffer_water_opacity_vertex_source,
    framebuffer_water_shake_fragment_source, framebuffer_water_shake_vertex_source,
};

pub(crate) const FINAL_EFFECT_SHADER_SPECS: &[super::SceneShaderSpec] = &[
    super::SceneShaderSpec {
        key: "we/image-waterwaves-final",
        family: super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/image-waterripple-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/image-waterripple-modulate-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/image-scroll-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/image-colorkey-scroll-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/image-cloudmotion-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/puppet-opacity-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/puppet-opacity-clipping-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/puppet-iris-waterripple-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/puppet-iris-waterripple-clipping-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/flat-rounded-opacity-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/tech-circle-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/framebuffer-water-quantized-water-opacity",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/framebuffer-water-quantized-shake-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
];

pub(crate) fn final_effect_parameter_layout(key: &str) -> &'static str {
    match key {
        "we/image-waterwaves-final"
        | "we/image-waterripple-final"
        | "we/image-waterripple-modulate-final"
        | "we/image-scroll-final"
        | "we/image-colorkey-scroll-final"
        | "we/image-cloudmotion-final"
        | "we/puppet-opacity-final"
        | "we/puppet-opacity-clipping-final"
        | "we/puppet-iris-waterripple-final"
        | "we/puppet-iris-waterripple-clipping-final"
        | "we/flat-rounded-opacity-final"
        | "we/tech-circle-final"
        | "we/framebuffer-water-quantized-water-opacity"
        | "we/framebuffer-water-quantized-shake-final" => "FinalEffectProgram",
        _ => "None",
    }
}

pub(crate) fn final_effect_sources(key: &str) -> (String, String) {
    let fragment = match key {
        "we/image-waterwaves-final" => final_waterwaves_fragment_source(),
        "we/image-waterripple-final" => final_waterripple_fragment_source(),
        "we/image-waterripple-modulate-final" => final_waterripple_modulate_fragment_source(),
        "we/image-scroll-final" => final_scroll_fragment_source(),
        "we/image-colorkey-scroll-final" => final_colorkey_scroll_fragment_source(),
        "we/image-cloudmotion-final" => final_cloudmotion_fragment_source(),
        "we/puppet-opacity-final" => final_puppet_opacity_fragment_source(),
        "we/puppet-opacity-clipping-final" => {
            final_clipping_fragment_source(final_puppet_opacity_fragment_source())
        }
        "we/puppet-iris-waterripple-final" => final_puppet_iris_waterripple_fragment_source(),
        "we/puppet-iris-waterripple-clipping-final" => {
            final_clipping_fragment_source(final_puppet_iris_waterripple_fragment_source())
        }
        "we/flat-rounded-opacity-final" => final_flat_rounded_opacity_fragment_source(),
        "we/tech-circle-final" => final_tech_circle_fragment_source(),
        "we/framebuffer-water-quantized-water-opacity" => {
            framebuffer_water_opacity_fragment_source()
        }
        "we/framebuffer-water-quantized-shake-final" => framebuffer_water_shake_fragment_source(),
        _ => panic!("unknown final effect shader {key:?}"),
    };
    let vertex = match key {
        "we/puppet-opacity-final" | "we/puppet-iris-waterripple-final" => {
            super::super::scene_puppet_skinning_vertex_source()
        }
        "we/puppet-opacity-clipping-final" | "we/puppet-iris-waterripple-clipping-final" => {
            super::super::scene_puppet_skinning_clippingtarget_vertex_source()
        }
        "we/flat-rounded-opacity-final" => flat_rounded_mask_support_vertex_source(),
        "we/framebuffer-water-quantized-water-opacity" => framebuffer_water_opacity_vertex_source(),
        "we/framebuffer-water-quantized-shake-final" => framebuffer_water_shake_vertex_source(),
        _ => super::super::scene_mesh_vertex_source(),
    };
    (vertex, fragment)
}

fn final_clipping_fragment_source(source: String) -> String {
    source
        .replacen(
            "layout(location = 0) out vec4 o_Color;",
            "layout(location = 2) in vec3 v_ScreenPos;\nlayout(location = 0) out vec4 o_Color;",
            1,
        )
        .replacen(
            "layout(set = 0, binding = 0) uniform sampler2D g_SourceTexture;",
            "layout(set = 0, binding = 0) uniform sampler2D g_SourceTexture;\nlayout(set = 0, binding = 8) uniform sampler2D g_FullAlphaMask;",
            1,
        )
        .replacen(
            "color.a *= v_VertexAlpha;",
            "vec2 screen_uv = (v_ScreenPos.xy / v_ScreenPos.z) * 0.5 + 0.5;\n    float clip_factor = texture(g_FullAlphaMask, screen_uv).r;\n    color.a *= clip_factor * v_VertexAlpha;",
            1,
        )
}

fn final_cloudmotion_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SourceTexture;
layout(set = 0, binding = 2) uniform sampler2D g_NoiseTexture;
layout(set = 0, binding = 3) uniform FinalCloudMotionProgram {
    vec4 g_ResolvedColorAlpha;
    vec4 g_TimeSpeedAmountDirection;
    vec4 g_ScaleScaleXAspectUnused;
} u_Effect;
void main() {
    vec2 source_uv = v_TexCoord;
    float amount = u_Effect.g_TimeSpeedAmountDirection.z;
    if (amount != 0.0) {
    float time = u_Effect.g_TimeSpeedAmountDirection.x
        * u_Effect.g_TimeSpeedAmountDirection.y;
    vec2 noise_uv = v_TexCoord;
    noise_uv.x *= max(u_Effect.g_ScaleScaleXAspectUnused.z, 0.0001);
    noise_uv *= max(u_Effect.g_ScaleScaleXAspectUnused.x, 0.1);
    noise_uv.x *= max(u_Effect.g_ScaleScaleXAspectUnused.y, 0.1);
    noise_uv.x += time;
    float noise = texture(g_NoiseTexture, noise_uv).x * 2.0 - 1.0;
    float angle = u_Effect.g_TimeSpeedAmountDirection.w + 1.5707963;
    vec2 direction = vec2(cos(angle), sin(angle));
    vec2 offset = direction * noise * amount;
    source_uv = clamp(v_TexCoord + offset, vec2(0.001), vec2(0.999));
    }
    vec4 color = texture(g_SourceTexture, source_uv);
    color *= u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn final_waterwaves_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SourceTexture;
layout(set = 0, binding = 1) uniform sampler2D g_MaskTexture;
layout(set = 0, binding = 3) uniform FinalWaterWavesMaterial {
    vec4 g_ResolvedColorAlpha;
    vec4 g_TimeSpeedScaleStrength;
    vec4 g_DirectionSpeed2Scale2Direction2;
    vec4 g_Offset2DualExponentExponent2;
    vec4 g_MaskEnabled;
    vec4 g_MaskResolution;
} u_Effect;
vec2 rotateVec2(vec2 value, float angle) {
    vec2 cs = vec2(cos(angle), sin(angle));
    return vec2(value.x * cs.x - value.y * cs.y,
        value.x * cs.y + value.y * cs.x);
}
float shapedSine(float phase, float exponent) {
    float wave = sin(phase);
    return pow(abs(wave), max(exponent, 0.0001)) * sign(wave);
}
void main() {
    vec2 source_uv = v_TexCoord;
    float strength = u_Effect.g_TimeSpeedScaleStrength.w;
    vec2 offset = vec2(0.0);
    if (strength != 0.0) {
    float mask = 1.0;
    if (u_Effect.g_MaskEnabled.x > 0.5) {
        vec2 mask_uv = v_TexCoord * u_Effect.g_MaskResolution.zw
            / max(u_Effect.g_MaskResolution.xy, vec2(1.0));
        mask = texture(g_MaskTexture, mask_uv).r;
    }
    vec2 direction = rotateVec2(
        vec2(0.0, 1.0), u_Effect.g_DirectionSpeed2Scale2Direction2.x);
    float distance0 = u_Effect.g_TimeSpeedScaleStrength.x
        * u_Effect.g_TimeSpeedScaleStrength.y
        + dot(v_TexCoord, direction) * u_Effect.g_TimeSpeedScaleStrength.z;
    float displacement = shapedSine(
        distance0, u_Effect.g_Offset2DualExponentExponent2.z);
    if (u_Effect.g_Offset2DualExponentExponent2.y > 0.5) {
        vec2 direction2 = rotateVec2(
            vec2(0.0, 1.0), u_Effect.g_DirectionSpeed2Scale2Direction2.w);
        float distance1 = (u_Effect.g_TimeSpeedScaleStrength.x
            + u_Effect.g_Offset2DualExponentExponent2.x)
            * u_Effect.g_DirectionSpeed2Scale2Direction2.y
            + dot(v_TexCoord, direction2)
                * u_Effect.g_DirectionSpeed2Scale2Direction2.z;
        displacement *= shapedSine(
            distance1, u_Effect.g_Offset2DualExponentExponent2.w);
    }
    offset = vec2(direction.y, -direction.x)
        * displacement * strength * strength * mask;
    }
    vec4 color = texture(g_SourceTexture, source_uv + offset)
        * u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn final_waterripple_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SourceTexture;
layout(set = 0, binding = 1) uniform sampler2D g_MaskTexture;
layout(set = 0, binding = 2) uniform sampler2D g_NormalTexture;
layout(set = 0, binding = 3) uniform FinalWaterRippleMaterial {
    vec4 g_ResolvedColorAlpha;
    vec4 g_TimeAnimationScaleScroll;
    vec4 g_DirectionStrengthAspectNormal;
    vec4 g_MaskFlags;
    vec4 g_MaskResolution;
} u_Effect;
vec2 rotateVec2(vec2 value, float angle) {
    vec2 cs = vec2(cos(angle), sin(angle));
    return vec2(value.x * cs.x - value.y * cs.y,
        value.x * cs.y + value.y * cs.x);
}
void main() {
    vec2 source_uv = v_TexCoord;
    float strength = u_Effect.g_DirectionStrengthAspectNormal.y;
    if (strength != 0.0) {
    vec2 scroll = rotateVec2(
        vec2(0.0, 1.0), u_Effect.g_DirectionStrengthAspectNormal.x)
        * u_Effect.g_TimeAnimationScaleScroll.w
        * u_Effect.g_TimeAnimationScaleScroll.w
        * u_Effect.g_TimeAnimationScaleScroll.x;
    float animation = u_Effect.g_TimeAnimationScaleScroll.x
        * u_Effect.g_TimeAnimationScaleScroll.y
        * u_Effect.g_TimeAnimationScaleScroll.y;
    vec4 ripple = vec4(source_uv + animation + scroll,
        source_uv * 1.333 - animation + scroll)
        * u_Effect.g_TimeAnimationScaleScroll.z;
    ripple.xz *= u_Effect.g_DirectionStrengthAspectNormal.z;
    ripple.yw *= u_Effect.g_MaskFlags.w;
    vec3 normal0 = texture(g_NormalTexture, ripple.xy).xyz * 2.0 - 1.0;
    vec3 normal1 = texture(g_NormalTexture, ripple.zw).xyz * 2.0 - 1.0;
    vec3 normal = normalize(vec3(normal0.xy + normal1.xy, normal0.z));
    float mask = 1.0;
    if (u_Effect.g_MaskFlags.x > 0.5) {
        vec2 mask_uv = source_uv * u_Effect.g_MaskResolution.zw
            / max(u_Effect.g_MaskResolution.xy, vec2(1.0));
        mask = texture(g_MaskTexture, mask_uv).r;
    }
    source_uv += normal.xy * strength * strength * mask;
    }
    vec4 color = texture(g_SourceTexture, source_uv);
    color *= u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn final_waterripple_modulate_fragment_source() -> String {
    final_waterripple_fragment_source().replacen(
        "    o_Color = color;",
        "    color.rgb *= color.a;\n    o_Color = color;",
        1,
    )
}

fn final_tech_circle_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 3) uniform FinalTechCircleProgram {
    vec4 g_ResolvedColorAlpha;
    vec4 g_ColorAlpha;
    vec4 g_TimeSpeedSkewRingRadius;
    vec4 g_RingWidthSegmentsSectorOffset;
    vec4 g_SectorWidthSegments;
} u_Effect;
const float TAU = 6.283185307179586;
float saw(float x) {
    return abs(mod(x * 2.0 + 1.0, 2.0) - 1.0);
}
float stripes(float count, float threshold, float x) {
    return step(1.0 - threshold, saw(x * count))
        * step(0.0, x) * step(x, 1.0);
}
void main() {
    vec2 centered = v_TexCoord - 0.5;
    vec2 uv = vec2(length(centered) * 2.0,
        atan(centered.x, centered.y) / TAU + 0.5);
    float ring_radius = u_Effect.g_TimeSpeedSkewRingRadius.w;
    float ring_width = max(u_Effect.g_RingWidthSegmentsSectorOffset.x, 0.000001);
    float perimeter_ratio = uv.x / max(ring_radius, 0.000001);
    uv.y += ((uv.x - ring_radius) / ring_width / max(perimeter_ratio, 0.000001))
        * u_Effect.g_TimeSpeedSkewRingRadius.z;
    float ring_value = stripes(1.0, 1.0,
        (uv.x - ring_radius + ring_width * 0.5) / ring_width);
    float sector_position = u_Effect.g_RingWidthSegmentsSectorOffset.w
        + u_Effect.g_TimeSpeedSkewRingRadius.x * u_Effect.g_TimeSpeedSkewRingRadius.y;
    float sector_width = max(u_Effect.g_SectorWidthSegments.x, 0.000001);
    float sector_coordinate = fract(uv.y - fract(sector_position - sector_width * 0.5));
    float sector_value = stripes(
        u_Effect.g_SectorWidthSegments.y,
        u_Effect.g_SectorWidthSegments.z,
        sector_coordinate / sector_width);
    float alpha = ring_value * sector_value * u_Effect.g_ColorAlpha.a
        * u_Effect.g_ResolvedColorAlpha.a * v_VertexAlpha;
    o_Color = vec4(
        u_Effect.g_ColorAlpha.rgb * u_Effect.g_ResolvedColorAlpha.rgb,
        alpha);
}
"#
    .to_owned()
}

fn final_scroll_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SourceTexture;
layout(set = 0, binding = 3) uniform FinalScrollProgram {
    vec4 g_ResolvedColorAlpha;
    vec4 g_TimeSpeed;
    vec4 g_Repeat;
} u_Effect;
void main() {
    vec2 source_uv = v_TexCoord;
    if (u_Effect.g_TimeSpeed.w > 0.5) {
        vec2 speed = u_Effect.g_TimeSpeed.yz;
        vec2 scroll = sign(speed) * speed * speed * u_Effect.g_TimeSpeed.x;
        source_uv = fract((v_TexCoord + scroll) * u_Effect.g_Repeat.xy);
    }
    vec4 color = texture(g_SourceTexture, source_uv)
        * u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn final_colorkey_scroll_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SourceTexture;
layout(set = 0, binding = 3) uniform FinalColorKeyScrollProgram {
    vec4 g_ResolvedColorAlpha;
    vec4 g_TimeSpeed;
    vec4 g_Repeat;
    vec4 g_AlphaFuzzToleranceInvert;
    vec4 g_KeyColorFlatten;
} u_Effect;
void main() {
    vec2 speed = u_Effect.g_TimeSpeed.yz;
    vec2 scroll = sign(speed) * speed * speed * u_Effect.g_TimeSpeed.x;
    vec2 source_uv = v_TexCoord;
    if (u_Effect.g_TimeSpeed.w > 0.5) {
        source_uv = fract((v_TexCoord + scroll) * u_Effect.g_Repeat.xy);
    }
    vec4 color = texture(g_SourceTexture, source_uv);
    float delta = dot(abs(u_Effect.g_KeyColorFlatten.rgb - color.rgb), vec3(1.0));
    float blend = smoothstep(
        0.001,
        0.002 + u_Effect.g_AlphaFuzzToleranceInvert.y,
        delta - u_Effect.g_AlphaFuzzToleranceInvert.z);
    if (u_Effect.g_AlphaFuzzToleranceInvert.w > 0.5) {
        blend = 1.0 - blend;
    }
    color.a *= mix(u_Effect.g_AlphaFuzzToleranceInvert.x, 1.0, blend);
    if (u_Effect.g_KeyColorFlatten.w > 0.5) {
        color.rgb *= color.a;
    }
    color *= u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn final_puppet_opacity_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SourceTexture;
layout(set = 0, binding = 1) uniform sampler2D g_OpacityMask;
layout(set = 0, binding = 3) uniform FinalPuppetOpacityProgram {
    vec4 g_ResolvedColorAlpha;
    vec4 g_OpacityMaskFlags;
    vec4 g_MaskResolution;
} u_Effect;
void main() {
    vec4 color = texture(g_SourceTexture, v_TexCoord);
    if (u_Effect.g_OpacityMaskFlags.y > 0.5) {
        vec2 mask_uv = v_TexCoord * u_Effect.g_MaskResolution.zw
            / max(u_Effect.g_MaskResolution.xy, vec2(1.0));
        color.a *= texture(g_OpacityMask, mask_uv).r;
    }
    color.a *= u_Effect.g_OpacityMaskFlags.x;
    color *= u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn final_puppet_iris_waterripple_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SourceTexture;
layout(set = 0, binding = 1) uniform sampler2D g_IrisMask;
layout(set = 0, binding = 2) uniform sampler2D g_RippleMask;
layout(set = 0, binding = 35) uniform sampler2D g_RippleNormal;
layout(set = 0, binding = 3) uniform FinalPuppetIrisRippleProgram {
    vec4 g_ResolvedColorAlpha;
    vec4 g_IrisTimeSpeedRoughNoise;
    vec4 g_IrisScalePhaseMask;
    vec4 g_IrisMaskResolution;
    vec4 g_IrisEyeColorBackground;
    vec4 g_RippleTimeAnimationScaleScroll;
    vec4 g_RippleDirectionStrengthAspectNormal;
    vec4 g_RippleFlags;
    vec4 g_RippleMaskResolution;
    vec4 g_StageEnabled;
} u_Effect;
vec2 rotateVec2(vec2 value, float angle) {
    vec2 cs = vec2(cos(angle), sin(angle));
    return vec2(value.x * cs.x - value.y * cs.y,
        value.x * cs.y + value.y * cs.x);
}
vec2 irisOffset() {
    float time = u_Effect.g_IrisTimeSpeedRoughNoise.x
        * u_Effect.g_IrisTimeSpeedRoughNoise.y
        + u_Effect.g_IrisScalePhaseMask.z;
    float low_dt = floor(time);
    vec2 motion2 = sin(1.9 * (low_dt + vec2(0.0, 1.0)));
    vec4 motion4 = sin(2.5 * (low_dt + vec4(0.0, 0.0, 1.0, 1.0))
        + vec4(1.0, 2.0, 1.0, 2.0));
    vec2 move_start = motion2.xx + motion4.xy;
    vec2 move_end = motion2.yy + motion4.zw;
    float blend = cos(fract(time) * 3.14159265358979323846) * -0.5 + 0.5;
    vec2 offset = mix(move_start, move_end, smoothstep(
        1.0 - u_Effect.g_IrisTimeSpeedRoughNoise.z, 1.0, blend));
    offset.x += sin(time) * u_Effect.g_IrisTimeSpeedRoughNoise.w;
    offset.y += cos(time) * u_Effect.g_IrisTimeSpeedRoughNoise.w;
    return offset * u_Effect.g_IrisScalePhaseMask.xy * 0.001;
}
vec2 rippleSourceUv(vec2 uv) {
    vec2 scroll = rotateVec2(
        vec2(0.0, 1.0), u_Effect.g_RippleDirectionStrengthAspectNormal.x)
        * u_Effect.g_RippleTimeAnimationScaleScroll.w
        * u_Effect.g_RippleTimeAnimationScaleScroll.w
        * u_Effect.g_RippleTimeAnimationScaleScroll.x;
    float animation = u_Effect.g_RippleTimeAnimationScaleScroll.x
        * u_Effect.g_RippleTimeAnimationScaleScroll.y
        * u_Effect.g_RippleTimeAnimationScaleScroll.y;
    vec4 ripple = vec4(uv + animation + scroll,
        uv * 1.333 - animation + scroll)
        * u_Effect.g_RippleTimeAnimationScaleScroll.z;
    ripple.xz *= u_Effect.g_RippleDirectionStrengthAspectNormal.z;
    ripple.yw *= u_Effect.g_RippleDirectionStrengthAspectNormal.w;
    vec3 normal = vec3(0.0, 0.0, 1.0);
    if (u_Effect.g_RippleFlags.y > 0.5) {
        vec3 normal0 = texture(g_RippleNormal, ripple.xy).xyz * 2.0 - 1.0;
        vec3 normal1 = texture(g_RippleNormal, ripple.zw).xyz * 2.0 - 1.0;
        normal = normalize(vec3(normal0.xy + normal1.xy, normal0.z));
    }
    float mask = 1.0;
    if (u_Effect.g_RippleFlags.x > 0.5) {
        vec2 mask_uv = uv * u_Effect.g_RippleMaskResolution.zw
            / max(u_Effect.g_RippleMaskResolution.xy, vec2(1.0));
        mask = texture(g_RippleMask, mask_uv).r;
    }
    float strength = u_Effect.g_RippleDirectionStrengthAspectNormal.y;
    return uv + normal.xy * strength * strength * mask;
}
void main() {
    vec2 ripple_uv = v_TexCoord;
    if (u_Effect.g_StageEnabled.y > 0.5) {
        ripple_uv = rippleSourceUv(v_TexCoord);
    }
    vec4 color;
    if (u_Effect.g_StageEnabled.x > 0.5) {
        vec2 offset = irisOffset();
        vec2 source_uv = ripple_uv + offset;
        float iris_mask = 1.0;
        vec2 mask_uv = ripple_uv;
        if (u_Effect.g_IrisScalePhaseMask.w > 0.5) {
            mask_uv = ripple_uv * u_Effect.g_IrisMaskResolution.zw
                / max(u_Effect.g_IrisMaskResolution.xy, vec2(1.0));
            iris_mask = texture(g_IrisMask, mask_uv).r;
            source_uv = ripple_uv + offset * iris_mask;
        }
        color = texture(g_SourceTexture, source_uv);
        if (u_Effect.g_IrisEyeColorBackground.w > 0.5) {
            float shifted_mask = texture(g_IrisMask, mask_uv + offset * iris_mask).r;
            color.rgb = mix(u_Effect.g_IrisEyeColorBackground.rgb, color.rgb, shifted_mask);
        }
    } else {
        color = texture(g_SourceTexture, ripple_uv);
    }
    color *= u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn final_flat_rounded_opacity_fragment_source() -> String {
    r#"#version 450
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec3 v_ObjectPixelExtent;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 3) uniform FinalRoundedOpacityProgram {
    vec4 g_ColorRadius;
    vec4 g_SizeSoftnessAlpha;
    vec4 g_BorderWidthOpacity;
    vec4 g_ResolvedColorAlpha;
} u_Effect;
float roundedBoxSdf(vec2 point, vec2 size, float radius) {
    vec2 half_size = size * 0.5;
    float half_min = min(half_size.x, half_size.y);
    float r = clamp(radius * half_min, 0.001, half_min);
    vec2 delta = abs(point) - (half_size - r);
    return length(max(delta, 0.0)) - r;
}
void main() {
    float width_pixels = max(v_ObjectPixelExtent.x, 1.0);
    float height_pixels = max(v_ObjectPixelExtent.y, 1.0);
    vec2 aspect_scale = vec2(
        max(1.0, width_pixels / height_pixels),
        max(1.0, height_pixels / width_pixels));
    vec2 mask_uv = (v_ObjectTexCoord - 0.5) * aspect_scale + 0.5;
    vec2 mask_size = u_Effect.g_SizeSoftnessAlpha.xy * aspect_scale;
    float distance = roundedBoxSdf(
        mask_uv - vec2(0.5), mask_size, u_Effect.g_ColorRadius.w);
    float edge_softness = u_Effect.g_SizeSoftnessAlpha.z
        / max(v_ObjectPixelExtent.z, 1.0) * 2.0;
    float mask_alpha = smoothstep(edge_softness, 0.0, distance);
    float rounded_enabled = step(0.5, u_Effect.g_BorderWidthOpacity.z);
    mask_alpha = mix(1.0, mask_alpha, rounded_enabled);
    float effect_alpha = mix(
        1.0,
        mask_alpha * u_Effect.g_SizeSoftnessAlpha.w,
        rounded_enabled);
    vec3 rounded_color = mix(
        u_Effect.g_ColorRadius.rgb, vec3(1.0), effect_alpha);
    float alpha = mask_alpha * u_Effect.g_BorderWidthOpacity.y;
    o_Color = vec4(
        rounded_color * u_Effect.g_ResolvedColorAlpha.rgb,
        alpha * u_Effect.g_ResolvedColorAlpha.a);
}
"#
    .to_owned()
}
