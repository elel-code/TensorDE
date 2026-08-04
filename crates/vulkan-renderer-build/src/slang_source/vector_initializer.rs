//! Explicit Slang constructors for generated floating-vector initializers.
//!
//! Wallpaper Engine's generated stage dialect permits a wider floating vector
//! expression to initialize a narrower vector. The assignment consumes the
//! declared width's leading components. Slang diagnoses that implicit
//! narrowing, so make the target type explicit at this cold lowering boundary.

use std::collections::BTreeMap;

use super::Declarations;

/// Lowers simple generated floating-vector initializers at their declared
/// width. A component-wise arithmetic expression uses that declared width as
/// its expected operand width, so every known wider vector operand receives
/// an explicit leading-component swizzle.
///
/// Array and multi-declarator statements remain untouched. The generated
/// dialect has no parser fallback; syntax outside this narrow form is left for
/// Slang to reject explicitly.
pub(super) fn lower_generated_vector_initializer_conversions(
    source: String,
    declarations: &Declarations,
) -> String {
    let symbols = vector_symbols(&source, declarations);
    let source = lower_vector_declaration_initializers(source, &symbols);
    lower_vector_compound_assignments(source, &symbols)
}

fn lower_vector_declaration_initializers(source: String, symbols: &BTreeMap<String, u8>) -> String {
    let mut lowered = String::with_capacity(source.len());
    let mut cursor = 0;

    while let Some(initializer) = next_vector_initializer(&source, cursor) {
        lowered.push_str(&source[cursor..initializer.expression_start]);
        lowered.push_str(&lower_vector_expression(
            &source[initializer.expression_start..initializer.expression_end],
            initializer.width,
            symbols,
        ));
        cursor = initializer.expression_end;
    }

    lowered.push_str(&source[cursor..]);
    lowered
}

/// Narrows a wider vector operand at the destination width of a simple
/// component-wise compound assignment. WE's DXBC lowers a `vec2 *= vec4`
/// expression by consuming the `.xy` components of the right-hand side; Slang
/// Slang requires that boundary to be explicit.
fn lower_vector_compound_assignments(source: String, symbols: &BTreeMap<String, u8>) -> String {
    let mut lowered = String::with_capacity(source.len());
    let mut cursor = 0;

    while let Some(assignment) = next_vector_compound_assignment(&source, cursor, symbols) {
        lowered.push_str(&source[cursor..assignment.expression_start]);
        lowered.push_str(&lower_vector_expression(
            &source[assignment.expression_start..assignment.expression_end],
            assignment.width,
            symbols,
        ));
        cursor = assignment.expression_end;
    }

    lowered.push_str(&source[cursor..]);
    lowered
}

struct VectorInitializer {
    width: u8,
    expression_start: usize,
    expression_end: usize,
}

fn next_vector_initializer(source: &str, mut cursor: usize) -> Option<VectorInitializer> {
    let bytes = source.as_bytes();
    while cursor < bytes.len() {
        cursor = skip_non_code(source, cursor);
        if cursor >= bytes.len() {
            return None;
        }
        if !is_identifier_start(bytes[cursor]) {
            cursor += 1;
            continue;
        }
        let type_start = cursor;
        cursor = identifier_end(bytes, cursor);
        let ty = &source[type_start..cursor];
        let Some(width) = vector_width(ty) else {
            continue;
        };
        let name_start = skip_trivia(source, cursor);
        if name_start >= bytes.len() || !is_identifier_start(bytes[name_start]) {
            continue;
        }
        let name_end = identifier_end(bytes, name_start);
        let after_name = skip_trivia(source, name_end);
        if after_name >= bytes.len()
            || bytes[after_name] != b'='
            || bytes.get(after_name + 1) == Some(&b'=')
        {
            continue;
        }
        let expression_start = skip_trivia(source, after_name + 1);
        let statement_end = statement_end(source, expression_start)?;
        let expression_end = trim_ascii_whitespace(source, expression_start, statement_end);
        if expression_start == expression_end
            || has_top_level_comma(&source[expression_start..expression_end])
        {
            cursor = statement_end.saturating_add(1);
            continue;
        }
        return Some(VectorInitializer {
            width,
            expression_start,
            expression_end,
        });
    }
    None
}

struct VectorCompoundAssignment {
    width: u8,
    expression_start: usize,
    expression_end: usize,
}

fn next_vector_compound_assignment(
    source: &str,
    mut cursor: usize,
    symbols: &BTreeMap<String, u8>,
) -> Option<VectorCompoundAssignment> {
    let bytes = source.as_bytes();
    while cursor < bytes.len() {
        cursor = skip_non_code(source, cursor);
        if cursor >= bytes.len() {
            return None;
        }
        if !is_identifier_start(bytes[cursor]) {
            cursor += 1;
            continue;
        }
        let lhs_start = cursor;
        cursor = identifier_end(bytes, cursor);
        let lhs = &source[lhs_start..cursor];
        let Some(width) = symbols.get(lhs).copied() else {
            continue;
        };
        let operator_start = skip_trivia(source, cursor);
        if !matches!(
            bytes.get(operator_start..operator_start + 2),
            Some(b"*=" | b"/=" | b"+=" | b"-=")
        ) {
            continue;
        }
        let expression_start = skip_trivia(source, operator_start + 2);
        let statement_end = statement_end(source, expression_start)?;
        let expression_end = trim_ascii_whitespace(source, expression_start, statement_end);
        if expression_start == expression_end
            || has_top_level_comma(&source[expression_start..expression_end])
        {
            cursor = statement_end.saturating_add(1);
            continue;
        }
        return Some(VectorCompoundAssignment {
            width,
            expression_start,
            expression_end,
        });
    }
    None
}

fn vector_symbols(source: &str, declarations: &Declarations) -> BTreeMap<String, u8> {
    let mut symbols = BTreeMap::new();
    for uniforms in declarations.uniforms_by_binding.values() {
        for uniform in uniforms {
            if let Some(width) = vector_width(&uniform.ty) {
                symbols.insert(plain_declarator(&uniform.declarator).to_owned(), width);
            }
        }
    }
    collect_source_vector_symbols(source, &mut symbols);
    symbols
}

fn plain_declarator(declarator: &str) -> &str {
    declarator
        .split_once('[')
        .map_or(declarator, |(name, _)| name)
}

fn collect_source_vector_symbols(source: &str, symbols: &mut BTreeMap<String, u8>) {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        cursor = skip_non_code(source, cursor);
        if cursor >= bytes.len() {
            break;
        }
        if !is_identifier_start(bytes[cursor]) {
            cursor += 1;
            continue;
        }
        let type_start = cursor;
        cursor = identifier_end(bytes, cursor);
        let Some(width) = vector_width(&source[type_start..cursor]) else {
            continue;
        };
        let name_start = skip_trivia(source, cursor);
        if name_start >= bytes.len() || !is_identifier_start(bytes[name_start]) {
            continue;
        }
        let name_end = identifier_end(bytes, name_start);
        if skip_trivia(source, name_end) < bytes.len()
            && bytes[skip_trivia(source, name_end)] != b'('
        {
            symbols.insert(source[name_start..name_end].to_owned(), width);
        }
    }
}

fn vector_width(ty: &str) -> Option<u8> {
    match ty {
        "vec2" => Some(2),
        "vec3" => Some(3),
        "vec4" => Some(4),
        _ => None,
    }
}

fn lower_vector_expression(
    expression: &str,
    target_width: u8,
    symbols: &BTreeMap<String, u8>,
) -> String {
    let (start, end) = trimmed_range(expression);
    if start == end {
        return expression.to_owned();
    }
    let core = &expression[start..end];
    if let Some(close) = enclosing_parenthesis(core)
        && close + 1 == core.len()
    {
        return format!(
            "{}({}){}",
            &expression[..start],
            lower_vector_expression(&core[1..close], target_width, symbols),
            &expression[end..]
        );
    }
    if let Some(operator) = top_level_arithmetic_operator(core) {
        return format!(
            "{}{}{}{}{}",
            &expression[..start],
            lower_vector_expression(&core[..operator], target_width, symbols),
            &core[operator..=operator],
            lower_vector_expression(&core[operator + 1..], target_width, symbols),
            &expression[end..]
        );
    }
    lower_vector_leaf(expression, start, end, target_width, symbols)
}

fn trimmed_range(source: &str) -> (usize, usize) {
    let bytes = source.as_bytes();
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    (start, trim_ascii_whitespace(source, start, bytes.len()))
}

fn enclosing_parenthesis(source: &str) -> Option<usize> {
    (source.as_bytes().first() == Some(&b'(')).then(|| matching_parenthesis(source, 0))?
}

fn matching_parenthesis(source: &str, opening: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = opening;
    let mut depth = 0_u32;
    while cursor < bytes.len() {
        let next = skip_non_code(source, cursor);
        if next != cursor {
            cursor = next;
            continue;
        }
        match bytes[cursor] {
            b'(' => depth += 1,
            b')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(cursor);
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn top_level_arithmetic_operator(source: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    let mut depth = 0_u32;
    let mut additive = None;
    let mut multiplicative = None;
    while cursor < bytes.len() {
        let next = skip_non_code(source, cursor);
        if next != cursor {
            cursor = next;
            continue;
        }
        match bytes[cursor] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'+' | b'-' if depth == 0 && is_binary_additive_operator(source, cursor) => {
                additive = Some(cursor)
            }
            b'*' | b'/' | b'%' if depth == 0 && bytes.get(cursor + 1) != Some(&b'=') => {
                multiplicative = Some(cursor)
            }
            _ => {}
        }
        cursor += 1;
    }
    additive.or(multiplicative)
}

fn is_binary_additive_operator(source: &str, offset: usize) -> bool {
    let bytes = source.as_bytes();
    if matches!(bytes.get(offset + 1), Some(b'+' | b'-' | b'=')) {
        return false;
    }
    let mut previous = offset;
    while previous > 0 {
        previous -= 1;
        if !bytes[previous].is_ascii_whitespace() {
            return !matches!(
                bytes[previous],
                b'(' | b'['
                    | b'{'
                    | b','
                    | b';'
                    | b'?'
                    | b':'
                    | b'='
                    | b'+'
                    | b'-'
                    | b'*'
                    | b'/'
                    | b'%'
                    | b'!'
                    | b'&'
                    | b'|'
                    | b'^'
                    | b'<'
                    | b'>'
            );
        }
    }
    false
}

fn lower_vector_leaf(
    expression: &str,
    start: usize,
    end: usize,
    target_width: u8,
    symbols: &BTreeMap<String, u8>,
) -> String {
    let core = &expression[start..end];
    let Some(width) = symbols
        .get(core)
        .copied()
        .or_else(|| vector_constructor_width(core))
    else {
        return expression.to_owned();
    };
    if width <= target_width {
        return expression.to_owned();
    }
    format!(
        "{}{}.{}{}",
        &expression[..start],
        core,
        component_suffix(target_width),
        &expression[end..]
    )
}

fn vector_constructor_width(expression: &str) -> Option<u8> {
    let bytes = expression.as_bytes();
    if bytes.is_empty() || !is_identifier_start(bytes[0]) {
        return None;
    }
    let name_end = identifier_end(bytes, 0);
    let width = vector_width(&expression[..name_end])?;
    let opening = skip_trivia(expression, name_end);
    (bytes.get(opening) == Some(&b'(')
        && matching_parenthesis(expression, opening) == Some(bytes.len() - 1))
    .then_some(width)
}

fn component_suffix(width: u8) -> &'static str {
    match width {
        2 => "xy",
        3 => "xyz",
        4 => "xyzw",
        _ => unreachable!("generated floating vector has an unsupported width"),
    }
}

fn skip_non_code(source: &str, cursor: usize) -> usize {
    let bytes = source.as_bytes();
    if bytes.get(cursor..cursor + 2) == Some(b"//") {
        return bytes[cursor..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| cursor + offset + 1);
    }
    if bytes.get(cursor..cursor + 2) == Some(b"/*") {
        return bytes[cursor + 2..]
            .windows(2)
            .position(|pair| pair == b"*/")
            .map_or(bytes.len(), |offset| cursor + 4 + offset);
    }
    if bytes.get(cursor) == Some(&b'\"') {
        let mut offset = cursor + 1;
        while offset < bytes.len() {
            if bytes[offset] == b'\\' {
                offset = offset.saturating_add(2);
                continue;
            }
            offset += 1;
            if bytes[offset - 1] == b'\"' {
                break;
            }
        }
        return offset;
    }
    cursor
}

fn skip_trivia(source: &str, mut cursor: usize) -> usize {
    let bytes = source.as_bytes();
    loop {
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let next = skip_non_code(source, cursor);
        if next == cursor {
            return cursor;
        }
        cursor = next;
    }
}

fn identifier_end(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes
        .get(cursor)
        .is_some_and(|byte| is_identifier_continue(*byte))
    {
        cursor += 1;
    }
    cursor
}

fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn statement_end(source: &str, mut cursor: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    while cursor < bytes.len() {
        let next = skip_non_code(source, cursor);
        if next != cursor {
            cursor = next;
            continue;
        }
        if bytes[cursor] == b';' {
            return Some(cursor);
        }
        cursor += 1;
    }
    None
}

fn trim_ascii_whitespace(source: &str, start: usize, mut end: usize) -> usize {
    let bytes = source.as_bytes();
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

fn has_top_level_comma(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    let mut nesting = 0_u32;
    while cursor < bytes.len() {
        let next = skip_non_code(source, cursor);
        if next != cursor {
            cursor = next;
            continue;
        }
        match bytes[cursor] {
            b'(' | b'[' | b'{' => nesting += 1,
            b')' | b']' | b'}' => nesting = nesting.saturating_sub(1),
            b',' if nesting == 0 => return true,
            _ => {}
        }
        cursor += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{ShaderCompileRequest, ShaderContract, ShaderStage, SlangCompiler};

    #[test]
    fn makes_simple_vector_initializer_conversions_explicit() {
        let lowered = lower_generated_vector_initializer_conversions(
            "vec4 texture0; vec4 texture1; vec2 scale = texture0 / texture1; vec3 color = vec4(1.0);"
                .to_owned(),
            &Declarations::default(),
        );

        assert_eq!(
            lowered,
            "vec4 texture0; vec4 texture1; vec2 scale = texture0.xy / texture1.xy; vec3 color = vec4(1.0).xyz;"
        );
    }

    #[test]
    fn preserves_non_simple_vector_declarations_and_comments() {
        let lowered = lower_generated_vector_initializer_conversions(
            "// vec2 ignored = wide;\nvec2 values[2] = vec2[2](a, b); vec2 left = a, right = b; vec2 function(vec4 value) { return value.xy; }".to_owned(),
            &Declarations::default(),
        );

        assert_eq!(
            lowered,
            "// vec2 ignored = wide;\nvec2 values[2] = vec2[2](a, b); vec2 left = a, right = b; vec2 function(vec4 value) { return value.xy; }"
        );
    }

    #[test]
    fn narrows_wide_operands_at_vector_compound_assignments() {
        let lowered = lower_generated_vector_initializer_conversions(
            "vec4 resolution; vec2 strength; strength *= 500 / resolution;".to_owned(),
            &Declarations::default(),
        );

        assert_eq!(
            lowered,
            "vec4 resolution; vec2 strength; strength *= 500 / resolution.xy;"
        );
    }

    #[test]
    fn production_o2_compiles_we_vector_narrowing_at_the_declared_width() {
        let direct = crate::lower_generated_stage_to_slang(
            r#"layout(location = 0) in vec3 a_Position;
layout(location = 0) out vec4 v_TexCoord;
uniform vec4 g_Texture0Resolution;
uniform vec4 g_Texture1Resolution;
uniform vec2 g_TexOffset;
void main() {
    vec2 scale = g_Texture0Resolution / g_Texture1Resolution;
    vec2 offset = g_TexOffset / g_Texture0Resolution;
    vec2 strength = vec2(1.0);
    strength *= 500 / g_Texture0Resolution;
    v_TexCoord = vec4(scale + offset + strength, 0.0, 1.0);
    gl_Position = vec4(a_Position, 1.0);
}"#,
            ShaderStage::Vertex,
        )
        .expect("WE vector narrowing Slang lowering");
        assert!(direct.contains(
            "vec2 scale = vulkanRendererUniforms0.g_Texture0Resolution.xy / vulkanRendererUniforms0.g_Texture1Resolution.xy;"
        ));
        assert!(direct.contains(
            "vec2 offset = vulkanRendererUniforms0.g_TexOffset / vulkanRendererUniforms0.g_Texture0Resolution.xy;"
        ));
        assert!(
            direct.contains("strength *= 500 / vulkanRendererUniforms0.g_Texture0Resolution.xy;")
        );

        let heap = crate::lower_slang_bindings_to_descriptor_heap(&direct, "main")
            .expect("WE vector narrowing descriptor-heap lowering");
        let base = std::env::temp_dir().join(format!(
            "vulkan-renderer-build-vector-initializer-{}",
            std::process::id()
        ));
        let source_path = base.with_extension("slang");
        let output_path = base.with_extension("spv");
        fs::write(&source_path, heap.source).expect("write vector initializer source");
        SlangCompiler::from_environment()
            .compile(&ShaderCompileRequest {
                source: source_path.clone(),
                entry_point: "main".to_owned(),
                stage: ShaderStage::Vertex,
                output: output_path.clone(),
                contract: ShaderContract::descriptor_heap(u64::from(heap.push_constant_bytes)),
            })
            .expect("fixed-Slang production O2 compile for vector narrowing");

        let _ = fs::remove_file(source_path);
        let _ = fs::remove_file(output_path);
    }
}
