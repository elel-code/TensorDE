use super::super::*;
use super::blend::{blend_fragment_source, blend_gradient_fragment_source};
use super::lightning::{
    lightning_fragment_source, lightning_object_mesh_vertex_source, lightning_vertex_source,
};
use super::lut::lut_fragment_source;
use super::oscilloscope::{
    oscilloscope_fragment_source, oscilloscope_object_mesh_vertex_source,
    oscilloscope_vertex_source,
};
use super::raindrop::raindrop_fragment_source;
use super::rounded_mask::rounded_mask_fragment_source;
use super::shimmer::shimmer_fragment_source;
use super::swing::swing_fragment_source;

#[path = "effect_program/audio_bars.rs"]
mod audio_bars;
#[path = "effect_program/caustics.rs"]
mod caustics;

use audio_bars::audio_bars_fragment_source;
use caustics::caustics_effect_fragment_source;

pub(crate) fn effect_vertex_source(key: &str, shader: &str, texture_slot_mask: u32) -> String {
    if key.contains("__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1") {
        return framebuffer_quantized_overlay_effect_vertex_source();
    }
    if shader == "effects/iris" {
        return iris_effect_vertex_source(texture_slot_mask);
    }
    if shader == "effects/cloudmotion" {
        return cloudmotion_effect_vertex_source();
    }
    if shader == "effects/111" {
        return lightning_vertex_source();
    }
    if shader == "effects/blend" || shader == "effects/blendgradient" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "effects/audio_responsive_oscilloscope" {
        return oscilloscope_vertex_source();
    }
    if shader == "effects/waterflow" {
        return waterflow_effect_vertex_source();
    }
    if shader == "effects/waterwaves" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "effects/scroll" || shader == "effects/skew" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "effects/tech_circle" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "effects/simple_audio_bars" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "effects/rounded_mask" {
        return object_local_effect_vertex_source();
    }
    if shader == "effects/rounded_mask_effect_edit" {
        return object_local_effect_vertex_source();
    }
    if shader == "effects/clipping_mask" {
        return super::clipping_mask::clipping_mask_vertex_source();
    }
    r#"#version 450
layout(location = 0) out vec2 v_TexCoord;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    v_TexCoord = uv;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_string()
}

pub(crate) fn effect_object_mesh_vertex_source(
    key: &str,
    shader: &str,
    texture_slot_mask: u32,
) -> Option<String> {
    if key.contains("__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1") {
        return None;
    }
    if shader == "effects/iris" {
        return None;
    }
    if shader == "effects/cloudmotion" {
        return Some(cloudmotion_effect_object_mesh_vertex_source());
    }
    if shader == "effects/111" {
        return Some(lightning_object_mesh_vertex_source());
    }
    if shader == "effects/waterflow" {
        return Some(waterflow_effect_object_mesh_vertex_source());
    }
    if shader == "effects/audio_responsive_oscilloscope" {
        return Some(oscilloscope_object_mesh_vertex_source());
    }
    if matches!(
        shader,
        "effects/blend"
            | "effects/blendgradient"
            | "effects/waterwaves"
            | "effects/scroll"
            | "effects/skew"
            | "effects/tech_circle"
            | "effects/simple_audio_bars"
    ) {
        return Some(object_uv_affine_effect_object_mesh_vertex_source());
    }
    if shader == "effects/rounded_mask" {
        return Some(object_local_effect_object_mesh_vertex_source());
    }
    if shader == "effects/rounded_mask_effect_edit" {
        return Some(object_local_effect_object_mesh_vertex_source());
    }
    if shader == "effects/clipping_mask" {
        if texture_slot_mask & (1 << 2) != 0 {
            return None;
        }
        return Some(super::clipping_mask::clipping_mask_object_mesh_vertex_source());
    }
    Some(super::super::scene_mesh_vertex_source())
}

fn cloudmotion_effect_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 3) uniform CloudMotionUniform {
    vec4 g_TimeSpeedAmountDirection;
    vec4 g_ScaleScaleXAspectUnused;
    vec4 g_Unused0;
    vec4 g_Unused1;
} u_Effect;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out vec2 v_NoiseTexCoord;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    float aspect_scaled_x = u_Effect.g_ScaleScaleXAspectUnused.z * uv.x;
    vec2 scaled_uv = vec2(aspect_scaled_x, uv.y)
        * u_Effect.g_ScaleScaleXAspectUnused.x;
    float time_offset = u_Effect.g_TimeSpeedAmountDirection.x
        * u_Effect.g_TimeSpeedAmountDirection.y;
    v_TexCoord = uv;
    v_NoiseTexCoord = vec2(
        scaled_uv.x * u_Effect.g_ScaleScaleXAspectUnused.y + time_offset,
        scaled_uv.y);
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_owned()
}

fn cloudmotion_effect_object_mesh_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 3) in uvec4 a_BlendIndices;
layout(location = 4) in vec4 a_BlendWeights;
layout(set = 0, binding = 2) uniform SceneDrawTransform {
    vec4 g_ModelViewProjectionMatrix[4];
} g_Draw;
layout(set = 0, binding = 3) uniform CloudMotionUniform {
    vec4 g_TimeSpeedAmountDirection;
    vec4 g_ScaleScaleXAspectUnused;
    vec4 g_Unused0;
    vec4 g_Unused1;
} u_Effect;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out vec2 v_NoiseTexCoord;
void main() {
    vec2 uv = a_TexCoord;
    float aspect_scaled_x = u_Effect.g_ScaleScaleXAspectUnused.z * uv.x;
    vec2 scaled_uv = vec2(aspect_scaled_x, uv.y)
        * u_Effect.g_ScaleScaleXAspectUnused.x;
    float time_offset = u_Effect.g_TimeSpeedAmountDirection.x
        * u_Effect.g_TimeSpeedAmountDirection.y;
    v_TexCoord = uv;
    v_NoiseTexCoord = vec2(
        scaled_uv.x * u_Effect.g_ScaleScaleXAspectUnused.y + time_offset,
        scaled_uv.y);
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

fn object_uv_affine_effect_object_mesh_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(set = 0, binding = 2) uniform ObjectUvAffineDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out vec2 v_ObjectTexCoord;
layout(location = 2) flat out vec4 v_ObjectUvToScreenUv;
void main() {
    v_TexCoord = a_TexCoord;
    v_ObjectTexCoord = a_TexCoord;
    v_ObjectUvToScreenUv = vec4(
        u_Draw.g_ObjectUvToScreenUvRow0.xy,
        u_Draw.g_ObjectUvToScreenUvRow1.xy);
    vec2 screen_uv = vec2(
        dot(u_Draw.g_ObjectUvToScreenUvRow0.xyz, vec3(a_TexCoord, 1.0)),
        dot(u_Draw.g_ObjectUvToScreenUvRow1.xyz, vec3(a_TexCoord, 1.0)));
    gl_Position = vec4(screen_uv * 2.0 - 1.0, 0.0, 1.0);
}
"#
    .to_owned()
}

fn waterflow_effect_object_mesh_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(set = 0, binding = 2) uniform WaterFlowDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(set = 0, binding = 3) uniform WaterFlowUniform {
    vec4 g_TimeSpeedFeatherStrength;
    vec4 g_PhaseScale;
    vec4 g_Texture1Resolution;
    vec4 g_Unused;
} u_Effect;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out vec2 v_ObjectTexCoord;
layout(location = 2) flat out vec4 v_ObjectUvToScreenUv;
layout(location = 3) flat out vec4 v_Cycles;
layout(location = 4) flat out vec2 v_BlendWeight;
layout(location = 5) out vec2 v_FlowTexCoord;
void main() {
    v_TexCoord = a_TexCoord;
    v_ObjectTexCoord = a_TexCoord;
    v_ObjectUvToScreenUv = vec4(
        u_Draw.g_ObjectUvToScreenUvRow0.xy,
        u_Draw.g_ObjectUvToScreenUvRow1.xy);
    float time_phase = u_Effect.g_TimeSpeedFeatherStrength.x
        * u_Effect.g_TimeSpeedFeatherStrength.y;
    vec4 cycles = fract(time_phase + vec4(0.0, 0.5, 0.25, 0.75));
    vec2 blend_phase = 2.0 * abs(vec2(cycles.x, cycles.z) - vec2(0.5));
    float feather = u_Effect.g_TimeSpeedFeatherStrength.z;
    vec2 smooth_range = vec2(0.5 - feather, 0.5 + feather);
    v_Cycles = cycles - vec4(0.5);
    v_BlendWeight = smoothstep(smooth_range.x, smooth_range.y, blend_phase);
    v_FlowTexCoord = a_TexCoord
        * u_Effect.g_Texture1Resolution.zw / u_Effect.g_Texture1Resolution.xy;
    vec2 screen_uv = vec2(
        dot(u_Draw.g_ObjectUvToScreenUvRow0.xyz, vec3(a_TexCoord, 1.0)),
        dot(u_Draw.g_ObjectUvToScreenUvRow1.xyz, vec3(a_TexCoord, 1.0)));
    gl_Position = vec4(screen_uv * 2.0 - 1.0, 0.0, 1.0);
}
"#
    .to_owned()
}

fn object_local_effect_object_mesh_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(set = 0, binding = 2) uniform ObjectLocalEffectDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out vec2 v_ObjectTexCoord;
layout(location = 2) flat out vec3 v_ObjectPixelExtent;
void main() {
    v_TexCoord = a_TexCoord;
    v_ObjectTexCoord = a_TexCoord;
    v_ObjectPixelExtent = u_Draw.g_ObjectUvToScreenUvRow0.xyz;
    vec2 screen_uv = vec2(
        dot(u_Draw.g_ObjectUvToScreenUvRow0.xyz, vec3(a_TexCoord, 1.0)),
        dot(u_Draw.g_ObjectUvToScreenUvRow1.xyz, vec3(a_TexCoord, 1.0)));
    gl_Position = vec4(screen_uv * 2.0 - 1.0, 0.0, 1.0);
}
"#
    .to_owned()
}

fn waterflow_effect_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 2) uniform WaterFlowDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(set = 0, binding = 3) uniform WaterFlowUniform {
    vec4 g_TimeSpeedFeatherStrength;
    vec4 g_PhaseScale;
    vec4 g_Texture1Resolution;
    vec4 g_Unused;
} u_Effect;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out vec2 v_ObjectTexCoord;
layout(location = 2) flat out vec4 v_ObjectUvToScreenUv;
layout(location = 3) flat out vec4 v_Cycles;
layout(location = 4) flat out vec2 v_BlendWeight;
layout(location = 5) out vec2 v_FlowTexCoord;
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
    float time_phase = u_Effect.g_TimeSpeedFeatherStrength.x
        * u_Effect.g_TimeSpeedFeatherStrength.y;
    vec4 cycles = fract(time_phase + vec4(0.0, 0.5, 0.25, 0.75));
    vec2 blend_phase = 2.0 * abs(vec2(cycles.x, cycles.z) - vec2(0.5));
    float feather = u_Effect.g_TimeSpeedFeatherStrength.z;
    vec2 smooth_range = vec2(0.5 - feather, 0.5 + feather);
    v_Cycles = cycles - vec4(0.5);
    v_BlendWeight = smoothstep(smooth_range.x, smooth_range.y, blend_phase);
    v_FlowTexCoord = v_ObjectTexCoord
        * u_Effect.g_Texture1Resolution.zw / u_Effect.g_Texture1Resolution.xy;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_owned()
}

fn framebuffer_quantized_overlay_effect_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 2) uniform FramebufferOverlayDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(location = 0) out vec2 v_FramebufferCoord;
layout(location = 1) out vec2 v_EffectCoord;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    v_FramebufferCoord = vec2(
        dot(u_Draw.g_ObjectUvToScreenUvRow0.xyz, vec3(uv, 1.0)),
        dot(u_Draw.g_ObjectUvToScreenUvRow1.xyz, vec3(uv, 1.0)));
    v_EffectCoord = uv;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_owned()
}

fn object_local_effect_vertex_source() -> String {
    r#"#version 450
layout(set = 0, binding = 2) uniform ObjectLocalEffectDrawUniform {
    vec4 g_ScreenUvToObjectUvRow0;
    vec4 g_ScreenUvToObjectUvRow1;
    vec4 g_ObjectUvToScreenUvRow0;
    vec4 g_ObjectUvToScreenUvRow1;
} u_Draw;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out vec2 v_ObjectTexCoord;
layout(location = 2) flat out vec3 v_ObjectPixelExtent;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    v_TexCoord = uv;
    v_ObjectTexCoord = vec2(
        dot(u_Draw.g_ScreenUvToObjectUvRow0.xyz, vec3(uv, 1.0)),
        dot(u_Draw.g_ScreenUvToObjectUvRow1.xyz, vec3(uv, 1.0)));
    v_ObjectPixelExtent = u_Draw.g_ObjectUvToScreenUvRow0.xyz;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_owned()
}

fn waterwaves_effect_vertex_source() -> String {
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

pub(crate) fn effect_fragment_source(key: &str, shader: &str, texture_slot_mask: u32) -> String {
    if shader == "effects/audioline" {
        return super::audio_line::audio_line_fragment_source(texture_slot_mask);
    }
    if shader == "effects/clipping_mask" {
        return super::clipping_mask::clipping_mask_fragment_source(texture_slot_mask);
    }
    if shader == "effects/custom_user_texture" {
        return super::custom_user_texture::custom_user_texture_fragment_source(
            key,
            texture_slot_mask,
        );
    }
    if shader == "effects/gradient_color" {
        return super::gradient_color::gradient_color_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/huan" {
        return super::ring::ring_fragment_source(texture_slot_mask);
    }
    if shader == "effects/qiu" {
        return super::sphere::sphere_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/tint" {
        return super::tint_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/spin" {
        return super::spin_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/auto_sway" {
        return super::auto_sway::auto_sway_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/blur_downsample4" {
        return super::blur::blur_downsample4_fragment_source(texture_slot_mask);
    }
    if shader == "effects/blur_gaussian" {
        return super::blur::blur_gaussian_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/blur_combine" {
        return super::blur::blur_combine_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/caustics" {
        let chromatic_zero = key.contains("__GILDER_CHROMATIC_ZERO_1");
        if key.contains("__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1") {
            return caustics_framebuffer_quantized_overlay_fragment_source(key, texture_slot_mask);
        }
        return caustics_effect_fragment_source(
            texture_slot_mask,
            chromatic_zero,
            key.contains("__GILDER_PATTERN_GLOW_SHARED_1"),
            key.contains("__GILDER_COLOR_EQUAL_1"),
        );
    }
    if shader == "effects/cloudmotion" {
        return cloudmotion_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/colorkey" {
        return colorkey_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/lut_loader" {
        return lut_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/111" {
        return lightning_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/swing" {
        return swing_fragment_source(texture_slot_mask);
    }
    if shader == "effects/raindrop_on_glass" {
        return raindrop_fragment_source(texture_slot_mask);
    }
    if shader == "effects/audio_responsive_oscilloscope" {
        return oscilloscope_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/blend" {
        return blend_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/blendgradient" {
        return blend_gradient_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/foliagesway" {
        return foliage_sway_fragment_source(texture_slot_mask);
    }
    if shader == "effects/shimmer" {
        return shimmer_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/waterwaves" {
        return waterwaves_fragment_source(texture_slot_mask);
    }
    if shader == "effects/waterflow" {
        return waterflow_fragment_source();
    }
    if shader == "effects/waterripple" {
        return waterripple_fragment_source(texture_slot_mask);
    }
    if shader == "effects/opacity" {
        return opacity_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/user_texture_alpha_overwrite_workaround" {
        return opacity_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/procedural_noise" {
        return super::procedural_noise::procedural_noise_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/iris" {
        return iris_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/shake" {
        return shake_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/scroll" {
        return scroll_effect_fragment_source();
    }
    if shader == "effects/skew" {
        return skew_effect_fragment_source(key);
    }
    if shader == "effects/tech_circle" {
        return tech_circle_fragment_source(key);
    }
    if shader == "effects/simple_audio_bars" {
        return audio_bars_fragment_source(key);
    }
    if shader == "effects/rounded_mask" {
        return rounded_mask_fragment_source(key);
    }
    if shader == "effects/rounded_mask_effect_edit" {
        return super::rounded_mask::rounded_mask_edit_fragment_source(key, texture_slot_mask);
    }
    panic!("scene shader {key:?} has no typed fragment contract")
}

fn caustics_framebuffer_quantized_overlay_fragment_source(
    key: &str,
    texture_slot_mask: u32,
) -> String {
    caustics_effect_fragment_source(
        texture_slot_mask,
        key.contains("__GILDER_CHROMATIC_ZERO_1"),
        key.contains("__GILDER_PATTERN_GLOW_SHARED_1"),
        key.contains("__GILDER_COLOR_EQUAL_1"),
    )
        .replacen(
            "layout(location = 0) in vec2 v_TexCoord;",
            "layout(location = 0) in vec2 v_FramebufferCoord;\nlayout(location = 1) in vec2 v_EffectCoord;",
            1,
        )
        .replacen(
            "vec4 albedo = texture(g_Texture0, v_TexCoord);",
            "vec4 albedo = quantizeUnorm8(texture(g_Texture0, v_FramebufferCoord));",
            1,
        )
        .replacen(
            "void main() {",
            "vec4 quantizeUnorm8(vec4 value) {\n    return roundEven(clamp(value, 0.0, 1.0) * 255.0) / 255.0;\n}\nvoid main() {",
            1,
        )
        .replacen(
            "vec2 causticsCoords = v_TexCoord;",
            "vec2 causticsCoords = v_EffectCoord;",
            1,
        )
}

fn tech_circle_fragment_source(key: &str) -> String {
    let polar = effect_combo_value_for_key(key, "COORD_SYS", 1) == 1;
    let ring_segments = effect_combo_value_for_key(key, "RING_SEGMENTS", 0) == 1;
    let sector_segments = effect_combo_value_for_key(key, "SECTOR_SEGMENTS", 0);
    let ratio_correction = effect_combo_value_for_key(key, "RATIO_CORRECTION", 0) == 1;
    let coordinate_expression = if polar {
        "vec2 centered = v_ObjectTexCoord - 0.5;\n    vec2 uv = vec2(length(centered) * 2.0, atan(centered.x, centered.y) / TAU + 0.5);"
    } else {
        "vec2 uv = v_ObjectTexCoord;"
    };
    let ratio_expression = if ratio_correction {
        "uv.y /= float(textureSize(g_Texture0, 0).x)\n        / float(max(textureSize(g_Texture0, 0).y, 1));"
    } else {
        ""
    };
    let ring_expression = if ring_segments {
        "float ring_value = ring(ring_radius, ring_width, u_Effect.g_RingWidthSegmentsSectorOffset.y, u_Effect.g_RingWidthSegmentsSectorOffset.z, uv);"
    } else {
        "float ring_value = ring(ring_radius, ring_width, 1.0, 1.0, uv);"
    };
    let sector_expression = match sector_segments {
        1 => {
            "float sector_value = sector(sector_position, sector_width, u_Effect.g_SectorWidthSegments.y, u_Effect.g_SectorWidthSegments.z, uv);"
        }
        2 => {
            "float sector_value = sector(sector_position, sector_width, u_Effect.g_SectorWidthSegments.y, u_Effect.g_SectorWidthSegments.z / max(perimeter_ratio, 0.000001), uv);"
        }
        _ => "float sector_value = sector(sector_position, sector_width, 1.0, 1.0, uv);",
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform TechCircleUniform {{
    vec4 g_ColorAlpha;
    vec4 g_TimeSpeedSkewRingRadius;
    vec4 g_RingWidthSegmentsSectorOffset;
    vec4 g_SectorWidthSegments;
}} u_Effect;
const float TAU = 6.283185307179586;
float saw(float x) {{
    return abs(mod(x * 2.0 + 1.0, 2.0) - 1.0);
}}
float stripes(float count, float threshold, float x) {{
    return step(1.0 - threshold, saw(x * count))
        * step(0.0, x) * step(x, 1.0);
}}
float ring(float distance, float width, float count, float threshold, vec2 uv) {{
    return stripes(count, threshold, (uv.x - distance + width * 0.5) / width);
}}
float sector(float position, float width, float count, float threshold, vec2 uv) {{
    float value = fract(uv.y - fract(position - width * 0.5));
    return stripes(count, threshold, value / width);
}}
void main() {{
    if (any(lessThan(v_ObjectTexCoord, vec2(0.0)))
        || any(greaterThan(v_ObjectTexCoord, vec2(1.0)))) {{
        o_Color = vec4(0.0);
        return;
    }}
    vec4 background = texture(g_Texture0, v_TexCoord);
    {coordinate_expression}
    {ratio_expression}
    float ring_radius = u_Effect.g_TimeSpeedSkewRingRadius.w;
    float ring_width = max(u_Effect.g_RingWidthSegmentsSectorOffset.x, 0.000001);
    float perimeter_ratio = uv.x / max(ring_radius, 0.000001);
    uv.y += ((uv.x - ring_radius) / ring_width / max(perimeter_ratio, 0.000001))
        * u_Effect.g_TimeSpeedSkewRingRadius.z;
    {ring_expression}
    float sector_position = u_Effect.g_RingWidthSegmentsSectorOffset.w
        + u_Effect.g_TimeSpeedSkewRingRadius.x * u_Effect.g_TimeSpeedSkewRingRadius.y;
    float sector_width = u_Effect.g_SectorWidthSegments.x;
    {sector_expression}
    float final_alpha = ring_value * sector_value * u_Effect.g_ColorAlpha.a;
    o_Color = (1.0 - final_alpha) * background
        + vec4(final_alpha * u_Effect.g_ColorAlpha.rgb, 1.0);
}}
"#
    )
}

fn scroll_effect_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec4 v_ObjectUvToScreenUv;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ScrollUniform {
    vec4 g_TimeSpeed;
    vec4 g_Repeat;
    vec4 g_Unused0;
    vec4 g_Unused1;
} u_Effect;
vec2 objectDeltaToScreen(vec2 delta) {
    return vec2(
        dot(v_ObjectUvToScreenUv.xy, delta),
        dot(v_ObjectUvToScreenUv.zw, delta));
}
void main() {
    if (any(lessThan(v_ObjectTexCoord, vec2(0.0)))
        || any(greaterThan(v_ObjectTexCoord, vec2(1.0)))) {
        o_Color = vec4(0.0);
        return;
    }
    vec2 speed = u_Effect.g_TimeSpeed.yz;
    vec2 scroll = sign(speed) * speed * speed * u_Effect.g_TimeSpeed.x;
    vec2 sample_object_uv = fract(
        (v_ObjectTexCoord + scroll) * u_Effect.g_Repeat.xy);
    vec2 sample_screen_uv = v_TexCoord
        + objectDeltaToScreen(sample_object_uv - v_ObjectTexCoord);
    o_Color = texture(g_Texture0, sample_screen_uv);
}
"#
    .to_owned()
}

fn skew_effect_fragment_source(key: &str) -> String {
    let repeat = effect_combo_value_for_key(key, "REPEAT", 1) != 0;
    let repeat_statement = if repeat {
        "sample_object_uv = fract(sample_object_uv);"
    } else {
        ""
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec4 v_ObjectUvToScreenUv;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform SkewUniform {{
    vec4 g_TopBottomLeftRight;
    vec4 g_Unused0;
    vec4 g_Unused1;
    vec4 g_Unused2;
}} u_Effect;
vec2 objectDeltaToScreen(vec2 delta) {{
    return vec2(
        dot(v_ObjectUvToScreenUv.xy, delta),
        dot(v_ObjectUvToScreenUv.zw, delta));
}}
void main() {{
    if (any(lessThan(v_ObjectTexCoord, vec2(0.0)))
        || any(greaterThan(v_ObjectTexCoord, vec2(1.0)))) {{
        o_Color = vec4(0.0);
        return;
    }}
    vec2 sample_object_uv = v_ObjectTexCoord;
    sample_object_uv.x -= mix(
        u_Effect.g_TopBottomLeftRight.x,
        u_Effect.g_TopBottomLeftRight.y,
        step(0.5, v_ObjectTexCoord.y));
    sample_object_uv.y += mix(
        u_Effect.g_TopBottomLeftRight.z,
        u_Effect.g_TopBottomLeftRight.w,
        step(0.5, v_ObjectTexCoord.x));
    {repeat_statement}
    vec2 sample_screen_uv = v_TexCoord
        + objectDeltaToScreen(sample_object_uv - v_ObjectTexCoord);
    o_Color = texture(g_Texture0, sample_screen_uv);
}}
"#
    )
}

pub(crate) fn effect_combo_value_for_key(key: &str, combo: &str, default: i64) -> i64 {
    let prefix = format!("{}_", combo.to_ascii_uppercase());
    key.split("__")
        .find_map(|component| {
            component
                .to_ascii_uppercase()
                .strip_prefix(&prefix)
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(default)
}
