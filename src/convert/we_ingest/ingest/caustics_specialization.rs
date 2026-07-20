//! Canonical caustics shader specialization from immutable material inputs.

use crate::convert::we_ingest::ir::WeIrMaterialConstant;

pub(super) const CAUSTICS_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6";
pub(super) const CAUSTICS_CHROMATIC_ZERO_SHADER: &str =
    "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_CHROMATIC_ZERO_1";
const CHROMATIC_ABERRATION: &str = "ui_editor_properties_chromatic_aberration";

pub(super) fn specialize_caustics_shader(
    shader: &str,
    constants: &[WeIrMaterialConstant],
) -> String {
    if shader != CAUSTICS_SHADER
        || !material_static_scalar_equals(constants, CHROMATIC_ABERRATION, 0.0)
    {
        return shader.to_owned();
    }
    CAUSTICS_CHROMATIC_ZERO_SHADER.to_owned()
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

    #[test]
    fn numeric_zero_selects_the_chromatic_zero_variant() {
        assert_eq!(
            specialize_caustics_shader(CAUSTICS_SHADER, &[constant(CHROMATIC_ABERRATION, "0")],),
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
            ),
            CAUSTICS_SHADER
        );
    }
}
