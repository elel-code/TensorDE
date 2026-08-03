const BASE: &str = r#"#define vec2 float2
#define vec3 float3
#define vec4 float4
#define ivec2 int2
#define ivec3 int3
#define ivec4 int4
#define uvec2 uint2
#define uvec3 uint3
#define uvec4 uint4
#define bvec2 bool2
#define bvec3 bool3
#define bvec4 bool4
#define mat2 float2x2
#define mat3 float3x3
#define mat4 float4x4
#define fract frac
#define mix lerp
#define inversesqrt rsqrt
#define roundEven round
#define texture2D(S, UV) S ## _texture.Sample(S ## _sampler, UV)
#define texture3D(S, UVW) S ## _texture.Sample(S ## _sampler, UVW)
#define texture(S, UV) S ## _texture.Sample(S ## _sampler, UV)
#define texture2DLod(S, UV, LOD) S ## _texture.SampleLevel(S ## _sampler, UV, LOD)
#define textureLod(S, UV, LOD) S ## _texture.SampleLevel(S ## _sampler, UV, LOD)
#define texelFetch(S, COORD, LOD) S ## _texture.Load(int3(COORD, LOD))
#define textureSize(S, LOD) gilderTextureSize_ ## S(LOD)
#define greaterThan(A, B) ((A) > (B))
#define greaterThanEqual(A, B) ((A) >= (B))
#define lessThan(A, B) ((A) < (B))
#define lessThanEqual(A, B) ((A) <= (B))
#define equal(A, B) ((A) == (B))
#define notEqual(A, B) ((A) != (B))
"#;

pub(super) fn emit(source: &str) -> String {
    let mut output = BASE.to_owned();
    if !source_defines_function(source, "mod") {
        output.push_str("#define mod(A, B) ((A) - (B) * floor((A) / (B)))\n");
    }
    output
}

fn source_defines_function(source: &str, name: &str) -> bool {
    identifier_offsets_outside_comments(source, name)
        .into_iter()
        .any(|offset| {
            let name_end = offset + name.len();
            let Some((open_relative, '(')) = source[name_end..]
                .char_indices()
                .find(|(_, character)| !character.is_whitespace())
            else {
                return false;
            };
            let open = name_end + open_relative;
            let Ok(close) = super::matching_delimiter(source, open, '(', ')') else {
                return false;
            };
            source[close + 1..]
                .chars()
                .find(|character| !character.is_whitespace())
                == Some('{')
        })
}

fn identifier_offsets_outside_comments(source: &str, name: &str) -> Vec<usize> {
    let bytes = source.as_bytes();
    let mut offsets = Vec::new();
    let mut cursor = 0;
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
                .map_or(bytes.len(), |relative| cursor + relative + 1);
            continue;
        }
        if bytes[cursor..].starts_with(b"/*") {
            in_block_comment = true;
            cursor += 2;
            continue;
        }
        if bytes[cursor].is_ascii_alphabetic() || bytes[cursor] == b'_' {
            let start = cursor;
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'_')
            {
                cursor += 1;
            }
            if &source[start..cursor] == name {
                offsets.push(start);
            }
        } else {
            cursor += 1;
        }
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omits_only_the_compatibility_macro_owned_by_an_authored_function() {
        let authored = emit(
            "// float mod(float a, float b) { return 0; }\nfloat mod(float a, float b) { return a; }",
        );
        assert!(!authored.contains("#define mod"));

        let comment_only = emit("/* float mod(float a, float b) { return 0; } */");
        assert!(comment_only.contains("#define mod"));
    }
}
