//! Canonical caustics shader specialization from immutable material inputs.

use crate::convert::we_ingest::ir::{WeIrMaterialConstant, WeIrMaterialTexture};

pub(super) const CAUSTICS_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6";
pub(super) const CAUSTICS_CHROMATIC_ZERO_SHADER: &str =
    "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_CHROMATIC_ZERO_1";
pub(super) const CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_CHROMATIC_ZERO_1__GILDER_PATTERN_GLOW_SHARED_1";
const CHROMATIC_ABERRATION: &str = "ui_editor_properties_chromatic_aberration";

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
    if material_slots_bind_same_resource(textures, 2, 5) {
        CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_SHADER.to_owned()
    } else {
        CAUSTICS_CHROMATIC_ZERO_SHADER.to_owned()
    }
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
