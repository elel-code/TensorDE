//! Shader-declared Wallpaper Engine built-in texture defaults.
//!
//! Effect instance JSON only serializes overrides. These defaults come from
//! the shader declarations and must become explicit typed-IR bindings before
//! graph and material lowering.

use std::collections::BTreeMap;

const WATER_CAUSTICS_TEXTURES: &[(u32, &str)] = &[
    (2, "pattern/voronoi_local"),
    (3, "util/uniform_256"),
    (4, "util/perlin_256"),
    (5, "pattern/voronoi"),
];
const CLOUD_MOTION_TEXTURES: &[(u32, &str)] = &[(2, "util/perlin_256")];
const FOLIAGE_SWAY_TEXTURES: &[(u32, &str)] = &[(2, "util/noise")];
const SHIMMER_TEXTURES: &[(u32, &str)] = &[(3, "gradient/gradient_ferro_fluid")];
const BLEND_GRADIENT_TEXTURES: &[(u32, &str)] = &[(2, "util/clouds_256")];

pub(super) fn apply_builtin_effect_texture_defaults(
    effect_file: &str,
    combos: &BTreeMap<String, i64>,
    bindings: &mut BTreeMap<u32, String>,
) {
    let normalized = effect_file.replace('\\', "/").to_ascii_lowercase();
    let defaults = if normalized.contains("watercaustics") || normalized.contains("water_caustics")
    {
        WATER_CAUSTICS_TEXTURES
    } else if normalized.ends_with("/cloudmotion/effect.json")
        || normalized == "effects/cloudmotion/effect.json"
    {
        CLOUD_MOTION_TEXTURES
    } else if (normalized.contains("foliagesway") || normalized.contains("foliage_sway"))
        && combos
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("MODE"))
            .is_none_or(|(_, mode)| *mode == 0)
    {
        FOLIAGE_SWAY_TEXTURES
    } else if normalized.ends_with("/shimmer/effect.json")
        || normalized == "effects/shimmer/effect.json"
    {
        SHIMMER_TEXTURES
    } else if normalized.contains("blendgradient") {
        BLEND_GRADIENT_TEXTURES
    } else {
        &[]
    };
    for (slot, path) in defaults {
        bindings.entry(*slot).or_insert_with(|| (*path).to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_caustics_fills_all_shader_declared_texture_slots() {
        let mut bindings = [(0, "previous".to_owned()), (5, "pattern/custom".to_owned())]
            .into_iter()
            .collect();
        apply_builtin_effect_texture_defaults(
            "effects/watercaustics/effect.json",
            &BTreeMap::new(),
            &mut bindings,
        );
        assert_eq!(
            bindings.get(&2).map(String::as_str),
            Some("pattern/voronoi_local")
        );
        assert_eq!(
            bindings.get(&3).map(String::as_str),
            Some("util/uniform_256")
        );
        assert_eq!(
            bindings.get(&4).map(String::as_str),
            Some("util/perlin_256")
        );
        assert_eq!(bindings.get(&5).map(String::as_str), Some("pattern/custom"));
    }

    #[test]
    fn cloud_motion_gets_its_perlin_source() {
        let mut bindings = BTreeMap::new();
        apply_builtin_effect_texture_defaults(
            "effects/cloudmotion/effect.json",
            &BTreeMap::new(),
            &mut bindings,
        );
        assert_eq!(
            bindings.get(&2).map(String::as_str),
            Some("util/perlin_256")
        );
    }

    #[test]
    fn shimmer_gets_its_shader_declared_gradient() {
        let mut bindings = BTreeMap::new();
        apply_builtin_effect_texture_defaults(
            "effects/shimmer/effect.json",
            &BTreeMap::new(),
            &mut bindings,
        );
        assert_eq!(
            bindings.get(&3).map(String::as_str),
            Some("gradient/gradient_ferro_fluid")
        );
    }

    #[test]
    fn blend_gradient_gets_its_shader_declared_noise() {
        let mut bindings = BTreeMap::new();
        apply_builtin_effect_texture_defaults(
            "effects/workshop/2962751255/blendgradient/effect.json",
            &BTreeMap::new(),
            &mut bindings,
        );
        assert_eq!(
            bindings.get(&2).map(String::as_str),
            Some("util/clouds_256")
        );
    }
}
