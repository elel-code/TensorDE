//! Normalize legacy Wallpaper Engine stage I/O after Rust specialization.

use std::collections::BTreeMap;

use vulkan_renderer_build::ShaderStage;

use super::shader_error;
use crate::convert::we_ingest::ingest::WeIngestError;

pub(super) fn normalize_stage_io_pair(
    vertex: &str,
    fragment: &str,
    specialized_vertex: &str,
    specialized_fragment: &str,
    program: &str,
) -> Result<[String; 2], WeIngestError> {
    let varying_locations = varying_locations(specialized_vertex, specialized_fragment, program)?;
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
        for declaration in specialized_declarations(source, "varying") {
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

fn specialized_declarations(source: &str, keyword: &str) -> Vec<String> {
    source
        .match_indices(keyword)
        .filter_map(|(start, _)| {
            let end = start + keyword.len();
            let identifier_boundary = |byte: Option<u8>| {
                byte.is_none_or(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
            };
            if !identifier_boundary(source[..start].bytes().next_back())
                || !source[end..]
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_whitespace())
            {
                return None;
            }
            source[end..]
                .split_once(';')
                .map(|(declaration, _)| declaration.trim().to_owned())
                .filter(|declaration| !declaration.is_empty())
        })
        .collect()
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
            let name = declaration_name(&declaration, program)?;
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
            let name = declaration_name(&declaration, program)?;
            let Some(location) = varying_locations.get(&name) else {
                output.push(line.to_owned());
                continue;
            };
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
    line.split_once("//")
        .map_or(line, |(declaration, _)| declaration)
        .trim()
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
    let invalid = || {
        shader_error(
            program,
            "program",
            format!("invalid stage-I/O declaration {declaration}"),
        )
    };
    let (head, array_count) = if let Some((head, count)) = declaration.split_once('[') {
        let count = count
            .strip_suffix(']')
            .map(str::trim)
            .and_then(|count| count.parse::<u32>().ok())
            .filter(|count| *count != 0)
            .ok_or_else(|| {
                shader_error(
                    program,
                    "program",
                    format!("invalid stage-I/O array {declaration}"),
                )
            })?;
        (head.trim_end(), count)
    } else {
        (declaration, 1)
    };
    let (source_type, name) = head.rsplit_once(char::is_whitespace).ok_or_else(invalid)?;
    let source_type = source_type
        .split_ascii_whitespace()
        .next_back()
        .ok_or_else(invalid)?;
    let type_span = source_type
        .strip_prefix("mat")
        .and_then(|width| width.chars().next())
        .and_then(|width| width.to_digit(10))
        .unwrap_or(1);
    let span = array_count
        .checked_mul(type_span)
        .ok_or_else(|| shader_error(program, "program", "stage-I/O location span exceeds u32"))?;
    Ok((name.trim().to_owned(), span))
}

fn declaration_name(declaration: &str, program: &str) -> Result<String, WeIngestError> {
    let declarator = declaration
        .split_ascii_whitespace()
        .next_back()
        .ok_or_else(|| {
            shader_error(
                program,
                "program",
                format!("invalid stage-I/O declaration {declaration}"),
            )
        })?;
    let name = declarator
        .split_once('[')
        .map_or(declarator, |(name, _)| name);
    (!name.is_empty()).then(|| name.to_owned()).ok_or_else(|| {
        shader_error(
            program,
            "program",
            format!("invalid stage-I/O declaration {declaration}"),
        )
    })
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
            "attribute vec3 a_Position;\nattribute vec2 a_TexCoord;\nvarying vec2 v_TexCoord; // authored UV\nvarying float v_Mask;\nvoid main() { gl_Position = vec4(a_Position, 1); }",
            "varying vec2 v_TexCoord; // authored UV\nvarying float v_Mask;\nvoid main() { gl_FragColor = vec4(v_TexCoord, v_Mask, 1); }",
            "attribute vec3 a_Position;\nattribute vec2 a_TexCoord;\nvarying vec2 v_TexCoord;\nvarying float v_Mask;",
            "varying vec2 v_TexCoord;\nvarying float v_Mask;",
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

    #[test]
    fn assigns_locations_from_the_combo_specialized_varying_interface() {
        let source = "#ifdef GLSL\nvarying vec4 audioValue[28];\n#else\nvarying vec4 audioValue[RESOLUTION];\n#endif\nvarying vec2 v_TexCoord;";
        let specialized = "void helper ( ) { } varying vec4 audioValue [ 16 ] ; varying vec2 v_TexCoord ; void main ( ) { }";
        let [vertex, fragment] = normalize_stage_io_pair(
            source,
            source,
            specialized,
            specialized,
            "workshop/test/effects/audio__RESOLUTION_16",
        )
        .expect("specialized stage I/O");

        for stage in [&vertex, &fragment] {
            assert!(stage.contains("layout(location = 0)"));
            assert!(stage.contains("vec4 audioValue[RESOLUTION];"));
            assert!(stage.contains("layout(location = 16)"));
            assert!(stage.contains("vec2 v_TexCoord;"));
        }
    }

    #[test]
    fn preserves_declarations_absent_from_the_specialized_interface() {
        let source = "#if MASK\nvarying vec2 v_TexCoordOpacity;\n#endif\nvarying vec2 v_TexCoord;";
        let specialized = "varying vec2 v_TexCoord ;";
        let [vertex, fragment] = normalize_stage_io_pair(
            source,
            source,
            specialized,
            specialized,
            "workshop/test/effects/blend__MASK_0",
        )
        .expect("inactive varying branch");

        for stage in [&vertex, &fragment] {
            assert!(stage.contains("varying vec2 v_TexCoordOpacity;"));
            assert!(stage.contains("layout(location = 0)"));
            assert!(stage.contains("vec2 v_TexCoord;"));
        }
    }
}
