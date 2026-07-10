use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/renderer/native_vulkan/video/demux_ffmpeg_shim.c");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }

    if env::var_os("CARGO_FEATURE_NATIVE_VULKAN_RENDERER").is_some() {
        build_scene_shader_catalog();
    }

    if env::var_os("CARGO_FEATURE_NATIVE_VULKAN_VIDEO").is_some() {
        build_ffmpeg_demux_shim();
    }
}

#[derive(Debug, Clone, Copy)]
struct SceneShaderSpec {
    key: &'static str,
    family: SceneShaderFamily,
}

#[derive(Debug, Clone, Copy)]
enum SceneShaderFamily {
    MeshGenericImage4,
    MeshGenericImage4PuppetSkinning,
    MeshColor,
    MeshColorPuppetSkinning,
    MeshText,
    MeshTextPuppetSkinning,
    MeshGenericParticle,
    MeshGenericImage4ClippingTarget,
    MeshGenericImage4ClippingTargetPuppetSkinning,
    MeshClippingMaskImage4,
    MeshClippingMaskImage4PuppetSkinning,
    FlatMinimalAlpha,
    FlatPassthrough,
    Effect,
}

const BUILTIN_SCENE_SHADER_SPECS: &[SceneShaderSpec] = &[
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_OVERLAY_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/cloudmotion__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/iris__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/iris__SLOTS_3__MASK_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/lightshafts__SLOTS_5__DIRECTDRAW_1__RAYCORNER_1__RAYMODE_2__RENDERING_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/opacity__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/opacity__SLOTS_3",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/scroll__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/skew__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/waterflow__SLOTS_7",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/waterripple__SLOTS_5",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/waterripple__SLOTS_7",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/waterwaves__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/waterwaves__SLOTS_3",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/waterwaves__SLOTS_3__DUALWAVES_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "minimalalpha",
        family: SceneShaderFamily::FlatMinimalAlpha,
    },
    SceneShaderSpec {
        key: "minimalalpha__SLOTS_1",
        family: SceneShaderFamily::FlatMinimalAlpha,
    },
    SceneShaderSpec {
        key: "passthrough",
        family: SceneShaderFamily::FlatPassthrough,
    },
    SceneShaderSpec {
        key: "we/clippingmaskimage4",
        family: SceneShaderFamily::MeshClippingMaskImage4,
    },
    SceneShaderSpec {
        key: "we/clippingmaskimage4__PUPPETSKINNING_1",
        family: SceneShaderFamily::MeshClippingMaskImage4PuppetSkinning,
    },
    SceneShaderSpec {
        key: "we/color",
        family: SceneShaderFamily::MeshColor,
    },
    SceneShaderSpec {
        key: "we/color__PUPPETSKINNING_1",
        family: SceneShaderFamily::MeshColorPuppetSkinning,
    },
    SceneShaderSpec {
        key: "we/genericimage2",
        family: SceneShaderFamily::MeshGenericImage4,
    },
    SceneShaderSpec {
        key: "we/genericimage2__PUPPETSKINNING_1",
        family: SceneShaderFamily::MeshGenericImage4PuppetSkinning,
    },
    SceneShaderSpec {
        key: "we/genericimage4",
        family: SceneShaderFamily::MeshGenericImage4,
    },
    SceneShaderSpec {
        key: "we/genericimage4__PUPPETSKINNING_1",
        family: SceneShaderFamily::MeshGenericImage4PuppetSkinning,
    },
    SceneShaderSpec {
        key: "we/genericimage4__CLIPPINGTARGET_1__CLIPPINGUVS_1",
        family: SceneShaderFamily::MeshGenericImage4ClippingTarget,
    },
    SceneShaderSpec {
        key: "we/genericimage4__PUPPETSKINNING_1__CLIPPINGTARGET_1__CLIPPINGUVS_1",
        family: SceneShaderFamily::MeshGenericImage4ClippingTargetPuppetSkinning,
    },
    SceneShaderSpec {
        key: "we/genericparticle",
        family: SceneShaderFamily::MeshGenericParticle,
    },
    SceneShaderSpec {
        key: "we/text",
        family: SceneShaderFamily::MeshText,
    },
    SceneShaderSpec {
        key: "we/text__PUPPETSKINNING_1",
        family: SceneShaderFamily::MeshTextPuppetSkinning,
    },
    SceneShaderSpec {
        key: "workshop/2123274886/effects/tech_circle__SLOTS_1__SECTOR_SEGMENTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2790231929/effects/foliagesway__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2790231929/effects/foliagesway__SLOTS_5",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2790231929/effects/waterripple__SLOTS_5",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2800594362/effects/clipping_mask__SLOTS_1__REPEAT_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/3082978660/effects/Simple_Audio_Bars__SLOTS_1__SHAPE_7",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/3083593512/effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1",
        family: SceneShaderFamily::Effect,
    },
];

fn build_scene_shader_catalog() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let shader_dir = out_dir.join("scene_shader_catalog");
    fs::create_dir_all(&shader_dir).expect("create scene shader catalog build dir");

    let mut generated = String::new();
    generated.push_str("#[derive(Debug, Clone, Copy)]\n");
    generated.push_str("pub struct BuiltinSceneShader {\n");
    generated.push_str("    pub key: &'static str,\n");
    generated.push_str("    pub vertex_spirv: &'static [u32],\n");
    generated.push_str("    pub fragment_spirv: &'static [u32],\n");
    generated.push_str("    pub parameter_layout: BuiltinSceneParameterLayout,\n");
    generated.push_str("}\n\n");

    let mut entries = String::new();
    for spec in BUILTIN_SCENE_SHADER_SPECS {
        let (vertex_source, fragment_source) = scene_shader_sources(*spec);
        let vertex_path = compile_scene_shader_stage(&shader_dir, spec.key, "vert", &vertex_source);
        let fragment_path =
            compile_scene_shader_stage(&shader_dir, spec.key, "frag", &fragment_source);
        let vertex_path = vertex_path
            .to_str()
            .expect("built-in scene vertex shader path must be UTF-8");
        let fragment_path = fragment_path
            .to_str()
            .expect("built-in scene fragment shader path must be UTF-8");
        let parameter_layout = scene_shader_parameter_layout(*spec);
        entries.push_str(&format!(
            "    BuiltinSceneShader {{ key: {:?}, vertex_spirv: vulkanalia::include_shader_code!({:?}), fragment_spirv: vulkanalia::include_shader_code!({:?}), parameter_layout: BuiltinSceneParameterLayout::{parameter_layout} }},\n",
            spec.key, vertex_path, fragment_path,
        ));
    }

    generated.push_str("pub static BUILTIN_SCENE_SHADERS: &[BuiltinSceneShader] = &[\n");
    generated.push_str(&entries);
    generated.push_str("];\n");

    fs::write(out_dir.join("gilder_scene_shader_catalog.rs"), generated)
        .expect("write built-in scene shader catalog");
}

fn scene_shader_parameter_layout(spec: SceneShaderSpec) -> &'static str {
    match spec.family {
        SceneShaderFamily::MeshGenericImage4
        | SceneShaderFamily::MeshGenericImage4PuppetSkinning
        | SceneShaderFamily::MeshColor
        | SceneShaderFamily::MeshColorPuppetSkinning
        | SceneShaderFamily::MeshText
        | SceneShaderFamily::MeshTextPuppetSkinning
        | SceneShaderFamily::MeshGenericParticle => "StandardMaterial",
        SceneShaderFamily::Effect => match effect_shader_name_for_key(spec.key) {
            "effects/iris" => "Iris",
            "effects/opacity" => "Opacity",
            "effects/waterwaves" => "WaterWaves",
            "effects/waterripple" | "workshop/2790231929/effects/waterripple" => "WaterRipple",
            _ => "None",
        },
        _ => "None",
    }
}

fn scene_shader_sources(spec: SceneShaderSpec) -> (String, String) {
    match spec.family {
        SceneShaderFamily::MeshGenericImage4 => {
            (scene_mesh_vertex_source(), genericimage4_fragment_source())
        }
        SceneShaderFamily::MeshGenericImage4PuppetSkinning => (
            scene_puppet_skinning_vertex_source(),
            genericimage4_fragment_source(),
        ),
        SceneShaderFamily::MeshColor => (scene_mesh_vertex_source(), color_fragment_source()),
        SceneShaderFamily::MeshColorPuppetSkinning => (
            scene_puppet_skinning_vertex_source(),
            color_fragment_source(),
        ),
        SceneShaderFamily::MeshText => (scene_mesh_vertex_source(), text_fragment_source()),
        SceneShaderFamily::MeshTextPuppetSkinning => (
            scene_puppet_skinning_vertex_source(),
            text_fragment_source(),
        ),
        SceneShaderFamily::MeshGenericParticle => (
            scene_mesh_vertex_source(),
            genericparticle_fragment_source(),
        ),
        SceneShaderFamily::MeshGenericImage4ClippingTarget => (
            scene_mesh_clippingtarget_vertex_source(),
            genericimage4_clippingtarget_fragment_source(),
        ),
        SceneShaderFamily::MeshGenericImage4ClippingTargetPuppetSkinning => (
            scene_puppet_skinning_clippingtarget_vertex_source(),
            genericimage4_clippingtarget_fragment_source(),
        ),
        SceneShaderFamily::MeshClippingMaskImage4 => (
            scene_mesh_vertex_source(),
            clippingmaskimage4_fragment_source(),
        ),
        SceneShaderFamily::MeshClippingMaskImage4PuppetSkinning => (
            scene_puppet_skinning_vertex_source(),
            clippingmaskimage4_fragment_source(),
        ),
        SceneShaderFamily::FlatMinimalAlpha => {
            (flattexture_vertex_source(), minimalalpha_fragment_source())
        }
        SceneShaderFamily::FlatPassthrough => {
            (flattexture_vertex_source(), passthrough_fragment_source())
        }
        SceneShaderFamily::Effect => {
            let shader = effect_shader_name_for_key(spec.key);
            let texture_slot_mask = effect_texture_slot_mask_for_key(spec.key);
            (
                effect_vertex_source(shader, texture_slot_mask),
                effect_fragment_source(shader, texture_slot_mask),
            )
        }
    }
}

fn compile_scene_shader_stage(shader_dir: &Path, key: &str, stage: &str, source: &str) -> PathBuf {
    let safe_name = key
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let source_path = shader_dir.join(format!("{safe_name}.{stage}.glsl"));
    let spirv_path = shader_dir.join(format!("{safe_name}.{stage}.spv"));
    fs::write(&source_path, source).expect("write build-time scene shader source");
    let output = Command::new("glslangValidator")
        .args(["-V", "--target-env", "vulkan1.3", "-S", stage, "-o"])
        .arg(&spirv_path)
        .arg(&source_path)
        .output()
        .expect("run glslangValidator for built-in scene shader");
    if !output.status.success() {
        panic!(
            "compile built-in scene shader {key} {stage} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let byte_len = fs::metadata(&spirv_path)
        .expect("stat build-time scene shader SPIR-V")
        .len();
    if byte_len < 4 || byte_len % 4 != 0 {
        panic!(
            "built-in scene shader {key} {stage} SPIR-V length {} is invalid",
            byte_len
        );
    }
    spirv_path
}

fn effect_shader_name_for_key(key: &str) -> &str {
    key.split("__").next().unwrap_or(key)
}

fn effect_texture_slot_mask_for_key(key: &str) -> u32 {
    for part in key.split("__") {
        if let Some(hex) = part.strip_prefix("SLOTS_") {
            return u32::from_str_radix(hex, 16).unwrap_or_else(|err| {
                panic!("invalid built-in scene shader SLOTS mask in {key}: {err}")
            });
        }
    }
    0
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
    v_ScreenPos = gl_Position.xyw;
    v_ScreenPos.y = -v_ScreenPos.y;
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
    v_ScreenPos = gl_Position.xyw;
    v_ScreenPos.y = -v_ScreenPos.y;
}
"#
    .to_owned()
}

fn genericimage4_fragment_source() -> String {
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
    o_Color = color;
}
"#
    .to_owned()
}

fn color_fragment_source() -> String {
    r#"#version 450
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 3) uniform ColorMaterial {
    vec4 g_Color4;
    vec4 g_Unused0;
    vec4 g_Unused1;
} g_Material;
void main() {
    vec4 color = g_Material.g_Color4;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn text_fragment_source() -> String {
    r#"#version 450
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 3) uniform TextMaterial {
    vec4 g_Color4;
    vec4 g_Unused0;
    vec4 g_Unused1;
} g_Material;
void main() {
    vec4 color = g_Material.g_Color4;
    color.a *= v_VertexAlpha;
    o_Color = color;
}
"#
    .to_owned()
}

fn genericparticle_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in float v_VertexAlpha;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform ParticleMaterial {
    vec4 g_Color4;
    vec4 g_Unused0;
    vec4 g_Unused1;
} g_Material;
void main() {
    vec4 color = texture(g_Texture0, v_TexCoord) * g_Material.g_Color4;
    color.a *= v_VertexAlpha;
    o_Color = color;
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
layout(location = 0) out float o_Mask;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 1) uniform sampler2D g_Texture1;
void main() {
    float albedo_alpha = texture(g_Texture0, v_TexCoord).a;
    float mask = texture(g_Texture1, v_TexCoord).r;
    float alpha = mix(pow(albedo_alpha, 4.0), albedo_alpha, mask) * v_VertexAlpha;
    o_Mask = mask * alpha;
}
"#
    .to_owned()
}

fn flattexture_vertex_source() -> String {
    r#"#version 450
layout(location = 0) in vec3 a_Position;
layout(location = 1) in vec2 a_TexCoord;
layout(location = 0) out vec2 v_TexCoord;
void main() {
    v_TexCoord = a_TexCoord;
    gl_Position = vec4(a_Position, 1.0);
}
"#
    .to_owned()
}

fn minimalalpha_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out float o_Alpha;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
void main() {
    o_Alpha = texture(g_Texture0, v_TexCoord).a;
}
"#
    .to_owned()
}

fn passthrough_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
void main() {
    o_Color = texture(g_Texture0, v_TexCoord);
}
"#
    .to_owned()
}

fn effect_vertex_source(shader: &str, texture_slot_mask: u32) -> String {
    if shader == "effects/iris" {
        return iris_effect_vertex_source(texture_slot_mask);
    }
    format!(
        r#"#version 450
layout(location = 0) out vec2 v_TexCoord;
void main() {{
    vec2 positions[3] = vec2[](
        vec2(-1.0, -1.0),
        vec2(3.0, -1.0),
        vec2(-1.0, 3.0)
    );
    vec2 position = positions[gl_VertexIndex];
    vec2 uv = position * 0.5 + 0.5;
    v_TexCoord = uv;
    gl_Position = vec4(position, 0.0, 1.0);
}}
"#
    )
}

fn effect_fragment_source(shader: &str, texture_slot_mask: u32) -> String {
    if shader == "effects/waterwaves" {
        return waterwaves_fragment_source(texture_slot_mask);
    }
    if shader == "effects/waterripple" || shader == "workshop/2790231929/effects/waterripple" {
        return waterripple_fragment_source(texture_slot_mask);
    }
    if shader == "effects/opacity" {
        return opacity_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/iris" {
        return iris_effect_fragment_source(texture_slot_mask);
    }
    let mut samplers = String::new();
    let mut first_slot = None;
    for slot in 0..32 {
        if texture_slot_mask & (1u32 << slot) != 0 {
            if first_slot.is_none() {
                first_slot = Some(slot);
            }
            samplers.push_str(&format!(
                "layout(set = 0, binding = {slot}) uniform sampler2D g_Texture{slot};\n"
            ));
        }
    }
    let sample = first_slot
        .map(|slot| format!("texture(g_Texture{slot}, v_TexCoord)"))
        .unwrap_or_else(|| "vec4(0.0)".to_owned());
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
{samplers}void main() {{
    vec4 color = {sample};
    o_Color = color;
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
            samplers.push_str(&format!(
                "layout(set = 0, binding = {slot}) uniform sampler2D g_Texture{slot};\n"
            ));
        }
    }
    samplers
}

fn waterwaves_fragment_source(texture_slot_mask: u32) -> String {
    let samplers = effect_sampler_declarations(texture_slot_mask);
    let mask_sample = if texture_slot_mask & (1 << 1) != 0 {
        "vec2 mask_uv = v_TexCoord * u_Effect.g_Texture1Resolution.zw / u_Effect.g_Texture1Resolution.xy;\n    float mask = texture(g_Texture1, mask_uv).r;"
    } else {
        "float mask = 1.0;"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
{samplers}layout(set = 0, binding = 3) uniform WaterWavesUniform {{
    vec4 g_TimeSpeedScaleStrength;
    vec4 g_DirectionSpeed2Scale2Direction2;
    vec4 g_Offset2DualMaskExponent;
    vec4 g_Texture1Resolution;
}} u_Effect;
vec2 rotateVec2(vec2 v, float r) {{
    vec2 cs = vec2(cos(r), sin(r));
    return vec2(v.x * cs.x - v.y * cs.y, v.x * cs.y + v.y * cs.x);
}}
void main() {{
    vec2 tex_coord = v_TexCoord;
    {mask_sample}
    vec2 direction = rotateVec2(vec2(0.0, -1.0), u_Effect.g_DirectionSpeed2Scale2Direction2.x);
    float distance0 = u_Effect.g_TimeSpeedScaleStrength.x * u_Effect.g_TimeSpeedScaleStrength.y
        + dot(tex_coord, direction) * u_Effect.g_TimeSpeedScaleStrength.z;
    vec2 offset = vec2(direction.y, -direction.x);
    float strength = u_Effect.g_TimeSpeedScaleStrength.w;
    tex_coord += sin(distance0) * offset * strength * strength * mask;
    if (u_Effect.g_Offset2DualMaskExponent.y > 0.5) {{
        vec2 direction2 = rotateVec2(vec2(0.0, -1.0), u_Effect.g_DirectionSpeed2Scale2Direction2.w);
        float distance1 = u_Effect.g_TimeSpeedScaleStrength.x * u_Effect.g_DirectionSpeed2Scale2Direction2.y
            + dot(tex_coord, direction2) * u_Effect.g_DirectionSpeed2Scale2Direction2.z
            + u_Effect.g_Offset2DualMaskExponent.x;
        vec2 offset2 = vec2(direction2.y, -direction2.x);
        tex_coord += sin(distance1) * offset2 * strength * strength * mask;
    }}
    o_Color = texture(g_Texture0, tex_coord);
}}
"#
    )
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
    float strength = u_Effect.g_DirectionStrengthAspectNormal.y;
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

fn build_ffmpeg_demux_shim() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let object = out_dir.join("demux_ffmpeg_shim.o");
    let archive = out_dir.join("libgilder_demux_ffmpeg_shim.a");
    let source = PathBuf::from("src/renderer/native_vulkan/video/demux_ffmpeg_shim.c");

    let pkg_config = Command::new("pkg-config")
        .args([
            "--cflags",
            "--libs",
            "libavformat",
            "libavcodec",
            "libavutil",
        ])
        .output()
        .expect("run pkg-config for FFmpeg");
    if !pkg_config.status.success() {
        panic!(
            "pkg-config libavformat/libavcodec/libavutil failed: {}",
            String::from_utf8_lossy(&pkg_config.stderr)
        );
    }
    let pkg_flags = String::from_utf8(pkg_config.stdout).expect("pkg-config output is UTF-8");
    let mut flags = pkg_flags.split_whitespace().collect::<Vec<_>>();

    let audio_cflags = Command::new("pkg-config")
        .args(["--cflags", "libpipewire-0.3", "libswresample"])
        .output()
        .expect("run pkg-config for audio cflags");
    if !audio_cflags.status.success() {
        panic!(
            "pkg-config libpipewire-0.3/libswresample --cflags failed: {}",
            String::from_utf8_lossy(&audio_cflags.stderr)
        );
    }
    let audio_cflags = String::from_utf8(audio_cflags.stdout).expect("pkg-config output is UTF-8");
    flags.extend(audio_cflags.split_whitespace());

    let mut cc = Command::new("cc");
    cc.args([
        "-std=c11",
        "-fPIC",
        "-O2",
        "-ffunction-sections",
        "-fdata-sections",
        "-c",
    ]);
    cc.args(
        flags.iter().copied().filter(|flag| {
            flag.starts_with("-I") || flag.starts_with("-D") || flag.starts_with("-f")
        }),
    );
    cc.arg(&source);
    cc.arg("-o");
    cc.arg(&object);
    let cc_output = cc.output().expect("compile FFmpeg demux shim");
    if !cc_output.status.success() {
        panic!(
            "compile FFmpeg demux shim failed: {}",
            String::from_utf8_lossy(&cc_output.stderr)
        );
    }

    let ar_output = Command::new("ar")
        .args(["crs"])
        .arg(&archive)
        .arg(&object)
        .output()
        .expect("archive FFmpeg demux shim");
    if !ar_output.status.success() {
        panic!(
            "archive FFmpeg demux shim failed: {}",
            String::from_utf8_lossy(&ar_output.stderr)
        );
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=gilder_demux_ffmpeg_shim");
    println!("cargo:rustc-link-lib=dl");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rustc-link-arg-bin=gilder-native-vulkan=-Wl,--gc-sections");
    println!("cargo:rustc-link-arg-bin=gilder-native-vulkan=-Wl,-z,pack-relative-relocs");
    for flag in flags {
        if let Some(lib) = flag.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={lib}");
        } else if let Some(path) = flag.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
        }
    }
}
