//! Canonical caustics shader specialization from immutable material inputs.

use crate::convert::we_ingest::ir::{WeIrMaterialConstant, WeIrMaterialTexture};

pub(super) const CAUSTICS_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6";
pub(super) const CAUSTICS_CHROMATIC_ZERO_SHADER: &str =
    "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_CHROMATIC_ZERO_1";
pub(super) const CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_CHROMATIC_ZERO_1__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1";
pub(super) const CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_COLOR_EQUAL_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_CHROMATIC_ZERO_1__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1__TENSOR_WALLPAPER_COLOR_EQUAL_1";
const CHROMATIC_ABERRATION: &str = "ui_editor_properties_chromatic_aberration";
const COLOR_START: &str = "ui_editor_properties_color_start";
const COLOR_END: &str = "ui_editor_properties_color_end";

pub(super) fn specialize_caustics_shader(
    shader: &str,
    constants: &[WeIrMaterialConstant],
    textures: &[WeIrMaterialTexture],
) -> String {
    if shader != CAUSTICS_SHADER
        || !material_static_scalar_equals(constants, CHROMATIC_ABERRATION, 0.0)
    {
        return shader.to_owned();
    }
    let shared = material_slots_bind_same_resource(textures, 2, 5);
    let color_equal = material_static_rgb_equal(constants, COLOR_START, COLOR_END);
    if shared && color_equal {
        CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_COLOR_EQUAL_SHADER.to_owned()
    } else if shared {
        CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_SHADER.to_owned()
    } else {
        CAUSTICS_CHROMATIC_ZERO_SHADER.to_owned()
    }
}

fn material_static_rgb_equal(
    constants: &[WeIrMaterialConstant],
    left_name: &str,
    right_name: &str,
) -> bool {
    let Some(left) = material_static_rgb(constants, left_name) else {
        return false;
    };
    let Some(right) = material_static_rgb(constants, right_name) else {
        return false;
    };
    left.iter().zip(right).all(|(a, b)| (a - b).abs() <= 1.0e-7)
}

fn material_static_rgb(constants: &[WeIrMaterialConstant], name: &str) -> Option<[f32; 3]> {
    let raw = constants
        .iter()
        .find(|constant| constant.name == name)?
        .value_json
        .trim();
    // Authored `"1 1 1"` or JSON array forms only; user-property objects reject.
    if raw.starts_with('{') {
        return None;
    }
    let cleaned = raw.trim_matches(|c| c == '"' || c == '[' || c == ']');
    let mut parts = cleaned
        .split([' ', ',', '\t'])
        .filter(|part| !part.is_empty());
    let r = parts.next()?.parse::<f32>().ok()?;
    let g = parts.next()?.parse::<f32>().ok()?;
    let b = parts.next()?.parse::<f32>().ok()?;
    if parts.next().is_some() || ![r, g, b].iter().all(|v| v.is_finite()) {
        return None;
    }
    Some([r, g, b])
}

fn material_slots_bind_same_resource(
    textures: &[WeIrMaterialTexture],
    left_slot: u32,
    right_slot: u32,
) -> bool {
    let resource_at = |slot| {
        textures
            .iter()
            .find(|texture| texture.slot == slot)
            .and_then(|texture| texture.resource)
    };
    resource_at(left_slot)
        .zip(resource_at(right_slot))
        .is_some_and(|(left, right)| left == right)
}

pub(super) fn material_static_scalar_equals(
    constants: &[WeIrMaterialConstant],
    name: &str,
    expected: f32,
) -> bool {
    constants
        .iter()
        .find(|constant| constant.name == name)
        .and_then(|constant| constant.value_json.trim().parse::<f32>().ok())
        .is_some_and(|value| value.is_finite() && (value - expected).abs() <= 1.0e-7)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(name: &str, value_json: &str) -> WeIrMaterialConstant {
        WeIrMaterialConstant {
            name: name.to_owned(),
            value_json: value_json.to_owned(),
        }
    }

    fn texture(slot: u32, resource: u32) -> WeIrMaterialTexture {
        WeIrMaterialTexture {
            slot,
            resource: Some(resource),
            path: format!("texture-{resource}"),
        }
    }

    #[test]
    fn numeric_zero_selects_the_chromatic_zero_variant() {
        assert_eq!(
            specialize_caustics_shader(
                CAUSTICS_SHADER,
                &[constant(CHROMATIC_ABERRATION, "0")],
                &[],
            ),
            CAUSTICS_CHROMATIC_ZERO_SHADER
        );
    }

    #[test]
    fn identical_pattern_and_glow_resources_select_one_sample_variant() {
        assert_eq!(
            specialize_caustics_shader(
                CAUSTICS_SHADER,
                &[constant(CHROMATIC_ABERRATION, "0")],
                &[texture(2, 41), texture(5, 41)],
            ),
            CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_SHADER
        );
        assert_eq!(
            specialize_caustics_shader(
                CAUSTICS_SHADER,
                &[constant(CHROMATIC_ABERRATION, "0")],
                &[texture(2, 41), texture(5, 42)],
            ),
            CAUSTICS_CHROMATIC_ZERO_SHADER
        );
    }

    #[test]
    fn equal_static_color_ramp_selects_color_equal_shared_variant() {
        assert_eq!(
            specialize_caustics_shader(
                CAUSTICS_SHADER,
                &[
                    constant(CHROMATIC_ABERRATION, "0"),
                    constant(COLOR_START, "\"1 1 1\""),
                    constant(COLOR_END, "\"1 1 1\""),
                ],
                &[texture(2, 41), texture(5, 41)],
            ),
            CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_COLOR_EQUAL_SHADER
        );
        assert_eq!(
            specialize_caustics_shader(
                CAUSTICS_SHADER,
                &[
                    constant(CHROMATIC_ABERRATION, "0"),
                    constant(COLOR_START, "1 0 0"),
                    constant(COLOR_END, "0 1 0"),
                ],
                &[texture(2, 41), texture(5, 41)],
            ),
            CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_SHADER
        );
    }

    #[test]
    fn user_binding_is_not_treated_as_static_zero() {
        assert_eq!(
            specialize_caustics_shader(
                CAUSTICS_SHADER,
                &[constant(
                    CHROMATIC_ABERRATION,
                    r#"{"user":"chromatic","value":0}"#,
                )],
                &[texture(2, 41), texture(5, 41)],
            ),
            CAUSTICS_SHADER
        );
    }
}
