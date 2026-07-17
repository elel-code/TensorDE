use super::super::*;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SceneShaderSpec {
    pub(crate) key: &'static str,
    pub(crate) family: SceneShaderFamily,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum SceneShaderFamily {
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
    MeshWaterWavesDirect,
    MeshUtilityComposite,
    EffectWaterWavesUvField,
    EffectImageRippleSource,
    FlatMinimalAlpha,
    FlatPassthrough,
    Effect,
}

const BUILTIN_SCENE_SHADER_SPECS: &[SceneShaderSpec] = &[
    SceneShaderSpec {
        key: "effects/111__SLOTS_1__BLENDMODE_7",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/111__SLOTS_1__BLENDMODE_31",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blend__SLOTS_3__BLENDMODE_0",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blend__SLOTS_3__BLENDMODE_0__TRANSFORMREPEAT_2__TRANSFORMUV_1",
        family: SceneShaderFamily::Effect,
    },
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
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_OVERLAY_1__GILDER_CHROMATIC_ZERO_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_OVERLAY_1__GILDER_CHROMATIC_ZERO_1__GILDER_PATTERN_GLOW_SHARED_1",
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
        key: "effects/shimmer__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/shimmer__SLOTS_3",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/shimmer__SLOTS_9",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/shimmer__SLOTS_b",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/swing__SLOTS_1",
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
        key: "we/image-effect-modulate-composite",
        family: SceneShaderFamily::MeshImageEffectComposite,
    },
    SceneShaderSpec {
        key: "we/flat-rounded-mask-composite",
        family: SceneShaderFamily::MeshFlatRoundedMaskComposite,
    },
    SceneShaderSpec {
        key: "we/flat-rounded-hsl-source",
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
        key: "we/image-foliage-ripple-composite__GILDER_FOLIAGE_POWER_TWO_1",
        family: SceneShaderFamily::MeshImageFoliageRippleComposite,
    },
    SceneShaderSpec {
        key: "we/image-foliage-ripple-screen-composite",
        family: SceneShaderFamily::MeshImageFoliageRippleScreenComposite,
    },
    SceneShaderSpec {
        key: "we/image-foliage-ripple-screen-composite__GILDER_FOLIAGE_POWER_TWO_1",
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
        key: "we/image-waterwaves-direct",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_2",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_3",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_4",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_5",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_6",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_7",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_2__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_3__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_4__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_5__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_6__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_7__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_2",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_3",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_4",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_5",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_6",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_7",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_2__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_3__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_4__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_5__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_6__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_7__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_2",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_3",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_4",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_5",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_6",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_7",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_2__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_3__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_4__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_5__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_6__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_7__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/effect-waterwaves-direct__STAGES_2",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/effect-waterwaves-direct__STAGES_3",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/effect-waterwaves-direct__STAGES_4",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/effect-waterwaves-direct__STAGES_5",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/effect-waterwaves-direct__STAGES_6",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/effect-waterwaves-direct__STAGES_7",
        family: SceneShaderFamily::MeshWaterWavesDirect,
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
        key: "effects/foliagesway__SLOTS_5",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/foliagesway__SLOTS_7",
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
        key: "workshop/3082978660/effects/Simple_Audio_Bars__SLOTS_1__ANTIALIAS_0__SHAPE_7",
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
    SceneShaderSpec {
        key: "workshop/3165346237/effects/lut_loader__SLOTS_3__CLAMP_0__QUAD_SIZE_64",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/3706822104/effects/raindrop_on_glass__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2799421411/effects/audio_responsive_oscilloscope__SLOTS_1__RESOLUTION_16",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2962751255/effects/blend__SLOTS_3__BLENDMODE_0__WRITEALPHA_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2962751255/effects/blend__SLOTS_3__BLENDMODE_0__TRANSFORMREPEAT_2__WRITEALPHA_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2962751255/effects/blendgradient__SLOTS_3__WRITEALPHA_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "workshop/2962751255/effects/blendgradient__SLOTS_7__WRITEALPHA_1",
        family: SceneShaderFamily::Effect,
    },
];

pub(crate) fn build_scene_shader_catalog() {
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
        .chain(super::FINAL_EFFECT_SHADER_SPECS)
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
        "pub static BUILTIN_PARTICLE_COMPUTE_SHADER: BuiltinParticleComputeShader = BuiltinParticleComputeShader {{ spirv: vulkanalia::include_shader_code!({:?}) }};\n",
        compute_path
    ));

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
            "effects/blend" | "workshop/2962751255/effects/blend" => "Blend",
            "workshop/2962751255/effects/blendgradient" => "BlendGradient",
            "workshop/3165346237/effects/lut_loader" => "Lut",
            "workshop/3706822104/effects/raindrop_on_glass" => "Raindrop",
            "workshop/2799421411/effects/audio_responsive_oscilloscope" => "Oscilloscope",
            "effects/caustics" => "Caustics",
            "effects/cloudmotion" => "CloudMotion",
            "effects/colorkey" => "ColorKey",
            "effects/iris" => "Iris",
            "effects/opacity" => "Opacity",
            "effects/scroll" => "Scroll",
            "effects/skew" => "Skew",
            "workshop/3083593512/effects/rounded_mask" => "RoundedMask",
            "effects/shake" => "Shake",
            "effects/shimmer" => "Shimmer",
            "effects/swing" => "Swing",
            "effects/foliagesway" => "FoliageSway",
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
                super::generic_image_multiply_fragment_source()
            } else {
                super::generic_image_fragment_source()
            };
            (scene_mesh_vertex_source(), fragment)
        }
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
        SceneShaderFamily::MeshObjectComposite => super::object_composite_sources(),
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
            scene_mesh_vertex_source(),
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
