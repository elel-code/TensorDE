//! Built-in scene shader catalog.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/shader-conventions.md`
//! - `reverse-engineered/gilder/shaders/**`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinSceneDescriptorBindingKind {
    InputAttachment,
    SampledImage,
    StorageImage,
    Sampler,
    UniformBuffer,
    StorageBuffer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinSceneDescriptorBinding {
    pub kind: BuiltinSceneDescriptorBindingKind,
    pub register: u32,
    pub push_offset: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct BuiltinSceneVertexShader {
    pub spirv: &'static [u32],
    pub push_constant_bytes: u32,
    pub bindings: &'static [BuiltinSceneDescriptorBinding],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinSceneParameterLayout {
    None,
    Blend,
    BlendGradient,
    BlurCombine,
    BlurGaussian,
    Particle,
    StandardMaterial,
    SceneColorBlend,
    Iris,
    Opacity,
    RoundedMask,
    Scroll,
    Skew,
    Spin,
    Shimmer,
    Swing,
    Tint,
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
            Self::Blend
                | Self::BlendGradient
                | Self::FinalEffectProgram
                | Self::Iris
                | Self::Particle
                | Self::RoundedMask
                | Self::Scroll
                | Self::Spin
                | Self::Skew
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
            Self::Blend
                | Self::BlendGradient
                | Self::BlurCombine
                | Self::BlurGaussian
                | Self::Iris
                | Self::Particle
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
                | Self::Shimmer
                | Self::Swing
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
    native_vulkan_scene_vertex_shader_for_primitive(shader, primitive)
        .map(|vertex| vertex.spirv)
}

pub fn native_vulkan_scene_vertex_shader_for_primitive(
    shader: &'static BuiltinSceneShader,
    primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive,
) -> Option<BuiltinSceneVertexShader> {
    if shader.vertex_primitive == primitive {
        return Some(shader.vertex);
    }
    (primitive == crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh)
        .then_some(shader.object_mesh_vertex)
        .flatten()
}

#[cfg(test)]
#[path = "shader_catalog/tests.rs"]
mod tests;
