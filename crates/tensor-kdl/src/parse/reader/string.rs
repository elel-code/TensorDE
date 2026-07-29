//! Identifier, quoted, raw, and multiline strings.

use crate::error::{CtxResult, ErrorCode};
use crate::parse::chars::{
    is_disallowed_literal, is_newline_char, is_non_identifier_char, is_unicode_space,
};
use crate::parse::reader::Parser;
use crate::parse::swar::find_quote_or_escape;
use crate::value::KdlStr;

impl<'a> Parser<'a> {
    pub(super) fn parse_identifier_string(&mut self) -> CtxResult<KdlStr<'a>> {
        let start = self.index;
        let c = self
            .peek_char()
            .ok_or_else(|| self.err(ErrorCode::InvalidIdent))?;

        if is_non_identifier_char(c) {
            return Err(self.err(ErrorCode::InvalidIdent));
        }

        if c == '+' || c == '-' {
            self.bump_char();
            if let Some(n) = self.peek_char() {
                if n.is_ascii_digit() {
                    self.index = start;
                    return Err(self
                        .err(ErrorCode::InvalidIdent)
                        .with_message("number-like"));
                }
                if n == '.' {
                    self.bump_char();
                    if let Some(n2) = self.peek_char() {
                        if n2.is_ascii_digit() {
                            self.index = start;
                            return Err(self.err(ErrorCode::InvalidIdent));
                        }
                        if !is_non_identifier_char(n2) {
                            self.bump_char();
                        }
                    }
                } else if !is_non_identifier_char(n) && !n.is_ascii_digit() {
                    self.bump_char();
                }
            }
        } else if c == '.' {
            self.bump_char();
            if let Some(n) = self.peek_char() {
                if n.is_ascii_digit() {
                    self.index = start;
                    return Err(self.err(ErrorCode::InvalidIdent));
                }
                if !is_non_identifier_char(n) {
                    self.bump_char();
                }
            }
        } else if c.is_ascii_digit() {
            return Err(self
                .err(ErrorCode::InvalidIdent)
                .with_message("identifier cannot start with a digit"));
        } else {
            self.bump_char();
        }

        while let Some(ch) = self.peek_char() {
            if is_non_identifier_char(ch) {
                break;
            }
            self.bump_char();
        }

        if self.index == start {
            return Err(self.err(ErrorCode::InvalidIdent));
        }

        let ident = &self.input[start..self.index];
        match ident {
            "true" | "false" | "null" | "inf" | "-inf" | "nan" => {
                return Err(self
                    .err_at(ErrorCode::InvalidIdent, start)
                    .with_message(format!(
                        "`{ident}` is not a valid identifier; use #{ident} for the keyword value"
                    )));
            }
            _ => {}
        }

        Ok(KdlStr::borrowed(ident))
    }

    pub(super) fn parse_quoted_string(&mut self) -> CtxResult<KdlStr<'a>> {
        if self.starts_with("\"\"\"") {
            return self.parse_multiline_quoted_string();
        }
        if self.peek_byte() != Some(b'"') {
            return Err(self.err(ErrorCode::Syntax).with_expected("`\"`"));
        }
        self.bump_byte();
        let start = self.index;
        let end_limit = self.bytes.len();
        let mut i = start;
        let mut needs_unescape = false;

        while i < end_limit {
            let special = find_quote_or_escape(self.bytes, i, end_limit);
            if special >= end_limit {
                return Err(self
                    .err(ErrorCode::UnexpectedEof)
                    .with_message("unclosed string"));
            }
            let b = self.bytes[special];
            if b == b'\\' {
                needs_unescape = true;
                i = special + 2;
                continue;
            }
            if b == b'"' {
                let raw = &self.input[start..special];
                if !needs_unescape {
                    // Bare newlines are illegal only when not introduced via ws-escape.
                    if raw.chars().any(is_newline_char) {
                        return Err(self
                            .err_at(ErrorCode::Syntax, start)
                            .with_message("newline in single-line string"));
                    }
                    if let Some(pos) = raw.chars().position(is_disallowed_literal) {
                        let off =
                            start + raw.chars().take(pos).map(|c| c.len_utf8()).sum::<usize>();
                        return Err(self.err_at(ErrorCode::DisallowedCodePoint, off));
                    }
                    self.index = special + 1;
                    return Ok(KdlStr::borrowed(raw));
                }
                self.index = start;
                return self.unescape_quoted_until(special);
            }
            i = special + 1;
        }
        Err(self
            .err(ErrorCode::UnexpectedEof)
            .with_message("unclosed string"))
    }

    fn unescape_quoted_until(&mut self, _end_quote_hint: usize) -> CtxResult<KdlStr<'a>> {
        // Re-scan with escape-aware logic: ws-escapes may include newlines that the
        // SWAR pre-pass treated as string body.
        self.ctx.clear_scratch();
        loop {
            if self.eof() {
                return Err(self
                    .err(ErrorCode::UnexpectedEof)
                    .with_message("unclosed string"));
            }
            let c = self
                .bump_char()
                .ok_or_else(|| self.err(ErrorCode::UnexpectedEof))?;
            if c == '"' {
                break;
            }
            if c == '\\' {
                self.interpret_escape()?;
                continue;
            }
            if is_disallowed_literal(c) {
                return Err(self.err_at(ErrorCode::DisallowedCodePoint, self.index - c.len_utf8()));
            }
            if is_newline_char(c) {
                return Err(self
                    .err(ErrorCode::Syntax)
                    .with_message("newline in single-line string"));
            }
            self.ctx.scratch.push(c);
        }
        if self.ctx.scratch.len() > self.ctx.max_string_len {
            return Err(self.err(ErrorCode::ExceededLimit));
        }
        Ok(KdlStr::owned(std::mem::take(&mut self.ctx.scratch)))
    }

    pub(super) fn interpret_escape(&mut self) -> CtxResult<()> {
        let c = self
            .bump_char()
            .ok_or_else(|| self.err(ErrorCode::InvalidEscape))?;
        match c {
            '"' => self.ctx.scratch.push('"'),
            '\\' => self.ctx.scratch.push('\\'),
            'b' => self.ctx.scratch.push('\u{0008}'),
            'f' => self.ctx.scratch.push('\u{000C}'),
            'n' => self.ctx.scratch.push('\n'),
            'r' => self.ctx.scratch.push('\r'),
            't' => self.ctx.scratch.push('\t'),
            's' => self.ctx.scratch.push(' '),
            // Solidus is NOT a valid escape in KDL 2 (`no_solidus_escape_fail`).
            'u' => {
                if self.peek_byte() != Some(b'{') {
                    return Err(self.err(ErrorCode::InvalidEscape).with_expected("`{`"));
                }
                self.bump_byte();
                let hex_start = self.index;
                while self
                    .peek_byte()
                    .is_some_and(|b| (b as char).is_ascii_hexdigit())
                {
                    self.bump_byte();
                }
                if self.peek_byte() != Some(b'}') {
                    return Err(self.err(ErrorCode::InvalidEscape).with_expected("`}`"));
                }
                let hex = &self.input[hex_start..self.index];
                if hex.is_empty() || hex.len() > 6 {
                    return Err(self.err_at(ErrorCode::InvalidEscape, hex_start));
                }
                let code = u32::from_str_radix(hex, 16)
                    .map_err(|_| self.err_at(ErrorCode::InvalidEscape, hex_start))?;
                if (0xD800..=0xDFFF).contains(&code) || code > 0x10FFFF {
                    return Err(self
                        .err_at(ErrorCode::InvalidEscape, hex_start)
                        .with_message("invalid unicode scalar"));
                }
                self.bump_byte();
                let ch = char::from_u32(code)
                    .ok_or_else(|| self.err_at(ErrorCode::InvalidEscape, hex_start))?;
                self.ctx.scratch.push(ch);
            }
            c if is_unicode_space(c) || is_newline_char(c) => {
                if c == '\r' && self.peek_byte() == Some(b'\n') {
                    self.bump_byte();
                }
                while let Some(n) = self.peek_char() {
                    if is_unicode_space(n) {
                        self.bump_char();
                    } else if is_newline_char(n) {
                        self.consume_newline();
                    } else {
                        break;
                    }
                }
            }
            _ => {
                return Err(self
                    .err(ErrorCode::InvalidEscape)
                    .with_message(format!("unknown escape `\\{c}`")));
            }
        }
        Ok(())
    }

    pub(super) fn parse_raw_string(&mut self) -> CtxResult<KdlStr<'a>> {
        if self.peek_byte() != Some(b'#') {
            return Err(self.err(ErrorCode::Syntax));
        }
        let mut hashes = 0usize;
        while self.peek_byte() == Some(b'#') {
            hashes += 1;
            self.bump_byte();
        }
        if self.starts_with("\"\"\"") {
            return self.parse_raw_multiline(hashes);
        }
        if self.peek_byte() != Some(b'"') {
            return Err(self
                .err(ErrorCode::Syntax)
                .with_message("raw string expected `\"` after `#`")
                .with_expected("`\"`"));
        }
        self.bump_byte();
        let start = self.index;
        let close = format!("\"{}", "#".repeat(hashes));
        if let Some(rel) = self.input[start..].find(&close) {
            let content = &self.input[start..start + rel];
            if content.chars().any(is_newline_char) {
                return Err(self
                    .err_at(ErrorCode::Syntax, start)
                    .with_message("newline in single-line raw string"));
            }
            if content.chars().any(is_disallowed_literal) {
                return Err(self.err_at(ErrorCode::DisallowedCodePoint, start));
            }
            self.index = start + rel + close.len();
            return Ok(KdlStr::borrowed(content));
        }
        Err(self
            .err(ErrorCode::UnexpectedEof)
            .with_message("unclosed raw string"))
    }
}
