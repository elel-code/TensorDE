use super::{SceneShaderFamily, SceneShaderSpec};

pub(super) const BUILTIN_SCENE_SHADER_SPECS: &[SceneShaderSpec] = &[
    SceneShaderSpec {
        key: "effects/111__SLOTS_1__BLENDMODE_7",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/111__SLOTS_1__BLENDMODE_31",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/auto_sway__SLOTS_1__DEBUG_0__DEBUG_NO_ALPHA_1__NODE_COUNT_4",
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
        key: "effects/blur_combine__SLOTS_5__BLENDMODE_1__COMPOSITE_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blur_downsample4__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blur_gaussian__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blur_gaussian__SLOTS_1__VERTICAL_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_CHROMATIC_ZERO_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_CHROMATIC_ZERO_1__GILDER_PATTERN_GLOW_SHARED_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_CHROMATIC_ZERO_1__GILDER_PATTERN_GLOW_SHARED_1__GILDER_COLOR_EQUAL_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__GILDER_CHROMATIC_ZERO_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__GILDER_CHROMATIC_ZERO_1__GILDER_PATTERN_GLOW_SHARED_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__GILDER_CHROMATIC_ZERO_1__GILDER_PATTERN_GLOW_SHARED_1__GILDER_COLOR_EQUAL_1",
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
        key: "effects/lut_loader__SLOTS_3__CLAMP_0__QUAD_SIZE_64",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/lut_loader__SLOTS_3__QUAD_SIZE_64",
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
        key: "effects/procedural_noise__SLOTS_1__AA_CATEGORY_1__BLENDMODE_20__STEPANIM_1",
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
        key: "we/minimalalpha",
        family: SceneShaderFamily::FlatMinimalAlpha,
    },
    SceneShaderSpec {
        key: "we/minimalalpha__SLOTS_1",
        family: SceneShaderFamily::FlatMinimalAlpha,
    },
    SceneShaderSpec {
        key: "we/passthrough",
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
        key: "we/objectcomposite-screen-group",
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
        key: "we/image-effect-composite__STATIC_BLACK_1",
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
        key: "we/image-waterwaves-direct__STAGES_8",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_9",
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
        key: "we/image-waterwaves-direct__STAGES_8__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-direct__STAGES_9__STATIC_BLACK_1",
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
        key: "we/image-waterwaves-multiply-direct__STAGES_8",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_9",
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
        key: "we/image-waterwaves-multiply-direct__STAGES_8__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/image-waterwaves-multiply-direct__STAGES_9__STATIC_BLACK_1",
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
        key: "we/puppet-waterwaves-direct__STAGES_8",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_9",
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
        key: "we/puppet-waterwaves-direct__STAGES_8__STATIC_BLACK_1",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/puppet-waterwaves-direct__STAGES_9__STATIC_BLACK_1",
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
        key: "we/effect-waterwaves-direct__STAGES_8",
        family: SceneShaderFamily::MeshWaterWavesDirect,
    },
    SceneShaderSpec {
        key: "we/effect-waterwaves-direct__STAGES_9",
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
        key: "effects/tech_circle__SLOTS_1__SECTOR_SEGMENTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/foliagesway__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/foliagesway__SLOTS_3",
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
        key: "effects/simple_audio_bars__SLOTS_1__SHAPE_7",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/simple_audio_bars__SLOTS_1__ANTIALIAS_0__SHAPE_7",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/rounded_mask__SLOTS_1__SOFT_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/raindrop_on_glass__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/audio_responsive_oscilloscope__SLOTS_5__RESOLUTION_16",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blend__SLOTS_3__BLENDMODE_0__WRITEALPHA_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blend__SLOTS_3__BLENDMODE_0__TRANSFORMREPEAT_2__WRITEALPHA_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blendgradient__SLOTS_3__WRITEALPHA_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blendgradient__SLOTS_7__WRITEALPHA_1",
        family: SceneShaderFamily::Effect,
    },
];
