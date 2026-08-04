//! Normalize legacy Wallpaper Engine stage I/O after Rust specialization.

use std::collections::{BTreeMap, BTreeSet};

use vulkan_renderer_build::ShaderStage;

use super::shader_error;
use crate::convert::we_ingest::ingest::WeIngestError;

#[derive(Debug, Clone, PartialEq, Eq)]
struct VaryingLocation {
    location: u32,
    declaration: String,
    fragment_active: bool,
}

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
) -> Result<BTreeMap<String, VaryingLocation>, WeIngestError> {
    let active_vertex_varyings = active_varyings(vertex, program)?;
    let active_fragment_varyings = active_varyings(fragment, program)?;
    let retained_varyings = active_vertex_varyings
        .union(&active_fragment_varyings)
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut declarations = BTreeMap::<String, (String, u32)>::new();
    let mut vertex_declarations = BTreeSet::new();
    let mut order = Vec::new();
    for (is_vertex, source) in [(true, vertex), (false, fragment)] {
        for declaration in specialized_declarations(source, "varying") {
            let (name, span) = declaration_identity(&declaration, program)?;
            if (is_vertex && !retained_varyings.contains(&name))
                || (!is_vertex && !active_fragment_varyings.contains(&name))
            {
                continue;
            }
            if is_vertex {
                vertex_declarations.insert(name.clone());
            }
            if let Some((existing, existing_span)) = declarations.get_mut(&name) {
                if existing != &declaration || *existing_span != span {
                    if !is_vertex {
                        if let Some(reconciled) = reconcile_fragment_vector_prefix(
                            existing,
                            &declaration,
                            fragment,
                            &name,
                        ) {
                            *existing = reconciled;
                            continue;
                        }
                    }
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
    for name in &active_fragment_varyings {
        if !vertex_declarations.contains(name) {
            return Err(shader_error(
                program,
                "vertex",
                format!("fragment varying {name} has no vertex producer"),
            ));
        }
    }
    let mut next = 0u32;
    let mut locations = BTreeMap::new();
    for name in order {
        let span = declarations[&name].1;
        let declaration = declarations[&name].0.clone();
        let fragment_active = active_fragment_varyings.contains(&name);
        locations.insert(
            name,
            VaryingLocation {
                location: next,
                declaration,
                fragment_active,
            },
        );
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

fn reconcile_fragment_vector_prefix(
    vertex_declaration: &str,
    fragment_declaration: &str,
    fragment: &str,
    name: &str,
) -> Option<String> {
    let vertex_width = varying_vector_width(vertex_declaration)?;
    let fragment_width = varying_vector_width(fragment_declaration)?;
    if vertex_width >= fragment_width
        || !fragment_vector_reads_fit_producer(fragment, name, vertex_width)
    {
        return None;
    }
    Some(vertex_declaration.to_owned())
}

fn varying_vector_width(declaration: &str) -> Option<u32> {
    if declaration.contains('[') {
        return None;
    }
    let (source_type, _) = declaration.rsplit_once(char::is_whitespace)?;
    source_type
        .split_ascii_whitespace()
        .next_back()?
        .strip_prefix("vec")?
        .parse::<u32>()
        .ok()
        .filter(|width| (2..=4).contains(width))
}

fn fragment_vector_reads_fit_producer(fragment: &str, name: &str, producer_width: u32) -> bool {
    let declaration_count = specialized_declarations(fragment, "varying")
        .into_iter()
        .filter_map(|declaration| declaration_name(&declaration, "stage-I/O probe").ok())
        .filter(|candidate| candidate == name)
        .count();
    identifier_occurrence_ranges(fragment, name)
        .into_iter()
        .skip(declaration_count)
        .all(|(_, end)| {
            let tail = fragment[end..].trim_start();
            let Some(swizzle) = tail.strip_prefix('.') else {
                return false;
            };
            let swizzle = swizzle
                .chars()
                .take_while(|character| character.is_ascii_alphabetic())
                .collect::<String>();
            !swizzle.is_empty()
                && swizzle.chars().all(|component| {
                    vector_component_index(component).is_some_and(|index| index < producer_width)
                })
        })
}

fn vector_component_index(component: char) -> Option<u32> {
    match component {
        'x' | 'r' | 's' => Some(0),
        'y' | 'g' | 't' => Some(1),
        'z' | 'b' | 'p' => Some(2),
        'w' | 'a' | 'q' => Some(3),
        _ => None,
    }
}

fn active_varyings(fragment: &str, program: &str) -> Result<BTreeSet<String>, WeIngestError> {
    let mut active = BTreeSet::new();
    let declarations = specialized_declarations(fragment, "varying")
        .into_iter()
        .map(|declaration| declaration_identity(&declaration, program))
        .collect::<Result<Vec<_>, _>>()?;
    for (name, _) in &declarations {
        let declaration_count = declarations
            .iter()
            .filter(|(candidate, _)| candidate == name)
            .count();
        if identifier_occurrences(fragment, &name) > declaration_count {
            active.insert(name.clone());
        }
    }
    Ok(active)
}

fn identifier_occurrences(source: &str, identifier: &str) -> usize {
    identifier_occurrence_ranges(source, identifier).len()
}

fn identifier_occurrence_ranges(source: &str, identifier: &str) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    let mut ranges = Vec::new();
    let mut in_block_comment = false;
    while cursor < bytes.len() {
        if in_block_comment {
            if bytes[cursor..].starts_with(b"*/") {
                in_block_comment = false;
                cursor += 2;
            } else {
                cursor += 1;
            }
            continue;
        }
        if bytes[cursor..].starts_with(b"//") {
            cursor = bytes[cursor..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |offset| cursor + offset + 1);
            continue;
        }
        if bytes[cursor..].starts_with(b"/*") {
            in_block_comment = true;
            cursor += 2;
            continue;
        }
        let starts_identifier = bytes[cursor].is_ascii_alphabetic() || bytes[cursor] == b'_';
        if !starts_identifier {
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
        if &source[start..cursor] == identifier {
            ranges.push((start, cursor));
        }
    }
    ranges
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
    varying_locations: &BTreeMap<String, VaryingLocation>,
    program: &str,
) -> Result<String, WeIngestError> {
    let mut output = Vec::new();
    if stage == ShaderStage::Fragment {
        output.push("layout(location = 0) out vec4 tensor_wallpaper_FragColor;".to_owned());
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
            let Some(interface) = varying_locations.get(&name) else {
                continue;
            };
            if stage == ShaderStage::Fragment && !interface.fragment_active {
                continue;
            }
            let direction = if stage == ShaderStage::Vertex {
                "out"
            } else {
                "in"
            };
            output.push(format!(
                "layout(location = {}) {direction} {};",
                interface.location, interface.declaration
            ));
            continue;
        }
        output.push(line.to_owned());
    }
    let output = output.join("\n");
    Ok(if stage == ShaderStage::Fragment {
        replace_identifier(&output, "gl_FragColor", "tensor_wallpaper_FragColor")
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
            "attribute vec3 a_Position;\nattribute vec2 a_TexCoord;\nvarying vec2 v_TexCoord;\nvarying float v_Mask;\nvoid main() { v_TexCoord = a_TexCoord; v_Mask = 1; }",
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
        assert!(fragment.contains("layout(location = 0) out vec4 tensor_wallpaper_FragColor;"));
        assert!(!fragment.contains("gl_FragColor"));
    }

    #[test]
    fn assigns_locations_from_the_combo_specialized_varying_interface() {
        let source = "#ifdef GLSL\nvarying vec4 audioValue[28];\n#else\nvarying vec4 audioValue[RESOLUTION];\n#endif\nvarying vec2 v_TexCoord;";
        let specialized = "varying vec4 audioValue[16];\nvarying vec2 v_TexCoord;\nvoid main() { gl_FragColor = audioValue[0] + vec4(v_TexCoord, 0, 0); }";
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
            assert!(stage.contains("vec4 audioValue[16];"));
            assert!(!stage.contains("audioValue[RESOLUTION]"));
            assert!(!stage.contains("audioValue[28]"));
            assert!(stage.contains("layout(location = 16)"));
            assert!(stage.contains("vec2 v_TexCoord;"));
        }
    }

    #[test]
    fn removes_specialized_varyings_not_read_by_fragment() {
        let vertex = "varying vec4 v_TexCoord;\nvarying vec3 v_ScreenCoord;\nvoid main() { v_TexCoord = vec4(0); }";
        let fragment = "varying vec4 v_TexCoord;\nvarying vec3 v_ScreenCoord;\nvoid main() { gl_FragColor = v_TexCoord; }";
        let [vertex, fragment] = normalize_stage_io_pair(
            vertex,
            fragment,
            vertex,
            fragment,
            "workshop/test/effects/clipping_mask__TEX_0",
        )
        .expect("inactive varying removal");

        for stage in [&vertex, &fragment] {
            assert!(stage.contains("layout(location = 0)"));
            assert!(stage.contains("vec4 v_TexCoord;"));
            assert!(!stage.contains("v_ScreenCoord"));
        }
    }

    #[test]
    fn retains_a_vertex_output_written_but_not_read_by_the_fragment() {
        let vertex = "varying vec2 v_TexCoord;\nvarying vec2 v_TexCoordBase;\nvoid main() { v_TexCoord = vec2(0); v_TexCoordBase = vec2(1); }";
        let fragment = "varying vec2 v_TexCoord;\nvarying vec2 v_TexCoordBase;\nvoid main() { gl_FragColor = vec4(v_TexCoord, 0, 1); }";
        let [vertex, fragment] = normalize_stage_io_pair(
            vertex,
            fragment,
            vertex,
            fragment,
            "workshop/test/effects/light_map__SLOTS_1",
        )
        .expect("producer-only output remains declared");

        assert!(vertex.contains("layout(location = 1) out vec2 v_TexCoordBase;"));
        assert!(vertex.contains("v_TexCoordBase = vec2(1)"));
        assert!(!fragment.contains("v_TexCoordBase"));
    }

    #[test]
    fn reconciles_a_wider_fragment_vector_when_all_reads_fit_the_producer_prefix() {
        let vertex = "varying vec2 v_TexCoord;\nvoid main() { v_TexCoord.xy = vec2(0); }";
        let fragment =
            "varying vec4 v_TexCoord;\nvoid main() { gl_FragColor = vec4(v_TexCoord.xy, 0, 1); }";
        let [vertex, fragment] = normalize_stage_io_pair(
            vertex,
            fragment,
            vertex,
            fragment,
            "package/effects/prefix__SLOTS_1",
        )
        .expect("fragment consumes only the producer prefix");

        assert!(vertex.contains("layout(location = 0) out vec2 v_TexCoord;"));
        assert!(fragment.contains("layout(location = 0) in vec2 v_TexCoord;"));
        assert!(fragment.contains("vec4(v_TexCoord.xy, 0, 1)"));
    }

    #[test]
    fn rejects_wider_fragment_vector_reads_outside_the_producer_prefix() {
        for body in [
            "gl_FragColor = vec4(v_TexCoord.z);",
            "gl_FragColor = v_TexCoord;",
        ] {
            let vertex = "varying vec2 v_TexCoord;\nvoid main() { v_TexCoord.xy = vec2(0); }";
            let fragment = format!("varying vec4 v_TexCoord;\nvoid main() {{ {body} }}");
            let error = normalize_stage_io_pair(
                vertex,
                &fragment,
                vertex,
                &fragment,
                "package/effects/prefix__SLOTS_1",
            )
            .expect_err("fragment read exceeds the vertex producer");

            assert!(
                error
                    .to_string()
                    .contains("v_TexCoord has conflicting declarations")
            );
        }
    }

    #[test]
    fn rejects_an_active_fragment_varying_without_vertex_producer() {
        let error = normalize_stage_io_pair(
            "void main() {}",
            "varying vec2 v_Mask;\nvoid main() { gl_FragColor = vec4(v_Mask, 0, 1); }",
            "void main() {}",
            "varying vec2 v_Mask;\nvoid main() { gl_FragColor = vec4(v_Mask, 0, 1); }",
            "workshop/test/effects/mask__SLOTS_1",
        )
        .expect_err("active fragment input needs a vertex producer");
        assert!(error.to_string().contains("v_Mask has no vertex producer"));
    }

    #[test]
    fn ignores_identifiers_inside_comments_when_classifying_live_varyings() {
        assert_eq!(
            identifier_occurrences("// v_Mask\n/* v_Mask */", "v_Mask"),
            0
        );
        assert_eq!(identifier_occurrences("v_Mask /* v_Mask */", "v_Mask"), 1);
    }
}
