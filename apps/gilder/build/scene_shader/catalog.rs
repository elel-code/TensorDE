use super::super::*;
use std::collections::BTreeSet;

#[path = "catalog/key.rs"]
mod key;
#[path = "catalog/specs.rs"]
mod specs;

use key::{effect_shader_name_for_key, effect_texture_slot_mask_for_key};
use vulkan_renderer_build::{ShaderCompileRequest, ShaderContract, ShaderStage, SlangCompiler};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SceneShaderSpec {
    pub(crate) key: &'static str,
    pub(crate) family: SceneShaderFamily,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SceneShaderFamily {
    MeshGenericImage4,
    MeshDynamicText,
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
    MeshWaterWavesDirect,
    MeshUtilityComposite,
    EffectWaterWavesUvField,
    EffectImageRippleSource,
    FlatMinimalAlpha,
    FlatPassthrough,
    Effect,
}

use specs::BUILTIN_SCENE_SHADER_SPECS;

pub(crate) fn build_scene_shader_origin_catalog() {
    let programs = BUILTIN_SCENE_SHADER_SPECS
        .iter()
        .chain(super::FINAL_EFFECT_SHADER_SPECS)
        .map(|spec| spec.key.split_once("__").map_or(spec.key, |(key, _)| key))
        .filter(|key| key.starts_with("effects/"))
        .collect::<BTreeSet<_>>();
    let patterns = programs
        .into_iter()
        .map(|program| format!("        {program:?}"))
        .collect::<Vec<_>>()
        .join(" |\n");
    let generated = format!(
        "pub(super) fn is_engine_builtin_effect_program(program: &str) -> bool {{\n    matches!(program,\n{patterns}\n    )\n}}\n"
    );
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out_dir.join("gilder_scene_shader_origins.rs"), generated)
        .expect("write built-in scene shader origin catalog");
}

pub(crate) fn build_scene_shader_catalog() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let shader_dir = out_dir.join("scene_shader_catalog");
    fs::create_dir_all(&shader_dir).expect("create scene shader catalog build dir");
    let mut generated = String::new();
    generated.push_str(super::input_attachment_catalog_type_source());
    generated.push_str("#[derive(Debug, Clone, Copy)]\n");
    generated.push_str("pub struct BuiltinSceneShader {\n");
    generated.push_str("    pub key: &'static str,\n");
    generated.push_str(
        "    pub vertex_primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive,\n",
    );
    generated.push_str("    pub vertex_spirv: &'static [u32],\n");
    generated.push_str("    pub object_mesh_vertex_spirv: Option<&'static [u32]>,\n");
    generated.push_str("    pub fragment_spirv: &'static [u32],\n");
    generated.push_str("    #[cfg(test)]\n    pub fragment_source: &'static str,\n");
    generated.push_str("    pub fragment_descriptor_heap_mode: BuiltinSceneDescriptorHeapMode,\n");
    generated.push_str("    pub local_read_shader: Option<BuiltinSceneLocalReadShader>,\n    pub fragment_coordinate_fetch_slot_mask: u32,\n");
    generated.push_str("    pub parameter_layout: BuiltinSceneParameterLayout,\n");
    generated.push_str("}\n\n");
    let mut entries = String::new();
    for spec in BUILTIN_SCENE_SHADER_SPECS
        .iter()
        .chain(super::FINAL_EFFECT_SHADER_SPECS)
    {
        let (vertex_source, fragment_source) = scene_shader_sources(*spec);
        let vertex_path = compile_scene_shader_stage(&shader_dir, spec.key, "vert", &vertex_source);
        let object_mesh_vertex_path = match spec.family {
            SceneShaderFamily::Effect => {
                let shader = effect_shader_name_for_key(spec.key);
                let texture_slot_mask = effect_texture_slot_mask_for_key(spec.key);
                super::effect_object_mesh_vertex_source(spec.key, shader, texture_slot_mask).map(
                    |source| {
                        compile_scene_shader_stage(
                            &shader_dir,
                            &format!("{}__OBJECT_MESH", spec.key),
                            "vert",
                            &source,
                        )
                    },
                )
            }
            _ => None,
        };
        let fragment_path =
            compile_scene_shader_stage(&shader_dir, spec.key, "frag", &fragment_source);
        let fragment_source_path =
            fragment_path.with_extension(scene_shader_source_extension(spec.key, "frag"));
        let input_attachment_fragment_source = super::input_attachment_fragment_source(matches!(
            spec.family,
            SceneShaderFamily::FlatPassthrough
        ));
        let input_attachment_fragment_path =
            input_attachment_fragment_source.as_ref().map(|source| {
                compile_scene_shader_stage(
                    &shader_dir,
                    &format!("{}__INPUT_ATTACHMENT", spec.key),
                    "frag",
                    source.source(),
                )
            });
        let vertex_path = vertex_path
            .to_str()
            .expect("built-in scene vertex shader path must be UTF-8");
        let fragment_path = fragment_path
            .to_str()
            .expect("built-in scene fragment shader path must be UTF-8");
        let fragment_source_path = fragment_source_path
            .to_str()
            .expect("built-in scene fragment source path must be UTF-8");
        let local_read_shader = input_attachment_fragment_source
            .as_ref()
            .zip(input_attachment_fragment_path.as_ref())
            .map_or_else(
                || "None".to_owned(),
                |(source, path)| source.catalog_expression(path),
            );
        let object_mesh_vertex_spirv = object_mesh_vertex_path.as_ref().map_or_else(
            || "None".to_owned(),
            |path| {
                format!(
                    "Some(vulkan_renderer::include_spirv!({:?}))",
                    path.to_str()
                        .expect("built-in object-mesh vertex shader path must be UTF-8")
                )
            },
        );
        let vertex_primitive = super::scene_shader_vertex_primitive(*spec);
        let parameter_layout = scene_shader_parameter_layout(*spec);
        let fragment_descriptor_heap_mode = scene_fragment_descriptor_heap_mode(spec.key);
        let fragment_coordinate_fetch_slot_mask =
            u32::from(spec.key == "we/flat-rounded-hsl-source");
        entries.push_str(&format!(
            "    BuiltinSceneShader {{ key: {:?}, vertex_primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive::{vertex_primitive}, vertex_spirv: vulkan_renderer::include_spirv!({:?}), object_mesh_vertex_spirv: {object_mesh_vertex_spirv}, fragment_spirv: vulkan_renderer::include_spirv!({:?}), #[cfg(test)] fragment_source: include_str!({:?}), fragment_descriptor_heap_mode: BuiltinSceneDescriptorHeapMode::{fragment_descriptor_heap_mode}, local_read_shader: {local_read_shader}, fragment_coordinate_fetch_slot_mask: {fragment_coordinate_fetch_slot_mask}, parameter_layout: BuiltinSceneParameterLayout::{parameter_layout} }},\n",
            spec.key,
            vertex_path,
            fragment_path,
            fragment_source_path,
        ));
    }
    generated.push_str("pub static BUILTIN_SCENE_SHADERS: &[BuiltinSceneShader] = &[\n");
    generated.push_str(&entries);
    generated.push_str("];\n");

    let compute_path = compile_scene_shader_stage(
        &shader_dir,
        "particle_compute",
        "comp",
        &super::particle_compute_source(),
    );
    let compute_path = compute_path
        .to_str()
        .expect("particle compute shader path must be UTF-8");
    generated.push_str("\n#[derive(Debug, Clone, Copy)]\n");
    generated.push_str("pub struct BuiltinParticleComputeShader {\n");
    generated.push_str("    pub spirv: &'static [u32],\n");
    generated.push_str("}\n\n");
    generated.push_str(&format!(
        "pub static BUILTIN_PARTICLE_COMPUTE_SHADER: BuiltinParticleComputeShader = BuiltinParticleComputeShader {{ spirv: vulkan_renderer::include_spirv!({:?}) }};\n",
        compute_path
    ));

    fs::write(out_dir.join("gilder_scene_shader_catalog.rs"), generated)
        .expect("write built-in scene shader catalog");
}

fn scene_shader_parameter_layout(spec: SceneShaderSpec) -> &'static str {
    match spec.family {
        SceneShaderFamily::MeshGenericImage4
        | SceneShaderFamily::MeshDynamicText
        | SceneShaderFamily::MeshGenericImage4PuppetSkinning
        | SceneShaderFamily::MeshColor
        | SceneShaderFamily::MeshColorPuppetSkinning
        | SceneShaderFamily::MeshText
        | SceneShaderFamily::MeshTextPuppetSkinning
        | SceneShaderFamily::MeshObjectComposite
        | SceneShaderFamily::MeshImageEffectComposite
        | SceneShaderFamily::MeshPuppetEffectComposite
        | SceneShaderFamily::MeshImageWaterWavesComposite
        | SceneShaderFamily::MeshPuppetWaterWavesComposite => "StandardMaterial",
        SceneShaderFamily::MeshGenericParticle => "Particle",
        SceneShaderFamily::MeshWaterWavesDirect => "WaterWavesDirect",
        SceneShaderFamily::MeshImageFoliageRippleComposite
        | SceneShaderFamily::MeshImageFoliageRippleScreenComposite => "FoliageRippleComposite",
        SceneShaderFamily::MeshImageRippleFlowComposite => "RippleFlowComposite",
        SceneShaderFamily::MeshFinalEffect => super::final_effect_parameter_layout(spec.key),
        SceneShaderFamily::MeshFlatRoundedMaskComposite => "RoundedMask",
        SceneShaderFamily::EffectWaterWavesUvField => "WaterWavesUvField",
        SceneShaderFamily::EffectImageRippleSource => "WaterRipple",
        SceneShaderFamily::Effect => match effect_shader_name_for_key(spec.key) {
            "effects/111" => "Lightning",
            "effects/audioline" => "AudioLine",
            "effects/auto_sway" => "AutoSway",
            "effects/blend" => "Blend",
            "effects/blendgradient" => "BlendGradient",
            "effects/blur_combine" => "BlurCombine",
            "effects/blur_downsample4" => "None",
            "effects/blur_gaussian" => "BlurGaussian",
            "effects/lut_loader" => "Lut",
            "effects/raindrop_on_glass" => "Raindrop",
            "effects/audio_responsive_oscilloscope" => "Oscilloscope",
            "effects/caustics" => "Caustics",
            "effects/cloudmotion" => "CloudMotion",
            "effects/clipping_mask" => "ClippingMask",
            "effects/colorkey" => "ColorKey",
            "effects/custom_user_texture" => "CustomUserTexture",
            "effects/gradient_color" => "GradientColor",
            "effects/huan" => "Ring",
            "effects/iris" => "Iris",
            "effects/opacity" => "Opacity",
            "effects/user_texture_alpha_overwrite_workaround" => "Opacity",
            "effects/procedural_noise" => "ProceduralNoise",
            "effects/scroll" => "Scroll",
            "effects/skew" => "Skew",
            "effects/rounded_mask" => "RoundedMask",
            "effects/rounded_mask_effect_edit" => "RoundedMask",
            "effects/qiu" => "Sphere",
            "effects/tint" => "Tint",
            "effects/spin" => "Spin",
            "effects/shake" => "Shake",
            "effects/shimmer" => "Shimmer",
            "effects/swing" => "Swing",
            "effects/foliagesway" => "FoliageSway",
            "effects/tech_circle" => "TechCircle",
            "effects/simple_audio_bars" => "AudioBars",
            "effects/waterwaves" => "WaterWaves",
            "effects/waterripple" => "WaterRipple",
            "effects/waterflow" => "WaterFlow",
            _ => "None",
        },
        _ => "None",
    }
}

fn scene_fragment_descriptor_heap_mode(key: &str) -> &'static str {
    if key == "effects/audioline__SLOTS_1" {
        "Native"
    } else {
        "Mapped"
    }
}

fn scene_shader_sources(spec: SceneShaderSpec) -> (String, String) {
    match spec.family {
        SceneShaderFamily::MeshGenericImage4 => {
            let fragment = if spec.key == "we/genericimage4-multiply-composite" {
                super::generic_image_multiply_fragment_source()
            } else {
                super::generic_image_fragment_source()
            };
            (scene_mesh_vertex_source(), fragment)
        }
        SceneShaderFamily::MeshDynamicText => (
            super::dynamic_text_vertex_source(),
            super::generic_image_fragment_source(),
        ),
        SceneShaderFamily::MeshGenericImage4PuppetSkinning => (
            scene_puppet_skinning_vertex_source(),
            super::generic_image_fragment_source(),
        ),
        SceneShaderFamily::MeshColor => {
            (scene_mesh_vertex_source(), super::color_fragment_source())
        }
        SceneShaderFamily::MeshColorPuppetSkinning => (
            scene_puppet_skinning_vertex_source(),
            super::color_fragment_source(),
        ),
        SceneShaderFamily::MeshText => (scene_mesh_vertex_source(), super::text_fragment_source()),
        SceneShaderFamily::MeshTextPuppetSkinning => (
            scene_puppet_skinning_vertex_source(),
            super::text_fragment_source(),
        ),
        SceneShaderFamily::MeshGenericParticle => (
            super::generic_particle_vertex_source(),
            super::generic_particle_fragment_source(),
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
        SceneShaderFamily::MeshObjectComposite => {
            if spec.key == "we/objectcomposite-screen-group" {
                super::screen_group_composite_sources()
            } else {
                super::object_composite_sources()
            }
        }
        SceneShaderFamily::MeshImageEffectSource => super::image_effect_source_sources(),
        SceneShaderFamily::MeshImageEffectComposite => {
            if spec.key == "we/image-effect-modulate-composite" {
                super::image_effect_modulate_composite_sources()
            } else {
                super::image_effect_composite_sources()
            }
        }
        SceneShaderFamily::MeshFlatRoundedMaskComposite => match spec.key {
            "we/flat-rounded-hsl-source" => super::flat_rounded_hsl_source_sources(),
            _ => super::flat_rounded_mask_composite_sources(),
        },
        SceneShaderFamily::MeshPuppetEffectSource => super::puppet_effect_source_sources(),
        SceneShaderFamily::MeshPuppetEffectComposite => super::puppet_effect_composite_sources(),
        SceneShaderFamily::MeshImageWaterWavesComposite => {
            if spec.key == "we/image-waterwaves-multiply-composite" {
                super::image_waterwaves_multiply_composite_sources()
            } else {
                super::image_waterwaves_composite_sources()
            }
        }
        SceneShaderFamily::MeshImageFoliageRippleComposite => {
            super::image_foliage_ripple_composite_sources(
                spec.key.contains("__GILDER_FOLIAGE_POWER_TWO_1"),
            )
        }
        SceneShaderFamily::MeshImageFoliageRippleScreenComposite => {
            super::image_foliage_ripple_screen_composite_sources(
                spec.key.contains("__GILDER_FOLIAGE_POWER_TWO_1"),
            )
        }
        SceneShaderFamily::MeshImageRippleFlowComposite => {
            if spec.key == "we/image-ripple-flow-multiply-composite" {
                super::image_ripple_flow_multiply_composite_sources()
            } else {
                super::image_ripple_flow_composite_sources()
            }
        }
        SceneShaderFamily::MeshFinalEffect => super::final_effect_sources(spec.key),
        SceneShaderFamily::MeshPuppetWaterWavesComposite => {
            super::puppet_waterwaves_composite_sources()
        }
        SceneShaderFamily::MeshWaterWavesDirect => super::waterwaves_direct_sources(
            spec.key.starts_with("we/puppet-waterwaves-direct"),
            spec.key.starts_with("we/effect-waterwaves-direct"),
            spec.key.starts_with("we/image-waterwaves-multiply-direct"),
            super::waterwaves_direct::stage_count_from_shader_key(spec.key),
            super::waterwaves_direct::static_black_output_from_shader_key(spec.key),
        ),
        SceneShaderFamily::MeshUtilityComposite => (
            flattexture_vertex_source(),
            super::passthrough_fragment_source(),
        ),
        SceneShaderFamily::EffectWaterWavesUvField => super::waterwaves_uv_field_sources(),
        SceneShaderFamily::EffectImageRippleSource => super::image_ripple_source_sources(),
        SceneShaderFamily::FlatMinimalAlpha => (
            flattexture_vertex_source(),
            super::minimal_alpha_fragment_source(),
        ),
        SceneShaderFamily::FlatPassthrough => (
            flattexture_vertex_source(),
            super::passthrough_fragment_source(),
        ),
        SceneShaderFamily::Effect => {
            let shader = effect_shader_name_for_key(spec.key);
            let texture_slot_mask = effect_texture_slot_mask_for_key(spec.key);
            (
                effect_vertex_source(spec.key, shader, texture_slot_mask),
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
    let source_extension = scene_shader_source_extension(key, stage);
    let source_path = shader_dir.join(format!("{safe_name}.{stage}.{source_extension}"));
    let spirv_path = shader_dir.join(format!("{safe_name}.{stage}.spv"));
    fs::write(&source_path, source).expect("write build-time scene shader source");
    if source_extension == "slang" {
        let shader_stage = match stage {
            "vert" => ShaderStage::Vertex,
            "frag" => ShaderStage::Fragment,
            "comp" => ShaderStage::Compute,
            _ => panic!("unsupported Slang scene shader stage {stage}"),
        };
        let entry_point = match shader_stage {
            ShaderStage::Vertex => "vertexMain",
            ShaderStage::Fragment => "fragmentMain",
            ShaderStage::Compute => "computeMain",
        };
        let contract = if key == "effects/audioline__SLOTS_1" && stage == "frag" {
            ShaderContract::descriptor_heap(12)
        } else {
            ShaderContract::mapped_descriptor_heap(0)
        };
        SlangCompiler::from_environment()
            .compile(&ShaderCompileRequest {
                source: source_path,
                entry_point: entry_point.to_owned(),
                stage: shader_stage,
                output: spirv_path.clone(),
                contract,
            })
            .unwrap_or_else(|error| panic!("compile built-in scene shader {key} {stage}: {error}"));
        return spirv_path;
    }
    let output = Command::new("glslangValidator")
        .args(["-V", "--target-env", "vulkan1.4", "-S", stage, "-o"])
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
    if byte_len < 4 || !byte_len.is_multiple_of(4) {
        panic!(
            "built-in scene shader {key} {stage} SPIR-V length {} is invalid",
            byte_len
        );
    }
    spirv_path
}

fn scene_shader_source_extension(key: &str, stage: &str) -> &'static str {
    if key == "effects/audioline__SLOTS_1" && stage == "frag" {
        "slang"
    } else {
        "glsl"
    }
}
