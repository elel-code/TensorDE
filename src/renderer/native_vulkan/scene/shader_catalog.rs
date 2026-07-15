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
    StandardMaterial,
    Iris,
    Opacity,
    RoundedMask,
    Scroll,
    Skew,
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
                | Self::FinalEffectProgram
                | Self::Iris
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
                | Self::Iris
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

pub fn native_vulkan_scene_shader_for_key(key: &str) -> Option<&'static BuiltinSceneShader> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    find_builtin_scene_shader(key)
        .or_else(|| find_we_builtin_scene_shader(key))
        .or_else(|| {
            key.rsplit_once('/')
                .and_then(|(_, basename)| find_builtin_scene_shader(basename))
        })
        .or_else(|| {
            key.rsplit_once('/')
                .and_then(|(_, basename)| find_we_builtin_scene_shader(basename))
        })
}

fn find_builtin_scene_shader(key: &str) -> Option<&'static BuiltinSceneShader> {
    BUILTIN_SCENE_SHADERS
        .iter()
        .find(|shader| shader.key.eq_ignore_ascii_case(key))
}

fn find_we_builtin_scene_shader(key: &str) -> Option<&'static BuiltinSceneShader> {
    if key.contains('/') {
        return None;
    }
    let we_key = format!("we/{key}");
    find_builtin_scene_shader(&we_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_catalog_resolves_we_material_names_without_runtime_files() {
        let shader = native_vulkan_scene_shader_for_key("genericimage4")
            .expect("genericimage4 built-in shader");
        assert_eq!(shader.key, "we/genericimage4");
        assert_eq!(
            shader.parameter_layout,
            BuiltinSceneParameterLayout::StandardMaterial
        );
        assert!(!shader.vertex_spirv.is_empty());
        assert!(!shader.fragment_spirv.is_empty());
        assert!(native_vulkan_scene_shader_for_key("missing-shader").is_none());
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
            native_vulkan_scene_shader_for_key("workshop/2790231929/effects/foliagesway__SLOTS_1")
                .expect("foliage sway shader")
                .parameter_layout,
            BuiltinSceneParameterLayout::FoliageSway
        );
        let rounded = native_vulkan_scene_shader_for_key(
            "workshop/3083593512/effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1",
        )
        .expect("rounded mask shader");
        assert_eq!(
            rounded.parameter_layout,
            BuiltinSceneParameterLayout::RoundedMask
        );
        assert!(rounded.fragment_spirv.len() > 200);
        for (key, layout) in [
            (
                "workshop/3082978660/effects/Simple_Audio_Bars__SLOTS_1__SHAPE_7",
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
                "workshop/2123274886/effects/tech_circle__SLOTS_1__SECTOR_SEGMENTS_1",
                BuiltinSceneParameterLayout::TechCircle,
            ),
            (
                "we/puppet-waterwaves-direct",
                BuiltinSceneParameterLayout::WaterWavesDirect,
            ),
        ] {
            let shader = native_vulkan_scene_shader_for_key(key).expect("object-local shader");
            assert_eq!(shader.parameter_layout, layout);
            assert!(shader.fragment_spirv.len() > 200);
        }
    }
}
