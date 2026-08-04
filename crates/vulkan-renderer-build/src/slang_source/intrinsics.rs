use std::collections::BTreeSet;

use super::Declarations;

pub(super) fn lower_generated_intrinsics(
    source: String,
    declarations: &Declarations,
) -> Result<String, String> {
    let matrix_identifiers = matrix_identifiers(&source, declarations);
    let source = lower_we_row_vector_matrix_mul(&source, &matrix_identifiers)?;
    lower_two_argument_atan(qualify_global_constants(source))
}

fn matrix_identifiers(source: &str, declarations: &Declarations) -> BTreeSet<String> {
    let mut identifiers = BTreeSet::new();
    for uniforms in declarations.uniforms_by_binding.values() {
        for uniform in uniforms {
            if is_matrix_type(&uniform.ty) {
                identifiers.insert(plain_declarator(&uniform.declarator).to_owned());
            }
        }
    }
    for block in &declarations.uniform_blocks {
        for declaration in block.body.split(';') {
            let mut parts = declaration.split_ascii_whitespace();
            if let (Some(ty), Some(name)) = (parts.next(), parts.next())
                && is_matrix_type(ty)
            {
                identifiers.insert(format!("{}.{}", block.instance, plain_declarator(name)));
            }
        }
    }
    collect_source_matrix_declarations(source, &mut identifiers);
    identifiers
}

fn plain_declarator(declarator: &str) -> &str {
    declarator
        .split_once('[')
        .map_or(declarator, |(name, _)| name)
}

fn is_matrix_type(ty: &str) -> bool {
    matches!(
        ty,
        "mat2" | "mat3" | "mat4" | "float2x2" | "float3x3" | "float4x4"
    )
}

fn collect_source_matrix_declarations(source: &str, identifiers: &mut BTreeSet<String>) {
    for ty in ["mat2", "mat3", "mat4", "float2x2", "float3x3", "float4x4"] {
        for (offset, _) in source.match_indices(ty) {
            let before = source[..offset].chars().next_back();
            let after_offset = offset + ty.len();
            let after = source[after_offset..].chars().next();
            if before.is_some_and(is_identifier_character)
                || after.is_some_and(is_identifier_character)
            {
                continue;
            }
            let name_start = source[after_offset..]
                .char_indices()
                .find(|(_, character)| !character.is_whitespace())
                .map_or(source.len(), |(relative, _)| after_offset + relative);
            if source[name_start..].starts_with('(') {
                continue;
            }
            let name_end = source[name_start..]
                .char_indices()
                .find(|(_, character)| !is_identifier_character(*character))
                .map_or(source.len(), |(relative, _)| name_start + relative);
            if name_start == name_end {
                continue;
            }
            let terminator = source[name_end..]
                .chars()
                .find(|character| !character.is_whitespace());
            if terminator.is_some_and(|character| matches!(character, ';' | '=' | '[' | ',' | '('))
            {
                identifiers.insert(source[name_start..name_end].to_owned());
            }
        }
    }
}

fn lower_we_row_vector_matrix_mul(
    source: &str,
    matrix_identifiers: &BTreeSet<String>,
) -> Result<String, String> {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some((call_start, arguments_start, arguments_end)) = next_mul_call(source, cursor)? {
        output.push_str(&source[cursor..call_start]);
        let arguments = &source[arguments_start..arguments_end];
        let (left, right) = split_two_arguments(arguments)?;
        let left = lower_we_row_vector_matrix_mul(left, matrix_identifiers)?;
        let right = lower_we_row_vector_matrix_mul(right, matrix_identifiers)?;
        let left_is_matrix = expression_is_matrix(&left, matrix_identifiers);
        let right_is_matrix = expression_is_matrix(&right, matrix_identifiers);
        if !left_is_matrix && right_is_matrix {
            output.push_str("mul(");
            output.push_str(right.trim());
            output.push_str(", ");
            output.push_str(left.trim());
            output.push(')');
        } else {
            output.push_str("mul(");
            output.push_str(&left);
            output.push(',');
            output.push_str(&right);
            output.push(')');
        }
        cursor = arguments_end + 1;
    }
    output.push_str(&source[cursor..]);
    Ok(output)
}

fn next_mul_call(source: &str, mut cursor: usize) -> Result<Option<(usize, usize, usize)>, String> {
    while let Some(relative) = source[cursor..].find("mul") {
        let call_start = cursor + relative;
        let name_end = call_start + "mul".len();
        let before = source[..call_start].chars().next_back();
        let after = source[name_end..].chars().next();
        if before.is_some_and(is_identifier_character) || after.is_some_and(is_identifier_character)
        {
            cursor = name_end;
            continue;
        }
        let Some((relative, character)) = source[name_end..]
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
        else {
            return Ok(None);
        };
        if character != '(' {
            cursor = name_end;
            continue;
        }
        let open = name_end + relative;
        let close = super::matching_delimiter(source, open, '(', ')')?;
        return Ok(Some((call_start, open + 1, close)));
    }
    Ok(None)
}

fn split_two_arguments(arguments: &str) -> Result<(&str, &str), String> {
    let mut depth = 0u32;
    let mut split = None;
    for (offset, character) in arguments.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 && split.replace(offset).is_some() => {
                return Err("generated mul has more than two top-level arguments".to_owned());
            }
            _ => {}
        }
    }
    let split = split.ok_or_else(|| "generated mul does not have two arguments".to_owned())?;
    Ok((&arguments[..split], &arguments[split + 1..]))
}

fn expression_is_matrix(expression: &str, matrix_identifiers: &BTreeSet<String>) -> bool {
    matrix_identifiers
        .iter()
        .any(|identifier| has_identifier(expression, identifier))
        || ["mat2", "mat3", "mat4", "float2x2", "float3x3", "float4x4"]
            .iter()
            .any(|ty| has_identifier(expression, ty))
}

fn has_identifier(source: &str, identifier: &str) -> bool {
    source.match_indices(identifier).any(|(offset, _)| {
        let before = source[..offset].chars().next_back();
        let after = source[offset + identifier.len()..].chars().next();
        !before.is_some_and(is_identifier_character) && !after.is_some_and(is_identifier_character)
    })
}

fn qualify_global_constants(source: String) -> String {
    let mut depth = 0u32;
    source
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let lowered = if depth == 0 && trimmed.starts_with("const ") {
                line.replacen("const ", "static const ", 1)
            } else {
                line.to_owned()
            };
            for character in line.chars() {
                match character {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            lowered
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn lower_two_argument_atan(source: String) -> Result<String, String> {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    while let Some(relative) = source[cursor..].find("atan") {
        let offset = cursor + relative;
        let end = offset + "atan".len();
        let before = source[..offset].chars().next_back();
        let after = source[end..].chars().next();
        if before.is_some_and(is_identifier_character) || after.is_some_and(is_identifier_character)
        {
            output.push_str(&source[cursor..end]);
            cursor = end;
            continue;
        }
        let arguments_start = source[end..]
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
            .map(|(relative, character)| end + relative + character.len_utf8())
            .filter(|start| source.as_bytes()[start - 1] == b'(');
        let Some(arguments_start) = arguments_start else {
            output.push_str(&source[cursor..end]);
            cursor = end;
            continue;
        };
        let arguments_end = super::matching_delimiter(&source, arguments_start - 1, '(', ')')?;
        if has_two_top_level_arguments(&source[arguments_start..arguments_end]) {
            output.push_str(&source[cursor..offset]);
            output.push_str("atan2");
            cursor = end;
        } else {
            output.push_str(&source[cursor..end]);
            cursor = end;
        }
    }
    output.push_str(&source[cursor..]);
    Ok(output)
}

fn has_two_top_level_arguments(arguments: &str) -> bool {
    let mut depth = 0u32;
    let mut commas = 0u32;
    for character in arguments.chars() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    commas == 1
}

fn is_identifier_character(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_only_two_argument_atan_and_global_constants() {
        let source = "const float TAU = 6.0;\nfloat f(float x) { const float y = atan(x); return atan(x, y); }";
        let lowered =
            lower_generated_intrinsics(source.to_owned(), &Declarations::default()).unwrap();

        assert!(lowered.starts_with("static const float TAU"));
        assert!(lowered.contains("const float y = atan(x)"));
        assert!(lowered.contains("return atan2(x, y)"));
    }

    #[test]
    fn swaps_only_we_row_vector_matrix_mul_operands() {
        let source =
            "mat4 model; mat4 view; vec4 point; vec4 transformed = mul(point, mul(model, view));";
        let lowered =
            lower_generated_intrinsics(source.to_owned(), &Declarations::default()).unwrap();

        assert!(lowered.contains("mul(mul(model, view), point)"));
    }
}
