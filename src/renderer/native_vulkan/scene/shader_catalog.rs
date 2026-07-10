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
    StandardMaterial,
    Iris,
    Opacity,
    FoliageSway,
    WaterWaves,
    WaterRipple,
}

impl BuiltinSceneParameterLayout {
    pub const fn uses_material_uniform(self) -> bool {
        !matches!(self, Self::None)
    }

    pub const fn uses_effect_draw_uniform(self) -> bool {
        matches!(self, Self::Iris)
    }

    pub const fn uses_scene_time(self) -> bool {
        matches!(
            self,
            Self::Iris | Self::FoliageSway | Self::WaterWaves | Self::WaterRipple
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
            native_vulkan_scene_shader_for_key(
                "workshop/2790231929/effects/foliagesway__SLOTS_1"
            )
            .expect("foliage sway shader")
            .parameter_layout,
            BuiltinSceneParameterLayout::FoliageSway
        );
    }
}
