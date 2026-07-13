use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "build/native_video.rs"]
mod native_video;
#[path = "build/scene_shader.rs"]
mod scene_shader;
#[path = "build/system_audio_monitor.rs"]
mod system_audio_monitor;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/renderer/native_vulkan/video/demux_ffmpeg_shim.c");
    println!(
        "cargo:rerun-if-changed=src/renderer/native_vulkan/audio/system_monitor/pipewire_monitor.c"
    );

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }

    if env::var_os("CARGO_FEATURE_NATIVE_VULKAN_RENDERER").is_some() {
        build_scene_shader_catalog();
        system_audio_monitor::build_system_audio_monitor();
    }

    if env::var_os("CARGO_FEATURE_NATIVE_VULKAN_VIDEO").is_some() {
        native_video::build_ffmpeg_demux_shim();
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
    MeshComposelayer,
    MeshObjectComposite,
    MeshImageEffectSource,
    MeshImageEffectComposite,
    MeshFlatRoundedMaskComposite,
    MeshPuppetEffectSource,
    MeshPuppetEffectComposite,
    MeshImageWaterWavesComposite,
    MeshImageFoliageRippleComposite,
    MeshImageFoliageRippleScreenComposite,
    MeshImageRippleFlowComposite,
    MeshFinalEffect,
    MeshPuppetWaterWavesComposite,
    MeshUtilityComposite,
    EffectWaterWavesUvField,
    EffectImageRippleSource,
    FlatMinimalAlpha,
    FlatPassthrough,
    Effect,
}

const BUILTIN_SCENE_SHADER_SPECS: &[SceneShaderSpec] = &[
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_21__BLENDMODE_6",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_OVERLAY_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/cloudmotion__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/cloudmotion__SLOTS_5",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/colorkey__SLOTS_1",
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
        key: "effects/shake__SLOTS_3",
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
        key: "we/composelayer",
        family: SceneShaderFamily::MeshComposelayer,
    },
    SceneShaderSpec {
        key: "we/flat",
        family: SceneShaderFamily::MeshColor,
    },
    SceneShaderSpec {
        key: "we/objectcomposite",
        family: SceneShaderFamily::MeshObjectComposite,
    },
    SceneShaderSpec {
        key: "we/image-effect-source",
        family: SceneShaderFamily::MeshImageEffectSource,
    },
    SceneShaderSpec {
        key: "we/image-effect-composite",
        family: SceneShaderFamily::MeshImageEffectComposite,
    },
    SceneShaderSpec {
        key: "we/flat-rounded-mask-composite",
        family: SceneShaderFamily::MeshFlatRoundedMaskComposite,
    },
    SceneShaderSpec {
        key: "we/puppet-effect-source",
        family: SceneShaderFamily::MeshPuppetEffectSource,
    },
    SceneShaderSpec {
        key: "we/puppet-effect-composite",
        family: SceneShaderFamily::MeshPuppetEffectComposite,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-composite",
        family: SceneShaderFamily::MeshImageWaterWavesComposite,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-composite",
        family: SceneShaderFamily::MeshImageWaterWavesComposite,
    },
    SceneShaderSpec {
        key: "we/image-foliage-ripple-composite",
        family: SceneShaderFamily::MeshImageFoliageRippleComposite,
    },
    SceneShaderSpec {
        key: "we/image-foliage-ripple-screen-composite",
        family: SceneShaderFamily::MeshImageFoliageRippleScreenComposite,
    },
    SceneShaderSpec {
        key: "we/image-ripple-flow-composite",
        family: SceneShaderFamily::MeshImageRippleFlowComposite,
    },
    SceneShaderSpec {
        key: "we/image-ripple-flow-multiply-composite",
        family: SceneShaderFamily::MeshImageRippleFlowComposite,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-composite",
        family: SceneShaderFamily::MeshPuppetWaterWavesComposite,
    },
    SceneShaderSpec {
        key: "we/utilitycomposite",
        family: SceneShaderFamily::MeshUtilityComposite,
    },
    SceneShaderSpec {
        key: "we/waterwaves-uv-field",
        family: SceneShaderFamily::EffectWaterWavesUvField,
    },
    SceneShaderSpec {
        key: "we/image-ripple-source",
        family: SceneShaderFamily::EffectImageRippleSource,
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
        key: "we/genericimage4-multiply-composite",
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
        key: "workshop/2790231929/effects/foliagesway__SLOTS_3",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2790231929/effects/foliagesway__SLOTS_5",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2790231929/effects/foliagesway__SLOTS_7",
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
        key: "workshop/3083593512/effects/rounded_mask__SLOTS_1__SOFT_1",
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
    for spec in BUILTIN_SCENE_SHADER_SPECS
        .iter()
        .chain(scene_shader::FINAL_EFFECT_SHADER_SPECS)
    {
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
        | SceneShaderFamily::MeshGenericParticle
        | SceneShaderFamily::MeshObjectComposite
        | SceneShaderFamily::MeshImageEffectComposite
        | SceneShaderFamily::MeshPuppetEffectComposite
        | SceneShaderFamily::MeshImageWaterWavesComposite
        | SceneShaderFamily::MeshPuppetWaterWavesComposite => "StandardMaterial",
        SceneShaderFamily::MeshImageFoliageRippleComposite
        | SceneShaderFamily::MeshImageFoliageRippleScreenComposite => "FoliageRippleComposite",
        SceneShaderFamily::MeshImageRippleFlowComposite => "RippleFlowComposite",
        SceneShaderFamily::MeshFinalEffect => scene_shader::final_effect_parameter_layout(spec.key),
        SceneShaderFamily::MeshFlatRoundedMaskComposite => "RoundedMask",
        SceneShaderFamily::EffectWaterWavesUvField => "WaterWavesUvField",
        SceneShaderFamily::EffectImageRippleSource => "WaterRipple",
        SceneShaderFamily::Effect => match effect_shader_name_for_key(spec.key) {
            "effects/caustics" => "Caustics",
            "effects/cloudmotion" => "CloudMotion",
            "effects/colorkey" => "ColorKey",
            "effects/iris" => "Iris",
            "effects/opacity" => "Opacity",
            "effects/scroll" => "Scroll",
            "effects/skew" => "Skew",
            "workshop/3083593512/effects/rounded_mask" => "RoundedMask",
            "effects/shake" => "Shake",
            "workshop/2790231929/effects/foliagesway" => "FoliageSway",
            "workshop/2123274886/effects/tech_circle" => "TechCircle",
            "workshop/3082978660/effects/Simple_Audio_Bars" => "AudioBars",
            "effects/waterwaves" => "WaterWaves",
            "effects/waterripple" | "workshop/2790231929/effects/waterripple" => "WaterRipple",
            "effects/waterflow" => "WaterFlow",
            _ => "None",
        },
        _ => "None",
    }
}

fn scene_shader_sources(spec: SceneShaderSpec) -> (String, String) {
    match spec.family {
        SceneShaderFamily::MeshGenericImage4 => {
            let fragment = if spec.key == "we/genericimage4-multiply-composite" {
                scene_shader::generic_image_multiply_fragment_source()
            } else {
                scene_shader::generic_image_fragment_source()
            };
            (scene_mesh_vertex_source(), fragment)
        }
        SceneShaderFamily::MeshGenericImage4PuppetSkinning => (
            scene_puppet_skinning_vertex_source(),
            scene_shader::generic_image_fragment_source(),
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
        SceneShaderFamily::MeshComposelayer => {
            (composelayer_vertex_source(), composelayer_fragment_source())
        }
        SceneShaderFamily::MeshObjectComposite => scene_shader::object_composite_sources(),
        SceneShaderFamily::MeshImageEffectSource => scene_shader::image_effect_source_sources(),
        SceneShaderFamily::MeshImageEffectComposite => {
            scene_shader::image_effect_composite_sources()
        }
        SceneShaderFamily::MeshFlatRoundedMaskComposite => {
            scene_shader::flat_rounded_mask_composite_sources()
        }
        SceneShaderFamily::MeshPuppetEffectSource => scene_shader::puppet_effect_source_sources(),
        SceneShaderFamily::MeshPuppetEffectComposite => {
            scene_shader::puppet_effect_composite_sources()
        }
        SceneShaderFamily::MeshImageWaterWavesComposite => {
            if spec.key == "we/image-waterwaves-multiply-composite" {
                scene_shader::image_waterwaves_multiply_composite_sources()
            } else {
                scene_shader::image_waterwaves_composite_sources()
            }
        }
        SceneShaderFamily::MeshImageFoliageRippleComposite => {
            scene_shader::image_foliage_ripple_composite_sources()
        }
        SceneShaderFamily::MeshImageFoliageRippleScreenComposite => {
            scene_shader::image_foliage_ripple_screen_composite_sources()
        }
        SceneShaderFamily::MeshImageRippleFlowComposite => {
            if spec.key == "we/image-ripple-flow-multiply-composite" {
                scene_shader::image_ripple_flow_multiply_composite_sources()
            } else {
                scene_shader::image_ripple_flow_composite_sources()
            }
        }
        SceneShaderFamily::MeshFinalEffect => scene_shader::final_effect_sources(spec.key),
        SceneShaderFamily::MeshPuppetWaterWavesComposite => {
            scene_shader::puppet_waterwaves_composite_sources()
        }
        SceneShaderFamily::MeshUtilityComposite => {
            (scene_mesh_vertex_source(), passthrough_fragment_source())
        }
        SceneShaderFamily::EffectWaterWavesUvField => scene_shader::waterwaves_uv_field_sources(),
        SceneShaderFamily::EffectImageRippleSource => scene_shader::image_ripple_source_sources(),
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
                effect_fragment_source(spec.key, shader, texture_slot_mask),
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
    if shader == "effects/waterwaves" || shader == "effects/waterflow" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "effects/scroll" || shader == "effects/skew" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "workshop/2123274886/effects/tech_circle" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "workshop/3082978660/effects/Simple_Audio_Bars" {
        return waterwaves_effect_vertex_source();
    }
    if shader == "workshop/3083593512/effects/rounded_mask" {
        return object_local_effect_vertex_source();
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

fn effect_fragment_source(key: &str, shader: &str, texture_slot_mask: u32) -> String {
    if shader == "effects/caustics" {
        return caustics_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/cloudmotion" {
        return cloudmotion_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/colorkey" {
        return colorkey_effect_fragment_source(texture_slot_mask);
    }
    if shader == "workshop/2790231929/effects/foliagesway" {
        return foliage_sway_fragment_source(texture_slot_mask);
    }
    if shader == "effects/waterwaves" {
        return waterwaves_fragment_source(texture_slot_mask);
    }
    if shader == "effects/waterflow" {
        return waterflow_fragment_source();
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
    if shader == "effects/shake" {
        return shake_effect_fragment_source(texture_slot_mask);
    }
    if shader == "effects/scroll" {
        return scroll_effect_fragment_source();
    }
    if shader == "effects/skew" {
        return skew_effect_fragment_source(key);
    }
    if shader == "workshop/2123274886/effects/tech_circle" {
        return tech_circle_fragment_source(key);
    }
    if shader == "workshop/3082978660/effects/Simple_Audio_Bars" {
        return audio_bars_fragment_source(key);
    }
    if shader == "workshop/3083593512/effects/rounded_mask" {
        return rounded_mask_fragment_source(key);
    }
    let mut samplers = String::new();
    let mut first_slot = None;
    for slot in 0..32 {
        if texture_slot_mask & (1u32 << slot) != 0 {
            if first_slot.is_none() {
                first_slot = Some(slot);
            }
            let binding = scene_texture_shader_binding(slot);
            samplers.push_str(&format!(
                "layout(set = 0, binding = {binding}) uniform sampler2D g_Texture{slot};\n"
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

fn audio_bars_fragment_source(key: &str) -> String {
    let shape = effect_combo_value_for_key(key, "SHAPE", 0);
    assert_eq!(shape, 7, "current audio-bars variant must be CENTER_V");
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform AudioBarsUniform {
    vec4 g_ColorOpacity;
    vec4 g_CountSpacingBounds;
    vec4 g_MinHeightRadiusVolumeAaX;
    vec4 g_AaY;
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
float roundedBoxSdf(vec2 point, vec2 half_size, float radius) {
    float r = clamp(radius, 0.0, min(half_size.x, half_size.y));
    vec2 delta = abs(point) - (half_size - r);
    return length(max(delta, 0.0)) - r;
}
void main() {
    if (any(lessThan(v_ObjectTexCoord, vec2(0.0)))
        || any(greaterThan(v_ObjectTexCoord, vec2(1.0)))) {
        o_Color = vec4(0.0);
        return;
    }
    float count = max(u_Effect.g_CountSpacingBounds.x, 1.0);
    float cell_width = 1.0 / count;
    float bar_width = cell_width
        * clamp(1.0 - u_Effect.g_CountSpacingBounds.y, 0.0, 1.0);
    float minimum_height = u_Effect.g_MinHeightRadiusVolumeAaX.x * bar_width;
    float lower_bound = u_Effect.g_CountSpacingBounds.z;
    float upper_bound = u_Effect.g_CountSpacingBounds.w;
    float frequency = floor(v_ObjectTexCoord.x * count) / count * 32.0;
    int frequency0 = int(mod(frequency, 32.0));
    int frequency1 = (frequency0 + 1) % 32;
    float frequency_blend = smoothstep(0.0, 1.0, fract(frequency));
    float left = mix(spectrumLeft(frequency0), spectrumLeft(frequency1), frequency_blend);
    float right = mix(spectrumRight(frequency0), spectrumRight(frequency1), frequency_blend);
    float volume = mix(left, right, step(0.5, v_ObjectTexCoord.y))
        * u_Effect.g_MinHeightRadiusVolumeAaX.z;
    float half_height = 0.5 * mix(
        max(lower_bound, minimum_height) * 2.0,
        upper_bound,
        volume);
    vec2 gradient_x = vec2(dFdx(v_ObjectTexCoord.x), dFdy(v_ObjectTexCoord.x));
    vec2 gradient_y = vec2(dFdx(v_ObjectTexCoord.y), dFdy(v_ObjectTexCoord.y));
    float width_pixels = 1.0 / max(length(gradient_x), 0.000001);
    float height_pixels = 1.0 / max(length(gradient_y), 0.000001);
    float x_correction = width_pixels / max(height_pixels, 0.000001);
    float cell_position = fract(v_ObjectTexCoord.x * count) - 0.5;
    vec2 point = vec2(
        cell_position * cell_width * x_correction,
        v_ObjectTexCoord.y - 0.5);
    vec2 half_size = vec2(bar_width * 0.5 * x_correction, half_height);
    float radius = u_Effect.g_MinHeightRadiusVolumeAaX.y
        * min(half_size.x, half_size.y);
    float distance = roundedBoxSdf(point, half_size, radius);
    float authored_aa = max(
        u_Effect.g_MinHeightRadiusVolumeAaX.w,
        u_Effect.g_AaY.x) * 15.0
        / float(max(textureSize(g_Texture0, 0).x, textureSize(g_Texture0, 0).y));
    float antialias = max(fwidth(distance), authored_aa);
    float bar = 1.0 - smoothstep(-antialias, antialias, distance);
    vec4 scene = texture(g_Texture0, v_TexCoord);
    float opacity = bar * u_Effect.g_ColorOpacity.a;
    vec3 base = mix(u_Effect.g_ColorOpacity.rgb, scene.rgb, scene.a);
    vec3 final_color = mix(base, u_Effect.g_ColorOpacity.rgb, opacity);
    o_Color = vec4(final_color, opacity);
}
"#
    .to_owned()
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

fn rounded_mask_fragment_source(key: &str) -> String {
    let square = effect_combo_value_for_key(key, "B_SQUARE", 1) != 0;
    let alpha_only = effect_combo_value_for_key(key, "C_ALPHA_ONLY", 1) != 0;
    let soft = effect_combo_value_for_key(key, "SOFT", 0) != 0;
    let size_expression = if square {
        "u_Effect.g_SizeSoftnessAlpha.xy"
    } else {
        "u_Effect.g_SizeSoftnessAlpha.xy * aspect_scale"
    };
    let edge_expression = if soft {
        "float edge_softness = u_Effect.g_SizeSoftnessAlpha.z\n        / max(v_ObjectPixelExtent.z, 1.0) * 2.0;\n    float mask_alpha = smoothstep(edge_softness, 0.0, distance);"
    } else {
        "float mask_alpha = 1.0 - step(0.0, distance);"
    };
    let output_expression = if alpha_only {
        "o_Color = vec4(source.rgb, source.a * mask_alpha * u_Effect.g_SizeSoftnessAlpha.w);"
    } else {
        "float alpha = source.a * mask_alpha * u_Effect.g_SizeSoftnessAlpha.w;\n    vec3 tint = u_Effect.g_ColorRadius.rgb;\n    vec3 blended = mix(tint, source.rgb, source.a);\n    o_Color = vec4(mix(tint, blended, alpha), mask_alpha);"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 1) in vec2 v_ObjectTexCoord;
layout(location = 2) flat in vec3 v_ObjectPixelExtent;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform RoundedMaskUniform {{
    vec4 g_ColorRadius;
    vec4 g_SizeSoftnessAlpha;
    vec4 g_BorderWidth;
    vec4 g_Unused;
}} u_Effect;
float roundedBoxSdf(vec2 point, vec2 size, float radius) {{
    vec2 half_size = size * 0.5;
    float half_min = min(half_size.x, half_size.y);
    float r = clamp(radius * half_min, 0.001, half_min);
    vec2 delta = abs(point) - (half_size - r);
    return length(max(delta, 0.0)) - r;
}}
void main() {{
    vec4 source = texture(g_Texture0, v_TexCoord);
    float width_pixels = max(v_ObjectPixelExtent.x, 1.0);
    float height_pixels = max(v_ObjectPixelExtent.y, 1.0);
    vec2 aspect_scale = vec2(
        max(1.0, width_pixels / height_pixels),
        max(1.0, height_pixels / width_pixels));
    vec2 mask_uv = (v_ObjectTexCoord - 0.5) * aspect_scale + 0.5;
    vec2 mask_size = {size_expression};
    float distance = roundedBoxSdf(
        mask_uv - vec2(0.5),
        mask_size,
        u_Effect.g_ColorRadius.w);
    {edge_expression}
    {output_expression}
}}
"#
    )
}

fn effect_combo_value_for_key(key: &str, combo: &str, default: i64) -> i64 {
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

fn caustics_effect_fragment_source(texture_slot_mask: u32) -> String {
    if texture_slot_mask & 0x3d != 0x3d {
        return caustics_compatibility_fragment_source();
    }
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 2) uniform sampler2D g_Texture2;
// Logical WE texture slot 3 uses binding 35 because binding 3 is the
// fragment material uniform ABI.
layout(set = 0, binding = 35) uniform sampler2D g_Texture3;
layout(set = 0, binding = 4) uniform sampler2D g_Texture4;
layout(set = 0, binding = 5) uniform sampler2D g_Texture5;
layout(set = 0, binding = 3) uniform CausticsUniform {
    vec4 g_TimeSpeedScaleBrightness;
    vec4 g_GlowDistortionChromaticBlur;
    vec4 g_ColorStart;
    vec4 g_ColorEnd;
} u_Effect;
void main() {
    vec4 albedo = texture(g_Texture0, v_TexCoord);
    float ratio = max(u_Effect.g_ColorEnd.w, 0.0001);
    vec2 causticsCoords = v_TexCoord;
    causticsCoords.x *= ratio;
    causticsCoords *= u_Effect.g_TimeSpeedScaleBrightness.z;

    vec2 noiseCoords = causticsCoords * 0.02;
    vec2 noiseCoords2 = causticsCoords * 0.0333;
    vec2 blendCoords = causticsCoords * 0.01333;
    vec2 shiftCoords = causticsCoords * 0.05;
    float time = u_Effect.g_TimeSpeedScaleBrightness.x
        * u_Effect.g_TimeSpeedScaleBrightness.y
        + u_Effect.g_ColorStart.w;
    noiseCoords.x += time * 0.005;
    noiseCoords2.y += time * 0.004111;
    blendCoords += time * 0.003777;
    shiftCoords += time * 0.01;

    vec4 shiftColor = texture(g_Texture4, shiftCoords) * 2.0 - 1.0;
    vec4 noiseColor = texture(g_Texture3, noiseCoords) * 2.0 - 1.0;
    vec4 noiseColor2 = texture(g_Texture3, noiseCoords2) * 2.0 - 1.0;
    float distortion = u_Effect.g_GlowDistortionChromaticBlur.y;
    causticsCoords += noiseColor.xy * 0.025 * distortion;
    causticsCoords += noiseColor2.xy * 0.025 * distortion;
    causticsCoords += shiftColor.rg * distortion;

    float chromatic = u_Effect.g_GlowDistortionChromaticBlur.z;
    vec2 leftCoords = causticsCoords - vec2(0.01 * chromatic, 0.0);
    vec2 rightCoords = causticsCoords + vec2(0.01 * chromatic, 0.0);
    vec3 caustics = vec3(
        texture(g_Texture2, leftCoords).r,
        texture(g_Texture2, causticsCoords).r,
        texture(g_Texture2, rightCoords).r);
    float glowSample = texture(g_Texture5, causticsCoords).r;
    vec4 blendColor = texture(g_Texture3, blendCoords);
    caustics = mix(
        caustics,
        vec3(glowSample),
        u_Effect.g_GlowDistortionChromaticBlur.w);
    float causticsSample = dot(caustics, vec3(0.33333));
    causticsSample = smoothstep(
        blendColor.x * 0.8,
        1.0 - blendColor.y * 0.2,
        causticsSample
            + glowSample * u_Effect.g_GlowDistortionChromaticBlur.x);
    vec3 causticsColor = u_Effect.g_TimeSpeedScaleBrightness.w
        * mix(u_Effect.g_ColorStart.rgb, u_Effect.g_ColorEnd.rgb, blendColor.x);
    causticsColor *= caustics;

    vec3 lightened = max(albedo.rgb, causticsColor);
    albedo.rgb = mix(albedo.rgb, lightened, clamp(causticsSample, 0.0, 1.0));
    o_Color = albedo;
}
"#
    .to_owned()
}

fn caustics_compatibility_fragment_source() -> String {
    r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
layout(set = 0, binding = 3) uniform CausticsUniform {
    vec4 g_TimeSpeedScaleBrightness;
    vec4 g_GlowDistortionChromaticBlur;
    vec4 g_ColorStart;
    vec4 g_ColorEnd;
} u_Effect;
void main() {
    o_Color = texture(g_Texture0, v_TexCoord);
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
        "texture(g_Texture2, noiseUv).x"
    } else {
        "valueNoise(noiseUv)"
    };
    format!(
        r#"#version 450
layout(location = 0) in vec2 v_TexCoord;
layout(location = 0) out vec4 o_Color;
layout(set = 0, binding = 0) uniform sampler2D g_Texture0;
{noise_sampler}
layout(set = 0, binding = 3) uniform CloudMotionUniform {{
    vec4 g_TimeSpeedAmountDirection;
    vec4 g_ScaleScaleXUnused;
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
    float time = u_Effect.g_TimeSpeedAmountDirection.x
        * u_Effect.g_TimeSpeedAmountDirection.y;
    vec2 noiseUv = v_TexCoord;
    noiseUv.x *= max(u_Effect.g_ScaleScaleXUnused.z, 0.0001);
    noiseUv *= max(u_Effect.g_ScaleScaleXUnused.x, 0.1);
    noiseUv.x *= max(u_Effect.g_ScaleScaleXUnused.y, 0.1);
    noiseUv.x += time;
    float noise = {noise_sample} * 2.0 - 1.0;
    float angle = u_Effect.g_TimeSpeedAmountDirection.w + 1.5707963;
    vec2 direction = vec2(cos(angle), sin(angle));
    vec2 offset = direction * noise * u_Effect.g_TimeSpeedAmountDirection.z;
    o_Color = texture(
        g_Texture0,
        clamp(v_TexCoord + offset, vec2(0.001), vec2(0.999)));
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
    float time_phase = u_Effect.g_TimeSpeedFeatherStrength.x
        * u_Effect.g_TimeSpeedFeatherStrength.y;
    vec4 cycles = fract(time_phase + vec4(0.0, 0.5, 0.25, 0.75)) - 0.5;
    float feather = u_Effect.g_TimeSpeedFeatherStrength.z;
    vec2 smooth_range = vec2(0.5 - feather, 0.5 + feather);
    vec2 blend_weight = smoothstep(smooth_range.x, smooth_range.y,
        2.0 * abs(vec2(cycles.x, cycles.z)));
    vec2 flow_uv = v_ObjectTexCoord
        * u_Effect.g_Texture1Resolution.zw / u_Effect.g_Texture1Resolution.xy;
    vec2 flow = (texture(g_Texture1, flow_uv).rg - vec2(0.498)) * 2.0;
    float strength = u_Effect.g_TimeSpeedFeatherStrength.w * 0.1;
    vec4 offset0 = flow.xyxy * strength * cycles.xxyy;
    vec4 offset1 = flow.xyxy * strength * cycles.zzww;
    vec4 first = mix(sourceAtObjectUv(v_ObjectTexCoord + offset0.xy),
        sourceAtObjectUv(v_ObjectTexCoord + offset0.zw), blend_weight.x);
    vec4 second = mix(sourceAtObjectUv(v_ObjectTexCoord + offset1.xy),
        sourceAtObjectUv(v_ObjectTexCoord + offset1.zw), blend_weight.y);
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
