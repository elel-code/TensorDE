use super::flat_rounded_mask_support_vertex_source;

#[path = "final_effect/lightning.rs"]
mod framebuffer_lightning;
#[path = "final_effect/lut.rs"]
mod framebuffer_lut;

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
        key: "we/audio-bars-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/framebuffer-water-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/framebuffer-water-post-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/framebuffer-lut16-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/framebuffer-lut64-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/framebuffer-lightning-screen-final",
        family: super::super::SceneShaderFamily::MeshFinalEffect,
    },
    super::super::SceneShaderSpec {
        key: "we/framebuffer-lightning-add-final",
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
        | "we/audio-bars-final"
        | "we/framebuffer-water-final"
        | "we/framebuffer-water-post-final"
        | "we/framebuffer-lut16-final"
        | "we/framebuffer-lut64-final"
        | "we/framebuffer-lightning-screen-final"
        | "we/framebuffer-lightning-add-final" => "FinalEffectProgram",
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
        "we/audio-bars-final" => final_audio_bars_fragment_source(),
        "we/framebuffer-water-final" => final_framebuffer_water_fragment_source(),
        "we/framebuffer-water-post-final" => final_framebuffer_water_post_fragment_source(),
        "we/framebuffer-lut16-final" => framebuffer_lut::framebuffer_lut_fragment_source(16),
        "we/framebuffer-lut64-final" => framebuffer_lut::framebuffer_lut_fragment_source(64),
        "we/framebuffer-lightning-screen-final" => {
            framebuffer_lightning::framebuffer_lightning_fragment_source(7)
        }
        "we/framebuffer-lightning-add-final" => {
            framebuffer_lightning::framebuffer_lightning_fragment_source(31)
        }
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
        "we/audio-bars-final" => final_audio_bars_vertex_source(),
        "we/framebuffer-water-final" | "we/framebuffer-water-post-final" => {
            framebuffer_water_vertex_source()
        }
        "we/framebuffer-lut16-final" | "we/framebuffer-lut64-final" => {
            framebuffer_lut::framebuffer_lut_vertex_source()
        }
        "we/framebuffer-lightning-screen-final" | "we/framebuffer-lightning-add-final" => {
            framebuffer_lightning::framebuffer_lightning_vertex_source()
        }
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
    vec2 offset = direction * noise * u_Effect.g_TimeSpeedAmountDirection.z;
    vec4 color = texture(g_SourceTexture,
        clamp(v_TexCoord + offset, vec2(0.001), vec2(0.999)));
    color *= u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn framebuffer_water_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 2) uniform FramebufferWaterDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out vec2 v_ObjectTexCoord;
layout(location = 2) flat out vec4 v_ObjectUvToScreenUv;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    v_TexCoord = uv;
    v_ObjectTexCoord = vec2(
        dot(u_Draw.g_ScreenUvToObjectUvRow0.xyz, vec3(uv, 1.0)),
        dot(u_Draw.g_ScreenUvToObjectUvRow1.xyz, vec3(uv, 1.0)));
    v_ObjectUvToScreenUv = vec4(
        u_Draw.g_ObjectUvToScreenUvRow0.xy,
        u_Draw.g_ObjectUvToScreenUvRow1.xy);
    gl_Position = vec4(position, 0.0, 1.0);
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
    float strength = u_Effect.g_TimeSpeedScaleStrength.w;
    vec2 offset = vec2(direction.y, -direction.x)
        * displacement * strength * strength * mask;
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
    float strength = u_Effect.g_DirectionStrengthAspectNormal.y;
    source_uv += normal.xy * strength * strength * mask;
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

fn final_audio_bars_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_MaskTexture;
layout(set = 0, binding = 3) uniform FinalAudioBarsProgram {
    vec4 g_ResolvedColorAlpha;
    vec4 g_ColorOpacity;
    vec4 g_CountSpacingBounds;
    vec4 g_MinHeightRadiusVolumeAaX;
    vec4 g_SkewTopBottomLeftRight;
    vec4 g_OpacityMask;
    vec4 g_MaskResolution;
    vec4 g_Reserved;
    vec4 g_SpectrumLeft[8];
    vec4 g_SpectrumRight[8];
} u_Effect;
float spectrumLeft(int band) {
    int index = clamp(band, 0, 31);
    return u_Effect.g_SpectrumLeft[index / 4][index % 4];
}
float spectrumRight(int band) {
    int index = clamp(band, 0, 31);
    return u_Effect.g_SpectrumRight[index / 4][index % 4];
}
float roundedBoxSdf(vec2 current_position, vec3 size, float correction, float radius_factor) {
    size *= 0.5;
    size.x *= correction;
    current_position.y -= size.y + size.z;
    size.y -= size.z;
    float radius = radius_factor * min(size.x, size.y);
    current_position.x *= correction;
    return length(max(abs(current_position) - size.xy + radius, 0.0)) - radius;
}
void main() {
    // Authored chain: Simple_Audio_Bars(SHAPE=7, BAR_STYLE=1), then skew MODE=0/REPEAT=1.
    // Fuse the latter by evaluating the audio source at the skewed source UV instead of rotating
    // each bar independently.
    vec2 output_uv = v_TexCoord;
    vec2 uv = output_uv;
    // The authored skew source computes `step` in the vertex shader. On a quad those four
    // corner values are linearly interpolated, so the fragment-equivalent mapping is a
    // continuous mix, not a per-fragment half-plane jump.
    uv.x -= mix(
        u_Effect.g_SkewTopBottomLeftRight.x,
        u_Effect.g_SkewTopBottomLeftRight.y,
        output_uv.y);
    uv.y += mix(
        u_Effect.g_SkewTopBottomLeftRight.z,
        u_Effect.g_SkewTopBottomLeftRight.w,
        output_uv.x);
    uv = fract(uv);
    uv.y = fract(0.5 - uv.y) + floor(uv.y);

    float count = max(u_Effect.g_CountSpacingBounds.x, 1.0);
    float correction = max(u_Effect.g_Reserved.y, 0.000001);
    float bar_distance = abs(fract(uv.x * count) * 2.0 - 1.0);
    float frequency = floor(uv.x * count) / count * 32.0;
    int frequency0 = int(mod(frequency, 32.0));
    int frequency1 = (frequency0 + 1) % 32;
    float blend = smoothstep(0.0, 1.0, fract(frequency));
    float left = mix(spectrumLeft(frequency0), spectrumLeft(frequency1), blend);
    float right = mix(spectrumRight(frequency0), spectrumRight(frequency1), blend);
    float volume_left = left * u_Effect.g_MinHeightRadiusVolumeAaX.z;
    float volume_right = right * u_Effect.g_MinHeightRadiusVolumeAaX.z;

    float rounded_width = (1.0 - u_Effect.g_CountSpacingBounds.y) / count;
    float minimum_height = u_Effect.g_MinHeightRadiusVolumeAaX.x
        * rounded_width * correction;
    float lower_bound = u_Effect.g_CountSpacingBounds.z;
    float upper_bound = u_Effect.g_CountSpacingBounds.w;
    float height_left = 0.5 * mix(
        max(lower_bound, minimum_height) * 2.0,
        upper_bound,
        volume_left);
    float height_right = 0.5 * mix(
        max(lower_bound, minimum_height) * 2.0,
        upper_bound,
        volume_right);

    vec2 center_left = vec2(bar_distance / count * 0.5, uv.y);
    vec2 center_right = vec2(bar_distance / count * 0.5, 1.0 - uv.y);
    float center_offset = min(rounded_width, max(height_left, height_right))
        * 0.5 * correction;
    center_left.y += center_offset;
    center_right.y += center_offset;
    float distance_left = roundedBoxSdf(
        center_left,
        vec3(rounded_width, height_left, 0.0),
        correction,
        u_Effect.g_MinHeightRadiusVolumeAaX.y);
    float distance_right = roundedBoxSdf(
        center_right,
        vec3(rounded_width, height_right, 0.0),
        correction,
        u_Effect.g_MinHeightRadiusVolumeAaX.y);
    float aa_factor = 15.0 / max(u_Effect.g_Reserved.z, 1.0);
    float aa_start = -u_Effect.g_MinHeightRadiusVolumeAaX.w * aa_factor;
    float aa_end = u_Effect.g_Reserved.x * aa_factor;
    float bar = 1.0 - min(
        smoothstep(aa_start, aa_end, distance_left),
        smoothstep(aa_start, aa_end, distance_right));
    float mask = 1.0;
    if (u_Effect.g_OpacityMask.y > 0.5) {
        vec2 mask_uv = output_uv * u_Effect.g_MaskResolution.zw
            / max(u_Effect.g_MaskResolution.xy, vec2(1.0));
        mask = texture(g_MaskTexture, mask_uv).r;
    }
    float alpha = bar * u_Effect.g_ColorOpacity.a * u_Effect.g_OpacityMask.x
        * mask * u_Effect.g_ResolvedColorAlpha.a * v_VertexAlpha;
    o_Color = vec4(
        u_Effect.g_ColorOpacity.rgb * u_Effect.g_ResolvedColorAlpha.rgb,
        alpha);
}
"#
    .to_owned()
}

fn final_audio_bars_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
layout(set = 0, binding = 2) uniform SceneDrawTransform {
    vec4 g_ModelViewProjectionMatrix[4];
} g_Draw;
void main() {
    v_TexCoord = a_TexCoord;
    v_VertexAlpha = a_Opacity;
    vec4 local_position = vec4(a_Position.xy, 0.0, 1.0);
    gl_Position = vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], local_position));
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
    vec2 speed = u_Effect.g_TimeSpeed.yz;
    vec2 scroll = sign(speed) * speed * speed * u_Effect.g_TimeSpeed.x;
    vec2 source_uv = fract((v_TexCoord + scroll) * u_Effect.g_Repeat.xy);
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
    vec2 source_uv = fract((v_TexCoord + scroll) * u_Effect.g_Repeat.xy);
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
    vec2 ripple_uv = rippleSourceUv(v_TexCoord);
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
    vec4 color = texture(g_SourceTexture, source_uv);
    if (u_Effect.g_IrisEyeColorBackground.w > 0.5) {
        float shifted_mask = texture(g_IrisMask, mask_uv + offset * iris_mask).r;
        color.rgb = mix(u_Effect.g_IrisEyeColorBackground.rgb, color.rgb, shifted_mask);
    }
    color *= u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn final_framebuffer_water_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec4 v_ObjectUvToScreenUv;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SceneSnapshot;
layout(set = 0, binding = 1) uniform sampler2D g_CausticsPattern;
layout(set = 0, binding = 2) uniform sampler2D g_CausticsNoise;
layout(set = 0, binding = 35) uniform sampler2D g_CausticsOffset;
layout(set = 0, binding = 4) uniform sampler2D g_CausticsGlow;
layout(set = 0, binding = 5) uniform sampler2D g_ShakeFlow;
layout(set = 0, binding = 3) uniform FinalFramebufferWaterProgram {
    vec4 g_ResolvedColorAlpha;
    vec4 g_CausticsTimeSpeedScaleBrightness;
    vec4 g_CausticsGlowDistortionChromaticBlur;
    vec4 g_CausticsColorStartTimeOffset;
    vec4 g_CausticsColorEndAspect;
    vec4 g_WavesTimeSpeedScaleStrength;
    vec4 g_WavesDirectionExponentOpacityUnused;
    vec4 g_ShakeTimeSpeedStrengthUnused;
    vec4 g_ShakeBoundsFriction;
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
vec2 screenDeltaToObject(vec2 delta) {
    mat2 object_to_screen = mat2(
        v_ObjectUvToScreenUv.x, v_ObjectUvToScreenUv.z,
        v_ObjectUvToScreenUv.y, v_ObjectUvToScreenUv.w);
    return inverse(object_to_screen) * delta;
}
vec2 objectDeltaToScreen(vec2 delta) {
    return vec2(dot(v_ObjectUvToScreenUv.xy, delta),
        dot(v_ObjectUvToScreenUv.zw, delta));
}
vec4 causticsAt(vec2 effect_uv) {
    vec4 albedo = texture(g_SceneSnapshot, effect_uv);
    float aspect = max(u_Effect.g_CausticsColorEndAspect.w, 0.0001);
    vec2 caustics_uv = effect_uv;
    caustics_uv.x *= aspect;
    caustics_uv *= u_Effect.g_CausticsTimeSpeedScaleBrightness.z;
    vec2 noise_uv = caustics_uv * 0.02;
    vec2 noise_uv2 = caustics_uv * 0.0333;
    vec2 blend_uv = caustics_uv * 0.01333;
    vec2 shift_uv = caustics_uv * 0.05;
    float time = u_Effect.g_CausticsTimeSpeedScaleBrightness.x
        * u_Effect.g_CausticsTimeSpeedScaleBrightness.y
        + u_Effect.g_CausticsColorStartTimeOffset.w;
    noise_uv.x += time * 0.005;
    noise_uv2.y += time * 0.004111;
    blend_uv += time * 0.003777;
    shift_uv += time * 0.01;
    vec4 shift = texture(g_CausticsOffset, shift_uv) * 2.0 - 1.0;
    vec4 noise0 = texture(g_CausticsNoise, noise_uv) * 2.0 - 1.0;
    vec4 noise1 = texture(g_CausticsNoise, noise_uv2) * 2.0 - 1.0;
    float distortion = u_Effect.g_CausticsGlowDistortionChromaticBlur.y;
    caustics_uv += (noise0.xy + noise1.xy) * 0.025 * distortion;
    caustics_uv += shift.rg * distortion;
    float chromatic = u_Effect.g_CausticsGlowDistortionChromaticBlur.z;
    vec3 caustics;
    if (abs(chromatic) <= 0.000001) {
        caustics = vec3(texture(g_CausticsPattern, caustics_uv).r);
    } else {
        caustics = vec3(
            texture(g_CausticsPattern, caustics_uv - vec2(0.01 * chromatic, 0.0)).r,
            texture(g_CausticsPattern, caustics_uv).r,
            texture(g_CausticsPattern, caustics_uv + vec2(0.01 * chromatic, 0.0)).r);
    }
    float glow = texture(g_CausticsGlow, caustics_uv).r;
    vec4 blend = texture(g_CausticsNoise, blend_uv);
    caustics = mix(caustics, vec3(glow),
        u_Effect.g_CausticsGlowDistortionChromaticBlur.w);
    float sample_alpha = smoothstep(blend.x * 0.8, 1.0 - blend.y * 0.2,
        dot(caustics, vec3(0.33333))
            + glow * u_Effect.g_CausticsGlowDistortionChromaticBlur.x);
    vec3 effect_color = u_Effect.g_CausticsTimeSpeedScaleBrightness.w
        * mix(u_Effect.g_CausticsColorStartTimeOffset.rgb,
            u_Effect.g_CausticsColorEndAspect.rgb, blend.x) * caustics;
    albedo.rgb = mix(albedo.rgb, max(albedo.rgb, effect_color), sample_alpha);
    return albedo;
}
void main() {
    float phase = u_Effect.g_ShakeTimeSpeedStrengthUnused.x
        * u_Effect.g_ShakeTimeSpeedStrengthUnused.y;
    float wave = sin(phase);
    float lower = u_Effect.g_ShakeBoundsFriction.x;
    float range = max(u_Effect.g_ShakeBoundsFriction.y - lower, 0.0001);
    wave = clamp((wave * 0.5 + 0.5 - lower) / range, 0.0, 1.0) * 2.0 - 1.0;
    vec2 flow = texture(g_ShakeFlow, v_TexCoord).rg * 2.0 - 1.0;
    float shake_strength = u_Effect.g_ShakeTimeSpeedStrengthUnused.z;
    vec2 effect_uv = clamp(v_TexCoord + flow * wave * shake_strength * shake_strength,
        vec2(0.001), vec2(0.999));
    vec2 object_uv = v_ObjectTexCoord + screenDeltaToObject(effect_uv - v_TexCoord);
    vec2 direction = rotateVec2(vec2(0.0, 1.0),
        u_Effect.g_WavesDirectionExponentOpacityUnused.x);
    float distance = u_Effect.g_WavesTimeSpeedScaleStrength.x
        * u_Effect.g_WavesTimeSpeedScaleStrength.y
        + dot(object_uv, direction) * u_Effect.g_WavesTimeSpeedScaleStrength.z;
    float displacement = shapedSine(
        distance, u_Effect.g_WavesDirectionExponentOpacityUnused.y);
    float wave_strength = u_Effect.g_WavesTimeSpeedScaleStrength.w;
    vec2 object_offset = vec2(direction.y, -direction.x)
        * displacement * wave_strength * wave_strength;
    effect_uv += objectDeltaToScreen(object_offset);
    object_uv += object_offset;
    vec4 color = causticsAt(effect_uv);
    color.a *= u_Effect.g_WavesDirectionExponentOpacityUnused.z;
    color *= u_Effect.g_ResolvedColorAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn final_framebuffer_water_post_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec4 v_ObjectUvToScreenUv;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_CausticsOutput;
layout(set = 0, binding = 1) uniform sampler2D g_ShakeFlow;
layout(set = 0, binding = 3) uniform FinalFramebufferWaterProgram {
    vec4 g_ResolvedColorAlpha;
    vec4 g_CausticsTimeSpeedScaleBrightness;
    vec4 g_CausticsGlowDistortionChromaticBlur;
    vec4 g_CausticsColorStartTimeOffset;
    vec4 g_CausticsColorEndAspect;
    vec4 g_WavesTimeSpeedScaleStrength;
    vec4 g_WavesDirectionExponentOpacityUnused;
    vec4 g_ShakeTimeSpeedStrengthUnused;
    vec4 g_ShakeBoundsFriction;
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
vec2 screenDeltaToObject(vec2 delta) {
    mat2 object_to_screen = mat2(
        v_ObjectUvToScreenUv.x, v_ObjectUvToScreenUv.z,
        v_ObjectUvToScreenUv.y, v_ObjectUvToScreenUv.w);
    return inverse(object_to_screen) * delta;
}
vec2 objectDeltaToScreen(vec2 delta) {
    return vec2(dot(v_ObjectUvToScreenUv.xy, delta),
        dot(v_ObjectUvToScreenUv.zw, delta));
}
void main() {
    float phase = u_Effect.g_ShakeTimeSpeedStrengthUnused.x
        * u_Effect.g_ShakeTimeSpeedStrengthUnused.y;
    float wave = sin(phase);
    float lower = u_Effect.g_ShakeBoundsFriction.x;
    float range = max(u_Effect.g_ShakeBoundsFriction.y - lower, 0.0001);
    wave = clamp((wave * 0.5 + 0.5 - lower) / range, 0.0, 1.0) * 2.0 - 1.0;
    vec2 flow = texture(g_ShakeFlow, v_TexCoord).rg * 2.0 - 1.0;
    float shake_strength = u_Effect.g_ShakeTimeSpeedStrengthUnused.z;
    vec2 effect_uv = clamp(v_TexCoord + flow * wave * shake_strength * shake_strength,
        vec2(0.001), vec2(0.999));
    vec2 object_uv = v_ObjectTexCoord + screenDeltaToObject(effect_uv - v_TexCoord);
    vec2 direction = rotateVec2(vec2(0.0, 1.0),
        u_Effect.g_WavesDirectionExponentOpacityUnused.x);
    float distance = u_Effect.g_WavesTimeSpeedScaleStrength.x
        * u_Effect.g_WavesTimeSpeedScaleStrength.y
        + dot(object_uv, direction) * u_Effect.g_WavesTimeSpeedScaleStrength.z;
    float displacement = shapedSine(
        distance, u_Effect.g_WavesDirectionExponentOpacityUnused.y);
    float wave_strength = u_Effect.g_WavesTimeSpeedScaleStrength.w;
    vec2 object_offset = vec2(direction.y, -direction.x)
        * displacement * wave_strength * wave_strength;
    effect_uv += objectDeltaToScreen(object_offset);
    vec4 color = texture(g_CausticsOutput, effect_uv);
    color.a *= u_Effect.g_WavesDirectionExponentOpacityUnused.z;
    color *= u_Effect.g_ResolvedColorAlpha;
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
    float effect_alpha = mask_alpha * u_Effect.g_SizeSoftnessAlpha.w;
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
