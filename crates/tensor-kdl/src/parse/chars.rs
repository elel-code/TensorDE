//! KDL 2.0 character classes from the specification tables.

/// Non-newline whitespace (KDL "unicode-space").
#[inline(always)]
pub const fn is_unicode_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            | '\u{2001}'
            | '\u{2002}'
            | '\u{2003}'
            | '\u{2004}'
            | '\u{2005}'
            | '\u{2006}'
            | '\u{2007}'
            | '\u{2008}'
            | '\u{2009}'
            | '\u{200A}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

/// Single newline code points (CRLF handled separately as one newline).
#[inline(always)]
pub const fn is_newline_char(c: char) -> bool {
    matches!(
        c,
        '\u{000D}' | '\u{000A}' | '\u{0085}' | '\u{000B}' | '\u{000C}' | '\u{2028}' | '\u{2029}'
    )
}

/// Code points forbidden as literal text anywhere in a KDL document
/// (except BOM as the first code point, handled by the reader).
#[inline(always)]
pub const fn is_disallowed_literal(c: char) -> bool {
    let u = c as u32;
    matches!(u, 0x0000..=0x0008 | 0x000E..=0x001F | 0x007F)
        || matches!(u, 0x200E..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069)
        || u == 0xFEFF
}

/// Characters that cannot appear in bare identifier strings.
#[inline(always)]
pub const fn is_non_identifier_char(c: char) -> bool {
    is_unicode_space(c)
        || is_newline_char(c)
        || matches!(
            c,
            '\\' | '/' | '(' | ')' | '{' | '}' | ';' | '[' | ']' | '"' | '#' | '='
        )
        || is_disallowed_literal(c)
}
