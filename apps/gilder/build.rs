use std::env;
use std::fs;
use std::path::PathBuf;

#[path = "build/native_video.rs"]
mod native_video;
#[path = "build/scene_shader.rs"]
mod scene_shader;
#[path = "build/system_audio_monitor.rs"]
mod system_audio_monitor;

use scene_shader::{
    SceneShaderFamily, SceneShaderSpec, effect_fragment_source, effect_vertex_source,
};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/scene_shader.rs");
    println!("cargo:rerun-if-changed=build/scene_shader");
    println!("cargo:rerun-if-changed=build/native_video.rs");
    println!("cargo:rerun-if-changed=build/system_audio_monitor.rs");
    println!("cargo:rerun-if-changed=src/renderer/native_vulkan/video/demux_ffmpeg_shim.c");
    println!(
        "cargo:rerun-if-changed=src/renderer/native_vulkan/audio/system_monitor/pipewire_monitor.c"
    );

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }

    scene_shader::build_scene_shader_origin_catalog();

    if env::var_os("CARGO_FEATURE_NATIVE_VULKAN_RENDERER").is_some() {
        scene_shader::build_scene_shader_catalog();
        system_audio_monitor::build_system_audio_monitor();
    }

    if env::var_os("CARGO_FEATURE_NATIVE_VULKAN_VIDEO").is_some() {
        native_video::build_ffmpeg_demux_shim();
    }
}

fn scene_mesh_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 3) in uvec4 a_BlendIndices;
layout(location = 4) in vec4 a_BlendWeights;
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

fn composelayer_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 0) out vec2 v_FramebufferCoord;
layout(set = 0, binding = 2) uniform SceneDrawTransform {
    vec4 g_ModelViewProjectionMatrix[4];
} g_Draw;
void main() {
    vec4 local_position = vec4(a_Position.xy, 0.0, 1.0);
    vec4 projected = vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], local_position));
    vec2 projected_ndc = projected.xy / max(abs(projected.w), 0.000001);
    // The scene clip matrix already converts WE's upward Y axis for Vulkan's
    // positive-height viewport, so framebuffer UV needs no second Y flip.
    v_FramebufferCoord = projected_ndc * 0.5 + 0.5;
    gl_Position = vec4(a_TexCoord * 2.0 - 1.0, 0.0, 1.0);
}
"#
    .to_owned()
}

fn composelayer_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_FramebufferCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
void main() {
    o_Color = texture(g_Texture0, v_FramebufferCoord);
}
"#
    .to_owned()
}

fn scene_puppet_skinning_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 3) in uvec4 a_BlendIndices;
layout(location = 4) in vec4 a_BlendWeights;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
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
void main() {
    v_TexCoord = a_TexCoord;
    vec4 raw_position = vec4(a_Position.xy, 0.0, 1.0);
    vec4 skinned_position = vec4(0.0);
    float skinned_alpha = 0.0;
    float total_weight = 0.0;
    for (uint slot = 0u; slot < 4u; slot++) {
        float weight = a_BlendWeights[slot];
        if (weight <= 0.0000001) {
            continue;
        }
        uint bone_index = a_BlendIndices[slot];
        GilderPuppetBonePalette bone = g_Puppet.g_Bones[bone_index];
        vec4 bone_position = vec4(
            dot(bone.row0, raw_position),
            dot(bone.row1, raw_position),
            dot(bone.row2, raw_position),
            dot(bone.row3, raw_position));
        skinned_position += bone_position * weight;
        skinned_alpha += bone.alpha.x * weight;
        total_weight += weight;
    }
    vec4 local_position = raw_position;
    v_VertexAlpha = a_Opacity;
    if (total_weight > 0.0000001) {
        local_position = skinned_position / total_weight;
        v_VertexAlpha *= skinned_alpha / total_weight;
    }
    gl_Position = vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], local_position));
}
"#
    .to_owned()
}

fn scene_puppet_skinning_clippingtarget_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 3) in uvec4 a_BlendIndices;
layout(location = 4) in vec4 a_BlendWeights;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
layout(location = 2) out vec3 v_ScreenPos;
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
void main() {
    v_TexCoord = a_TexCoord;
    vec4 raw_position = vec4(a_Position.xy, 0.0, 1.0);
    vec4 skinned_position = vec4(0.0);
    float skinned_alpha = 0.0;
    float total_weight = 0.0;
    for (uint slot = 0u; slot < 4u; slot++) {
        float weight = a_BlendWeights[slot];
        if (weight <= 0.0000001) {
            continue;
        }
        uint bone_index = a_BlendIndices[slot];
        GilderPuppetBonePalette bone = g_Puppet.g_Bones[bone_index];
        vec4 bone_position = vec4(
            dot(bone.row0, raw_position),
            dot(bone.row1, raw_position),
            dot(bone.row2, raw_position),
            dot(bone.row3, raw_position));
        skinned_position += bone_position * weight;
        skinned_alpha += bone.alpha.x * weight;
        total_weight += weight;
    }
    vec4 local_position = raw_position;
    v_VertexAlpha = a_Opacity;
    if (total_weight > 0.0000001) {
        local_position = skinned_position / total_weight;
        v_VertexAlpha *= skinned_alpha / total_weight;
    }
    gl_Position = vec4(
        dot(g_Draw.g_ModelViewProjectionMatrix[0], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[1], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[2], local_position),
        dot(g_Draw.g_ModelViewProjectionMatrix[3], local_position));
    // The runtime uses a positive-height Vulkan viewport. NDC Y therefore
    // already maps directly to framebuffer UV Y and must not be flipped here.
    v_ScreenPos = gl_Position.xyw;
}
"#
    .to_owned()
}

fn scene_mesh_clippingtarget_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 2) in float a_Opacity;
layout(location = 0) out vec2 v_TexCoord;
layout(location = 1) out float v_VertexAlpha;
layout(location = 2) out vec3 v_ScreenPos;
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
    // The runtime uses a positive-height Vulkan viewport. NDC Y therefore
    // already maps directly to framebuffer UV Y and must not be flipped here.
    v_ScreenPos = gl_Position.xyw;
}
"#
    .to_owned()
}

fn genericimage4_clippingtarget_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 2) in vec3 v_ScreenPos;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 8) uniform sampler2D g_Texture8;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord);
    vec2 screen_uv = (v_ScreenPos.xy / v_ScreenPos.z) * 0.5 + 0.5;
    color.a *= texture(g_Texture8, screen_uv).r * v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn clippingmaskimage4_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Mask;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
void main() {
    float albedo_alpha = texture(g_Texture0, v_TexCoord).a;
    float mask = texture(g_Texture1, v_TexCoord).r;
    float alpha = mix(pow(albedo_alpha, 4.0), albedo_alpha, mask) * v_VertexAlpha;
    o_Mask = vec4(mask * alpha, 0.0, 0.0, alpha);
}
"#
    .to_owned()
}

fn flattexture_vertex_source() -> String {
    r#"#version 450
layout(location = 0) out vec2 v_TexCoord;
void main() {
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    v_TexCoord = position * 0.5 + 0.5;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#
    .to_owned()
}

fn colorkey_effect_fragment_source(_texture_slot_mask: u32) -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ColorKeyUniform {
    vec4 g_AlphaFuzzToleranceInvert;
    vec4 g_KeyColorFlatten;
    vec4 g_Unused0;
    vec4 g_Unused1;
} u_Effect;
void main() {
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float delta = dot(abs(u_Effect.g_KeyColorFlatten.rgb - albedo.rgb), vec3(1.0));
    float blend = smoothstep(
        0.001,
        0.002 + u_Effect.g_AlphaFuzzToleranceInvert.y,
        delta - u_Effect.g_AlphaFuzzToleranceInvert.z);
    if (u_Effect.g_AlphaFuzzToleranceInvert.w > 0.5) {
        blend = 1.0 - blend;
    }
    albedo.a *= mix(u_Effect.g_AlphaFuzzToleranceInvert.x, 1.0, blend);
    if (u_Effect.g_KeyColorFlatten.w > 0.5) {
        albedo.rgb *= albedo.a;
    }
    o_Color = albedo;
}
"#
    .to_owned()
}

fn cloudmotion_effect_fragment_source(texture_slot_mask: u32) -> String {
    let noise_sampler = if texture_slot_mask & (1 << 2) != 0 {
        "layout(set = 0, binding = 2) uniform sampler2D g_Texture2;"
    } else {
        ""
    };
    let noise_sample = if texture_slot_mask & (1 << 2) != 0 {
        "texture(g_Texture2, v_NoiseTexCoord).x"
    } else {
        "valueNoise(v_NoiseTexCoord)"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_NoiseTexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
{noise_sampler}
layout(set = 0, binding = 3) uniform CloudMotionUniform {{
    vec4 g_TimeSpeedAmountDirection;
    vec4 g_ScaleScaleXAspectUnused;
    vec4 g_Unused0;
    vec4 g_Unused1;
}} u_Effect;
float hash21(vec2 p) {{
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}}
float valueNoise(vec2 p) {{
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2(1.0, 0.0)), f.x),
               mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0)), f.x), f.y);
}}
void main() {{
    float noise = {noise_sample} * 2.0 - 1.0;
    float angle = u_Effect.g_TimeSpeedAmountDirection.w + 1.570796;
    vec2 direction = vec2(cos(angle), sin(angle));
    vec2 offset = direction * noise * u_Effect.g_TimeSpeedAmountDirection.z;
    o_Color = texture(g_Texture0, v_TexCoord + offset);
}}
"#
    )
}

fn shake_effect_fragment_source(texture_slot_mask: u32) -> String {
    let flow_sampler = if texture_slot_mask & (1 << 1) != 0 {
        "layout(set = 0, binding = 1) uniform sampler2D g_Texture1;"
    } else {
        ""
    };
    let flow_sample = if texture_slot_mask & (1 << 1) != 0 {
        "texture(g_Texture1, v_TexCoord).rg * 2.0 - 1.0"
    } else {
        "vec2(1.0, 0.0)"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
{flow_sampler}
layout(set = 0, binding = 3) uniform ShakeUniform {{
    vec4 g_TimeSpeedStrengthUnused;
    vec4 g_BoundsFriction;
    vec4 g_Unused0;
    vec4 g_Unused1;
}} u_Effect;
void main() {{
    float phase = u_Effect.g_TimeSpeedStrengthUnused.x
        * u_Effect.g_TimeSpeedStrengthUnused.y;
    float wave = sin(phase);
    float lower = u_Effect.g_BoundsFriction.x;
    float range = max(u_Effect.g_BoundsFriction.y - lower, 0.0001);
    wave = clamp((wave * 0.5 + 0.5 - lower) / range, 0.0, 1.0) * 2.0 - 1.0;
    vec2 flow = {flow_sample};
    float strength = u_Effect.g_TimeSpeedStrengthUnused.z;
    vec2 offset = flow * wave * strength * strength;
    o_Color = texture(g_Texture0, clamp(v_TexCoord + offset, vec2(0.001), vec2(0.999)));
}}
"#
    )
}

fn iris_effect_vertex_source(texture_slot_mask: u32) -> String {
    let mask_uv = if texture_slot_mask & (1 << 1) != 0 {
        "if (u_IrisVertex.g_ScalePhaseMask.w > 0.5 && u_IrisVertex.g_Texture1Resolution.x > 0.0 && u_IrisVertex.g_Texture1Resolution.y > 0.0) {\n        v_TexCoord.zw = vec2(uv.x * u_IrisVertex.g_Texture1Resolution.z / u_IrisVertex.g_Texture1Resolution.x,\n                             uv.y * u_IrisVertex.g_Texture1Resolution.w / u_IrisVertex.g_Texture1Resolution.y);\n    }"
    } else {
        ""
    };
    format!(
        r#"#version 450
layout(set = 0, binding = 2) uniform IrisVertexUniform {{
    vec4 g_TimeSpeedRoughNoise;
    vec4 g_ScalePhaseMask;
    vec4 g_Texture1Resolution;
}} u_IrisVertex;
layout(location = 0) out vec4 v_TexCoord;
layout(location = 1) out vec2 v_TexCoordIris;
void main() {{
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    v_TexCoord = uv.xyxy;
    {mask_uv}

    float time = u_IrisVertex.g_TimeSpeedRoughNoise.x * u_IrisVertex.g_TimeSpeedRoughNoise.y
        + u_IrisVertex.g_ScalePhaseMask.z;
    float lowDt = floor(time);
    vec2 motion2 = sin(1.9 * (lowDt + vec2(0.0, 1.0)));
    vec4 motion4 = sin(2.5 * (lowDt + vec4(0.0, 0.0, 1.0, 1.0)) + vec4(1.0, 2.0, 1.0, 2.0));
    vec2 moveStart = motion2.xx + motion4.xy;
    vec2 moveEnd = motion2.yy + motion4.zw;
    float smoothInput = cos(fract(time) * 3.14159265358979323846) * -0.5 + 0.5;
    vec2 da = mix(moveStart, moveEnd, smoothstep(1.0 - u_IrisVertex.g_TimeSpeedRoughNoise.z, 1.0, smoothInput));
    da.x += sin(time) * u_IrisVertex.g_TimeSpeedRoughNoise.w;
    da.y += cos(time) * u_IrisVertex.g_TimeSpeedRoughNoise.w;
    da *= u_IrisVertex.g_ScalePhaseMask.xy * 0.001;
    v_TexCoordIris = da;
    gl_Position = vec4(position, 0.0, 1.0);
}}
"#
    )
}

fn iris_effect_fragment_source(texture_slot_mask: u32) -> String {
    let samplers = effect_sampler_declarations(texture_slot_mask);
    let iris_sample = if texture_slot_mask & (1 << 1) != 0 {
        r#"float mask = texture(g_Texture1, v_TexCoord.zw).r;
    vec4 iris = texture(g_Texture0, v_TexCoord.xy + v_TexCoordIris * mask);
    float irisMask = texture(g_Texture1, v_TexCoord.zw + v_TexCoordIris * mask).r;
    if (u_IrisFragment.g_EyeColorBackground.w > 0.5) {
        iris.rgb = mix(u_IrisFragment.g_EyeColorBackground.rgb, iris.rgb, irisMask);
    }"#
    } else {
        "vec4 iris = texture(g_Texture0, v_TexCoord.xy + v_TexCoordIris);"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec4 v_TexCoord;
layout(location = 1) in vec2 v_TexCoordIris;
layout(location = 0) out vec4 o_Color;
{samplers}layout(set = 0, binding = 3) uniform IrisFragmentUniform {{
    vec4 g_EyeColorBackground;
}} u_IrisFragment;
void main() {{
    {iris_sample}
    o_Color = iris;
}}
"#
    )
}

fn effect_sampler_declarations(texture_slot_mask: u32) -> String {
    let mut samplers = String::new();
    for slot in 0..32 {
        if texture_slot_mask & (1u32 << slot) != 0 {
            let binding = scene_texture_shader_binding(slot);
            samplers.push_str(&format!(
                "layout(set = 0, binding = {binding}) uniform sampler2D g_Texture{slot};\n"
            ));
        }
    }
    samplers
}

fn scene_texture_shader_binding(slot: u32) -> u32 {
    if slot == 3 { 35 } else { slot }
}

fn foliage_sway_fragment_source(texture_slot_mask: u32) -> String {
    let samplers = effect_sampler_declarations(texture_slot_mask);
    let noise_sample = if texture_slot_mask & (1 << 2) != 0 {
        "float noise = texture(g_Texture2, v_TexCoord * u_Effect.g_PowerNoiseScaleRatioDirection.y).g;"
    } else {
        "float noise = valueNoise(v_TexCoord * max(u_Effect.g_PowerNoiseScaleRatioDirection.y, 0.0001) * 64.0);"
    };
    let mask_sample = if texture_slot_mask & (1 << 1) != 0 {
        "float mask = texture(g_Texture1, v_TexCoord).r;"
    } else {
        "float mask = 1.0;"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
{samplers}layout(set = 0, binding = 3) uniform FoliageSwayUniform {{
    vec4 g_TimeSpeedStrengthPhase;
    vec4 g_PowerNoiseScaleRatioDirection;
    vec4 g_Texture0Resolution;
    vec4 g_Reserved;
}} u_Effect;
vec2 rotateVec2(vec2 v, float r) {{
    vec2 cs = vec2(cos(r), sin(r));
    return vec2(v.x * cs.x - v.y * cs.y, v.x * cs.y + v.y * cs.x);
}}
float hash12(vec2 p) {{
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}}
float valueNoise(vec2 p) {{
    vec2 cell = floor(p);
    vec2 local = fract(p);
    local = local * local * (3.0 - 2.0 * local);
    return mix(
        mix(hash12(cell), hash12(cell + vec2(1.0, 0.0)), local.x),
        mix(hash12(cell + vec2(0.0, 1.0)), hash12(cell + vec2(1.0)), local.x),
        local.y);
}}
vec4 shapedSine(vec4 phase, float power) {{
    vec4 wave = sin(phase);
    return pow(abs(wave), vec4(max(power, 0.0001))) * sign(wave);
}}
void main() {{
    float width = max(u_Effect.g_Texture0Resolution.z, 1.0);
    float height = max(u_Effect.g_Texture0Resolution.w, 1.0);
    float ratio = max(u_Effect.g_PowerNoiseScaleRatioDirection.z, 0.0001);
    float aspect = max(width / height * ratio, 0.0001);
    float direction = u_Effect.g_PowerNoiseScaleRatioDirection.w;
    vec2 offsetScale = rotateVec2(vec2(1.0 / aspect, aspect), direction);
    vec2 phasePosition = rotateVec2(v_TexCoord, direction);
    {noise_sample}
    {mask_sample}
    float phase = (noise * 6.283185307179586
        + phasePosition.x * 10.0
        + phasePosition.y * 5.0) * u_Effect.g_TimeSpeedStrengthPhase.w;
    float time = u_Effect.g_TimeSpeedStrengthPhase.x
        * u_Effect.g_TimeSpeedStrengthPhase.y;
    vec4 sines = shapedSine(
        phase + time * vec4(1.0, -0.16161616, 0.0083333, -0.00019841),
        u_Effect.g_PowerNoiseScaleRatioDirection.x);
    vec4 cosines = shapedSine(
        0.4 + phase + time * vec4(-0.5, 0.041666666, -0.0013888889, 0.000024801587),
        u_Effect.g_PowerNoiseScaleRatioDirection.x);
    float amplitude = u_Effect.g_TimeSpeedStrengthPhase.z
        * u_Effect.g_TimeSpeedStrengthPhase.z * 0.005 * mask;
    vec2 offset = offsetScale * vec2(
        dot(sines, vec4(amplitude)),
        dot(cosines, vec4(amplitude)));
    o_Color = texture(g_Texture0, v_TexCoord + offset);
}}
"#
    )
}

fn waterwaves_fragment_source(texture_slot_mask: u32) -> String {
    let samplers = effect_sampler_declarations(texture_slot_mask);
    let mask_sample = if texture_slot_mask & (1 << 1) != 0 {
        "vec2 mask_uv = v_ObjectTexCoord * u_Effect.g_Texture1Resolution.zw / u_Effect.g_Texture1Resolution.xy;\n    float mask = texture(g_Texture1, mask_uv).r;"
    } else {
        "float mask = 1.0;"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec4 v_ObjectUvToScreenUv;
layout(location = 0) out vec4 o_Color;
{samplers}layout(set = 0, binding = 3) uniform WaterWavesUniform {{
    vec4 g_TimeSpeedScaleStrength;
    vec4 g_DirectionSpeed2Scale2Direction2;
    vec4 g_Offset2DualExponentExponent2;
    vec4 g_Texture1Resolution;
}} u_Effect;
vec2 rotateVec2(vec2 v, float r) {{
    vec2 cs = vec2(cos(r), sin(r));
    return vec2(v.x * cs.x - v.y * cs.y, v.x * cs.y + v.y * cs.x);
}}
float shapedSine(float phase, float exponent) {{
    float wave = sin(phase);
    return pow(abs(wave), max(exponent, 0.0001)) * sign(wave);
}}
void main() {{
    vec2 source_uv = v_TexCoord;
    vec2 motion_uv = v_ObjectTexCoord;
    {mask_sample}
    vec2 direction = rotateVec2(vec2(0.0, 1.0), u_Effect.g_DirectionSpeed2Scale2Direction2.x);
    float distance0 = u_Effect.g_TimeSpeedScaleStrength.x * u_Effect.g_TimeSpeedScaleStrength.y
        + dot(motion_uv, direction) * u_Effect.g_TimeSpeedScaleStrength.z;
    vec2 offset = vec2(direction.y, -direction.x);
    float strength = u_Effect.g_TimeSpeedScaleStrength.w;
    float displacement = shapedSine(
        distance0,
        u_Effect.g_Offset2DualExponentExponent2.z);
    if (u_Effect.g_Offset2DualExponentExponent2.y > 0.5) {{
        vec2 direction2 = rotateVec2(vec2(0.0, 1.0), u_Effect.g_DirectionSpeed2Scale2Direction2.w);
        float distance1 = (u_Effect.g_TimeSpeedScaleStrength.x
            + u_Effect.g_Offset2DualExponentExponent2.x)
            * u_Effect.g_DirectionSpeed2Scale2Direction2.y
            + dot(motion_uv, direction2) * u_Effect.g_DirectionSpeed2Scale2Direction2.z;
        displacement *= shapedSine(
            distance1,
            u_Effect.g_Offset2DualExponentExponent2.w);
    }}
    vec2 object_uv_offset = displacement * offset * strength * strength * mask;
    vec2 screen_uv_offset = vec2(
        dot(v_ObjectUvToScreenUv.xy, object_uv_offset),
        dot(v_ObjectUvToScreenUv.zw, object_uv_offset));
    o_Color = texture(g_Texture0, source_uv + screen_uv_offset);
}}
"#
    )
}

fn waterflow_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec4 v_ObjectUvToScreenUv;
layout(location = 3) flat in vec4 v_Cycles;
layout(location = 4) flat in vec2 v_BlendWeight;
layout(location = 5) in vec2 v_FlowTexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;
layout(set = 0, binding = 3) uniform WaterFlowUniform {
    vec4 g_TimeSpeedFeatherStrength;
    vec4 g_PhaseScale;
    vec4 g_Texture1Resolution;
    vec4 g_Unused;
} u_Effect;
vec2 objectDeltaToScreen(vec2 delta) {
    return vec2(dot(v_ObjectUvToScreenUv.xy, delta),
        dot(v_ObjectUvToScreenUv.zw, delta));
}
vec4 sourceAtObjectUv(vec2 object_uv) {
    return texture(g_Texture0,
        v_TexCoord + objectDeltaToScreen(object_uv - v_ObjectTexCoord));
}
void main() {
    if (any(lessThan(v_ObjectTexCoord, vec2(0.0)))
        || any(greaterThan(v_ObjectTexCoord, vec2(1.0)))) {
        o_Color = vec4(0.0);
        return;
    }
    vec2 flow = (texture(g_Texture1, v_FlowTexCoord).rg - vec2(0.498)) * 2.0;
    float strength = u_Effect.g_TimeSpeedFeatherStrength.w * 0.1;
    vec4 offset0 = flow.xyxy * strength * v_Cycles.xxyy;
    vec4 offset1 = flow.xyxy * strength * v_Cycles.zzww;
    vec4 first = mix(sourceAtObjectUv(v_ObjectTexCoord + offset0.xy),
        sourceAtObjectUv(v_ObjectTexCoord + offset0.zw), v_BlendWeight.x);
    vec4 second = mix(sourceAtObjectUv(v_ObjectTexCoord + offset1.xy),
        sourceAtObjectUv(v_ObjectTexCoord + offset1.zw), v_BlendWeight.y);
    float phase = texture(g_Texture2,
        v_ObjectTexCoord * u_Effect.g_PhaseScale.x).r;
    vec4 flowed = mix(first, second, smoothstep(0.2, 0.8, phase));
    o_Color = mix(sourceAtObjectUv(v_ObjectTexCoord), flowed, length(flow));
}
"#
    .to_owned()
}

fn waterripple_fragment_source(texture_slot_mask: u32) -> String {
    let samplers = effect_sampler_declarations(texture_slot_mask);
    let normal_sample = if texture_slot_mask & (1 << 2) != 0 {
        "vec4 n1_sample = texture(g_Texture2, ripple.xy);\n    vec4 n2_sample = texture(g_Texture2, ripple.zw);\n    vec3 n1 = n1_sample.xyz * 2.0 - 1.0;\n    vec3 n2 = n2_sample.xyz * 2.0 - 1.0;"
    } else if texture_slot_mask & (1 << 1) != 0 {
        "vec4 n1_sample = texture(g_Texture1, ripple.xy);\n    vec4 n2_sample = texture(g_Texture1, ripple.zw);\n    vec3 n1 = n1_sample.xyz * 2.0 - 1.0;\n    vec3 n2 = n2_sample.xyz * 2.0 - 1.0;"
    } else {
        "vec3 n1 = vec3(0.0, 0.0, 1.0);\n    vec3 n2 = vec3(0.0);"
    };
    let mask_sample = if texture_slot_mask & (1 << 1) != 0 && texture_slot_mask & (1 << 2) != 0 {
        "vec2 mask_uv = v_TexCoord * u_Effect.g_TextureMaskResolution.zw / u_Effect.g_TextureMaskResolution.xy;\n    float mask = texture(g_Texture1, mask_uv).r;"
    } else {
        "float mask = 1.0;"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
{samplers}layout(set = 0, binding = 3) uniform WaterRippleUniform {{
    vec4 g_TimeAnimationScaleScroll;
    vec4 g_DirectionStrengthAspectNormal;
    vec4 g_MaskFlags;
    vec4 g_TextureMaskResolution;
}} u_Effect;
vec2 rotateVec2(vec2 v, float r) {{
    vec2 cs = vec2(cos(r), sin(r));
    return vec2(v.x * cs.x - v.y * cs.y, v.x * cs.y + v.y * cs.x);
}}
void main() {{
    vec2 tex_coord = v_TexCoord;
    float strength = u_Effect.g_DirectionStrengthAspectNormal.y;
    if (strength == 0.0) {{
        o_Color = texture(g_Texture0, tex_coord);
        return;
    }}
    vec2 scroll = rotateVec2(vec2(0.0, 1.0), u_Effect.g_DirectionStrengthAspectNormal.x)
        * u_Effect.g_TimeAnimationScaleScroll.w
        * u_Effect.g_TimeAnimationScaleScroll.w
        * u_Effect.g_TimeAnimationScaleScroll.x;
    float anim = u_Effect.g_TimeAnimationScaleScroll.x
        * u_Effect.g_TimeAnimationScaleScroll.y
        * u_Effect.g_TimeAnimationScaleScroll.y;
    vec4 ripple = vec4(tex_coord + anim + scroll, tex_coord * 1.333 - anim + scroll)
        * u_Effect.g_TimeAnimationScaleScroll.z;
    ripple.xz *= u_Effect.g_DirectionStrengthAspectNormal.z;
    ripple.yw *= u_Effect.g_MaskFlags.w;
    {normal_sample}
    vec3 normal = normalize(vec3(n1.xy + n2.xy, n1.z));
    {mask_sample}
    tex_coord += normal.xy * strength * strength * mask;
    o_Color = texture(g_Texture0, tex_coord);
}}
"#
    )
}

fn opacity_effect_fragment_source(texture_slot_mask: u32) -> String {
    let samplers = effect_sampler_declarations(texture_slot_mask);
    let mask_sample = if texture_slot_mask & (1 << 1) != 0 {
        "vec2 mask_uv = v_TexCoord * u_Effect.g_Texture1Resolution.zw / u_Effect.g_Texture1Resolution.xy;\n    color.a *= texture(g_Texture1, mask_uv).r;"
    } else {
        ""
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
{samplers}layout(set = 0, binding = 3) uniform OpacityUniform {{
    vec4 g_AlphaMask;
    vec4 g_Texture1Resolution;
    vec4 g_Reserved0;
    vec4 g_Reserved1;
}} u_Effect;
void main() {{
    vec4 color = texture(g_Texture0, v_TexCoord);
    {mask_sample}
    color.a *= u_Effect.g_AlphaMask.x;
    o_Color = color;
}}
"#
    )
}
