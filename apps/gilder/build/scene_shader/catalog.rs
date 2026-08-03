use super::super::*;
use std::collections::BTreeSet;
use std::path::Path;

#[path = "catalog/installed_effects.rs"]
mod installed_effects;
#[path = "catalog/key.rs"]
mod key;
#[path = "catalog/native_stage.rs"]
mod native_stage;
#[path = "catalog/specs.rs"]
mod specs;

use installed_effects::INSTALLED_EFFECT_PROGRAMS;
use key::{effect_shader_name_for_key, effect_texture_slot_mask_for_key};
use native_stage::{
    builtin_binding_expressions, compile_generated_scene_fragment, compile_generated_scene_vertex,
    compile_native_particle_compute, compile_native_slang_scene_fragment,
    compile_native_slang_scene_input_attachment, compile_native_slang_scene_vertex,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SceneShaderSpec {
    pub(crate) key: &'static str,
    pub(crate) family: SceneShaderFamily,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SceneShaderFamily {
    MeshGenericImage4,
    MeshSceneColorBlend,
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
    DepthParallax,
    Pulse,
    Shake,
    Effect,
}

#[derive(Debug, Clone)]
struct SceneShaderCatalogSpec {
    key: String,
    family: SceneShaderFamily,
}

use specs::BUILTIN_SCENE_SHADER_SPECS;

pub(crate) fn build_scene_shader_origin_catalog() {
    let programs = INSTALLED_EFFECT_PROGRAMS
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    for program in BUILTIN_SCENE_SHADER_SPECS
        .iter()
        .chain(super::FINAL_EFFECT_SHADER_SPECS)
        .map(|spec| spec.key.split_once("__").map_or(spec.key, |(key, _)| key))
        .filter(|key| key.starts_with("effects/"))
    {
        assert!(
            programs.contains(program),
            "scene catalog effect {program:?} is not owned by the installed 46-effect registry"
        );
    }
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
    generated.push_str("    pub vertex: BuiltinSceneVertexShader,\n");
    generated.push_str("    pub object_mesh_vertex: Option<BuiltinSceneVertexShader>,\n");
    generated.push_str("    pub fragment_spirv: &'static [u32],\n");
    generated.push_str("    #[cfg(test)]\n    pub fragment_source: &'static str,\n");
    generated.push_str("    pub fragment_push_constant_bytes: u32,\n");
    generated.push_str("    pub fragment_bindings: &'static [BuiltinSceneDescriptorBinding],\n");
    generated.push_str("    pub local_read_shader: Option<BuiltinSceneLocalReadShader>,\n    pub fragment_coordinate_fetch_slot_mask: u32,\n");
    generated.push_str("    pub parameter_layout: BuiltinSceneParameterLayout,\n");
    generated.push_str("}\n\n");
    let depth_parallax_variants = super::depth_parallax::catalog_variants().collect::<Vec<_>>();
    let pulse_variants = super::pulse::catalog_variants().collect::<Vec<_>>();
    let shake_variants = super::shake::catalog_variants().collect::<Vec<_>>();
    let mut specs = BUILTIN_SCENE_SHADER_SPECS
        .iter()
        .chain(super::FINAL_EFFECT_SHADER_SPECS)
        .map(|spec| SceneShaderCatalogSpec {
            key: spec.key.to_owned(),
            family: spec.family,
        })
        .collect::<Vec<_>>();
    specs.extend(
        depth_parallax_variants
            .iter()
            .copied()
            .map(|variant| SceneShaderCatalogSpec {
                key: super::depth_parallax::catalog_key(variant),
                family: SceneShaderFamily::DepthParallax,
            }),
    );
    specs.extend(
        pulse_variants
            .iter()
            .copied()
            .map(|variant| SceneShaderCatalogSpec {
                key: super::pulse::catalog_key(variant),
                family: SceneShaderFamily::Pulse,
            }),
    );
    specs.extend(
        shake_variants
            .iter()
            .copied()
            .map(|variant| SceneShaderCatalogSpec {
                key: super::shake::catalog_key(variant),
                family: SceneShaderFamily::Shake,
            }),
    );
    let expected_entry_count = specs.len();
    let mut actual_entry_count = 0usize;
    let mut entries = String::new();
    for spec in &specs {
        actual_entry_count += 1;
        let (vertex_source, fragment_source) = scene_shader_sources(spec.family, &spec.key);
        let fragment =
            compile_scene_fragment(&shader_dir, spec.family, &spec.key, &fragment_source);
        let vertex = compile_scene_vertex(
            &shader_dir,
            spec.family,
            &spec.key,
            &vertex_source,
            fragment.push_constant_bytes,
        );
        let object_mesh_vertex = match spec.family {
            SceneShaderFamily::FlatPassthrough => Some(compile_native_slang_scene_vertex(
                &shader_dir,
                &format!("{}__OBJECT_MESH", spec.key),
                &super::mesh_vertex_source(),
                fragment.push_constant_bytes,
            )),
            SceneShaderFamily::Effect => {
                let shader = effect_shader_name_for_key(&spec.key);
                let texture_slot_mask = effect_texture_slot_mask_for_key(&spec.key);
                if shader == "effects/tint" && texture_slot_mask == 0x03 {
                    Some(compile_native_slang_scene_vertex(
                        &shader_dir,
                        &format!("{}__OBJECT_MESH", spec.key),
                        &super::tint_masked_object_mesh_vertex_source(),
                        fragment.push_constant_bytes,
                    ))
                } else if shader == "effects/waterflow" {
                    let source = super::waterflow_object_mesh_vertex_source();
                    Some(compile_native_slang_scene_vertex(
                        &shader_dir,
                        &format!("{}__OBJECT_MESH", spec.key),
                        &source,
                        fragment.push_constant_bytes,
                    ))
                } else if shader == "effects/waterripple" && texture_slot_mask == 0x05 {
                    Some(compile_native_slang_scene_vertex(
                        &shader_dir,
                        &format!("{}__OBJECT_MESH", spec.key),
                        &super::mesh_vertex_source(),
                        fragment.push_constant_bytes,
                    ))
                } else {
                    super::effect_object_mesh_vertex_source(&spec.key, shader, texture_slot_mask)
                        .map(|source| {
                            compile_generated_scene_vertex(
                                &shader_dir,
                                &format!("{}__OBJECT_MESH", spec.key),
                                &source,
                                fragment.push_constant_bytes,
                            )
                        })
                }
            }
            SceneShaderFamily::Pulse => {
                let variant = super::pulse::variant_from_catalog_key(&spec.key)
                    .expect("generated Pulse catalog key must have a typed variant");
                let source = super::pulse::object_mesh_vertex_source(variant);
                Some(compile_native_slang_scene_vertex(
                    &shader_dir,
                    &format!("{}__OBJECT_MESH", spec.key),
                    &source,
                    fragment.push_constant_bytes,
                ))
            }
            _ => None,
        };
        let vertex_path = vertex
            .spirv
            .to_str()
            .expect("built-in scene vertex shader path must be UTF-8");
        let fragment_path = fragment
            .spirv
            .to_str()
            .expect("built-in scene fragment shader path must be UTF-8");
        let fragment_source_path = fragment
            .source
            .to_str()
            .expect("built-in scene fragment source path must be UTF-8");
        assert!(
            fragment_source_path.ends_with(".source.slang"),
            "built-in scene shader {} did not retain native Slang source",
            spec.key
        );
        let local_read_shader = if matches!(spec.family, SceneShaderFamily::FlatPassthrough) {
            let source = super::flat_passthrough_input_attachment_source();
            let stage = compile_native_slang_scene_input_attachment(
                &shader_dir,
                &format!("{}__INPUT_ATTACHMENT", spec.key),
                source.source(),
                fragment.push_constant_bytes,
            );
            source.catalog_expression(
                &stage.spirv,
                stage.push_constant_bytes,
                &builtin_binding_expressions(&stage.bindings),
            )
        } else {
            "None".to_owned()
        };
        let object_mesh_vertex = object_mesh_vertex.as_ref().map_or_else(
            || "None".to_owned(),
            |vertex| {
                let bindings = builtin_binding_expressions(&vertex.bindings);
                format!(
                    "Some(BuiltinSceneVertexShader {{ spirv: vulkan_renderer::include_spirv!({:?}), push_constant_bytes: {}, bindings: &[{}] }})",
                    vertex
                        .spirv
                        .to_str()
                        .expect("built-in object-mesh vertex shader path must be UTF-8")
                    ,
                    vertex.push_constant_bytes,
                    bindings,
                )
            },
        );
        let vertex_primitive = super::scene_shader_vertex_primitive(spec.family, &spec.key);
        let parameter_layout = scene_shader_parameter_layout(spec.family, &spec.key);
        let vertex_bindings = builtin_binding_expressions(&vertex.bindings);
        let fragment_bindings = builtin_binding_expressions(&fragment.bindings);
        let fragment_coordinate_fetch_slot_mask =
            u32::from(spec.key == "we/flat-rounded-hsl-source");
        entries.push_str(&format!(
            "    BuiltinSceneShader {{ key: {:?}, vertex_primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive::{vertex_primitive}, vertex: BuiltinSceneVertexShader {{ spirv: vulkan_renderer::include_spirv!({:?}), push_constant_bytes: {}, bindings: &[{}] }}, object_mesh_vertex: {object_mesh_vertex}, fragment_spirv: vulkan_renderer::include_spirv!({:?}), #[cfg(test)] fragment_source: include_str!({:?}), fragment_push_constant_bytes: {}, fragment_bindings: &[{}], local_read_shader: {local_read_shader}, fragment_coordinate_fetch_slot_mask: {fragment_coordinate_fetch_slot_mask}, parameter_layout: BuiltinSceneParameterLayout::{parameter_layout} }},\n",
            spec.key,
            vertex_path,
            vertex.push_constant_bytes,
            vertex_bindings,
            fragment_path,
            fragment_source_path,
            fragment.push_constant_bytes,
            fragment_bindings,
        ));
    }
    assert_eq!(
        actual_entry_count, expected_entry_count,
        "native scene shader catalog coverage regressed"
    );
    generated.push_str("pub static BUILTIN_SCENE_SHADERS: &[BuiltinSceneShader] = &[\n");
    generated.push_str(&entries);
    generated.push_str("];\n");
    generated.push_str(&super::depth_parallax::resolver_source(
        &depth_parallax_variants,
    ));
    generated.push_str(&super::pulse::resolver_source(&pulse_variants));
    generated.push_str(&super::shake::resolver_source(&shake_variants));

    let compute = compile_native_particle_compute(
        &shader_dir,
        "particle_compute",
        super::particle_compute_source(),
    );
    let compute_path = compute
        .spirv
        .to_str()
        .expect("particle compute shader path must be UTF-8");
    let compute_bindings = builtin_binding_expressions(&compute.bindings);
    generated.push_str("\n#[derive(Debug, Clone, Copy)]\n");
    generated.push_str("pub struct BuiltinParticleComputeShader {\n");
    generated.push_str("    pub spirv: &'static [u32],\n");
    generated.push_str("    pub push_constant_bytes: u32,\n");
    generated.push_str("    pub bindings: &'static [BuiltinSceneDescriptorBinding],\n");
    generated.push_str("}\n\n");
    generated.push_str(&format!(
        "pub static BUILTIN_PARTICLE_COMPUTE_SHADER: BuiltinParticleComputeShader = BuiltinParticleComputeShader {{ spirv: vulkan_renderer::include_spirv!({:?}), push_constant_bytes: {}, bindings: &[{}] }};\n",
        compute_path,
        compute.push_constant_bytes,
        compute_bindings,
    ));

    fs::write(out_dir.join("gilder_scene_shader_catalog.rs"), generated)
        .expect("write built-in scene shader catalog");
}

fn compile_scene_fragment(
    shader_dir: &Path,
    family: SceneShaderFamily,
    key: &str,
    source: &str,
) -> native_stage::NativeSceneStage {
    if has_version_controlled_native_slang_source(family, key) {
        compile_native_slang_scene_fragment(shader_dir, key, source)
    } else {
        compile_generated_scene_fragment(shader_dir, key, source)
    }
}

fn compile_scene_vertex(
    shader_dir: &Path,
    family: SceneShaderFamily,
    key: &str,
    source: &str,
    push_base_bytes: u32,
) -> native_stage::NativeSceneStage {
    if has_version_controlled_native_slang_source(family, key) {
        compile_native_slang_scene_vertex(shader_dir, key, source, push_base_bytes)
    } else {
        compile_generated_scene_vertex(shader_dir, key, source, push_base_bytes)
    }
}

fn scene_shader_parameter_layout(family: SceneShaderFamily, key: &str) -> &'static str {
    match family {
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
        SceneShaderFamily::MeshSceneColorBlend => "SceneColorBlend",
        SceneShaderFamily::MeshGenericParticle => "Particle",
        SceneShaderFamily::MeshWaterWavesDirect => "WaterWavesDirect",
        SceneShaderFamily::MeshImageFoliageRippleComposite
        | SceneShaderFamily::MeshImageFoliageRippleScreenComposite => "FoliageRippleComposite",
        SceneShaderFamily::MeshImageRippleFlowComposite => "RippleFlowComposite",
        SceneShaderFamily::MeshFinalEffect => super::final_effect_parameter_layout(key),
        SceneShaderFamily::MeshFlatRoundedMaskComposite => "RoundedMask",
        SceneShaderFamily::EffectWaterWavesUvField => "WaterWavesUvField",
        SceneShaderFamily::EffectImageRippleSource => "WaterRipple",
        SceneShaderFamily::DepthParallax => "DepthParallax",
        SceneShaderFamily::Pulse => "Pulse",
        SceneShaderFamily::Shake => "Shake",
        SceneShaderFamily::Effect => match effect_shader_name_for_key(key) {
            "effects/blend" => "Blend",
            "effects/blendgradient" => "BlendGradient",
            "effects/blur_combine" => "BlurCombine",
            "effects/blur_downsample4" => "None",
            "effects/blur_gaussian" => "BlurGaussian",
            "effects/caustics" => "Caustics",
            "effects/cloudmotion" => "CloudMotion",
            "effects/colorkey" => "ColorKey",
            "effects/iris" => "Iris",
            "effects/opacity" => "Opacity",
            "effects/scroll" => "Scroll",
            "effects/skew" => "Skew",
            "effects/tint" => "Tint",
            "effects/spin" => "Spin",
            "effects/shake" => "Shake",
            "effects/shimmer" => "Shimmer",
            "effects/swing" => "Swing",
            "effects/foliagesway" => "FoliageSway",
            "effects/waterwaves" => "WaterWaves",
            "effects/waterripple" => "WaterRipple",
            "effects/waterflow" => "WaterFlow",
            _ => "None",
        },
        _ => "None",
    }
}

fn scene_shader_sources(family: SceneShaderFamily, key: &str) -> (String, String) {
    match family {
        SceneShaderFamily::MeshGenericImage4 if key == "we/genericimage4" => {
            super::generic_image_sources()
        }
        SceneShaderFamily::MeshGenericImage4 => {
            let fragment = if key == "we/genericimage4-multiply-composite" {
                super::generic_image_multiply_fragment_source()
            } else {
                super::generic_image_fragment_source()
            };
            (scene_mesh_vertex_source(), fragment)
        }
        SceneShaderFamily::MeshSceneColorBlend => super::scene_color_blend_sources(),
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
        SceneShaderFamily::MeshComposelayer => super::composelayer_sources(),
        SceneShaderFamily::MeshObjectComposite => {
            if key == "we/objectcomposite-screen-group" {
                super::screen_group_composite_sources()
            } else {
                super::object_composite_sources()
            }
        }
        SceneShaderFamily::MeshImageEffectSource => super::image_effect_source_sources(),
        SceneShaderFamily::MeshImageEffectComposite => {
            if key == "we/image-effect-modulate-composite" {
                super::image_effect_modulate_composite_sources()
            } else {
                super::image_effect_composite_sources()
            }
        }
        SceneShaderFamily::MeshFlatRoundedMaskComposite => match key {
            "we/flat-rounded-hsl-source" => super::flat_rounded_hsl_source_sources(),
            _ => super::flat_rounded_mask_composite_sources(),
        },
        SceneShaderFamily::MeshPuppetEffectComposite => super::puppet_effect_composite_sources(),
        SceneShaderFamily::MeshImageWaterWavesComposite => {
            if key == "we/image-waterwaves-multiply-composite" {
                super::image_waterwaves_multiply_composite_sources()
            } else {
                super::image_waterwaves_composite_sources()
            }
        }
        SceneShaderFamily::MeshImageFoliageRippleComposite => {
            super::image_foliage_ripple_composite_sources(
                key.contains("__GILDER_FOLIAGE_POWER_TWO_1"),
            )
        }
        SceneShaderFamily::MeshImageFoliageRippleScreenComposite => {
            super::image_foliage_ripple_screen_composite_sources(
                key.contains("__GILDER_FOLIAGE_POWER_TWO_1"),
            )
        }
        SceneShaderFamily::MeshImageRippleFlowComposite => {
            if key == "we/image-ripple-flow-multiply-composite" {
                super::image_ripple_flow_multiply_composite_sources()
            } else {
                super::image_ripple_flow_composite_sources()
            }
        }
        SceneShaderFamily::MeshFinalEffect => super::final_effect_sources(key),
        SceneShaderFamily::MeshPuppetWaterWavesComposite => {
            super::puppet_waterwaves_composite_sources()
        }
        SceneShaderFamily::MeshWaterWavesDirect => super::waterwaves_direct_sources(
            key.starts_with("we/puppet-waterwaves-direct"),
            key.starts_with("we/effect-waterwaves-direct"),
            key.starts_with("we/image-waterwaves-multiply-direct"),
            super::waterwaves_direct::stage_count_from_shader_key(key),
            super::waterwaves_direct::static_black_output_from_shader_key(key),
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
        SceneShaderFamily::Effect if effect_shader_name_for_key(key) == "effects/waterflow" => {
            super::waterflow_sources()
        }
        SceneShaderFamily::Effect
            if effect_shader_name_for_key(key) == "effects/tint"
                && effect_texture_slot_mask_for_key(key) == 0x03 =>
        {
            super::tint_masked_sources(key)
        }
        SceneShaderFamily::Effect
            if effect_shader_name_for_key(key) == "effects/waterripple"
                && effect_texture_slot_mask_for_key(key) == 0x05 =>
        {
            super::waterripple_slots_5_sources()
        }
        SceneShaderFamily::Effect => {
            let shader = effect_shader_name_for_key(key);
            let texture_slot_mask = effect_texture_slot_mask_for_key(key);
            (
                effect_vertex_source(key, shader, texture_slot_mask),
                effect_fragment_source(key, shader, texture_slot_mask),
            )
        }
        SceneShaderFamily::DepthParallax => {
            let variant = super::depth_parallax::variant_from_catalog_key(key)
                .expect("generated Depth Parallax catalog key must have a typed variant");
            (
                super::depth_parallax::vertex_source(variant),
                super::depth_parallax::fragment_source(variant),
            )
        }
        SceneShaderFamily::Pulse => {
            let variant = super::pulse::variant_from_catalog_key(key)
                .expect("generated Pulse catalog key must have a typed variant");
            (
                super::pulse::fullscreen_vertex_source(variant),
                super::pulse::fragment_source(variant),
            )
        }
        SceneShaderFamily::Shake => {
            let variant = super::shake::variant_from_catalog_key(key)
                .expect("generated Shake catalog key must have a typed variant");
            (
                super::shake::vertex_source(),
                super::shake::fragment_source(variant),
            )
        }
    }
}

fn has_version_controlled_native_slang_source(family: SceneShaderFamily, key: &str) -> bool {
    matches!(
        family,
        SceneShaderFamily::MeshSceneColorBlend
            | SceneShaderFamily::MeshComposelayer
            | SceneShaderFamily::MeshImageEffectSource
            | SceneShaderFamily::DepthParallax
            | SceneShaderFamily::Pulse
            | SceneShaderFamily::Shake
    ) || key == "we/genericimage4"
        || matches!(family, SceneShaderFamily::Effect)
            && matches!(
                (
                    effect_shader_name_for_key(key),
                    effect_texture_slot_mask_for_key(key)
                ),
                ("effects/tint", 0x03) | ("effects/waterflow", _) | ("effects/waterripple", 0x05)
            )
}
