use super::SceneShaderFamily;

pub(crate) fn scene_shader_vertex_primitive(family: SceneShaderFamily, key: &str) -> &'static str {
    use SceneShaderFamily::*;

    match family {
        MeshGenericParticle => "ParticleBillboard",
        MeshFlatRoundedMaskComposite => "ObjectUvSupportQuad",
        MeshFinalEffect if key == "we/flat-rounded-opacity-final" => "ObjectUvSupportQuad",
        MeshFinalEffect if key == "we/framebuffer-water-quantized-water-opacity" => {
            "FullscreenTriangle"
        }
        MeshImageEffectSource => "ObjectUvSupportQuad",
        MeshObjectComposite
        | MeshUtilityComposite
        | EffectWaterWavesUvField
        | EffectImageRippleSource
        | FlatMinimalAlpha
        | FlatPassthrough
        | DepthParallax
        | Pulse
        | Shake
        | Effect => "FullscreenTriangle",
        MeshWaterWavesDirect if key.starts_with("we/effect-waterwaves-direct") => {
            "FullscreenTriangle"
        }
        MeshGenericImage4
        | MeshSceneColorBlend
        | MeshDynamicText
        | MeshGenericImage4PuppetSkinning
        | MeshColor
        | MeshColorPuppetSkinning
        | MeshText
        | MeshTextPuppetSkinning
        | MeshGenericImage4ClippingTarget
        | MeshGenericImage4ClippingTargetPuppetSkinning
        | MeshClippingMaskImage4
        | MeshClippingMaskImage4PuppetSkinning
        | MeshComposelayer
        | MeshImageEffectComposite
        | MeshPuppetEffectComposite
        | MeshPuppetEffectCompositeClipping
        | MeshImageWaterWavesComposite
        | MeshImageFoliageRippleComposite
        | MeshImageFoliageRippleScreenComposite
        | MeshImageRippleFlowComposite
        | MeshFinalEffect
        | MeshPuppetWaterWavesComposite
        | MeshWaterWavesDirect => "ObjectMesh",
    }
}
