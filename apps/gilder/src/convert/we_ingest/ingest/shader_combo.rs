//! Typed Wallpaper Engine shader combo declarations.
//!
//! Shader source `[COMBO]` declarations define the ABI distinction between an
//! omitted override and an explicit value equal to zero.

use serde_json::Value;

use crate::convert::we_ingest::ir::WeIrShaderComboDefinition;

pub(super) fn parse_shader_combo_definitions(
    shader_key: &str,
    source: &str,
) -> Vec<WeIrShaderComboDefinition> {
    source
        .lines()
        .filter_map(|line| {
            line.split_once("[COMBO]")
                .map(|(_, declaration)| declaration)
        })
        .filter_map(|declaration| declaration.find('{').map(|start| &declaration[start..]))
        .filter_map(|json| serde_json::from_str::<Value>(json).ok())
        .filter_map(|declaration| {
            let name = declaration.get("combo")?.as_str()?.trim();
            let default_value = integer_value(declaration.get("default")?)?;
            (!name.is_empty()).then(|| WeIrShaderComboDefinition {
                shader_key: shader_key.to_owned(),
                name: name.to_owned(),
                default_value,
            })
        })
        .collect()
}

fn integer_value(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| {
            value.as_f64().and_then(|value| {
                (value.is_finite() && value.fract() == 0.0).then_some(value as i64)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numeric_combo_defaults_from_shader_comments() {
        let source = r#"
// [COMBO] {"material":"Transparency only","combo":"C_ALPHA_ONLY","type":"options","default":1}
// [COMBO] {"material":"Keep Square","combo":"B_SQUARE","type":"options","default":1.0}
// unrelated
"#;

        assert_eq!(
            parse_shader_combo_definitions("effects/rounded_mask", source),
            vec![
                WeIrShaderComboDefinition {
                    shader_key: "effects/rounded_mask".to_owned(),
                    name: "C_ALPHA_ONLY".to_owned(),
                    default_value: 1,
                },
                WeIrShaderComboDefinition {
                    shader_key: "effects/rounded_mask".to_owned(),
                    name: "B_SQUARE".to_owned(),
                    default_value: 1,
                },
            ]
        );
    }
}
