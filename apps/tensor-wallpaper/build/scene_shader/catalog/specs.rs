use super::{SceneShaderFamily, SceneShaderSpec};

pub(super) const BUILTIN_SCENE_SHADER_SPECS: &[SceneShaderSpec] = &[
    SceneShaderSpec {
        key: "tensor-wallpaper/dynamic-text",
        family: SceneShaderFamily::MeshDynamicText,
    },
    SceneShaderSpec {
        key: "effects/tint__SLOTS_1__BLENDMODE_0",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/tint__SLOTS_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/tint__SLOTS_3",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blur_combine__SLOTS_5",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/blur_combine__SLOTS_5__BLENDMODE_5__COMPOSITE_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/spin__SLOTS_1__REPEAT_0",
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
        key: "effects/blur_combine__SLOTS_5__BLENDMODE_2__COMPOSITE_1",
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
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_CHROMATIC_ZERO_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_CHROMATIC_ZERO_1__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_CHROMATIC_ZERO_1__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1__TENSOR_WALLPAPER_COLOR_EQUAL_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__TENSOR_WALLPAPER_CHROMATIC_ZERO_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__TENSOR_WALLPAPER_CHROMATIC_ZERO_1__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__TENSOR_WALLPAPER_CHROMATIC_ZERO_1__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1__TENSOR_WALLPAPER_COLOR_EQUAL_1",
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
        key: "effects/waterwaves__SLOTS_5",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/waterwaves__SLOTS_5__DUALWAVES_1",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/waterwaves__SLOTS_7",
        family: SceneShaderFamily::Effect,
    },
    SceneShaderSpec {
        key: "effects/waterwaves__SLOTS_7__DUALWAVES_1",
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
        key: "we/puppet-effect-composite",
        family: SceneShaderFamily::MeshPuppetEffectComposite,
    },
    SceneShaderSpec {
        key: "we/puppet-effect-composite-clipping",
        family: SceneShaderFamily::MeshPuppetEffectCompositeClipping,
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
        key: "we/image-foliage-ripple-composite__TENSOR_WALLPAPER_FOLIAGE_POWER_TWO_1",
        family: SceneShaderFamily::MeshImageFoliageRippleComposite,
    },
    SceneShaderSpec {
        key: "we/image-foliage-ripple-screen-composite",
        family: SceneShaderFamily::MeshImageFoliageRippleScreenComposite,
    },
    SceneShaderSpec {
        key: "we/image-foliage-ripple-screen-composite__TENSOR_WALLPAPER_FOLIAGE_POWER_TWO_1",
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
        key: "we/genericimage4-scene-color-blend",
        family: SceneShaderFamily::MeshSceneColorBlend,
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
