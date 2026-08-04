//! Strict Wallpaper Engine combo and compiler-macro environment.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{AuthoredProgramSpec, shader_error};
use crate::convert::we_ingest::ingest::WeIngestError;
use crate::convert::we_ingest::ingest::shader_combo::parse_shader_combo_declaration;

pub(super) fn compiler_definitions(
    spec: &AuthoredProgramSpec,
    sources: [&str; 2],
) -> Result<Vec<(String, String)>, WeIngestError> {
    let mut values = BTreeMap::<String, i64>::new();
    let mut authored_defaults = BTreeMap::<String, i64>::new();
    let mut authored_conditionals = BTreeSet::new();
    let mut invalid_declarations = Vec::new();
    for source in sources {
        authored_conditionals.extend(preprocessor_condition_identifiers(source));
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
        for name in sampler_combo_names(source, &spec.program_key)? {
            values.entry(name).or_insert(0);
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
        if name != "SLOTS" && !values.contains_key(&name) && !authored_conditionals.contains(&name)
        {
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
        let (name, default) = match parse_shader_combo_declaration(declaration) {
            Ok(fields) => fields,
            Err(error) => {
                invalid.push(InvalidComboDeclaration {
                    line_number: line_index + 1,
                    declaration: declaration.to_owned(),
                    error,
                });
                continue;
            }
        };
        valid.push((name, default));
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

fn sampler_combo_names(source: &str, program: &str) -> Result<Vec<String>, WeIngestError> {
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
        let Some(_slot) = name
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
            combos.push(combo.to_owned());
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

fn preprocessor_condition_identifiers(source: &str) -> BTreeSet<String> {
    let source = source_without_comments(source);
    let mut identifiers = BTreeSet::new();
    for line in source.lines() {
        let Some(directive) = line.trim_start().strip_prefix('#') else {
            continue;
        };
        let directive = directive.trim_start();
        let command_end = directive
            .find(char::is_whitespace)
            .unwrap_or(directive.len());
        let (command, condition) = directive.split_at(command_end);
        if !matches!(command, "if" | "elif" | "ifdef" | "ifndef") {
            continue;
        }
        let bytes = condition.as_bytes();
        let mut cursor = 0;
        while cursor < bytes.len() {
            if !bytes[cursor].is_ascii_alphabetic() && bytes[cursor] != b'_' {
                cursor += 1;
                continue;
            }
            let start = cursor;
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'_')
            {
                cursor += 1;
            }
            let identifier = &condition[start..cursor];
            if identifier != "defined" {
                identifiers.insert(identifier.to_owned());
            }
        }
    }
    identifiers
}

fn source_without_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    let mut in_block_comment = false;
    while cursor < bytes.len() {
        if in_block_comment {
            if bytes[cursor..].starts_with(b"*/") {
                in_block_comment = false;
                output.push_str("  ");
                cursor += 2;
            } else {
                output.push(if bytes[cursor] == b'\n' { '\n' } else { ' ' });
                cursor += 1;
            }
            continue;
        }
        if bytes[cursor..].starts_with(b"//") {
            output.push_str("  ");
            cursor += 2;
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                output.push(' ');
                cursor += 1;
            }
            continue;
        }
        if bytes[cursor..].starts_with(b"/*") {
            in_block_comment = true;
            output.push_str("  ");
            cursor += 2;
            continue;
        }
        output.push(char::from(bytes[cursor]));
        cursor += 1;
    }
    output
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
    fn bound_sampler_does_not_enable_its_combo_without_an_authored_override() {
        let source =
            r#"uniform sampler2D g_Texture2; // {"combo":"OPACITYMASK","default":"util/white"}"#;
        let inactive = AuthoredProgramSpec {
            program_key: "workshop/test/effects/rounded_mask__SLOTS_5".to_owned(),
            source_key: "workshop/test/effects/rounded_mask".to_owned(),
            texture_slot_mask: 5,
        };
        let inactive = compiler_definitions(&inactive, ["", source])
            .expect("inactive sampler combo")
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        assert_eq!(inactive.get("OPACITYMASK").map(String::as_str), Some("0"));

        let active = AuthoredProgramSpec {
            program_key: "workshop/test/effects/rounded_mask__SLOTS_5__OPACITYMASK_1".to_owned(),
            source_key: "workshop/test/effects/rounded_mask".to_owned(),
            texture_slot_mask: 5,
        };
        let active = compiler_definitions(&active, ["", source])
            .expect("explicit sampler combo")
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        assert_eq!(active.get("OPACITYMASK").map(String::as_str), Some("1"));
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
    fn injects_function_macros_after_source_version_directive() {
        let source = inject_we_compiler_preamble("#version 450\nvoid main() { CAST2(1.0); }");
        let mut lines = source.lines();
        assert_eq!(lines.next(), Some("#version 450"));
        assert_eq!(lines.next(), Some("#define CAST2(v) vec2(v, v)"));
        assert!(source.contains("#define texSample2D(s, uv) texture2D(s, uv)"));
        assert!(!source.contains("#define mul("));
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
    fn accepts_a_material_specialization_referenced_by_an_authored_condition() {
        let spec = AuthoredProgramSpec {
            program_key: "workshop/test/effects/blur_gaussian__SLOTS_1__HIGH_QUALITY_1__VERTICAL_1"
                .to_owned(),
            source_key: "workshop/test/effects/blur_gaussian".to_owned(),
            texture_slot_mask: 1,
        };
        let definitions = compiler_definitions(
            &spec,
            [
                "// [COMBO] {\"combo\":\"HIGH_QUALITY\",\"default\":0}",
                "#if VERTICAL\nfloat offset = 1.0;\n#endif",
            ],
        )
        .expect("material combo is consumed by the authored conditional")
        .into_iter()
        .collect::<BTreeMap<_, _>>();

        assert_eq!(
            definitions.get("HIGH_QUALITY").map(String::as_str),
            Some("1")
        );
        assert_eq!(definitions.get("VERTICAL").map(String::as_str), Some("1"));
    }

    #[test]
    fn comment_only_condition_names_do_not_authorize_a_specialization() {
        let spec = AuthoredProgramSpec {
            program_key: "workshop/test/effects/example__SLOTS_1__VERTICAL_1".to_owned(),
            source_key: "workshop/test/effects/example".to_owned(),
            texture_slot_mask: 1,
        };
        let error =
            compiler_definitions(&spec, ["#if 0 // VERTICAL\n#endif", "/* #if VERTICAL */"])
                .expect_err("comments are not authored conditional declarations");

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

    #[test]
    fn exact_machine_suffix_owns_the_default_when_only_ui_metadata_is_malformed() {
        let spec = AuthoredProgramSpec {
            program_key: "workshop/test/effects/sway__SLOTS_1__VERTICAL_RELATIVE_WIDTH_0"
                .to_owned(),
            source_key: "workshop/test/effects/sway".to_owned(),
            texture_slot_mask: 1,
        };
        let malformed_ui = r#"// [COMBO] {"material":"missing quote,"combo":"VERTICAL_RELATIVE_WIDTH","type":"options","default":1,"require":{"VERTICAL_AUTO_MASK":1}}"#;

        let definitions = compiler_definitions(&spec, [malformed_ui, ""])
            .expect("exact machine-field projection")
            .into_iter()
            .collect::<BTreeMap<_, _>>();

        assert_eq!(
            definitions
                .get("VERTICAL_RELATIVE_WIDTH")
                .map(String::as_str),
            Some("0")
        );
    }
}
