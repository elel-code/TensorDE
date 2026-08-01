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
    #[inline(always)]
    pub fn ensure_capacity(&mut self, additional: usize) -> Result<(), ErrorCtx> {
        match &mut self.kind {
            SinkKind::Grow { buf } => {
                grow_reserve(buf, additional);
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
    ///
    /// Hot path: after an upfront [`Self::ensure_capacity`] / padding reserve,
    /// grow dumps avoid re-checking capacity on every structural token when the
    /// String still has room (Glaze `assign_maybe_cast` after size check).
    #[inline(always)]
    pub fn push_byte(&mut self, c: u8) -> Result<(), ErrorCtx> {
        match &mut self.kind {
            SinkKind::Grow { buf } => {
                if buf.len() == buf.capacity() {
                    // Glaze dump: resize when ix == size (2× or 128 min).
                    grow_reserve(buf, 1);
                }
                // UTF-8 single-byte ASCII structural chars (space, `{`, `}`, …).
                debug_assert!(c.is_ascii());
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
    #[inline(always)]
    pub fn push_str(&mut self, s: &str) -> Result<(), ErrorCtx> {
        let raw = s.as_bytes();
        match &mut self.kind {
            SinkKind::Grow { buf } => {
                let n = raw.len();
                if buf.len().saturating_add(n) > buf.capacity() {
                    grow_reserve(buf, n);
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

    /// Dump `n` copies of an ASCII byte (Glaze `dumpn` for indent / padding runs).
    #[inline(always)]
    pub fn push_byte_n(&mut self, c: u8, n: usize) -> Result<(), ErrorCtx> {
        if n == 0 {
            return Ok(());
        }
        debug_assert!(c.is_ascii());
        match &mut self.kind {
            SinkKind::Grow { buf } => {
                if buf.len().saturating_add(n) > buf.capacity() {
                    grow_reserve(buf, n);
                }
                // SAFETY-free: ASCII byte as char is a single UTF-8 unit.
                buf.extend(std::iter::repeat_n(c as char, n));
                Ok(())
            }
            SinkKind::Fixed { bytes, ix } => {
                if *ix + n > bytes.len() {
                    let fit = bytes.len().saturating_sub(*ix);
                    bytes[*ix..].fill(c);
                    *ix = bytes.len();
                    let _ = fit;
                    return Err(ErrorCtx::new(ErrorCode::BufferOverflow, *ix)
                        .with_message("fixed write buffer too small"));
                }
                bytes[*ix..*ix + n].fill(c);
                *ix += n;
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

/// Glaze `vector_like` resize: 2× needed, at least `2 * write_padding_bytes`.
#[inline(always)]
fn grow_reserve(buf: &mut String, additional: usize) {
    let needed = buf
        .len()
        .saturating_add(additional)
        .saturating_add(WRITE_PADDING_BYTES.min(16));
    if needed > buf.capacity() {
        let new_cap = (needed * 2).max(2 * WRITE_PADDING_BYTES);
        buf.reserve(new_cap.saturating_sub(buf.capacity()));
    }
}

/// Write suite-canonical indent (4 spaces per level) — single dumpn (Glaze indent).
#[inline(always)]
pub fn write_indent(out: &mut WriteSink<'_>, level: usize) -> Result<(), ErrorCtx> {
    out.push_byte_n(b' ', level.saturating_mul(4))
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
                // `\u{…}` — stack only (Glaze dump escapes without heap).
                let mut tmp = [0u8; 16];
                let n = write_unicode_escape(&mut tmp, u32::from(c));
                out.push_str(core::str::from_utf8(&tmp[..n]).unwrap())?;
            }
            c => out.push_char(c)?,
        }
    }
    out.push_byte(b'"')
}

/// Write `\u{hhhh}` into `buf`; returns byte length (max 12 for U+10FFFF).
#[inline(always)]
fn write_unicode_escape(buf: &mut [u8; 16], cp: u32) -> usize {
    buf[0] = b'\\';
    buf[1] = b'u';
    buf[2] = b'{';
    // lowercase hex without alloc
    let mut hex = [0u8; 8];
    let mut v = cp;
    let mut n = 0usize;
    if v == 0 {
        hex[0] = b'0';
        n = 1;
    } else {
        while v > 0 {
            let d = (v & 0xf) as u8;
            hex[n] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
            n += 1;
            v >>= 4;
        }
        hex[..n].reverse();
    }
    buf[3..3 + n].copy_from_slice(&hex[..n]);
    buf[3 + n] = b'}';
    4 + n
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

/// Dump integer without heap (Glaze `write_chars` / `itoa` role).
///
/// Cite: `util/itoa.hpp`, `core/write_chars.hpp` — stack buffer → dump.
#[inline(always)]
pub fn write_i128(out: &mut WriteSink<'_>, n: i128) -> Result<(), ErrorCtx> {
    let mut buf = [0u8; 40];
    out.push_str(format_i128_into(&mut buf, n))
}

/// Dump `u128` without heap (wider than `i128` cast path).
#[inline(always)]
pub fn write_u128(out: &mut WriteSink<'_>, n: u128) -> Result<(), ErrorCtx> {
    let mut buf = [0u8; 40];
    out.push_str(format_u128_into(&mut buf, n))
}

/// Dump float without heap intermediate (Glaze `write_chars` for floats).
#[inline(always)]
pub fn write_f64(out: &mut WriteSink<'_>, f: f64) -> Result<(), ErrorCtx> {
    let mut buf = [0u8; 64];
    out.push_str(format_float_into(&mut buf, f))
}

#[cfg(feature = "dom")]
pub fn write_f64_lexical(out: &mut WriteSink<'_>, lex: &str, value: f64) -> Result<(), ErrorCtx> {
    // Lexical path still may allocate when cleaning underscores; suite tooling only.
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
    let mut buf = [0u8; 64];
    format_float_into(&mut buf, value).to_owned()
}

/// Format `i128` into `buf`; returns the written substring (Glaze `to_chars`).
#[inline(always)]
fn format_i128_into(buf: &mut [u8; 40], n: i128) -> &str {
    // i128::MIN cannot negate; emit fixed digits.
    if n == i128::MIN {
        const MIN: &[u8] = b"-170141183460469231731687303715884105728";
        buf[..MIN.len()].copy_from_slice(MIN);
        return core::str::from_utf8(&buf[..MIN.len()]).unwrap();
    }
    if n >= 0 {
        format_u128_into(buf, n as u128)
    } else {
        let body = format_u128_into(buf, (-n) as u128);
        let start = 40 - body.len() - 1;
        buf[start] = b'-';
        // body already occupies buf[40-len..]; shift not needed if we wrote at end.
        // format_u128_into writes at the end of buf; prefix '-' just before it.
        core::str::from_utf8(&buf[start..]).unwrap()
    }
}

#[inline(always)]
fn format_u128_into(buf: &mut [u8; 40], mut n: u128) -> &str {
    if n == 0 {
        buf[39] = b'0';
        return core::str::from_utf8(&buf[39..]).unwrap();
    }
    let mut i = 40;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    core::str::from_utf8(&buf[i..]).unwrap()
}

/// Stack formatter for suite-canonical floats (no heap on the hot path).
fn format_float_into(buf: &mut [u8; 64], f: f64) -> &str {
    if f.is_nan() {
        buf[..4].copy_from_slice(b"#nan");
        return core::str::from_utf8(&buf[..4]).unwrap();
    }
    if f.is_infinite() {
        if f.is_sign_negative() {
            buf[..5].copy_from_slice(b"#-inf");
            return core::str::from_utf8(&buf[..5]).unwrap();
        }
        buf[..4].copy_from_slice(b"#inf");
        return core::str::from_utf8(&buf[..4]).unwrap();
    }
    if f == 0.0 {
        if f.is_sign_negative() {
            buf[..4].copy_from_slice(b"-0.0");
            return core::str::from_utf8(&buf[..4]).unwrap();
        }
        buf[..3].copy_from_slice(b"0.0");
        return core::str::from_utf8(&buf[..3]).unwrap();
    }

    // Small stack writer for Display-style pieces without String.
    struct StackBuf<'a> {
        buf: &'a mut [u8],
        len: usize,
    }
    impl core::fmt::Write for StackBuf<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let raw = s.as_bytes();
            if self.len + raw.len() > self.buf.len() {
                return Err(core::fmt::Error);
            }
            self.buf[self.len..self.len + raw.len()].copy_from_slice(raw);
            self.len += raw.len();
            Ok(())
        }
    }

    let abs = f.abs();
    let mut w = StackBuf {
        buf: &mut buf[..],
        len: 0,
    };
    use core::fmt::Write as _;
    if abs >= 1e10 || (abs > 0.0 && abs < 1e-4) {
        let exp = f.abs().log10().floor() as i32;
        let mant = f / 10f64.powi(exp);
        let mant = (mant * 1e12).round() / 1e12;
        if mant.fract().abs() < 1e-12 {
            let _ = write!(w, "{}", mant as i64);
        } else {
            let _ = write!(w, "{mant}");
        }
        if exp >= 0 {
            let _ = write!(w, "E+{exp}");
        } else {
            let _ = write!(w, "E{exp}");
        }
    } else if f.fract() == 0.0 {
        let _ = write!(w, "{f:.1}");
    } else {
        let _ = write!(w, "{f}");
        // Ensure a decimal point for KDL suite style when Display omitted it.
        let s = core::str::from_utf8(&w.buf[..w.len]).unwrap_or("");
        if !s.contains('.') {
            let _ = w.write_str(".0");
        }
    }
    let len = w.len;
    core::str::from_utf8(&buf[..len]).unwrap_or("0.0")
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
