//! Built-in scene shader catalog.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/shader-conventions.md`
//! - `reverse-engineered/gilder/shaders/**`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinSceneDescriptorHeapMode {
    Mapped,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinSceneParameterLayout {
    None,
    AudioBars,
    AudioLine,
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
    Spin,
    Shimmer,
    Swing,
    TechCircle,
    Tint,
    Caustics,
    CloudMotion,
    ClippingMask,
    ColorKey,
    CustomUserTexture,
    FoliageSway,
    FoliageRippleComposite,
    FinalEffectProgram,
    FinalWaterRipple,
    FinalWaterWaves,
    GradientColor,
    Ring,
    RippleFlowComposite,
    Shake,
    Sphere,
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
                | Self::Spin
                | Self::Skew
                | Self::TechCircle
                | Self::Tint
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
                | Self::AudioLine
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
                | Self::GradientColor
                | Self::Ring
                | Self::FoliageSway
                | Self::FoliageRippleComposite
                | Self::FinalEffectProgram
                | Self::FinalWaterRipple
                | Self::FinalWaterWaves
                | Self::RippleFlowComposite
                | Self::Shake
                | Self::Sphere
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
#[path = "shader_catalog/tests.rs"]
mod tests;
