//! Normalize legacy GLSL stage I/O before invoking Slang's GLSL frontend.

use std::collections::BTreeMap;

use vulkan_renderer_build::ShaderStage;

use super::shader_error;
use crate::convert::we_ingest::ingest::WeIngestError;

pub(super) fn normalize_stage_io_pair(
    vertex: &str,
    fragment: &str,
    program: &str,
) -> Result<[String; 2], WeIngestError> {
    let varying_locations = varying_locations(vertex, fragment, program)?;
    Ok([
        normalize_stage_io(vertex, ShaderStage::Vertex, &varying_locations, program)?,
        normalize_stage_io(fragment, ShaderStage::Fragment, &varying_locations, program)?,
    ])
}

fn varying_locations(
    vertex: &str,
    fragment: &str,
    program: &str,
) -> Result<BTreeMap<String, u32>, WeIngestError> {
    let mut declarations = BTreeMap::<String, (String, u32)>::new();
    let mut order = Vec::new();
    for source in [vertex, fragment] {
        for line in source.lines() {
            let Some(declaration) = legacy_declaration(line, "varying") else {
                continue;
            };
            let (name, span) = declaration_identity(&declaration, program)?;
            if let Some((existing, existing_span)) = declarations.get(&name) {
                if existing != &declaration || *existing_span != span {
                    return Err(shader_error(
                        program,
                        "program",
                        format!("varying {name} has conflicting declarations"),
                    ));
                }
            } else {
                order.push(name.clone());
                declarations.insert(name, (declaration, span));
            }
        }
    }
    let mut next = 0u32;
    let mut locations = BTreeMap::new();
    for name in order {
        let span = declarations[&name].1;
        locations.insert(name, next);
        next = next.checked_add(span).ok_or_else(|| {
            shader_error(
                program,
                "program",
                "varying locations exceed the u32 domain",
            )
        })?;
    }
    Ok(locations)
}

fn normalize_stage_io(
    source: &str,
    stage: ShaderStage,
    varying_locations: &BTreeMap<String, u32>,
    program: &str,
) -> Result<String, WeIngestError> {
    let mut output = Vec::new();
    if stage == ShaderStage::Fragment {
        output.push("layout(location = 0) out vec4 gilder_FragColor;".to_owned());
    }
    for line in source.trim_start_matches('\u{feff}').lines() {
        if let Some(declaration) = legacy_declaration(line, "attribute") {
            if stage != ShaderStage::Vertex {
                return Err(shader_error(
                    program,
                    stage.slang_name(),
                    "fragment shader declares a vertex attribute",
                ));
            }
            let (name, _) = declaration_identity(&declaration, program)?;
            let location = vertex_attribute_location(&name).ok_or_else(|| {
                shader_error(
                    program,
                    "vertex",
                    format!("unknown WE vertex attribute {name}"),
                )
            })?;
            output.push(format!("layout(location = {location}) in {declaration};"));
            continue;
        }
        if let Some(declaration) = legacy_declaration(line, "varying") {
            let (name, _) = declaration_identity(&declaration, program)?;
            let location = varying_locations.get(&name).ok_or_else(|| {
                shader_error(
                    program,
                    stage.slang_name(),
                    format!("unmapped varying {name}"),
                )
            })?;
            let direction = if stage == ShaderStage::Vertex {
                "out"
            } else {
                "in"
            };
            output.push(format!(
                "layout(location = {location}) {direction} {declaration};"
            ));
            continue;
        }
        output.push(line.to_owned());
    }
    let output = output.join("\n");
    Ok(if stage == ShaderStage::Fragment {
        replace_identifier(&output, "gl_FragColor", "gilder_FragColor")
    } else {
        output
    })
}

fn legacy_declaration(line: &str, keyword: &str) -> Option<String> {
    line.trim()
        .strip_prefix(keyword)
        .and_then(|line| {
            let trimmed = line.trim_start();
            (trimmed.len() != line.len()).then_some(trimmed)
        })
        .and_then(|line| line.strip_suffix(';'))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
}

fn declaration_identity(declaration: &str, program: &str) -> Result<(String, u32), WeIngestError> {
    let (source_type, declarator) =
        declaration
            .rsplit_once(char::is_whitespace)
            .ok_or_else(|| {
                shader_error(
                    program,
                    "program",
                    format!("invalid stage-I/O declaration {declaration}"),
                )
            })?;
    let (name, array_count) = if let Some((name, count)) = declarator.split_once('[') {
        let count = count
            .strip_suffix(']')
            .and_then(|count| count.parse::<u32>().ok())
            .filter(|count| *count != 0)
            .ok_or_else(|| {
                shader_error(
                    program,
                    "program",
                    format!("invalid stage-I/O array {declarator}"),
                )
            })?;
        (name, count)
    } else {
        (declarator, 1)
    };
    let type_span = source_type
        .strip_prefix("mat")
        .and_then(|width| width.chars().next())
        .and_then(|width| width.to_digit(10))
        .unwrap_or(1);
    let span = array_count
        .checked_mul(type_span)
        .ok_or_else(|| shader_error(program, "program", "stage-I/O location span exceeds u32"))?;
    Ok((name.to_owned(), span))
}

fn vertex_attribute_location(name: &str) -> Option<u32> {
    match name {
        "a_Position" | "a_PositionVec4" => Some(0),
        "a_TexCoord" | "a_TexCoordVec4" => Some(1),
        "a_Normal" => Some(2),
        "a_Tangent4" => Some(3),
        "a_BlendIndices" => Some(4),
        "a_BlendWeights" => Some(5),
        "a_Color" => Some(6),
        "a_PositionC1" => Some(7),
        _ => None,
    }
}

fn replace_identifier(source: &str, name: &str, replacement: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut start = 0;
    while let Some(relative) = source[start..].find(name) {
        let found = start + relative;
        let end = found + name.len();
        let boundary = |character: Option<char>| {
            character
                .is_none_or(|character| !(character.is_ascii_alphanumeric() || character == '_'))
        };
        output.push_str(&source[start..found]);
        if boundary(source[..found].chars().next_back()) && boundary(source[end..].chars().next()) {
            output.push_str(replacement);
        } else {
            output.push_str(name);
        }
        start = end;
    }
    output.push_str(&source[start..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_legacy_stage_io_with_shared_locations() {
        let [vertex, fragment] = normalize_stage_io_pair(
            "attribute vec3 a_Position;\nattribute vec2 a_TexCoord;\nvarying vec2 v_TexCoord;\nvarying float v_Mask;\nvoid main() { gl_Position = vec4(a_Position, 1); }",
            "varying vec2 v_TexCoord;\nvarying float v_Mask;\nvoid main() { gl_FragColor = vec4(v_TexCoord, v_Mask, 1); }",
            "workshop/test/effects/example__SLOTS_1",
        )
        .expect("stage normalization");

        assert!(vertex.contains("layout(location = 0) in vec3 a_Position;"));
        assert!(vertex.contains("layout(location = 1) in vec2 a_TexCoord;"));
        assert!(vertex.contains("layout(location = 0) out vec2 v_TexCoord;"));
        assert!(vertex.contains("layout(location = 1) out float v_Mask;"));
        assert!(fragment.contains("layout(location = 0) in vec2 v_TexCoord;"));
        assert!(fragment.contains("layout(location = 1) in float v_Mask;"));
        assert!(fragment.contains("layout(location = 0) out vec4 gilder_FragColor;"));
        assert!(!fragment.contains("gl_FragColor"));
    }
}
