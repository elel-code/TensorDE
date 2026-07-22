//! Built-in scene shader catalog.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/shaders/**`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinSceneParameterLayout {
    None,
    AudioBars,
    AutoSway,
    Blend,
    BlendGradient,
    BlurCombine,
    BlurGaussian,
    Particle,
    StandardMaterial,
    Iris,
    Lightning,
    Lut,
    Oscilloscope,
    Opacity,
    ProceduralNoise,
    Raindrop,
    RoundedMask,
    Scroll,
    Skew,
    Shimmer,
    Swing,
    TechCircle,
    Caustics,
    CloudMotion,
    ColorKey,
    FoliageSway,
    FoliageRippleComposite,
    FinalEffectProgram,
    FinalWaterRipple,
    FinalWaterWaves,
    RippleFlowComposite,
    Shake,
    WaterWaves,
    WaterWavesDirect,
    WaterWavesUvField,
    WaterRipple,
    WaterFlow,
}

impl BuiltinSceneParameterLayout {
    pub const fn uses_material_uniform(self) -> bool {
        !matches!(self, Self::None)
    }

    pub const fn uses_effect_draw_uniform(self) -> bool {
        matches!(
            self,
            Self::AudioBars
                | Self::Blend
                | Self::BlendGradient
                | Self::Oscilloscope
                | Self::FinalEffectProgram
                | Self::Iris
                | Self::Particle
                | Self::RoundedMask
                | Self::Scroll
                | Self::Skew
                | Self::TechCircle
                | Self::WaterFlow
                | Self::WaterWaves
                | Self::WaterWavesDirect
                | Self::WaterWavesUvField
        )
    }

    pub const fn uses_dynamic_material_input(self) -> bool {
        matches!(
            self,
            Self::AudioBars
                | Self::AutoSway
                | Self::Blend
                | Self::BlendGradient
                | Self::BlurCombine
                | Self::BlurGaussian
                | Self::Iris
                | Self::Lightning
                | Self::Oscilloscope
                | Self::Particle
                | Self::ProceduralNoise
                | Self::Caustics
                | Self::CloudMotion
                | Self::FoliageSway
                | Self::FoliageRippleComposite
                | Self::FinalEffectProgram
                | Self::FinalWaterRipple
                | Self::FinalWaterWaves
                | Self::RippleFlowComposite
                | Self::Shake
                | Self::Scroll
                | Self::Raindrop
                | Self::Shimmer
                | Self::Swing
                | Self::TechCircle
                | Self::WaterFlow
                | Self::WaterWaves
                | Self::WaterWavesUvField
                | Self::WaterRipple
        )
    }
}

include!(concat!(env!("OUT_DIR"), "/gilder_scene_shader_catalog.rs"));

pub fn native_vulkan_scene_shader_catalog() -> &'static [BuiltinSceneShader] {
    BUILTIN_SCENE_SHADERS
}

pub fn native_vulkan_particle_compute_shader() -> &'static BuiltinParticleComputeShader {
    &BUILTIN_PARTICLE_COMPUTE_SHADER
}

pub fn native_vulkan_scene_shader_for_key(key: &str) -> Option<&'static BuiltinSceneShader> {
    BUILTIN_SCENE_SHADERS
        .iter()
        .find(|shader| shader.key == key)
}

pub fn native_vulkan_scene_vertex_spirv_for_primitive(
    shader: &'static BuiltinSceneShader,
    primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive,
) -> Option<&'static [u32]> {
    if shader.vertex_primitive == primitive {
        return Some(shader.vertex_spirv);
    }
    (primitive == crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh)
        .then_some(shader.object_mesh_vertex_spirv)
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_catalog_resolves_we_material_names_without_runtime_files() {
        let shader = native_vulkan_scene_shader_for_key("we/genericimage4")
            .expect("genericimage4 built-in shader");
        assert_eq!(shader.key, "we/genericimage4");
        assert_eq!(
            shader.parameter_layout,
            BuiltinSceneParameterLayout::StandardMaterial
        );
        assert_eq!(
            shader.vertex_primitive,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh
        );
        assert!(!shader.vertex_spirv.is_empty());
        assert!(shader.object_mesh_vertex_spirv.is_none());
        assert!(!shader.fragment_spirv.is_empty());
        assert!(shader.input_attachment_fragment_spirv.is_none());
        assert!(native_vulkan_scene_shader_for_key("missing-shader").is_none());
        assert!(native_vulkan_scene_shader_for_key("genericimage4").is_none());
        assert!(native_vulkan_scene_shader_for_key("WE/genericimage4").is_none());
        assert!(native_vulkan_scene_shader_for_key(" we/genericimage4").is_none());
    }

    #[test]
    fn effect_catalog_carries_distinct_fullscreen_and_object_mesh_vertex_domains() {
        let shimmer = native_vulkan_scene_shader_for_key("effects/shimmer__SLOTS_9")
            .expect("shimmer shader");
        assert_eq!(
            shimmer.vertex_primitive,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        );
        let object_mesh = native_vulkan_scene_vertex_spirv_for_primitive(
            shimmer,
            crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .expect("shimmer object-mesh vertex shader");
        assert_ne!(object_mesh, shimmer.vertex_spirv);
        assert_eq!(
            native_vulkan_scene_vertex_spirv_for_primitive(
                shimmer,
                crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            ),
            Some(shimmer.vertex_spirv)
        );

        let iris = native_vulkan_scene_shader_for_key("effects/iris__SLOTS_3__MASK_1")
            .expect("iris shader");
        assert!(
            native_vulkan_scene_vertex_spirv_for_primitive(
                iris,
                crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            )
            .is_none()
        );
    }

    #[test]
    fn passthrough_catalog_exposes_only_the_explicit_exact_pixel_variant() {
        let passthrough = native_vulkan_scene_shader_for_key("we/passthrough")
            .expect("passthrough shader");
        let variant = passthrough
            .input_attachment_fragment_spirv
            .expect("exact-pixel input-attachment variant");
        assert!(!variant.is_empty());
        assert!(native_vulkan_scene_shader_for_key("we/genericimage4")
            .expect("generic image shader")
            .input_attachment_fragment_spirv
            .is_none());
    }

    #[test]
    fn shader_catalog_carries_typed_effect_parameter_layouts() {
        assert_eq!(
            native_vulkan_scene_shader_for_key("effects/iris__SLOTS_3__MASK_1")
                .expect("iris shader")
                .parameter_layout,
            BuiltinSceneParameterLayout::Iris
        );
        assert_eq!(
            native_vulkan_scene_shader_for_key("effects/waterwaves__SLOTS_1")
                .expect("waterwaves shader")
                .parameter_layout,
            BuiltinSceneParameterLayout::WaterWaves
        );
        assert_eq!(
            native_vulkan_scene_shader_for_key("effects/foliagesway__SLOTS_1")
                .expect("foliage sway shader")
                .parameter_layout,
            BuiltinSceneParameterLayout::FoliageSway
        );
        let rounded = native_vulkan_scene_shader_for_key(
            "effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1",
        )
        .expect("rounded mask shader");
        assert_eq!(
            rounded.parameter_layout,
            BuiltinSceneParameterLayout::RoundedMask
        );
        assert!(rounded.fragment_spirv.len() > 200);
        for (key, layout) in [
            (
                "effects/auto_sway__SLOTS_1__DEBUG_0__DEBUG_NO_ALPHA_1__NODE_COUNT_4",
                BuiltinSceneParameterLayout::AutoSway,
            ),
            (
                "effects/blur_combine__SLOTS_5__BLENDMODE_1__COMPOSITE_1",
                BuiltinSceneParameterLayout::BlurCombine,
            ),
            (
                "effects/blur_gaussian__SLOTS_1__VERTICAL_1",
                BuiltinSceneParameterLayout::BlurGaussian,
            ),
            (
                "effects/procedural_noise__SLOTS_1__AA_CATEGORY_1__BLENDMODE_20__STEPANIM_1",
                BuiltinSceneParameterLayout::ProceduralNoise,
            ),
            (
                "effects/simple_audio_bars__SLOTS_1__SHAPE_7",
                BuiltinSceneParameterLayout::AudioBars,
            ),
            (
                "effects/scroll__SLOTS_1",
                BuiltinSceneParameterLayout::Scroll,
            ),
            ("effects/skew__SLOTS_1", BuiltinSceneParameterLayout::Skew),
            (
                "effects/waterflow__SLOTS_7",
                BuiltinSceneParameterLayout::WaterFlow,
            ),
            (
                "effects/tech_circle__SLOTS_1__SECTOR_SEGMENTS_1",
                BuiltinSceneParameterLayout::TechCircle,
            ),
            (
                "we/puppet-waterwaves-direct__STAGES_7",
                BuiltinSceneParameterLayout::WaterWavesDirect,
            ),
            (
                "we/effect-waterwaves-direct__STAGES_6",
                BuiltinSceneParameterLayout::WaterWavesDirect,
            ),
        ] {
            let shader = native_vulkan_scene_shader_for_key(key).expect("object-local shader");
            assert_eq!(shader.parameter_layout, layout);
            assert!(shader.fragment_spirv.len() > 200);
        }
    }

    #[test]
    fn effect_target_waterwaves_identity_is_not_emitted_for_mesh_composites() {
        let effect = native_vulkan_scene_shader_for_key(
            "we/effect-waterwaves-direct__STAGES_6",
        )
        .expect("effect-target waterwaves shader");
        let image = native_vulkan_scene_shader_for_key(
            "we/image-waterwaves-direct__STAGES_6",
        )
        .expect("image waterwaves shader");
        let puppet = native_vulkan_scene_shader_for_key(
            "we/puppet-waterwaves-direct__STAGES_6",
        )
        .expect("puppet waterwaves shader");

        assert_ne!(effect.fragment_spirv, image.fragment_spirv);
        assert_eq!(image.fragment_spirv, puppet.fragment_spirv);

        let effect_two = native_vulkan_scene_shader_for_key(
            "we/effect-waterwaves-direct__STAGES_2",
        )
        .expect("two-stage effect-target waterwaves shader");
        let image_two = native_vulkan_scene_shader_for_key(
            "we/image-waterwaves-direct__STAGES_2",
        )
        .expect("two-stage image waterwaves shader");
        assert_eq!(effect_two.fragment_spirv, image_two.fragment_spirv);
    }
}
