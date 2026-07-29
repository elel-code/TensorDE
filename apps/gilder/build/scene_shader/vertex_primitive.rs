use super::{SceneShaderFamily, SceneShaderSpec};

pub(crate) fn scene_shader_vertex_primitive(spec: SceneShaderSpec) -> &'static str {
    use SceneShaderFamily::*;

    match spec.family {
        MeshGenericParticle => "ParticleBillboard",
        MeshFlatRoundedMaskComposite => "ObjectUvSupportQuad",
        MeshFinalEffect if spec.key == "we/flat-rounded-opacity-final" => "ObjectUvSupportQuad",
        MeshFinalEffect if spec.key == "we/framebuffer-water-quantized-water-opacity" => {
            "FullscreenTriangle"
        }
        MeshObjectComposite
        | MeshUtilityComposite
        | EffectWaterWavesUvField
        | EffectImageRippleSource
        | FlatMinimalAlpha
        | FlatPassthrough
        | Effect => "FullscreenTriangle",
        MeshWaterWavesDirect if spec.key.starts_with("we/effect-waterwaves-direct") => {
            "FullscreenTriangle"
        }
        MeshGenericImage4
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
        | MeshImageEffectSource
        | MeshImageEffectComposite
        | MeshPuppetEffectSource
        | MeshPuppetEffectComposite
        | MeshImageWaterWavesComposite
        | MeshImageFoliageRippleComposite
        | MeshImageFoliageRippleScreenComposite
        | MeshImageRippleFlowComposite
        | MeshFinalEffect
        | MeshPuppetWaterWavesComposite
        | MeshWaterWavesDirect => "ObjectMesh",
    }
}
