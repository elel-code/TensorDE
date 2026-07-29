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
    for source in sources {
        for (name, default) in strict_combo_defaults(source, &spec.program_key)? {
            if let Some(previous) = values.insert(name.clone(), default)
                && previous != default
            {
                return Err(shader_error(
                    &spec.program_key,
                    "program",
                    format!("combo {name} has conflicting defaults {previous} and {default}"),
                ));
            }
        }
        for (name, slot) in sampler_combo_slots(source, &spec.program_key)? {
            values.insert(name, i64::from(spec.texture_slot_mask & (1 << slot) != 0));
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

fn strict_combo_defaults(source: &str, program: &str) -> Result<Vec<(String, i64)>, WeIngestError> {
    let mut defaults = Vec::new();
    for (line_index, line) in source.lines().enumerate() {
        let Some((_, declaration)) = line.split_once("[COMBO]") else {
            continue;
        };
        let start = declaration.find('{').ok_or_else(|| {
            shader_error(
                program,
                "program",
                format!(
                    "combo declaration on line {} has no JSON object",
                    line_index + 1
                ),
            )
        })?;
        let value: Value = serde_json::from_str(&declaration[start..]).map_err(|error| {
            shader_error(
                program,
                "program",
                format!(
                    "invalid combo declaration on line {}: {error}",
                    line_index + 1
                ),
            )
        })?;
        let name = value
            .get("combo")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                shader_error(
                    program,
                    "program",
                    format!("combo declaration on line {} has no name", line_index + 1),
                )
            })?;
        let default = value
            .get("default")
            .and_then(integer_value)
            .ok_or_else(|| {
                shader_error(
                    program,
                    "program",
                    format!(
                        "combo {name} on line {} has no integer default",
                        line_index + 1
                    ),
                )
            })?;
        defaults.push((name.to_owned(), default));
    }
    Ok(defaults)
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
    let suffix = spec
        .program_key
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
        assert!(!definitions.keys().any(|name| name.contains('(')));
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
}
