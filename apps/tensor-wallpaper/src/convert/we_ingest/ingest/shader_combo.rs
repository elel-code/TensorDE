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
        .filter_map(|declaration| parse_shader_combo_declaration(declaration).ok())
        .map(|(name, default_value)| WeIrShaderComboDefinition {
            shader_key: shader_key.to_owned(),
            name,
            default_value,
        })
        .collect()
}

pub(super) fn parse_shader_combo_declaration(declaration: &str) -> Result<(String, i64), String> {
    match serde_json::from_str::<Value>(declaration) {
        Ok(value) => combo_name_and_default(&value),
        Err(full_error) => {
            let mut projections =
                declaration
                    .match_indices("\"combo\"")
                    .filter_map(|(start, _)| {
                        let projected = format!("{{{}", &declaration[start..]);
                        serde_json::from_str::<Value>(&projected)
                            .ok()
                            .and_then(|value| combo_name_and_default(&value).ok())
                    });
            let Some(projection) = projections.next() else {
                return Err(full_error.to_string());
            };
            if projections.next().is_some() {
                return Err("combo declaration has ambiguous machine-field projections".to_owned());
            }
            Ok(projection)
        }
    }
}

fn combo_name_and_default(value: &Value) -> Result<(String, i64), String> {
    let name = value
        .get("combo")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "combo declaration has no combo name".to_owned())?;
    let default_value = value
        .get("default")
        .and_then(integer_value)
        .ok_or_else(|| format!("combo {name} has no integer default"))?;
    Ok((name.to_owned(), default_value))
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

    #[test]
    fn parses_an_exact_machine_suffix_after_malformed_ui_metadata() {
        let declaration = r#"{"material":"missing quote,"combo":"VERTICAL_RELATIVE_WIDTH","type":"options","default":1,"require":{"VERTICAL_AUTO_MASK":1}}"#;

        assert_eq!(
            parse_shader_combo_declaration(declaration).expect("machine projection"),
            ("VERTICAL_RELATIVE_WIDTH".to_owned(), 1)
        );
    }

    #[test]
    fn rejects_a_malformed_machine_suffix() {
        let declaration =
            r#"{"material":"missing quote,"combo":"CATEGORY","default":0,"options":{Color:0}}"#;

        assert!(parse_shader_combo_declaration(declaration).is_err());
    }
}
