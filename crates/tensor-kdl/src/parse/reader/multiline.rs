//! Multi-line `"""` / raw multi-line strings (KDL 2 dedent + ws-escape rules).

use crate::error::{CtxResult, ErrorCode};
use crate::parse::chars::{is_disallowed_literal, is_newline_char, is_unicode_space};
use crate::value::KdlStr;

use crate::parse::reader::Parser;

impl<'a> Parser<'a> {
    pub(super) fn parse_multiline_quoted_string(&mut self) -> CtxResult<KdlStr<'a>> {
        self.parse_multiline_string(0, false)
    }

    pub(super) fn parse_raw_multiline(&mut self, hashes: usize) -> CtxResult<KdlStr<'a>> {
        self.parse_multiline_string(hashes, true)
    }

    fn parse_multiline_string(&mut self, hashes: usize, raw: bool) -> CtxResult<KdlStr<'a>> {
        if !self.consume_exact("\"\"\"") {
            return Err(self.err(ErrorCode::Syntax));
        }
        if !self.consume_newline() {
            return Err(self
                .err(ErrorCode::Syntax)
                .with_message("multiline string must start with a newline after \"\"\""));
        }
        let closer = format!("\"\"\"{}", "#".repeat(hashes));
        let body_start = self.index;
        let (close_at, end_index) = self
            .find_multiline_closer(body_start, &closer, raw)
            .ok_or_else(|| {
                self.err(ErrorCode::UnexpectedEof)
                    .with_message("unclosed multiline string")
            })?;
        let raw_body = &self.input[body_start..close_at];
        let text = if raw {
            process_raw_multiline(raw_body)
                .map_err(|msg| self.err_at(ErrorCode::Syntax, body_start).with_message(msg))?
        } else {
            process_escaped_multiline(raw_body)
                .map_err(|msg| self.err_at(ErrorCode::Syntax, body_start).with_message(msg))?
        };
        self.index = end_index;
        if text.len() > self.ctx.max_string_len {
            return Err(self.err(ErrorCode::ExceededLimit));
        }
        Ok(KdlStr::owned(text))
    }

    /// Locate the closing delimiter.
    ///
    /// Returns `(index_of_closer, index_after_closer)`.
    fn find_multiline_closer(
        &self,
        body_start: usize,
        closer: &str,
        raw: bool,
    ) -> Option<(usize, usize)> {
        if raw {
            // Cut-point: first matching closer sequence ends the string.
            return self.input[body_start..].find(closer).map(|rel| {
                let at = body_start + rel;
                (at, at + closer.len())
            });
        }

        // Quoted multiline: a closer is valid if its line prefix is only
        // unicode-spaces and/or ws-escapes (`\` + space/newline runs).
        // Also skip `"""` that are escaped as `\"""` in the body.
        let mut pos = body_start;
        while let Some(rel) = self.input[pos..].find(closer) {
            let at = pos + rel;
            // Escaped delimiter: odd number of backslashes immediately before.
            if backslash_escape_count_before(self.input, at) % 2 == 1 {
                pos = at + 1;
                continue;
            }
            let line_start = self.input[..at]
                .rfind('\n')
                .map(|p| p + 1)
                .unwrap_or(body_start);
            // Closing line prefix is from line_start, but not before body_start.
            let prefix_start = line_start.max(body_start);
            let prefix = &self.input[prefix_start..at];
            // If closer is on the first body line with no prior newline in body,
            // prefix is from body_start — still OK for empty indented `"""\n\t"""`.
            if closing_line_prefix_ok(prefix) {
                return Some((at, at + closer.len()));
            }
            pos = at + 1;
        }
        None
    }
}

fn backslash_escape_count_before(input: &str, index: usize) -> usize {
    let bytes = input.as_bytes();
    let mut n = 0usize;
    let mut i = index;
    while i > 0 && bytes[i - 1] == b'\\' {
        n += 1;
        i -= 1;
    }
    n
}

fn closing_line_prefix_ok(prefix: &str) -> bool {
    let mut chars = prefix.chars().peekable();
    while let Some(c) = chars.next() {
        if is_unicode_space(c) {
            continue;
        }
        if c == '\\' {
            let mut saw_ws = false;
            while let Some(&n) = chars.peek() {
                if is_unicode_space(n) || is_newline_char(n) {
                    saw_ws = true;
                    chars.next();
                } else {
                    break;
                }
            }
            if !saw_ws {
                return false;
            }
            continue;
        }
        return false;
    }
    true
}

/// Resolve ws-escapes, then dedent, then other escapes (KDL 2 order).
fn process_escaped_multiline(raw_body: &str) -> Result<String, &'static str> {
    let mut phase1 = String::with_capacity(raw_body.len());
    let mut chars = raw_body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek().copied() {
                Some(n) if is_unicode_space(n) || is_newline_char(n) => {
                    chars.next();
                    while let Some(&n) = chars.peek() {
                        if is_unicode_space(n) || is_newline_char(n) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                Some(_) | None => {
                    phase1.push('\\');
                    if let Some(n) = chars.next() {
                        phase1.push(n);
                    }
                }
            }
        } else {
            if is_disallowed_literal(c) {
                return Err("disallowed code point in multiline string");
            }
            phase1.push(c);
        }
    }

    let (content, indent) = split_closing_indent(&phase1)?;
    let dedented = dedent_lines(content, &indent)?;
    apply_non_ws_escapes(&dedented)
}

fn process_raw_multiline(raw_body: &str) -> Result<String, &'static str> {
    for c in raw_body.chars() {
        if is_disallowed_literal(c) {
            return Err("disallowed code point in raw multiline string");
        }
    }
    let (content, indent) = split_closing_indent(raw_body)?;
    dedent_lines(content, &indent)
}

fn split_closing_indent(phase1: &str) -> Result<(&str, String), &'static str> {
    if phase1.is_empty() {
        return Ok(("", String::new()));
    }
    // Body is only indent spaces (empty indented string): `"""\n\t"""` → body `\t`.
    if phase1.chars().all(is_unicode_space) {
        return Ok(("", phase1.to_owned()));
    }
    let Some(nl) = phase1.rfind('\n') else {
        return Err("multiline string closing quotes must be on their own line after dedent");
    };
    let after = &phase1[nl + 1..];
    if !after.chars().all(is_unicode_space) {
        return Err("multiline string closing line must contain only whitespace after ws-escapes");
    }
    Ok((&phase1[..nl], after.to_owned()))
}

fn dedent_lines(content: &str, indent: &str) -> Result<String, &'static str> {
    if content.is_empty() {
        return Ok(String::new());
    }
    let mut out = String::with_capacity(content.len());
    for (i, line) in content.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let line = line.strip_suffix('\r').unwrap_or(line);
        // Spec: lines that contain only literal whitespace (not `\t` escapes etc.)
        // always become empty, regardless of the closing indent prefix.
        if line.chars().all(is_unicode_space) {
            continue;
        }
        if indent.is_empty() {
            out.push_str(line);
            continue;
        }
        if let Some(rest) = line.strip_prefix(indent) {
            out.push_str(rest);
        } else {
            return Err("multiline string indent mismatch");
        }
    }
    Ok(out)
}

fn apply_non_ws_escapes(s: &str) -> Result<String, &'static str> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(esc) = chars.next() else {
            return Err("invalid escape at end of multiline string");
        };
        match esc {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            's' => out.push(' '),
            'b' => out.push('\u{0008}'),
            'f' => out.push('\u{000C}'),
            'u' => {
                if chars.next() != Some('{') {
                    return Err("invalid unicode escape");
                }
                let mut hex = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(h) if h.is_ascii_hexdigit() => hex.push(h),
                        _ => return Err("invalid unicode escape"),
                    }
                    if hex.len() > 6 {
                        return Err("invalid unicode escape");
                    }
                }
                if hex.is_empty() {
                    return Err("invalid unicode escape");
                }
                let code = u32::from_str_radix(&hex, 16).map_err(|_| "invalid unicode escape")?;
                if (0xD800..=0xDFFF).contains(&code) || code > 0x10FFFF {
                    return Err("invalid unicode escape");
                }
                out.push(char::from_u32(code).ok_or("invalid unicode escape")?);
            }
            _ => return Err("invalid escape in multiline string"),
        }
    }
    Ok(out)
}
