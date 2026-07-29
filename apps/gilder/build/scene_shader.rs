#[path = "scene_shader/audio_line.rs"]
mod audio_line;
#[path = "scene_shader/auto_sway.rs"]
mod auto_sway;
#[path = "scene_shader/blend.rs"]
mod blend;
#[path = "scene_shader/blur.rs"]
mod blur;
#[path = "scene_shader/catalog.rs"]
mod catalog;
#[path = "scene_shader/clipping_mask.rs"]
mod clipping_mask;
#[path = "scene_shader/core_material.rs"]
mod core_material;
#[path = "scene_shader/custom_user_texture.rs"]
mod custom_user_texture;
#[path = "scene_shader/effect_program.rs"]
mod effect_program;
#[path = "scene_shader/final_effect.rs"]
mod final_effect;
#[path = "scene_shader/flat_rounded_hsl.rs"]
mod flat_rounded_hsl;
#[path = "scene_shader/gradient_color.rs"]
mod gradient_color;
#[path = "scene_shader/lightning.rs"]
mod lightning;
#[path = "scene_shader/local_read.rs"]
mod local_read;
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
#[path = "scene_shader/ring.rs"]
mod ring;
#[path = "scene_shader/rounded_mask.rs"]
mod rounded_mask;
#[path = "scene_shader/shimmer.rs"]
mod shimmer;
#[path = "scene_shader/sphere.rs"]
mod sphere;
#[path = "scene_shader/spin.rs"]
mod spin;
#[path = "scene_shader/swing.rs"]
mod swing;
#[path = "scene_shader/tint.rs"]
mod tint;
#[path = "scene_shader/vertex_primitive.rs"]
mod vertex_primitive;
#[path = "scene_shader/waterwaves_composite.rs"]
mod waterwaves_composite;
#[path = "scene_shader/waterwaves_direct.rs"]
mod waterwaves_direct;

pub(crate) use catalog::build_scene_shader_origin_catalog;
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
use flat_rounded_hsl::flat_rounded_hsl_source_sources;
pub(crate) use local_read::{
    input_attachment_catalog_type_source, input_attachment_fragment_source,
};
pub(super) use particle::generic_particle_vertex_source;
pub(super) use particle_compute::particle_compute_source;
pub(super) use spin::spin_fragment_source;
pub(super) use tint::tint_fragment_source;
pub(crate) use vertex_primitive::scene_shader_vertex_primitive;
pub(super) use waterwaves_composite::{
    image_waterwaves_composite_sources, image_waterwaves_multiply_composite_sources,
    puppet_waterwaves_composite_sources, waterwaves_uv_field_sources,
};
pub(super) use waterwaves_direct::waterwaves_direct_sources;

pub(super) fn generic_image_fragment_source() -> String {
    generic_image_fragment(false)
}

pub(super) fn dynamic_text_vertex_source() -> String {
    r#"#version 450
layout(location = 5) in vec4 a_GlyphPosition;
layout(location = 6) in vec4 a_GlyphAtlasUv;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
layout(set = 0, binding = 2) uniform SceneDrawTransform {
    vec4 g_ModelViewProjectionMatrix[4];
} g_Draw;
void main() {
    vec2 corners[6] = vec2[](
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0));
    vec2 corner = corners[gl_VertexIndex];
    vec2 local_position = vec2(
        mix(a_GlyphPosition.x, a_GlyphPosition.z, corner.x),
        mix(a_GlyphPosition.w, a_GlyphPosition.y, corner.y));
    v_TexCoord = vec2(
        mix(a_GlyphAtlasUv.x, a_GlyphAtlasUv.z, corner.x),
        mix(a_GlyphAtlasUv.y, a_GlyphAtlasUv.w, corner.y));
    v_VertexAlpha = 1.0;
    vec4 local = vec4(local_position, 0.0, 1.0);
    gl_Position = vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], local),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], local),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], local),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], local));
}
"#
    .to_owned()
}

pub(super) fn generic_image_multiply_fragment_source() -> String {
    generic_image_fragment(true)
}

fn generic_image_fragment(premultiply: bool) -> String {
    let premultiply = if premultiply {
        "    color.rgb *= color.a;\n"
    } else {
        ""
    };
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
    let premultiply = if premultiply {
        "    color.rgb *= color.a;\n"
    } else {
        ""
    };
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

pub(super) fn flat_rounded_mask_support_vertex_source() -> String {
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
    let premultiply = if premultiply_output {
        "    color.rgb *= color.a;\n"
    } else {
        ""
    };
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
    let premultiply = if premultiply {
        "    color.rgb *= color.a;\n"
    } else {
        ""
    };
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
