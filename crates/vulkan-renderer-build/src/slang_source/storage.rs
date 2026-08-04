use super::{declaration_parts, layout_binding, split_layout};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StorageBuffer {
    instance: String,
    member: String,
    element_type: String,
    binding: u32,
    read_only: bool,
}

impl StorageBuffer {
    pub(super) fn instance(&self) -> &str {
        &self.instance
    }
}

pub(super) fn parse_storage_buffer(
    lines: &[&str],
) -> Result<Option<(StorageBuffer, usize)>, String> {
    let header = lines
        .first()
        .ok_or_else(|| "missing generated storage-buffer header".to_owned())?
        .trim();
    let Some((layout, rest)) = split_layout(header)? else {
        return Ok(None);
    };
    let (read_only, rest) = if let Some(rest) = rest.strip_prefix("readonly ") {
        (true, rest)
    } else if let Some(rest) = rest.strip_prefix("writeonly ") {
        (false, rest)
    } else {
        (false, rest)
    };
    let Some(rest) = rest.strip_prefix("buffer ") else {
        return Ok(None);
    };
    let name = rest
        .split_once('{')
        .map(|(name, _)| name.trim())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("invalid generated storage-buffer header: {header}"))?;
    if !rest.ends_with('{') {
        return Err(format!(
            "generated storage-buffer opening brace must terminate its header: {header}"
        ));
    }
    let mut members: Vec<(String, String)> = Vec::new();
    for (offset, line) in lines.iter().enumerate().skip(1) {
        let trimmed = line.trim();
        if let Some(instance) = trimmed
            .strip_prefix('}')
            .map(str::trim)
            .and_then(|line| line.strip_suffix(';'))
        {
            let instance = instance.trim();
            if instance.is_empty() {
                return Err(format!(
                    "generated storage buffer {name} lacks an instance name"
                ));
            }
            let [(element_type, member)] = members.as_slice() else {
                return Err(format!(
                    "generated storage buffer {name} must contain exactly one unsized member"
                ));
            };
            let member = member.strip_suffix("[]").ok_or_else(|| {
                format!("generated storage buffer {name} member {member} is not unsized")
            })?;
            if member.is_empty() {
                return Err(format!(
                    "generated storage buffer {name} has an empty member name"
                ));
            }
            return Ok(Some((
                StorageBuffer {
                    instance: instance.to_owned(),
                    member: member.to_owned(),
                    element_type: element_type.clone(),
                    binding: layout_binding(layout)?,
                    read_only,
                },
                offset + 1,
            )));
        }
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        members.push(declaration_parts(trimmed)?);
    }
    Err(format!("unterminated generated storage buffer {name}"))
}

pub(super) fn rewrite_member_accesses(
    mut source: String,
    buffers: &[StorageBuffer],
) -> Result<String, String> {
    for buffer in buffers {
        let access = format!("{}.{}", buffer.instance, buffer.member);
        source = replace_access(&source, &access, &buffer.instance);
        let residual = format!("{}.", buffer.instance);
        if source.contains(&residual) {
            return Err(format!(
                "generated storage buffer {} has an unsupported member access",
                buffer.instance
            ));
        }
    }
    Ok(source)
}

pub(super) fn emit_storage_buffer(buffer: &StorageBuffer) -> String {
    let (kind, register) = if buffer.read_only {
        ("StructuredBuffer", 't')
    } else {
        ("RWStructuredBuffer", 'u')
    };
    format!(
        "{kind}<{}> {} : register({register}{});\n",
        buffer.element_type, buffer.instance, buffer.binding
    )
}

fn replace_access(source: &str, access: &str, replacement: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for (offset, _) in source.match_indices(access) {
        let before = source[..offset].chars().next_back();
        let after = source[offset + access.len()..].chars().next();
        if before.is_some_and(is_identifier_character) || after.is_some_and(is_identifier_character)
        {
            continue;
        }
        output.push_str(&source[cursor..offset]);
        output.push_str(replacement);
        cursor = offset + access.len();
    }
    output.push_str(&source[cursor..]);
    output
}

fn is_identifier_character(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_a_read_only_unsized_block_to_a_structured_buffer() {
        let source = [
            "layout(std430, set = 0, binding = 4) readonly buffer Bones {",
            "    float4 rows[];",
            "} g_Bones;",
        ];
        let (buffer, consumed) = parse_storage_buffer(&source).unwrap().unwrap();

        assert_eq!(consumed, 3);
        assert_eq!(
            emit_storage_buffer(&buffer),
            "StructuredBuffer<float4> g_Bones : register(t4);\n"
        );
        assert_eq!(
            rewrite_member_accesses("float4 value = g_Bones.rows[index];".to_owned(), &[buffer])
                .unwrap(),
            "float4 value = g_Bones[index];"
        );
    }
}
