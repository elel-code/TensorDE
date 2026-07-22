#[path = "scene_shader/auto_sway.rs"]
mod auto_sway;
#[path = "scene_shader/blend.rs"]
mod blend;
#[path = "scene_shader/blur.rs"]
mod blur;
#[path = "scene_shader/catalog.rs"]
mod catalog;
#[path = "scene_shader/core_material.rs"]
mod core_material;
#[path = "scene_shader/effect_program.rs"]
mod effect_program;
#[path = "scene_shader/final_effect.rs"]
mod final_effect;
#[path = "scene_shader/lightning.rs"]
mod lightning;
#[path = "scene_shader/lut.rs"]
mod lut;
#[path = "scene_shader/oscilloscope.rs"]
mod oscilloscope;
#[path = "scene_shader/particle.rs"]
mod particle;
#[path = "scene_shader/particle_compute.rs"]
mod particle_compute;
#[path = "scene_shader/procedural_noise.rs"]
mod procedural_noise;
#[path = "scene_shader/raindrop.rs"]
mod raindrop;
#[path = "scene_shader/shimmer.rs"]
mod shimmer;
#[path = "scene_shader/swing.rs"]
mod swing;
#[path = "scene_shader/vertex_primitive.rs"]
mod vertex_primitive;
#[path = "scene_shader/waterwaves_direct.rs"]
mod waterwaves_direct;

pub(super) use core_material::{
    color_fragment_source, generic_particle_fragment_source, minimal_alpha_fragment_source,
    passthrough_fragment_source, text_fragment_source,
};
pub(crate) use effect_program::{
    effect_fragment_source, effect_object_mesh_vertex_source, effect_vertex_source,
};
pub(super) use final_effect::{
    FINAL_EFFECT_SHADER_SPECS, final_effect_parameter_layout, final_effect_sources,
};
pub(super) use particle::generic_particle_vertex_source;
pub(super) use particle_compute::particle_compute_source;
pub(crate) use vertex_primitive::scene_shader_vertex_primitive;
pub(super) use waterwaves_direct::waterwaves_direct_sources;

pub(super) fn generic_image_fragment_source() -> String {
    generic_image_fragment(false)
}

pub(super) fn generic_image_multiply_fragment_source() -> String {
    generic_image_fragment(true)
}

fn generic_image_fragment(premultiply: bool) -> String {
    let premultiply = premultiply
        .then_some("    color.rgb *= color.a;\n")
        .unwrap_or("");
    [
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform GenericImage4Material {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
} g_Material;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord) * g_Material.g_Color4;
    color.a *= v_VertexAlpha;
"#,
        premultiply,
        r#"    o_Color = color;
}
"#,
    ]
    .concat()
}

pub(super) fn object_composite_sources() -> (String, String) {
    let vertex = r#"#version 450
layout(location = 0) out vec2 v_TexCoord;
layout(set = 0, binding = 2) uniform ObjectCompositeDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
void main() {
    vec2 positions[3] = vec2[](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    vec2 position = positions[gl_VertexIndex];
    vec2 screen_uv = position * 0.5 + 0.5;
    v_TexCoord = vec2(
        dot(u_Draw.g_ScreenUvToObjectUvRow0.xyz, vec3(screen_uv, 1.0)),
        dot(u_Draw.g_ScreenUvToObjectUvRow1.xyz, vec3(screen_uv, 1.0)));
    gl_Position = vec4(position, 0.0, 1.0);
}
"#;
    let fragment = r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ObjectCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
} g_Material;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord);
    color.rgb *= g_Material.g_Color4.rgb;
    color.a *= g_Material.g_Color4.a;
    o_Color = color;
}
"#;
    (vertex.to_owned(), fragment.to_owned())
}

pub(super) fn puppet_effect_source_sources() -> (String, String) {
    image_effect_source_sources()
}

pub(super) fn screen_group_composite_sources() -> (String, String) {
    let vertex = r#"#version 450
layout(location = 0) out vec2 v_TexCoord;
void main() {
    vec2 position = vec2(
        gl_VertexIndex == 2 ? 3.0 : -1.0,
        gl_VertexIndex == 1 ? 3.0 : -1.0);
    v_TexCoord = position * 0.5 + 0.5;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#;
    let fragment = r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ScreenGroupCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
} g_Material;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord);
    color.rgb *= g_Material.g_Color4.rgb;
    color.a *= g_Material.g_Color4.a;
    o_Color = color;
}
"#;
    (vertex.to_owned(), fragment.to_owned())
}

pub(super) fn image_effect_source_sources() -> (String, String) {
    let vertex = r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
void main() {
    v_TexCoord = a_TexCoord;
    v_VertexAlpha = a_Opacity;
    gl_Position = vec4(a_TexCoord * 2.0 - 1.0, 0.0, 1.0);
}
"#;
    let fragment = r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord);
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#;
    (vertex.to_owned(), fragment.to_owned())
}

pub(super) fn image_effect_composite_sources() -> (String, String) {
    image_effect_composite_sources_with_premultiply(false)
}

pub(super) fn image_effect_modulate_composite_sources() -> (String, String) {
    image_effect_composite_sources_with_premultiply(true)
}

fn image_effect_composite_sources_with_premultiply(premultiply: bool) -> (String, String) {
    let vertex = super::scene_mesh_vertex_source();
    let premultiply = premultiply
        .then_some("    color.rgb *= color.a;\n")
        .unwrap_or_default();
    let fragment = [
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ImageEffectCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
} g_Material;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord) * g_Material.g_Color4;
    color.a *= v_VertexAlpha;
"#,
        premultiply,
        r#"    o_Color = color;
}
"#,
    ]
    .concat();
    (vertex, fragment)
}

pub(super) fn flat_rounded_mask_composite_sources() -> (String, String) {
    let vertex = flat_rounded_mask_support_vertex_source();
    let fragment = r#"#version 450
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec3 v_ObjectPixelExtent;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 3) uniform FlatRoundedMaskCompositeMaterial {
    vec4 g_ColorRadius;
    vec4 g_SizeSoftnessAlpha;
    vec4 g_BorderWidth;
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
        mask_uv - vec2(0.5),
        mask_size,
        u_Effect.g_ColorRadius.w);
    float edge_softness = u_Effect.g_SizeSoftnessAlpha.z
        / max(v_ObjectPixelExtent.z, 1.0) * 2.0;
    float mask_alpha = smoothstep(edge_softness, 0.0, distance);
    float effect_enabled = step(0.5, u_Effect.g_BorderWidth.y);
    mask_alpha = mix(1.0, mask_alpha, effect_enabled);
    float effect_alpha = mix(
        1.0,
        mask_alpha * u_Effect.g_SizeSoftnessAlpha.w,
        effect_enabled);
    vec3 rounded_color = mix(
        u_Effect.g_ColorRadius.rgb,
        vec3(1.0),
        effect_alpha);
    o_Color = vec4(
        rounded_color * u_Effect.g_ResolvedColorAlpha.rgb,
        mask_alpha * u_Effect.g_ResolvedColorAlpha.a);
}
"#;
    (vertex, fragment.to_owned())
}

pub(super) fn flat_rounded_hsl_source_sources() -> (String, String) {
    let vertex = flat_rounded_mask_support_vertex_source();
    let fragment = r#"#version 450
layout(location = 0) in vec2 v_ScreenTexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec3 v_ObjectPixelExtent;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SceneSnapshot;
layout(set = 0, binding = 3) uniform FlatRoundedMaskCompositeMaterial {
    vec4 g_ColorRadius;
    vec4 g_SizeSoftnessAlpha;
    vec4 g_BorderWidth;
    vec4 g_ResolvedColorAlpha;
} u_Effect;
float roundedBoxSdf(vec2 point, vec2 size, float radius) {
    vec2 half_size = size * 0.5;
    float half_min = min(half_size.x, half_size.y);
    float r = clamp(radius * half_min, 0.001, half_min);
    vec2 delta = abs(point) - (half_size - r);
    return length(max(delta, 0.0)) - r;
}
float blendLum(vec3 color) {
    return dot(color, vec3(0.30, 0.59, 0.11));
}
vec3 clipBlendColor(vec3 color) {
    float lum = blendLum(color);
    float low = min(min(color.r, color.g), color.b);
    float high = max(max(color.r, color.g), color.b);
    if (low < 0.0) color = vec3(lum) + (color - vec3(lum)) * lum / (lum - low);
    if (high > 1.0) color = vec3(lum)
        + (color - vec3(lum)) * (1.0 - lum) / (high - lum);
    return color;
}
vec3 setBlendLum(vec3 color, float lum) {
    return clipBlendColor(color + vec3(lum - blendLum(color)));
}
void main() {
    float width_pixels = max(v_ObjectPixelExtent.x, 1.0);
    float height_pixels = max(v_ObjectPixelExtent.y, 1.0);
    vec2 aspect_scale = vec2(max(1.0, width_pixels / height_pixels),
        max(1.0, height_pixels / width_pixels));
    vec2 mask_uv = (v_ObjectTexCoord - 0.5) * aspect_scale + 0.5;
    vec2 mask_size = u_Effect.g_SizeSoftnessAlpha.xy * aspect_scale;
    float distance = roundedBoxSdf(mask_uv - vec2(0.5), mask_size,
        u_Effect.g_ColorRadius.w);
    float edge_softness = u_Effect.g_SizeSoftnessAlpha.z
        / max(v_ObjectPixelExtent.z, 1.0) * 2.0;
    float mask_alpha = smoothstep(edge_softness, 0.0, distance);
    float effect_enabled = step(0.5, u_Effect.g_BorderWidth.y);
    mask_alpha = mix(1.0, mask_alpha, effect_enabled);
    float effect_alpha = mix(
        1.0,
        mask_alpha * u_Effect.g_SizeSoftnessAlpha.w,
        effect_enabled);
    vec3 source = mix(u_Effect.g_ColorRadius.rgb, vec3(1.0), effect_alpha)
        * u_Effect.g_ResolvedColorAlpha.rgb;
    vec4 destination = texelFetch(g_SceneSnapshot, ivec2(gl_FragCoord.xy), 0);
    vec3 blended = setBlendLum(source, blendLum(destination.rgb));
    float source_alpha = mask_alpha * u_Effect.g_ResolvedColorAlpha.a;
    float destination_alpha = destination.a;
    vec3 result = blended * source_alpha * destination_alpha
        + source * source_alpha * (1.0 - destination_alpha)
        + destination.rgb * destination_alpha * (1.0 - source_alpha);
    float result_alpha = source_alpha + destination_alpha
        - source_alpha * destination_alpha;
    o_Color = vec4(result, result_alpha);
}
"#;
    (vertex, fragment.to_owned())
}

fn flat_rounded_mask_support_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 2) uniform FlatRoundedMaskDrawUniform {
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
    vec4 g_ObjectPixelExtent;
    vec4 g_ObjectUvBounds;
} u_Draw;
layout(location = 0) out vec2 v_ScreenTexCoord;
layout(location = 1) out vec2 v_ObjectTexCoord;
layout(location = 2) flat out vec3 v_ObjectPixelExtent;
void main() {
    vec2 corners[6] = vec2[](
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, 1.0),
        vec2(1.0, 0.0),
        vec2(1.0, 1.0)
    );
    vec2 object_uv = mix(
        u_Draw.g_ObjectUvBounds.xy,
        u_Draw.g_ObjectUvBounds.zw,
        corners[gl_VertexIndex]);
    vec2 screen_uv = vec2(
        dot(u_Draw.g_ObjectUvToScreenUvRow0.xyz, vec3(object_uv, 1.0)),
        dot(u_Draw.g_ObjectUvToScreenUvRow1.xyz, vec3(object_uv, 1.0)));
    v_ObjectTexCoord = object_uv;
    v_ScreenTexCoord = screen_uv;
    v_ObjectPixelExtent = u_Draw.g_ObjectPixelExtent.xyz;
    gl_Position = vec4(screen_uv * 2.0 - 1.0, 0.0, 1.0);
}
"#
    .to_owned()
}

pub(super) fn waterwaves_uv_field_sources() -> (String, String) {
    (
        waterwaves_uv_vertex_source(),
        waterwaves_uv_field_fragment_source(),
    )
}

pub(super) fn image_waterwaves_composite_sources() -> (String, String) {
    (
        super::scene_mesh_vertex_source(),
        image_waterwaves_composite_fragment(false),
    )
}

pub(super) fn image_waterwaves_multiply_composite_sources() -> (String, String) {
    (
        super::scene_mesh_vertex_source(),
        image_waterwaves_composite_fragment(true),
    )
}

pub(super) fn image_foliage_ripple_composite_sources(foliage_power_two: bool) -> (String, String) {
    (
        super::scene_mesh_vertex_source(),
        foliage_ripple_fragment_source(false, foliage_power_two),
    )
}

pub(super) fn image_foliage_ripple_screen_composite_sources(
    foliage_power_two: bool,
) -> (String, String) {
    (
        super::scene_mesh_vertex_source(),
        foliage_ripple_fragment_source(true, foliage_power_two),
    )
}

fn foliage_ripple_fragment_source(premultiply_output: bool, foliage_power_two: bool) -> String {
    let premultiply = premultiply_output
        .then_some("    color.rgb *= color.a;\n")
        .unwrap_or_default();
    let shaped_sine = if foliage_power_two {
        r#"vec4 shapedSine(vec4 phase, float power) {
    vec4 wave = sin(phase);
    return wave * abs(wave);
}
"#
    } else {
        r#"vec4 shapedSine(vec4 phase, float power) {
    vec4 wave = sin(phase);
    return pow(abs(wave), vec4(max(power, 0.0001))) * sign(wave);
}
"#
    };
    [
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_SourceTexture;
layout(set = 0, binding = 1) uniform sampler2D g_FoliageNoise;
layout(set = 0, binding = 35) uniform sampler2D g_RippleNormal;
layout(set = 0, binding = 3) uniform FoliageRippleCompositeMaterial {
    vec4 g_ResolvedColorAlpha;
    vec4 g_FoliageTimeSpeedStrengthPhase;
    vec4 g_FoliagePowerNoiseScaleRatioDirection;
    vec4 g_SourceResolution;
    vec4 g_RippleTimeAnimationScaleScroll;
    vec4 g_RippleDirectionStrengthAspectNormal;
} u_Effect;
vec2 rotateVec2(vec2 value, float angle) {
    vec2 cs = vec2(cos(angle), sin(angle));
    return vec2(value.x * cs.x - value.y * cs.y,
        value.x * cs.y + value.y * cs.x);
}
vec4 shapedSine(vec4 phase, float power);
vec2 rippleSourceUv(vec2 uv) {
    float strength = u_Effect.g_RippleDirectionStrengthAspectNormal.y;
    if (strength == 0.0) {
        return uv;
    }
    vec2 scroll = rotateVec2(
        vec2(0.0, 1.0),
        u_Effect.g_RippleDirectionStrengthAspectNormal.x)
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
    vec3 n1 = texture(g_RippleNormal, ripple.xy).xyz * 2.0 - 1.0;
    vec3 n2 = texture(g_RippleNormal, ripple.zw).xyz * 2.0 - 1.0;
    vec3 normal = normalize(vec3(n1.xy + n2.xy, n1.z));
    return uv + normal.xy * strength * strength;
}
vec2 foliageSourceUv(vec2 uv) {
    float strength = u_Effect.g_FoliageTimeSpeedStrengthPhase.z;
    if (strength == 0.0) {
        return uv;
    }
    float width = max(u_Effect.g_SourceResolution.z, 1.0);
    float height = max(u_Effect.g_SourceResolution.w, 1.0);
    float ratio = max(u_Effect.g_FoliagePowerNoiseScaleRatioDirection.z, 0.0001);
    float aspect = max(width / height * ratio, 0.0001);
    float direction = u_Effect.g_FoliagePowerNoiseScaleRatioDirection.w;
    vec2 offset_scale = rotateVec2(vec2(1.0 / aspect, aspect), direction);
    vec2 phase_position = rotateVec2(uv, direction);
    float noise = texture(g_FoliageNoise,
        uv * u_Effect.g_FoliagePowerNoiseScaleRatioDirection.y).g;
    float phase = (noise * 6.283185307179586
        + phase_position.x * 10.0
        + phase_position.y * 5.0)
        * u_Effect.g_FoliageTimeSpeedStrengthPhase.w;
    float time = u_Effect.g_FoliageTimeSpeedStrengthPhase.x
        * u_Effect.g_FoliageTimeSpeedStrengthPhase.y;
    vec4 sines = shapedSine(
        phase + time * vec4(1.0, -0.16161616, 0.0083333, -0.00019841),
        u_Effect.g_FoliagePowerNoiseScaleRatioDirection.x);
    vec4 cosines = shapedSine(
        0.4 + phase + time * vec4(-0.5, 0.041666666, -0.0013888889, 0.000024801587),
        u_Effect.g_FoliagePowerNoiseScaleRatioDirection.x);
    float amplitude = strength * strength * 0.005;
    vec2 offset = offset_scale * vec2(
        dot(sines, vec4(amplitude)),
        dot(cosines, vec4(amplitude)));
    return uv + offset;
}
"#,
        shaped_sine,
        r#"void main() {
    vec2 ripple_uv = rippleSourceUv(v_TexCoord);
    vec4 color = texture(g_SourceTexture, foliageSourceUv(ripple_uv))
        * u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
"#,
        premultiply,
        r#"    o_Color = color;
}
"#,
    ]
    .concat()
}

pub(super) fn image_ripple_source_sources() -> (String, String) {
    (
        super::flattexture_vertex_source(),
        super::waterripple_fragment_source(0x5),
    )
}

pub(super) fn image_ripple_flow_composite_sources() -> (String, String) {
    image_ripple_flow_composite_sources_with_premultiply(false)
}

pub(super) fn image_ripple_flow_multiply_composite_sources() -> (String, String) {
    image_ripple_flow_composite_sources_with_premultiply(true)
}

fn image_ripple_flow_composite_sources_with_premultiply(premultiply: bool) -> (String, String) {
    let vertex = image_ripple_flow_composite_vertex_source();
    let premultiply = premultiply
        .then_some("    color.rgb *= color.a;\n")
        .unwrap_or("");
    let fragment = [
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 2) flat in vec4 v_Cycles;
layout(location = 3) flat in vec2 v_BlendWeight;
layout(location = 4) in vec2 v_FlowTexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_RippleTexture;
layout(set = 0, binding = 1) uniform sampler2D g_FlowTexture;
layout(set = 0, binding = 2) uniform sampler2D g_PhaseTexture;
layout(set = 0, binding = 3) uniform RippleFlowCompositeMaterial {
    vec4 g_ResolvedColorAlpha;
    vec4 g_TimeSpeedFeatherStrength;
    vec4 g_PhaseScale;
    vec4 g_FlowResolution;
} u_Effect;
vec4 sourceAtUv(vec2 uv) {
    return texture(g_RippleTexture, uv);
}
void main() {
    if (any(lessThan(v_TexCoord, vec2(0.0)))
        || any(greaterThan(v_TexCoord, vec2(1.0)))) {
        o_Color = vec4(0.0);
        return;
    }
    float strength = u_Effect.g_TimeSpeedFeatherStrength.w * 0.1;
    if (strength == 0.0) {
        vec4 color = sourceAtUv(v_TexCoord)
            * u_Effect.g_ResolvedColorAlpha;
        color.a *= v_VertexAlpha;
"#,
        premultiply,
        r#"        o_Color = color;
        return;
    }
    vec2 flow = (texture(g_FlowTexture, v_FlowTexCoord).rg - vec2(0.498)) * 2.0;
    vec4 offset0 = flow.xyxy * strength * v_Cycles.xxyy;
    vec4 offset1 = flow.xyxy * strength * v_Cycles.zzww;
    vec4 first = mix(sourceAtUv(v_TexCoord + offset0.xy),
        sourceAtUv(v_TexCoord + offset0.zw), v_BlendWeight.x);
    vec4 second = mix(sourceAtUv(v_TexCoord + offset1.xy),
        sourceAtUv(v_TexCoord + offset1.zw), v_BlendWeight.y);
    float phase = texture(g_PhaseTexture,
        v_TexCoord * u_Effect.g_PhaseScale.x).r;
    vec4 flowed = mix(first, second, smoothstep(0.2, 0.8, phase));
    vec4 color = mix(sourceAtUv(v_TexCoord), flowed, length(flow))
        * u_Effect.g_ResolvedColorAlpha;
    color.a *= v_VertexAlpha;
"#,
        premultiply,
        r#"    o_Color = color;
}
"#,
    ]
    .concat();
    (vertex, fragment)
}

fn image_ripple_flow_composite_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
layout(location = 2) flat out vec4 v_Cycles;
layout(location = 3) flat out vec2 v_BlendWeight;
layout(location = 4) out vec2 v_FlowTexCoord;
layout(set = 0, binding = 2) uniform SceneDrawTransform {
    vec4 g_ModelViewProjectionMatrix[4];
} g_Draw;
layout(set = 0, binding = 3) uniform RippleFlowCompositeMaterial {
    vec4 g_ResolvedColorAlpha;
    vec4 g_TimeSpeedFeatherStrength;
    vec4 g_PhaseScale;
    vec4 g_FlowResolution;
} u_Effect;
void main() {
    v_TexCoord = a_TexCoord;
    v_VertexAlpha = a_Opacity;
    float time_phase = u_Effect.g_TimeSpeedFeatherStrength.x
        * u_Effect.g_TimeSpeedFeatherStrength.y;
    vec4 cycles = fract(time_phase + vec4(0.0, 0.5, 0.25, 0.75));
    vec2 blend_phase = 2.0 * abs(vec2(cycles.x, cycles.z) - vec2(0.5));
    float feather = u_Effect.g_TimeSpeedFeatherStrength.z;
    vec2 smooth_range = vec2(0.5 - feather, 0.5 + feather);
    v_Cycles = cycles - vec4(0.5);
    v_BlendWeight = smoothstep(smooth_range.x, smooth_range.y, blend_phase);
    v_FlowTexCoord = a_TexCoord
        * u_Effect.g_FlowResolution.zw / u_Effect.g_FlowResolution.xy;
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

pub(super) fn puppet_waterwaves_composite_sources() -> (String, String) {
    (
        puppet_effect_composite_vertex(),
        puppet_waterwaves_composite_fragment(),
    )
}

fn image_waterwaves_composite_fragment(premultiply: bool) -> String {
    let premultiply = premultiply
        .then_some("    color.rgb *= color.a;\n")
        .unwrap_or("");
    [
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 3) uniform ImageWaterWavesCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
    vec4 g_EffectAtlas;
} g_Material;
void main() {
    vec2 atlas_size = vec2(textureSize(g_Texture1, 0));
    vec2 atlas_min = g_Material.g_EffectAtlas.zw + 0.5 / atlas_size;
    vec2 atlas_max = g_Material.g_EffectAtlas.zw
        + g_Material.g_EffectAtlas.xy - 0.5 / atlas_size;
    vec2 atlas_uv = mix(atlas_min, atlas_max, clamp(v_TexCoord, 0.0, 1.0));
    vec2 source_uv = clamp(texture(g_Texture1, atlas_uv).rg, vec2(0.001), vec2(0.999));
    vec4 source_color = texture(g_Texture0, source_uv);
    const float alpha_noise = 4.0 / 255.0;
    source_color.a = max(source_color.a - alpha_noise, 0.0) / (1.0 - alpha_noise);
    if (source_color.a == 0.0) {
        discard;
    }
    vec4 color = source_color * g_Material.g_Color4;
    color.a *= v_VertexAlpha;
"#,
        premultiply,
        r#"    o_Color = color;
}
"#,
    ]
    .concat()
}

fn puppet_waterwaves_composite_fragment() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_EffectTexCoord;
layout(location = 1) in float v_BoneAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 3) uniform PuppetWaterWavesCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
    vec4 g_EffectAtlas;
} g_Material;
void main() {
    vec2 atlas_size = vec2(textureSize(g_Texture1, 0));
    vec2 atlas_min = g_Material.g_EffectAtlas.zw + 0.5 / atlas_size;
    vec2 atlas_max = g_Material.g_EffectAtlas.zw
        + g_Material.g_EffectAtlas.xy - 0.5 / atlas_size;
    vec2 atlas_uv = mix(atlas_min, atlas_max, clamp(v_EffectTexCoord, 0.0, 1.0));
    vec2 source_uv = clamp(texture(g_Texture1, atlas_uv).rg, vec2(0.001), vec2(0.999));
    vec4 color = texture(g_Texture0, source_uv);
    const float alpha_noise = 4.0 / 255.0;
    color.a = max(color.a - alpha_noise, 0.0) / (1.0 - alpha_noise);
    if (color.a == 0.0) {
        discard;
    }
    color *= g_Material.g_Color4;
    color.a *= v_BoneAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn waterwaves_uv_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 2) uniform WaterWavesDrawUniform {
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
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    float layer = u_Draw.g_ScreenUvToObjectUvRow0.w;
    vec2 atlas_grid = max(vec2(
        u_Draw.g_ScreenUvToObjectUvRow1.w,
        u_Draw.g_ObjectUvToScreenUvRow0.w), vec2(1.0));
    vec2 atlas_tile = vec2(mod(layer, atlas_grid.x), floor(layer / atlas_grid.x));
    vec2 atlas_uv = (atlas_tile + uv) / atlas_grid;
    v_TexCoord = uv;
    v_ObjectTexCoord = vec2(
        dot(u_Draw.g_ScreenUvToObjectUvRow0.xyz, vec3(uv, 1.0)),
        dot(u_Draw.g_ScreenUvToObjectUvRow1.xyz, vec3(uv, 1.0)));
    v_ObjectUvToScreenUv = vec4(
        u_Draw.g_ObjectUvToScreenUvRow0.xy,
        u_Draw.g_ObjectUvToScreenUvRow1.xy);
    gl_Position = vec4(atlas_uv * 2.0 - 1.0, 0.0, 1.0);
}
"#
    .to_owned()
}

fn waterwaves_uv_field_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec4 v_ObjectUvToScreenUv;
layout(location = 0) out vec2 o_Uv;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;
layout(set = 0, binding = 3) uniform WaterWavesUvFieldUniform {
    vec4 g_Chain;
    vec4 g_Stage[36];
} u_Effect;
layout(set = 0, binding = 4) uniform sampler2D g_Texture4;
layout(set = 0, binding = 5) uniform sampler2D g_Texture5;
layout(set = 0, binding = 6) uniform sampler2D g_Texture6;
layout(set = 0, binding = 7) uniform sampler2D g_Texture7;
layout(set = 0, binding = 8) uniform sampler2D g_Texture8;
layout(set = 0, binding = 9) uniform sampler2D g_Texture9;
layout(set = 0, binding = 35) uniform sampler2D g_Texture3;
float shapedSine(float phase, float exponent) {
    float wave = sin(phase);
    if (exponent == 1.0) return wave;
    if (exponent == 2.0) return wave * abs(wave);
    return pow(abs(wave), max(exponent, 0.0001)) * sign(wave);
}
float stageMask(int stage, vec2 uv) {
    if (stage == 0) return texture(g_Texture1, uv).r;
    if (stage == 1) return texture(g_Texture2, uv).r;
    if (stage == 2) return texture(g_Texture3, uv).r;
    if (stage == 3) return texture(g_Texture4, uv).r;
    if (stage == 4) return texture(g_Texture5, uv).r;
    if (stage == 5) return texture(g_Texture6, uv).r;
    if (stage == 6) return texture(g_Texture7, uv).r;
    if (stage == 7) return texture(g_Texture8, uv).r;
    return texture(g_Texture9, uv).r;
}
vec2 stageOffset(int stage, vec2 motion_uv) {
    int base = stage * 4;
    vec4 phase_scale_strength2_mask = u_Effect.g_Stage[base];
    vec4 direction_phase2_scale2 = u_Effect.g_Stage[base + 1];
    vec4 direction2_exponents = u_Effect.g_Stage[base + 2];
    vec4 mask_resolution = u_Effect.g_Stage[base + 3];
    if (phase_scale_strength2_mask.z <= 0.0) return vec2(0.0);
    float mask = 1.0;
    if (phase_scale_strength2_mask.w > 0.5) {
        vec2 mask_uv = motion_uv * mask_resolution.zw / mask_resolution.xy;
        mask = stageMask(stage, mask_uv);
    }
    vec2 direction = direction_phase2_scale2.xy;
    float distance0 = phase_scale_strength2_mask.x
        + dot(motion_uv, direction) * phase_scale_strength2_mask.y;
    vec2 offset_direction = vec2(direction.y, -direction.x);
    float displacement = shapedSine(distance0, direction2_exponents.z);
    if (direction_phase2_scale2.w > 0.0) {
        vec2 direction2 = direction2_exponents.xy;
        float distance1 = direction_phase2_scale2.z
            + dot(motion_uv, direction2) * direction_phase2_scale2.w;
        displacement *= shapedSine(distance1, direction2_exponents.w);
    }
    vec2 object_uv_offset = displacement * offset_direction
        * phase_scale_strength2_mask.z * mask;
    return vec2(
        dot(v_ObjectUvToScreenUv.xy, object_uv_offset),
        dot(v_ObjectUvToScreenUv.zw, object_uv_offset));
}
void main() {
    int stage_count = clamp(int(u_Effect.g_Chain.x + 0.5), 0, 9);
    vec2 source_uv = v_TexCoord;
    for (int stage = 8; stage >= 0; --stage) {
        if (stage < stage_count) {
            source_uv += stageOffset(stage, source_uv);
        }
    }
    o_Uv = source_uv;
}
"#
    .to_owned()
}

pub(super) fn puppet_effect_composite_sources() -> (String, String) {
    (
        puppet_effect_composite_vertex(),
        puppet_effect_composite_fragment(),
    )
}

fn puppet_effect_composite_vertex() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 3) in uvec4 a_BlendIndices;
layout(location = 4) in vec4 a_BlendWeights;
layout(location = 0) out vec2 v_EffectTexCoord;
layout(location = 1) out float v_BoneAlpha;
layout(set = 0, binding = 2) uniform SceneDrawTransform {
    vec4 g_ModelViewProjectionMatrix[4];
} g_Draw;
struct GilderPuppetBonePalette {
    vec4 row0;
    vec4 row1;
    vec4 row2;
    vec4 row3;
    vec4 alpha;
};
layout(std430, set = 0, binding = 4) readonly buffer ScenePuppetBones {
    GilderPuppetBonePalette g_Bones[];
} g_Puppet;
vec4 projectPosition(vec4 position) {
    return vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], position));
}
void main() {
    vec4 raw_position = vec4(a_Position.xy, 0.0, 1.0);
    v_EffectTexCoord = a_TexCoord;
    vec4 skinned_position = vec4(0.0);
    float skinned_alpha = 0.0;
    float total_weight = 0.0;
    for (uint slot = 0u; slot < 4u; slot++) {
        float weight = a_BlendWeights[slot];
        if (weight <= 0.0000001) {
            continue;
        }
        GilderPuppetBonePalette bone = g_Puppet.g_Bones[a_BlendIndices[slot]];
        skinned_position += vec4(
            dot(bone.row0, raw_position),
            dot(bone.row1, raw_position),
            dot(bone.row2, raw_position),
            dot(bone.row3, raw_position)) * weight;
        skinned_alpha += bone.alpha.x * weight;
        total_weight += weight;
    }
    vec4 local_position = raw_position;
    v_BoneAlpha = 1.0;
    if (total_weight > 0.0000001) {
        local_position = skinned_position / total_weight;
        v_BoneAlpha = skinned_alpha / total_weight;
    }
    gl_Position = projectPosition(local_position);
}
"#
    .to_owned()
}

fn puppet_effect_composite_fragment() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_EffectTexCoord;
layout(location = 1) in float v_BoneAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform PuppetEffectCompositeMaterial {
    vec4 g_Color4;
    vec4 g_RoughnessMetallic;
    vec4 g_SpecularTint;
} g_Material;
void main() {
    vec4 color = texture(g_Texture0, v_EffectTexCoord) * g_Material.g_Color4;
    color.a *= v_BoneAlpha;
    o_Color = color;
}
"#
    .to_owned()
}
pub(crate) use catalog::{SceneShaderFamily, SceneShaderSpec, build_scene_shader_catalog};
