//! Strict authored uniform-to-material metadata parsing.

use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ShaderUniformMetadata {
    material_parameters: BTreeMap<String, String>,
}

impl ShaderUniformMetadata {
    pub(super) fn material_parameter(&self, uniform: &str) -> Option<&str> {
        self.material_parameters.get(uniform).map(String::as_str)
    }
}

pub(super) fn parse_shader_uniform_metadata(source: &str) -> Result<ShaderUniformMetadata, String> {
    let mut material_parameters = BTreeMap::new();
    for (line_index, line) in source.lines().enumerate() {
        let Some((declaration, annotation)) = line.split_once("//") else {
            continue;
        };
        let Some(name) = uniform_declaration_name(declaration) else {
            continue;
        };
        let Some(json_start) = annotation.find('{') else {
            continue;
        };
        let json_end = annotation.rfind('}').ok_or_else(|| {
            format!(
                "shader uniform annotation on line {} is missing its closing brace",
                line_index + 1
            )
        })?;
        if !annotation[json_end + 1..].trim().is_empty() {
            return Err(format!(
                "shader uniform annotation on line {} has trailing content",
                line_index + 1
            ));
        }
        let metadata: Value =
            serde_json::from_str(&annotation[json_start..=json_end]).map_err(|error| {
                format!(
                    "invalid shader uniform annotation on line {}: {error}",
                    line_index + 1
                )
            })?;
        let Some(material) = metadata.get("material") else {
            continue;
        };
        let material = material.as_str().ok_or_else(|| {
            format!(
                "shader uniform material on line {} must be a string",
                line_index + 1
            )
        })?;
        if material.is_empty() {
            return Err(format!(
                "shader uniform material on line {} must not be empty",
                line_index + 1
            ));
        }
        if material_parameters
            .insert(name.to_owned(), material.to_owned())
            .is_some()
        {
            return Err(format!(
                "shader uniform {name:?} repeats authored material metadata"
            ));
        }
    }
    Ok(ShaderUniformMetadata {
        material_parameters,
    })
}

fn uniform_declaration_name(declaration: &str) -> Option<&str> {
    let declaration = declaration.trim();
    let declaration = declaration.strip_prefix("uniform ")?;
    let declaration = declaration.strip_suffix(';')?.trim();
    let name = declaration.split_ascii_whitespace().next_back()?;
    (!name.is_empty() && !name.contains(['[', ']'])).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exact_authored_material_aliases_without_prefix_inference() {
        let metadata = parse_shader_uniform_metadata(
            r#"
uniform mat4 g_ModelViewProjectionMatrix;
uniform vec2 u_Size; // {"material":"Size","default":"1 1"}
uniform vec2 g_Offset; // {"material":"offset"}
uniform float g_Direction; // {"material":"angle"}
uniform vec4 g_Texture0Resolution;
"#,
        )
        .expect("uniform metadata");

        assert_eq!(metadata.material_parameter("u_Size"), Some("Size"));
        assert_eq!(metadata.material_parameter("g_Offset"), Some("offset"));
        assert_eq!(metadata.material_parameter("g_Direction"), Some("angle"));
        assert_eq!(
            metadata.material_parameter("g_ModelViewProjectionMatrix"),
            None
        );
        assert_eq!(metadata.material_parameter("g_Texture0Resolution"), None);
    }

    #[test]
    fn rejects_invalid_duplicate_and_non_string_metadata() {
        let invalid = "uniform float value; // {\"material\":}";
        assert!(parse_shader_uniform_metadata(invalid).is_err());

        let duplicate = r#"
uniform float value; // {"material":"first"}
uniform float value; // {"material":"second"}
"#;
        assert!(parse_shader_uniform_metadata(duplicate).is_err());

        let non_string = "uniform float value; // {\"material\":7}";
        assert!(parse_shader_uniform_metadata(non_string).is_err());
    }
}
