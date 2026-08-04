//! Shader-declared Wallpaper Engine texture defaults.
//!
//! Effect instance JSON contains overrides, while the authored shader is the
//! authority for ordinary assets such as `util/noise` and hidden bindings such
//! as `_rt_FullFrameBuffer`. Parse these on the converter cold path so the
//! render graph never has to infer descriptors.

use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ShaderTextureDefault {
    pub slot: u32,
    pub target: String,
    requirements: Vec<(String, i64)>,
}

pub(super) fn parse_shader_texture_defaults(
    source: &str,
) -> Result<Vec<ShaderTextureDefault>, String> {
    let mut defaults = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let Some((declaration, annotation)) = line.split_once("//") else {
            continue;
        };
        let declaration = declaration.trim();
        let Some(name) = declaration
            .strip_prefix("uniform sampler2D ")
            .and_then(|declaration| declaration.split(';').next())
            .map(str::trim)
        else {
            continue;
        };
        let Some(slot) = name
            .strip_prefix("g_Texture")
            .and_then(|slot| slot.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(json_start) = annotation.find('{') else {
            continue;
        };
        let json_end = annotation.rfind('}').ok_or_else(|| {
            format!(
                "shader sampler annotation on line {} is missing its closing brace",
                line_index + 1
            )
        })?;
        let metadata: Value =
            serde_json::from_str(&annotation[json_start..=json_end]).map_err(|source| {
                format!(
                    "invalid shader sampler annotation on line {}: {source}",
                    line_index + 1
                )
            })?;
        let Some(target) = metadata.get("default").and_then(Value::as_str) else {
            continue;
        };
        let mut requirements = Vec::new();
        if let Some(require) = metadata.get("require") {
            let require = require.as_object().ok_or_else(|| {
                format!(
                    "shader sampler requirement on line {} must be an object",
                    line_index + 1
                )
            })?;
            for (name, value) in require {
                let value = integer_value(value).ok_or_else(|| {
                    format!(
                        "shader sampler requirement {name:?} on line {} must be an integer",
                        line_index + 1
                    )
                })?;
                requirements.push((name.clone(), value));
            }
        }
        defaults.push(ShaderTextureDefault {
            slot,
            target: target.to_owned(),
            requirements,
        });
    }
    Ok(defaults)
}

pub(super) fn apply_shader_texture_defaults(
    defaults: &[ShaderTextureDefault],
    combos: &BTreeMap<String, i64>,
    combo_defaults: &BTreeMap<String, i64>,
    bindings: &mut BTreeMap<u32, String>,
) {
    for default in defaults
        .iter()
        .filter(|default| default.is_enabled(combos, combo_defaults))
    {
        bindings
            .entry(default.slot)
            .or_insert_with(|| default.target.clone());
    }
}

impl ShaderTextureDefault {
    fn is_enabled(
        &self,
        combos: &BTreeMap<String, i64>,
        combo_defaults: &BTreeMap<String, i64>,
    ) -> bool {
        self.requirements.iter().all(|(name, expected)| {
            combo_value(combos, name)
                .or_else(|| combo_value(combo_defaults, name))
                .unwrap_or(0)
                == *expected
        })
    }
}

fn combo_value(values: &BTreeMap<String, i64>, name: &str) -> Option<i64> {
    values
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
        .map(|(_, value)| *value)
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
    fn parses_runtime_and_ordinary_texture_defaults_with_requirements() {
        let source = r#"
uniform sampler2D g_Texture0; // {"material":"framebuffer","hidden":true}
uniform sampler2D g_Texture2; // {"default":"_rt_FullFrameBuffer","hidden":true,"material":"backgroundTexture"}
uniform sampler2D g_Texture3; // {"default":"gradient/test","require":{"GRADIENT":1}}
"#;

        assert_eq!(
            parse_shader_texture_defaults(source).expect("texture defaults"),
            vec![
                ShaderTextureDefault {
                    slot: 2,
                    target: "_rt_FullFrameBuffer".to_owned(),
                    requirements: Vec::new(),
                },
                ShaderTextureDefault {
                    slot: 3,
                    target: "gradient/test".to_owned(),
                    requirements: vec![("GRADIENT".to_owned(), 1)],
                },
            ]
        );
    }

    #[test]
    fn applies_texture_default_only_when_authored_combo_requirement_is_active() {
        let defaults = parse_shader_texture_defaults(
            r#"uniform sampler2D g_Texture4; // {"default":"_rt_mask","require":{"MASK":1}}"#,
        )
        .expect("texture defaults");
        let mut bindings = BTreeMap::new();

        apply_shader_texture_defaults(
            &defaults,
            &BTreeMap::new(),
            &[("MASK".to_owned(), 0)].into_iter().collect(),
            &mut bindings,
        );
        assert!(bindings.is_empty());

        apply_shader_texture_defaults(
            &defaults,
            &[("mask".to_owned(), 1)].into_iter().collect(),
            &BTreeMap::new(),
            &mut bindings,
        );
        assert_eq!(bindings.get(&4).map(String::as_str), Some("_rt_mask"));
    }

    #[test]
    fn rejects_malformed_sampler_metadata_instead_of_silently_dropping_it() {
        let error = parse_shader_texture_defaults(
            r#"uniform sampler2D g_Texture2; // {"default":"_rt_framebuffer""#,
        )
        .expect_err("malformed metadata must fail");

        assert!(error.contains("closing brace"));
    }
}
