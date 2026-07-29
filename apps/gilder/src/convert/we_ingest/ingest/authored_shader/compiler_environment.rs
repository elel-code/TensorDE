//! Strict Wallpaper Engine combo and compiler-macro environment.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{AuthoredProgramSpec, shader_error};
use crate::convert::we_ingest::ingest::WeIngestError;

pub(super) fn compiler_definitions(
    spec: &AuthoredProgramSpec,
    sources: [&str; 2],
) -> Result<Vec<(String, String)>, WeIngestError> {
    let mut values = BTreeMap::<String, i64>::new();
    let mut authored_defaults = BTreeMap::<String, i64>::new();
    let mut invalid_declarations = Vec::new();
    for source in sources {
        let declarations = combo_default_declarations(source);
        invalid_declarations.extend(declarations.invalid);
        for (name, default) in declarations.valid {
            if let Some(previous) = values.insert(name.clone(), default)
                && previous != default
            {
                return Err(shader_error(
                    &spec.program_key,
                    "program",
                    format!("combo {name} has conflicting defaults {previous} and {default}"),
                ));
            }
            authored_defaults.insert(name, default);
        }
        for (name, slot) in sampler_combo_slots(source, &spec.program_key)? {
            values.insert(name, i64::from(spec.texture_slot_mask & (1 << slot) != 0));
        }
    }
    for invalid in invalid_declarations {
        let duplicate = combo_default_hint(&invalid.declaration)
            .is_some_and(|(name, default)| authored_defaults.get(name) == Some(&default));
        if !duplicate {
            return Err(shader_error(
                &spec.program_key,
                "program",
                format!(
                    "invalid combo declaration on line {}: {}",
                    invalid.line_number, invalid.error
                ),
            ));
        }
    }
    for (name, value) in variant_overrides(spec)? {
        if name != "SLOTS" && !values.contains_key(&name) {
            return Err(shader_error(
                &spec.program_key,
                "program",
                format!("specialization {name} has no authored declaration"),
            ));
        }
        values.insert(name, value);
    }
    values.insert("HLSL".to_owned(), 1);
    values.insert("SLOTS".to_owned(), i64::from(spec.texture_slot_mask));
    Ok(values
        .into_iter()
        .map(|(name, value)| (name, value.to_string()))
        .collect())
}

pub(super) fn remove_unreferenced_frontend_declarations(
    source: &str,
    preprocessed: &str,
) -> String {
    source
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            let uniform = trimmed.strip_prefix("uniform ").and_then(|declaration| {
                declaration
                    .split(';')
                    .next()
                    .and_then(|declaration| declaration.split_ascii_whitespace().next_back())
                    .filter(|name| !name.contains(['[', ']']))
            });
            let stage_io = (trimmed.starts_with("layout(")
                && (trimmed.contains(") in ") || trimmed.contains(") out ")))
            .then(|| {
                trimmed
                    .strip_suffix(';')
                    .and_then(|declaration| declaration.split_whitespace().next_back())
            })
            .flatten();
            let Some(name) = uniform
                .or(stage_io)
                .map(str::trim)
                .filter(|name| !name.is_empty())
            else {
                return line;
            };
            if identifier_occurrence_count(preprocessed, name) <= 1 {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn identifier_occurrence_count(source: &str, identifier: &str) -> usize {
    source
        .match_indices(identifier)
        .filter(|(start, _)| {
            let end = start + identifier.len();
            let before = source[..*start].bytes().next_back();
            let after = source[end..].bytes().next();
            before.is_none_or(|byte| !is_identifier_byte(byte))
                && after.is_none_or(|byte| !is_identifier_byte(byte))
        })
        .count()
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub(super) fn inject_we_compiler_preamble(source: &str) -> String {
    const PREAMBLE: &str = "\
#define CAST2(v) vec2(v, v)\n\
#define CAST3(v) vec3(v, v, v)\n\
#define CAST4(v) vec4(v, v, v, v)\n\
#define CAST3X3(v) mat3(v)\n\
#define CASTF(v) float(v)\n\
#define CASTU(v) uint(v)\n\
#define texSample2D(s, uv) texture2D(s, uv)\n\
#define texSample2DLod(s, uv, lod) texture2DLod(s, uv, lod)\n\
#define texSample3D(s, uvw) texture3D(s, uvw)\n\
#define mul(a, b) ((b) * (a))\n\
#define frac(v) fract(v)\n\
#define saturate(v) clamp(v, 0.0, 1.0)";

    let mut lines = source.lines();
    let Some(first) = lines.next() else {
        return PREAMBLE.to_owned();
    };
    if first.trim_start().starts_with("#version") {
        let rest = lines.collect::<Vec<_>>().join("\n");
        format!("{first}\n{PREAMBLE}\n{rest}")
    } else {
        format!("{PREAMBLE}\n{source}")
    }
}

struct ComboDefaultDeclarations {
    valid: Vec<(String, i64)>,
    invalid: Vec<InvalidComboDeclaration>,
}

struct InvalidComboDeclaration {
    line_number: usize,
    declaration: String,
    error: String,
}

fn combo_default_declarations(source: &str) -> ComboDefaultDeclarations {
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let Some((_, declaration)) = line.split_once("[COMBO]") else {
            continue;
        };
        let Some(start) = declaration.find('{') else {
            invalid.push(InvalidComboDeclaration {
                line_number: line_index + 1,
                declaration: declaration.to_owned(),
                error: "has no JSON object".to_owned(),
            });
            continue;
        };
        let declaration = &declaration[start..];
        let value: Value = match serde_json::from_str(declaration) {
            Ok(value) => value,
            Err(error) => {
                invalid.push(InvalidComboDeclaration {
                    line_number: line_index + 1,
                    declaration: declaration.to_owned(),
                    error: error.to_string(),
                });
                continue;
            }
        };
        let Some(name) = value
            .get("combo")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            invalid.push(InvalidComboDeclaration {
                line_number: line_index + 1,
                declaration: declaration.to_owned(),
                error: "has no combo name".to_owned(),
            });
            continue;
        };
        let Some(default) = value.get("default").and_then(integer_value) else {
            invalid.push(InvalidComboDeclaration {
                line_number: line_index + 1,
                declaration: declaration.to_owned(),
                error: format!("combo {name} has no integer default"),
            });
            continue;
        };
        valid.push((name.to_owned(), default));
    }
    ComboDefaultDeclarations { valid, invalid }
}

fn combo_default_hint(declaration: &str) -> Option<(&str, i64)> {
    let name = declaration
        .split_once("\"combo\"")?
        .1
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    let name = name.split_once('"')?.0;
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return None;
    }
    let default = declaration
        .split_once("\"default\"")?
        .1
        .trim_start()
        .strip_prefix(':')?
        .trim_start();
    let end = default
        .find(|character: char| !character.is_ascii_digit() && character != '-')
        .unwrap_or(default.len());
    Some((name, default[..end].parse().ok()?))
}

fn sampler_combo_slots(source: &str, program: &str) -> Result<Vec<(String, u32)>, WeIngestError> {
    let mut combos = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let Some((declaration, annotation)) = line.split_once("//") else {
            continue;
        };
        let Some(name) = declaration
            .trim()
            .strip_prefix("uniform sampler2D ")
            .and_then(|declaration| declaration.split(';').next())
            .map(str::trim)
        else {
            continue;
        };
        let Some(slot) = name
            .strip_prefix("g_Texture")
            .and_then(|slot| slot.parse::<u32>().ok())
            .filter(|slot| *slot < 32)
        else {
            continue;
        };
        let Some(start) = annotation.find('{') else {
            continue;
        };
        let value: Value = serde_json::from_str(&annotation[start..]).map_err(|error| {
            shader_error(
                program,
                "program",
                format!(
                    "invalid sampler annotation on line {}: {error}",
                    line_index + 1
                ),
            )
        })?;
        if let Some(combo) = value.get("combo").and_then(Value::as_str) {
            combos.push((combo.to_owned(), slot));
        }
    }
    Ok(combos)
}

fn variant_overrides(spec: &AuthoredProgramSpec) -> Result<Vec<(String, i64)>, WeIngestError> {
    let program_identity = spec
        .program_key
        .strip_prefix("package/")
        .unwrap_or(&spec.program_key);
    let suffix = program_identity
        .strip_prefix(&spec.source_key)
        .and_then(|suffix| suffix.strip_prefix("__"))
        .ok_or_else(|| {
            shader_error(
                &spec.program_key,
                "program",
                format!(
                    "program identity does not extend source key {}",
                    spec.source_key
                ),
            )
        })?;
    suffix
        .split("__")
        .map(|part| {
            let (name, value) = part.rsplit_once('_').ok_or_else(|| {
                shader_error(
                    &spec.program_key,
                    "program",
                    format!("invalid specialization segment {part}"),
                )
            })?;
            let value = if name == "SLOTS" {
                i64::from_str_radix(value, 16)
            } else {
                value.parse::<i64>()
            }
            .map_err(|error| {
                shader_error(
                    &spec.program_key,
                    "program",
                    format!("invalid specialization value {part}: {error}"),
                )
            })?;
            Ok((name.to_owned(), value))
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
    fn uses_defaults_overrides_and_sampler_presence() {
        let spec = AuthoredProgramSpec {
            program_key: "workshop/test/effects/audio__SLOTS_1__RESOLUTION_16".to_owned(),
            source_key: "workshop/test/effects/audio".to_owned(),
            texture_slot_mask: 1,
        };
        let definitions = compiler_definitions(
            &spec,
            [
                "// [COMBO] {\"combo\":\"RESOLUTION\",\"default\":32}\nuniform sampler2D g_Texture1; // {\"combo\":\"HasExternal\"}",
                "// [COMBO] {\"combo\":\"RESOLUTION\",\"default\":32}",
            ],
        )
        .expect("compiler environment")
        .into_iter()
        .collect::<BTreeMap<_, _>>();

        assert_eq!(
            definitions.get("RESOLUTION").map(String::as_str),
            Some("16")
        );
        assert_eq!(
            definitions.get("HasExternal").map(String::as_str),
            Some("0")
        );
        assert_eq!(definitions.get("SLOTS").map(String::as_str), Some("1"));
        assert_eq!(definitions.get("HLSL").map(String::as_str), Some("1"));
        assert!(!definitions.keys().any(|name| name.contains('(')));
    }

    #[test]
    fn direct_package_identity_extends_its_unprefixed_source_key() {
        let spec = AuthoredProgramSpec {
            program_key: "package/effects/huan__SLOTS_1".to_owned(),
            source_key: "effects/huan".to_owned(),
            texture_slot_mask: 1,
        };

        assert_eq!(
            variant_overrides(&spec).expect("package specialization"),
            vec![("SLOTS".to_owned(), 1)]
        );
    }

    #[test]
    fn removes_only_declarations_proven_unreferenced_after_preprocessing() {
        let source = "uniform sampler2D g_Texture1; // {\"combo\":\"TEX\"}\n\
uniform sampler2D g_Texture2; // {\"combo\":\"MASK\"}\n\
uniform sampler2D g_Texture3; // {\"material\":\"background\"}\n\
uniform float u_Used; // {\"material\":\"used\",\"default\":1}\n\
uniform vec2 u_Unused; // {\"material\":\"unused\",\"default\":\"0 0\"}\n\
layout(location = 0) out vec2 v_Used;\n\
layout(location = 1) out vec3 v_Unused;";
        let specialized = remove_unreferenced_frontend_declarations(
            source,
            "uniform sampler2D g_Texture1; uniform sampler2D g_Texture2; \
             uniform sampler2D g_Texture3; \
             vec4 main() { return texture2D(g_Texture2, vec2(0.0)) + \
             texture2D(g_Texture3, vec2(0.0)) + vec4(v_Used, 0.0, u_Used); } \
             uniform float u_Used; uniform vec2 u_Unused; \
             out vec2 v_Used; out vec3 v_Unused;",
        );

        assert!(!specialized.contains("g_Texture1"));
        assert!(specialized.contains("g_Texture2"));
        assert!(specialized.contains("g_Texture3"));
        assert!(specialized.contains("u_Used"));
        assert!(!specialized.contains("u_Unused"));
        assert!(specialized.contains("v_Used"));
        assert!(!specialized.contains("v_Unused"));
        assert_eq!(specialized.lines().count(), 6);
    }

    #[test]
    fn injects_function_macros_after_glsl_version() {
        let source = inject_we_compiler_preamble("#version 450\nvoid main() { CAST2(1.0); }");
        let mut lines = source.lines();
        assert_eq!(lines.next(), Some("#version 450"));
        assert_eq!(lines.next(), Some("#define CAST2(v) vec2(v, v)"));
        assert!(source.contains("#define texSample2D(s, uv) texture2D(s, uv)"));
    }

    #[test]
    fn rejects_undeclared_specialization_instead_of_guessing() {
        let spec = AuthoredProgramSpec {
            program_key: "workshop/test/effects/example__SLOTS_1__UNKNOWN_1".to_owned(),
            source_key: "workshop/test/effects/example".to_owned(),
            texture_slot_mask: 1,
        };
        let error =
            compiler_definitions(&spec, ["", ""]).expect_err("unknown specialization must fail");
        assert!(error.to_string().contains("has no authored declaration"));
    }

    #[test]
    fn invalid_optional_metadata_is_redundant_only_with_an_exact_valid_stage_declaration() {
        let spec = AuthoredProgramSpec {
            program_key: "workshop/test/effects/noise__SLOTS_1__CATEGORY_1".to_owned(),
            source_key: "workshop/test/effects/noise".to_owned(),
            texture_slot_mask: 1,
        };
        let valid = "// [COMBO] {\"combo\":\"CATEGORY\",\"default\":0,\"options\":{\"Color\":0}}";
        let invalid_duplicate =
            "// [COMBO] {\"combo\":\"CATEGORY\",\"default\":0,\"options\":{Color:0}}";

        let definitions = compiler_definitions(&spec, [valid, invalid_duplicate])
            .expect("valid stage owns the exact default");
        assert!(definitions.contains(&("CATEGORY".to_owned(), "1".to_owned())));

        let error = compiler_definitions(&spec, [invalid_duplicate, ""])
            .expect_err("malformed metadata cannot become the authority");
        assert!(error.to_string().contains("invalid combo declaration"));
    }
}
