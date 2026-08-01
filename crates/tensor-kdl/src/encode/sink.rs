//! Glaze-shaped write buffer (`references/glaze/core/write.hpp` + `util/dump.hpp`).
//!
//! - Growable: auto `ensure_capacity` with 2× growth (`buffer_traits`).
//! - Fixed: bounds-checked; overflow → [`ErrorCode::BufferOverflow`].
//! - `written()` / `ix` maps to Glaze `error_ctx.count` on success.

use crate::error::{ErrorCode, ErrorCtx};
use crate::parse::chars::{is_disallowed_literal, is_non_identifier_char};

/// Glaze `write_padding_bytes` (`opts.hpp`) — reserve headroom for dump bursts.
pub const WRITE_PADDING_BYTES: usize = 256;

/// Output sink for typed write (Glaze `B& b, size_t& ix`).
pub struct WriteSink<'a> {
    kind: SinkKind<'a>,
}

enum SinkKind<'a> {
    /// Resizable: bytes live in `buf.as_mut_vec()`-style via String as Latin-1/UTF-8.
    /// We keep `String` and write UTF-8; `ix` is `buf.len()` after each dump.
    Grow {
        buf: &'a mut String,
    },
    Fixed {
        bytes: &'a mut [u8],
        ix: usize,
    },
}

impl<'a> WriteSink<'a> {
    /// Glaze resizable buffer (`std::string`). Prefills padding capacity.
    pub fn string(buf: &'a mut String) -> Self {
        if buf.capacity() < 2 * WRITE_PADDING_BYTES {
            buf.reserve(2 * WRITE_PADDING_BYTES);
        }
        Self {
            kind: SinkKind::Grow { buf },
        }
    }

    /// Glaze fixed / span buffer.
    pub fn slice(bytes: &'a mut [u8]) -> Self {
        Self {
            kind: SinkKind::Fixed { bytes, ix: 0 },
        }
    }

    /// Bytes written so far (Glaze `ix` / `error_ctx.count`).
    pub fn written(&self) -> usize {
        match &self.kind {
            SinkKind::Grow { buf } => buf.len(),
            SinkKind::Fixed { ix, .. } => *ix,
        }
    }

    /// Grow-path reserve with padding headroom (Glaze `vector_like` dump resize).
    ///
    /// Cite: `util/dump.hpp` — padding only applies to **resizable** buffers;
    /// fixed spans are exact-size checked in [`Self::push_byte`] / [`Self::push_str`].
    pub fn ensure_capacity(&mut self, additional: usize) -> Result<(), ErrorCtx> {
        match &mut self.kind {
            SinkKind::Grow { buf } => {
                // Growth includes write_padding_bytes headroom (dump_newline_indent style).
                let needed = buf
                    .len()
                    .saturating_add(additional)
                    .saturating_add(WRITE_PADDING_BYTES.min(16));
                if needed > buf.capacity() {
                    // 2× growth amortizes reallocations (Glaze buffer_traits).
                    let new_cap = (needed * 2).max(2 * WRITE_PADDING_BYTES);
                    buf.reserve(new_cap.saturating_sub(buf.capacity()));
                }
                Ok(())
            }
            SinkKind::Fixed { bytes, ix } => {
                let needed = *ix + additional;
                if needed > bytes.len() {
                    Err(ErrorCtx::new(ErrorCode::BufferOverflow, *ix)
                        .with_message("fixed write buffer too small"))
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Dump one byte (Glaze `dump(c, b, ix)`).
    pub fn push_byte(&mut self, c: u8) -> Result<(), ErrorCtx> {
        match &mut self.kind {
            SinkKind::Grow { buf } => {
                // Resizable: ensure room + padding headroom, then write.
                let needed = buf
                    .len()
                    .saturating_add(1)
                    .saturating_add(WRITE_PADDING_BYTES.min(16));
                if needed > buf.capacity() {
                    let new_cap = (needed * 2).max(2 * WRITE_PADDING_BYTES);
                    buf.reserve(new_cap.saturating_sub(buf.capacity()));
                }
                buf.push(c as char);
                Ok(())
            }
            SinkKind::Fixed { bytes, ix } => {
                // Bounded: exact bounds only — no padding (Glaze dump.hpp contract).
                if *ix >= bytes.len() {
                    return Err(ErrorCtx::new(ErrorCode::BufferOverflow, *ix)
                        .with_message("fixed write buffer too small"));
                }
                bytes[*ix] = c;
                *ix += 1;
                Ok(())
            }
        }
    }

    /// Dump a UTF-8 string slice (Glaze `dump(sv, b, ix)`).
    pub fn push_str(&mut self, s: &str) -> Result<(), ErrorCtx> {
        let raw = s.as_bytes();
        match &mut self.kind {
            SinkKind::Grow { buf } => {
                let needed = buf
                    .len()
                    .saturating_add(raw.len())
                    .saturating_add(WRITE_PADDING_BYTES.min(16));
                if needed > buf.capacity() {
                    let new_cap = (needed * 2).max(2 * WRITE_PADDING_BYTES);
                    buf.reserve(new_cap.saturating_sub(buf.capacity()));
                }
                buf.push_str(s);
                Ok(())
            }
            SinkKind::Fixed { bytes, ix } => {
                // Write as many bytes as fit, then report overflow with consumed = full length
                // (Glaze-style count at failure for format_error indexing).
                if *ix + raw.len() > bytes.len() {
                    let n = bytes.len().saturating_sub(*ix);
                    if n > 0 {
                        bytes[*ix..*ix + n].copy_from_slice(&raw[..n]);
                    }
                    *ix = bytes.len();
                    return Err(ErrorCtx::new(ErrorCode::BufferOverflow, *ix)
                        .with_message("fixed write buffer too small"));
                }
                bytes[*ix..*ix + raw.len()].copy_from_slice(raw);
                *ix += raw.len();
                Ok(())
            }
        }
    }

    pub fn push_char(&mut self, c: char) -> Result<(), ErrorCtx> {
        let mut tmp = [0u8; 4];
        self.push_str(c.encode_utf8(&mut tmp))
    }

    /// Finalize; return byte count (Glaze `finalize` + `count`).
    pub fn finish(self) -> usize {
        self.written()
    }
}

/// Write suite-canonical indent (4 spaces per level).
pub fn write_indent(out: &mut WriteSink<'_>, level: usize) -> Result<(), ErrorCtx> {
    for _ in 0..level {
        out.push_str("    ")?;
    }
    Ok(())
}

/// Write a KDL identifier or quoted string (suite Translation Rules).
pub fn write_ident_or_string(out: &mut WriteSink<'_>, s: &str) -> Result<(), ErrorCtx> {
    if is_bare_ident(s) {
        out.push_str(s)
    } else {
        write_quoted(out, s)
    }
}

pub fn write_quoted(out: &mut WriteSink<'_>, s: &str) -> Result<(), ErrorCtx> {
    out.push_byte(b'"')?;
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\"")?,
            '\\' => out.push_str("\\\\")?,
            '\n' => out.push_str("\\n")?,
            '\r' => out.push_str("\\r")?,
            '\t' => out.push_str("\\t")?,
            '\u{0008}' => out.push_str("\\b")?,
            '\u{000C}' => out.push_str("\\f")?,
            c if c.is_control() || is_disallowed_literal(c) => {
                let mut tmp = String::new();
                use std::fmt::Write as _;
                let _ = write!(tmp, "\\u{{{:x}}}", u32::from(c));
                out.push_str(&tmp)?;
            }
            c => out.push_char(c)?,
        }
    }
    out.push_byte(b'"')
}

pub fn is_bare_ident(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    match s {
        "true" | "false" | "null" | "inf" | "-inf" | "nan" => return false,
        _ => {}
    }
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if first.is_ascii_digit() {
        return false;
    }
    if first == '.' && chars.clone().next().is_some_and(|c| c.is_ascii_digit()) {
        return false;
    }
    if first == '+' || first == '-' {
        match s.chars().nth(1) {
            Some(n) if n.is_ascii_digit() => return false,
            Some('.') if s.chars().nth(2).is_some_and(|c| c.is_ascii_digit()) => {
                return false;
            }
            _ => {}
        }
    }
    !s.chars()
        .any(|c| is_non_identifier_char(c) || is_disallowed_literal(c))
}

pub fn write_bool(out: &mut WriteSink<'_>, v: bool) -> Result<(), ErrorCtx> {
    out.push_str(if v { "#true" } else { "#false" })
}

pub fn write_null(out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
    out.push_str("#null")
}

pub fn write_i128(out: &mut WriteSink<'_>, n: i128) -> Result<(), ErrorCtx> {
    out.push_str(&n.to_string())
}

pub fn write_f64(out: &mut WriteSink<'_>, f: f64) -> Result<(), ErrorCtx> {
    out.push_str(&format_float(f))
}

#[cfg(feature = "dom")]
pub fn write_f64_lexical(out: &mut WriteSink<'_>, lex: &str, value: f64) -> Result<(), ErrorCtx> {
    out.push_str(&format_float_lexical(lex, value))
}

#[cfg(feature = "dom")]
fn format_float_lexical(lex: &str, value: f64) -> String {
    let cleaned: String = lex.chars().filter(|c| *c != '_').collect();
    if !value.is_finite()
        && (cleaned.contains('e') || cleaned.contains('E') || cleaned.contains('.'))
    {
        let mut s = cleaned.replace('e', "E");
        if let Some(idx) = s.find('E') {
            let rest = &s[idx + 1..];
            if !rest.is_empty() && !rest.starts_with('+') && !rest.starts_with('-') {
                s = format!("{}E+{}", &s[..idx], rest);
            }
        }
        return s;
    }
    if cleaned.contains('e') || cleaned.contains('E') {
        let mut s = cleaned.replace('e', "E");
        if let Some(idx) = s.find('E') {
            let rest = &s[idx + 1..];
            if !rest.is_empty() && !rest.starts_with('+') && !rest.starts_with('-') {
                s = format!("{}E+{}", &s[..idx], rest);
            }
        }
        return s;
    }
    if cleaned.contains('.') {
        return cleaned;
    }
    format_float(value)
}

fn format_float(f: f64) -> String {
    if f.is_nan() {
        return "#nan".to_owned();
    }
    if f.is_infinite() {
        return if f.is_sign_negative() {
            "#-inf".to_owned()
        } else {
            "#inf".to_owned()
        };
    }
    if f == 0.0 {
        return if f.is_sign_negative() {
            "-0.0".to_owned()
        } else {
            "0.0".to_owned()
        };
    }
    let abs = f.abs();
    if abs >= 1e10 || (abs > 0.0 && abs < 1e-4) {
        let exp = f.abs().log10().floor() as i32;
        let mant = f / 10f64.powi(exp);
        let mant = (mant * 1e12).round() / 1e12;
        let mant_s = if mant.fract().abs() < 1e-12 {
            format!("{}", mant as i64)
        } else {
            format!("{mant}")
        };
        let sign = if exp >= 0 { "+" } else { "" };
        format!("{mant_s}E{sign}{exp}")
    } else if f.fract() == 0.0 {
        format!("{f:.1}")
    } else {
        let s = format!("{f}");
        if !s.contains('.') {
            format!("{s}.0")
        } else {
            s
        }
    }
}

pub fn write_node_header(
    out: &mut WriteSink<'_>,
    indent: usize,
    type_name: Option<&str>,
    name: &str,
) -> Result<(), ErrorCtx> {
    write_indent(out, indent)?;
    if let Some(ty) = type_name {
        out.push_byte(b'(')?;
        write_ident_or_string(out, ty)?;
        out.push_byte(b')')?;
    }
    write_ident_or_string(out, name)
}

pub fn write_argument_prefix(out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
    out.push_byte(b' ')
}

pub fn write_property_key(out: &mut WriteSink<'_>, key: &str) -> Result<(), ErrorCtx> {
    out.push_byte(b' ')?;
    write_ident_or_string(out, key)?;
    out.push_byte(b'=')
}

pub fn write_node_end_leaf(out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
    out.push_byte(b'\n')
}

pub fn write_children_open(out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
    out.push_str(" {\n")
}

pub fn write_children_close(out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
    write_indent(out, indent)?;
    out.push_str("}\n")
}

pub fn write_flag_line(out: &mut WriteSink<'_>, indent: usize, name: &str) -> Result<(), ErrorCtx> {
    write_node_header(out, indent, None, name)?;
    write_node_end_leaf(out)
}

pub fn write_arg_node_line<S: super::EncodeScalar>(
    out: &mut WriteSink<'_>,
    indent: usize,
    name: &str,
    value: &S,
) -> Result<(), ErrorCtx> {
    write_node_header(out, indent, None, name)?;
    write_argument_prefix(out)?;
    value.write_scalar(out)?;
    write_node_end_leaf(out)
}

pub fn write_prop_node_line<S: super::EncodeScalar>(
    out: &mut WriteSink<'_>,
    indent: usize,
    name: &str,
    key: &str,
    value: &S,
) -> Result<(), ErrorCtx> {
    write_node_header(out, indent, None, name)?;
    write_property_key(out, key)?;
    value.write_scalar(out)?;
    write_node_end_leaf(out)
}
