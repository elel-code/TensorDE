//! Explicit native-Slang constructors for generated integer initializers.
//!
//! Wallpaper Engine's generated stage dialect accepts legacy numeric coercions
//! such as `int flag = step(...)`. Native Slang diagnoses the implicit
//! float-to-integer conversion, so preserve that authored coercion explicitly
//! at the cold generated-stage boundary.

use std::collections::BTreeMap;

const INTEGER_TYPES: &[&str] = &[
    "int", "uint", "ivec2", "ivec3", "ivec4", "uvec2", "uvec3", "uvec4", "int2", "int3", "int4",
    "uint2", "uint3", "uint4",
];

/// Wraps each simple generated integer variable initializer in the declared
/// type's constructor. This is deliberately syntax-directed: array and
/// multi-declarator statements remain untouched instead of guessing their
/// semantics.
pub(super) fn lower_generated_integer_initializers(source: String) -> String {
    let source = lower_integer_declaration_initializers(source);
    lower_known_float_integer_arguments(source)
}

fn lower_integer_declaration_initializers(source: String) -> String {
    let mut lowered = String::with_capacity(source.len());
    let mut cursor = 0;

    while let Some(initializer) = next_integer_initializer(&source, cursor) {
        lowered.push_str(&source[cursor..initializer.expression_start]);
        lowered.push_str(initializer.ty);
        lowered.push('(');
        lowered.push_str(&source[initializer.expression_start..initializer.expression_end]);
        lowered.push(')');
        cursor = initializer.expression_end;
    }

    lowered.push_str(&source[cursor..]);
    lowered
}

/// Makes the legacy float-to-integer conversion explicit when a statically
/// known floating scalar is passed to a user-defined integer parameter.
///
/// This only handles direct calls with a parsed function signature and an
/// exact scalar symbol. Nested/unknown expressions remain native-Slang errors
/// rather than receiving a guessed cast.
fn lower_known_float_integer_arguments(source: String) -> String {
    let signatures = integer_parameter_signatures(&source);
    if signatures.is_empty() {
        return source;
    }
    let scalar_symbols = scalar_symbols(&source);
    let mut lowered = String::with_capacity(source.len());
    let mut cursor = 0;

    while let Some(call) = next_integer_parameter_call(&source, cursor, &signatures) {
        lowered.push_str(&source[cursor..call.arguments_start]);
        lowered.push_str(&lower_call_arguments(
            &source[call.arguments_start..call.arguments_end],
            &call.parameters,
            &scalar_symbols,
        ));
        cursor = call.arguments_end;
    }

    lowered.push_str(&source[cursor..]);
    lowered
}

struct IntegerInitializer<'a> {
    ty: &'a str,
    expression_start: usize,
    expression_end: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IntegerParameter {
    Int,
    Uint,
}

impl IntegerParameter {
    fn constructor(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Uint => "uint",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScalarSymbol {
    Float,
    Integer,
}

struct IntegerParameterCall {
    arguments_start: usize,
    arguments_end: usize,
    parameters: Vec<Option<IntegerParameter>>,
}

fn integer_parameter_signatures(source: &str) -> BTreeMap<String, Vec<Option<IntegerParameter>>> {
    let bytes = source.as_bytes();
    let mut signatures = BTreeMap::new();
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
        cursor = identifier_end(bytes, cursor);
        let name_start = skip_trivia(source, cursor);
        if name_start >= bytes.len() || !is_identifier_start(bytes[name_start]) {
            continue;
        }
        let name_end = identifier_end(bytes, name_start);
        let opening = skip_trivia(source, name_end);
        if bytes.get(opening) != Some(&b'(') {
            continue;
        }
        let Some(closing) = matching_parenthesis(source, opening) else {
            cursor = opening.saturating_add(1);
            continue;
        };
        let after = skip_trivia(source, closing + 1);
        if !matches!(bytes.get(after), Some(b'{' | b';')) {
            cursor = closing.saturating_add(1);
            continue;
        }
        let parameters = integer_parameters(&source[opening + 1..closing]);
        if parameters.iter().any(Option::is_some) {
            signatures.insert(source[name_start..name_end].to_owned(), parameters);
        }
        cursor = closing.saturating_add(1);
    }
    signatures
}

fn integer_parameters(source: &str) -> Vec<Option<IntegerParameter>> {
    argument_ranges(source)
        .into_iter()
        .map(|(start, end)| {
            let ty = source[start..end]
                .split_ascii_whitespace()
                .find(|token| !matches!(*token, "const" | "in" | "out" | "inout"));
            match ty {
                Some("int") => Some(IntegerParameter::Int),
                Some("uint") => Some(IntegerParameter::Uint),
                _ => None,
            }
        })
        .collect()
}

fn scalar_symbols(source: &str) -> BTreeMap<String, ScalarSymbol> {
    let bytes = source.as_bytes();
    let mut symbols = BTreeMap::new();
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
        let kind = match &source[type_start..cursor] {
            "float" => ScalarSymbol::Float,
            "int" | "uint" => ScalarSymbol::Integer,
            _ => continue,
        };
        let name_start = skip_trivia(source, cursor);
        if name_start >= bytes.len() || !is_identifier_start(bytes[name_start]) {
            continue;
        }
        let name_end = identifier_end(bytes, name_start);
        if bytes.get(skip_trivia(source, name_end)) != Some(&b'(') {
            symbols.insert(source[name_start..name_end].to_owned(), kind);
        }
    }
    symbols
}

fn next_integer_parameter_call(
    source: &str,
    mut cursor: usize,
    signatures: &BTreeMap<String, Vec<Option<IntegerParameter>>>,
) -> Option<IntegerParameterCall> {
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
        let name_start = cursor;
        cursor = identifier_end(bytes, cursor);
        let Some(parameters) = signatures.get(&source[name_start..cursor]) else {
            continue;
        };
        let opening = skip_trivia(source, cursor);
        if bytes.get(opening) != Some(&b'(') {
            continue;
        }
        let closing = matching_parenthesis(source, opening)?;
        if bytes.get(skip_trivia(source, closing + 1)) == Some(&b'{') {
            cursor = closing.saturating_add(1);
            continue;
        }
        return Some(IntegerParameterCall {
            arguments_start: opening + 1,
            arguments_end: closing,
            parameters: parameters.clone(),
        });
    }
    None
}

fn lower_call_arguments(
    source: &str,
    parameters: &[Option<IntegerParameter>],
    symbols: &BTreeMap<String, ScalarSymbol>,
) -> String {
    let arguments = argument_ranges(source);
    if arguments.len() != parameters.len() {
        return source.to_owned();
    }
    let mut lowered = String::with_capacity(source.len());
    let mut cursor = 0;
    for ((start, end), parameter) in arguments.into_iter().zip(parameters) {
        lowered.push_str(&source[cursor..start]);
        let argument = &source[start..end];
        if let Some(parameter) = parameter
            && argument_is_known_float(argument, symbols)
        {
            lowered.push_str(&wrap_integer_argument(argument, *parameter));
        } else {
            lowered.push_str(argument);
        }
        cursor = end;
    }
    lowered.push_str(&source[cursor..]);
    lowered
}

fn argument_is_known_float(argument: &str, symbols: &BTreeMap<String, ScalarSymbol>) -> bool {
    symbols.get(argument.trim()) == Some(&ScalarSymbol::Float)
}

fn wrap_integer_argument(argument: &str, parameter: IntegerParameter) -> String {
    let start = argument
        .bytes()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(argument.len());
    let end = trim_ascii_whitespace(argument, start, argument.len());
    format!(
        "{}{}({}){}",
        &argument[..start],
        parameter.constructor(),
        &argument[start..end],
        &argument[end..]
    )
}

fn next_integer_initializer(source: &str, mut cursor: usize) -> Option<IntegerInitializer<'_>> {
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
        if !INTEGER_TYPES.contains(&ty) {
            continue;
        }
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
        return Some(IntegerInitializer {
            ty,
            expression_start,
            expression_end,
        });
    }
    None
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

fn argument_ranges(source: &str) -> Vec<(usize, usize)> {
    if source.trim().is_empty() {
        return Vec::new();
    }
    let bytes = source.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 0;
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
            b',' if nesting == 0 => {
                ranges.push((start, cursor));
                start = cursor + 1;
            }
            _ => {}
        }
        cursor += 1;
    }
    ranges.push((start, bytes.len()));
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_simple_integer_initializers_without_touching_multi_declarators() {
        let lowered = lower_generated_integer_initializers(
            "int flag = step(a, b) * step(c, d); int left = 0, right = 1;".to_owned(),
        );

        assert_eq!(
            lowered,
            "int flag = int(step(a, b) * step(c, d)); int left = 0, right = 1;"
        );
    }

    #[test]
    fn handles_for_initializer_and_ignores_comments() {
        let lowered = lower_generated_integer_initializers(
            "// int ignored = step(a, b);\nfor (int index = value; index < 4; ++index) {}"
                .to_owned(),
        );

        assert!(lowered.starts_with("// int ignored = step(a, b);"));
        assert!(lowered.contains("int index = int(value);"));
    }

    #[test]
    fn wraps_known_float_arguments_for_integer_parameters() {
        let lowered = lower_generated_integer_initializers(
            "float consume(in int value) { return float(value); } float mask = 1.0; float result = consume(mask);"
                .to_owned(),
        );

        assert_eq!(
            lowered,
            "float consume(in int value) { return float(value); } float mask = 1.0; float result = consume(int(mask));"
        );
    }
}
