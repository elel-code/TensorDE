use super::super::*;
use super::blend::{blend_fragment_source, blend_gradient_fragment_source};
use super::shimmer::shimmer_fragment_source;
use super::swing::swing_fragment_source;

#[path = "effect_program/caustics.rs"]
mod caustics;

use caustics::caustics_effect_fragment_source;

pub(crate) fn effect_vertex_source(key: &str, shader: &str, texture_slot_mask: u32) -> String {
    if key.contains("__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1") {
        return framebuffer_quantized_overlay_effect_vertex_source();
    }
    if shader == "effects/iris" {
        return iris_effect_vertex_source(texture_slot_mask);
    }
    if shader == "effects/cloudmotion" {
        return cloudmotion_effect_vertex_source();
    }
    if shader == "effects/blend" || shader == "effects/blendgradient" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "effects/waterwaves" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "effects/scroll" {
        return waterwaves_effect_vertex_source();
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
    _texture_slot_mask: u32,
) -> Option<String> {
    if key.contains("__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1") {
        return None;
    }
    if shader == "effects/iris" {
        return None;
    }
    if shader == "effects/cloudmotion" {
        return Some(cloudmotion_effect_object_mesh_vertex_source());
    }
    if matches!(
        shader,
        "effects/blend" | "effects/blendgradient" | "effects/waterwaves" | "effects/scroll"
    ) {
        return Some(object_uv_affine_effect_object_mesh_vertex_source());
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
    if shader == "effects/tint" {
        return super::tint_fragment_source(key, texture_slot_mask);
    }
    if shader == "effects/spin" {
        return super::spin_fragment_source(key, texture_slot_mask);
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
        let chromatic_zero = key.contains("__TENSOR_WALLPAPER_CHROMATIC_ZERO_1");
        if key.contains("__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1") {
            return caustics_framebuffer_quantized_overlay_fragment_source(key, texture_slot_mask);
        }
        return caustics_effect_fragment_source(
            texture_slot_mask,
            chromatic_zero,
            key.contains("__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1"),
            key.contains("__TENSOR_WALLPAPER_COLOR_EQUAL_1"),
        );
    }
    if shader == "effects/cloudmotion" {
        return cloudmotion_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/colorkey" {
        return colorkey_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/swing" {
        return swing_fragment_source(texture_slot_mask);
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
    if shader == "effects/waterripple" {
        return waterripple_fragment_source(texture_slot_mask);
    }
    if shader == "effects/opacity" {
        return opacity_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/iris" {
        return iris_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/scroll" {
        return scroll_effect_fragment_source();
    }
    panic!("scene shader {key:?} has no typed fragment contract")
}

fn caustics_framebuffer_quantized_overlay_fragment_source(
    key: &str,
    texture_slot_mask: u32,
) -> String {
    caustics_effect_fragment_source(
        texture_slot_mask,
        key.contains("__TENSOR_WALLPAPER_CHROMATIC_ZERO_1"),
        key.contains("__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1"),
        key.contains("__TENSOR_WALLPAPER_COLOR_EQUAL_1"),
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
