//! Error types aligned with Glaze `error_ctx` / `format_error`.
//!
//! Authority:
//! - `references/glaze/include/glaze/core/context.hpp` (`error_ctx`)
//! - `references/glaze/include/glaze/core/reflect.hpp` (`format_error`)
//! - `references/glaze/include/glaze/util/validate.hpp` (`get_source_info`, `generate_error_string`)
//!
//! See `docs/kdl/glaze-alignment.md`.

use std::borrow::Cow;
use std::fmt;

/// Machine-readable failure class (Glaze `error_code` analogue for KDL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ErrorCode {
    /// Glaze `error_code::none` — success / no error.
    #[default]
    None,
    UnexpectedEof,
    Syntax,
    InvalidEscape,
    InvalidNumber,
    InvalidIdent,
    InvalidKeyword,
    ExpectedNodeName,
    ExpectedValue,
    ExpectedEquals,
    ExpectedBrace,
    ExpectedTerminator,
    UnknownProperty,
    UnknownChild,
    MissingProperty,
    MissingArgument,
    MissingChild,
    DuplicateProperty,
    TypeMismatch,
    ExceededMaxDepth,
    ExceededLimit,
    DisallowedCodePoint,
    UnexpectedToken,
    /// Glaze `no_read_input` — empty buffer.
    NoReadInput,
    /// Glaze `buffer_overflow` — fixed write buffer too small
    /// (`references/glaze/docs/writing.md`, `error_code::buffer_overflow`).
    BufferOverflow,
}

impl ErrorCode {
    /// Human key for the code (Glaze `meta<error_code>::keys[...]` role).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::UnexpectedEof => "unexpected end of input",
            Self::Syntax => "syntax error",
            Self::InvalidEscape => "invalid escape sequence",
            Self::InvalidNumber => "invalid number",
            Self::InvalidIdent => "invalid identifier",
            Self::InvalidKeyword => "invalid keyword",
            Self::ExpectedNodeName => "expected node name",
            Self::ExpectedValue => "expected value",
            Self::ExpectedEquals => "expected `=`",
            Self::ExpectedBrace => "expected `{` or `}`",
            Self::ExpectedTerminator => "expected node terminator",
            Self::UnknownProperty => "unknown property",
            Self::UnknownChild => "unknown child node",
            Self::MissingProperty => "missing property",
            Self::MissingArgument => "missing argument",
            Self::MissingChild => "missing child node",
            Self::DuplicateProperty => "duplicate property",
            Self::TypeMismatch => "type mismatch",
            Self::ExceededMaxDepth => "exceeded maximum nesting depth",
            Self::ExceededLimit => "exceeded configured limit",
            Self::DisallowedCodePoint => "disallowed code point",
            Self::UnexpectedToken => "unexpected token",
            Self::NoReadInput => "no read input",
            Self::BufferOverflow => "buffer overflow",
        }
    }

    pub const fn is_none(self) -> bool {
        matches!(self, Self::None)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unified read/write error context — Glaze `glz::error_ctx`.
///
/// Glaze fields (`context.hpp`):
/// - `count` → [`Self::consumed`] (bytes processed; **also** the index `format_error` uses)
/// - `ec` → [`Self::code`]
/// - `custom_error_message` → [`Self::message`]
///
/// `operator bool()` in Glaze is true when there **is** an error; see [`Self::is_err`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ErrorCtx {
    /// Bytes processed from the start of the input (Glaze `count`).
    ///
    /// On failure this is the index passed to [`format_error`], matching
    /// `format_error(pe, buffer)` → `get_source_info(buffer, pe.count)`.
    pub consumed: usize,
    pub code: ErrorCode,
    pub message: Option<Cow<'static, str>>,
    /// KDL-specific expected-token hint (no direct Glaze field; folded into message in format).
    pub expected: Option<&'static str>,
}

impl ErrorCtx {
    /// Success context with `consumed` bytes (Glaze `error_code::none`).
    pub fn ok(consumed: usize) -> Self {
        Self {
            consumed,
            code: ErrorCode::None,
            message: None,
            expected: None,
        }
    }

    pub fn new(code: ErrorCode, consumed: usize) -> Self {
        Self {
            consumed,
            code,
            message: None,
            expected: None,
        }
    }

    pub fn with_consumed(mut self, consumed: usize) -> Self {
        self.consumed = consumed;
        self
    }

    pub fn with_message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        self.message = Some(message.into());
        self
    }

    pub fn with_expected(mut self, expected: &'static str) -> Self {
        self.expected = Some(expected);
        self
    }

    /// Glaze `error_ctx::operator bool()` — true when there **is** an error.
    pub const fn is_err(&self) -> bool {
        !self.code.is_none()
    }

    pub const fn is_ok(&self) -> bool {
        self.code.is_none()
    }

    /// Line (1-based) and column (1-based) from `consumed` into `input`.
    ///
    /// Unlike Glaze's text formatter, this remains useful at logical EOF: an
    /// incomplete final line reports its insertion point, and input ending in a
    /// newline reports the next empty line. This is used by structured tools
    /// such as miette; [`format_error`] itself retains Glaze's exact EOF
    /// `index N` spelling.
    pub fn line_col(&self, input: &str) -> (usize, usize) {
        let position = source_position(input, self.consumed);
        (position.line, position.column)
    }
}

impl fmt::Display for ErrorCtx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Glaze format_error(error_ctx) without buffer: key + optional custom message.
        f.write_str(self.code.as_str())?;
        if let Some(expected) = self.expected {
            write!(f, " (expected {expected})")?;
        }
        if let Some(message) = &self.message {
            write!(f, " {message}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ErrorCtx {}

/// Source window for diagnostics — Glaze `detail::source_info` (`validate.hpp`).
#[derive(Debug, Clone)]
struct SourceInfo {
    line: usize,
    column: usize,
    context: String,
    index: usize,
    front_truncation: usize,
    rear_truncation: usize,
}

/// A safe byte position for source spans and user-facing line labels.
///
/// `ErrorCtx::consumed` is a byte count, like Glaze's `error_ctx::count`.
/// Parser-produced offsets are UTF-8 boundaries, but callers can construct an
/// `ErrorCtx` themselves. Normalizing only presentation spans keeps those
/// callers from causing invalid Rust `str` slices while preserving the original
/// count in Glaze-shaped text output.
#[derive(Debug, Clone, Copy)]
struct SourcePosition {
    offset: usize,
    line: usize,
    column: usize,
    line_start: usize,
    #[cfg_attr(not(feature = "diagnostics"), allow(dead_code))]
    line_end: usize,
}

fn floor_char_boundary(input: &str, mut offset: usize) -> usize {
    offset = offset.min(input.len());
    while offset > 0 && !input.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn source_position(input: &str, index: usize) -> SourcePosition {
    let offset = floor_char_boundary(input, index);
    let prefix = &input[..offset];
    let line = prefix.bytes().filter(|&byte| byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map(|at| at + 1).unwrap_or(0);
    let line_end = input[offset..]
        .find('\n')
        .map(|relative| offset + relative)
        .unwrap_or(input.len());

    SourcePosition {
        offset,
        line,
        column: offset - line_start + 1,
        line_start,
        line_end,
    }
}

/// Glaze `detail::get_source_info` for UTF-8 Rust strings.
///
/// Glaze deliberately returns an empty context whenever `index >= buffer.size()`.
/// Keep that exact contract for [`format_error`], rather than fabricating a
/// caret on the final character. Structured diagnostics use [`source_position`]
/// to retain a useful EOF insertion point.
fn source_info(buffer: &str, index: usize) -> SourceInfo {
    if index >= buffer.len() {
        return SourceInfo {
            line: 0,
            column: 0,
            context: String::new(),
            index,
            front_truncation: 0,
            rear_truncation: 0,
        };
    }

    let position = source_position(buffer, index);
    let probe = position.offset;
    let line_start = position.line_start;
    // `validate.hpp` searches after the error byte. For UTF-8, move after the
    // whole scalar instead of slicing through one of its continuation bytes.
    let after_probe = probe
        + buffer[probe..]
            .chars()
            .next()
            .expect("probe is inside a non-empty source")
            .len_utf8();
    let line_end = buffer[after_probe..]
        .find('\n')
        .map(|relative| after_probe + relative)
        .unwrap_or(buffer.len());

    let mut context_begin = line_start;
    let mut context_end = line_end;
    let mut front_truncation = 0usize;
    let mut rear_truncation = 0usize;

    // Glaze: if context length > 64, truncate around the column. Keep every
    // Rust slice on a UTF-8 boundary even if Glaze's byte window would split a
    // multi-byte scalar.
    if context_end.saturating_sub(context_begin) > 64 {
        if position.column <= 32 {
            rear_truncation = 64;
            context_end = floor_char_boundary(buffer, (context_begin + 64).min(buffer.len()));
        } else {
            let requested_begin = line_start + position.column - 32;
            context_begin = floor_char_boundary(buffer, requested_begin);
            front_truncation = context_begin - line_start;
            if context_end.saturating_sub(context_begin) > 64 {
                rear_truncation = front_truncation + 64;
                context_end =
                    floor_char_boundary(buffer, (line_start + rear_truncation).min(buffer.len()));
            }
        }
    }

    let mut context = buffer[context_begin..context_end].to_owned();
    // Glaze `convert_tabs_to_single_spaces` (`validate.hpp`).
    context = context.replace('\t', " ");

    SourceInfo {
        line: position.line,
        column: position.column,
        context,
        index,
        front_truncation,
        rear_truncation,
    }
}

/// Format an error without a buffer (Glaze `format_error(const error_ctx&)`).
pub fn format_error_code(err: &ErrorCtx) -> String {
    err.to_string()
}

fn append_error_type(out: &mut String, err: &ErrorCtx) {
    out.push_str(err.code.as_str());
    if let Some(expected) = err.expected {
        out.push_str(" (expected ");
        out.push_str(expected);
        out.push(')');
    }
}

fn format_error_with_name(err: &ErrorCtx, input: &str, name: &str) -> String {
    if err.code.is_none() {
        return String::new();
    }
    let info = source_info(input, err.consumed);
    // Glaze `generate_error_string` shape:
    //   filename:line:column: <error>\n   <context>\n   <pad>^
    let mut out =
        String::with_capacity(name.len() + info.context.len() + err.code.as_str().len() + 128);
    if !name.is_empty() {
        out.push_str(name);
        out.push(':');
    }
    if info.context.is_empty() {
        out.push_str("index ");
        out.push_str(&info.index.to_string());
        out.push_str(": ");
        append_error_type(&mut out, err);
    } else {
        out.push_str(&info.line.to_string());
        out.push(':');
        out.push_str(&info.column.to_string());
        out.push_str(": ");
        append_error_type(&mut out, err);
        out.push('\n');
        if info.front_truncation > 0 {
            out.push_str("...");
        } else {
            out.push_str("   ");
        }
        out.push_str(&info.context);
        if info.rear_truncation > 0 {
            out.push_str("...");
        }
        out.push_str("\n   ");
        let pad = info
            .column
            .saturating_sub(1)
            .saturating_sub(info.front_truncation);
        for _ in 0..pad {
            out.push(' ');
        }
        out.push('^');
    }
    if let Some(message) = &err.message {
        out.push(' ');
        out.push_str(message);
    }
    out
}

/// Format an error against source text.
///
/// Glaze: `format_error(const error_ctx& pe, const auto& buffer)` uses **`pe.count`**
/// as the source index (`reflect.hpp`). We use [`ErrorCtx::consumed`].
pub fn format_error(err: &ErrorCtx, input: &str) -> String {
    format_error_with_name(err, input, "")
}

/// Format an error against source text with a filename or buffer identifier.
///
/// This mirrors Glaze `detail::generate_error_string(error, info, filename)`
/// (`include/glaze/util/validate.hpp`). The filename prefixes both ordinary
/// line/column diagnostics and exact-EOF `index N` diagnostics.
pub fn format_error_named(err: &ErrorCtx, input: &str, name: &str) -> String {
    format_error_with_name(err, input, name)
}

/// Public error type for `Result`-style APIs (Glaze `expected<T, error_ctx>` error arm).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub ctx: ErrorCtx,
}

impl Error {
    pub fn from_ctx(ctx: ErrorCtx) -> Self {
        Self { ctx }
    }

    pub fn format_with_source(&self, input: &str) -> String {
        format_error(&self.ctx, input)
    }

    /// Like [`Self::format_with_source`] with a filename or buffer identifier.
    pub fn format_with_named_source(&self, input: &str, name: &str) -> String {
        format_error_named(&self.ctx, input, name)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ctx.fmt(f)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.ctx)
    }
}

impl From<ErrorCtx> for Error {
    fn from(ctx: ErrorCtx) -> Self {
        Self::from_ctx(ctx)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
pub type CtxResult<T> = std::result::Result<T, ErrorCtx>;

/// Build a miette [`miette::Report`] from [`ErrorCtx`] + source
/// (feature `diagnostics`).
///
/// Core path stays Glaze `format_error`; miette is an optional presentation layer
/// (`docs/kdl/glaze-alignment.md` §2). Prefer [`report_error_named`] when a file
/// path is known (richer related spans).
#[cfg(feature = "diagnostics")]
pub fn report_error(err: &ErrorCtx, input: &str) -> miette::Report {
    miette_support::report_from_ctx(err, input, None)
}

/// Like [`report_error`] with an explicit source name (path or buffer id).
#[cfg(feature = "diagnostics")]
pub fn report_error_named(err: &ErrorCtx, input: &str, name: &str) -> miette::Report {
    miette_support::report_from_ctx(err, input, Some(name))
}

#[cfg(feature = "diagnostics")]
mod miette_support {
    use super::*;
    use miette::{Diagnostic, LabeledSpan, NamedSource, SourceCode, SourceOffset, SourceSpan};
    use std::sync::Arc;

    /// Precomputed miette presentation for one KDL error.
    ///
    /// P-G9c: primary label at `consumed` (Glaze `count`); secondary label for
    /// the current line snippet window (Glaze `get_source_info` context). The
    /// actual source lives once in `NamedSource<Arc<String>>` on the report.
    #[derive(Debug)]
    struct KdlDiagnostic {
        ctx: ErrorCtx,
        formatted: String,
        line: usize,
        column: usize,
        offset: usize,
        primary_len: usize,
        line_start: usize,
        line_end: usize,
    }

    impl KdlDiagnostic {
        fn from_ctx(ctx: ErrorCtx, input: &str, name: Option<&str>) -> Self {
            let position = source_position(input, ctx.consumed);
            let formatted = name.map_or_else(
                || format_error(&ctx, input),
                |source_name| format_error_named(&ctx, input, source_name),
            );
            let primary_len = input[position.offset..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
            Self {
                ctx,
                formatted,
                line: position.line,
                column: position.column,
                offset: position.offset,
                primary_len,
                line_start: position.line_start,
                line_end: position.line_end.max(position.line_start),
            }
        }
    }

    impl fmt::Display for KdlDiagnostic {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            if self.formatted.is_empty() {
                self.ctx.fmt(f)
            } else {
                f.write_str(&self.formatted)
            }
        }
    }

    impl std::error::Error for KdlDiagnostic {}

    impl Diagnostic for KdlDiagnostic {
        fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
            Some(Box::new(self.ctx.code.as_str()))
        }

        fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
            let loc = format!("{}:{}", self.line, self.column);
            match (&self.ctx.expected, &self.ctx.message) {
                (Some(exp), Some(msg)) => {
                    Some(Box::new(format!("{loc}: expected {exp}; {msg}"))
                        as Box<dyn fmt::Display + 'a>)
                }
                (Some(exp), None) => {
                    Some(Box::new(format!("{loc}: expected {exp}")) as Box<dyn fmt::Display + 'a>)
                }
                (None, Some(msg)) => {
                    Some(Box::new(format!("{loc}: {msg}")) as Box<dyn fmt::Display + 'a>)
                }
                (None, None) => Some(Box::new(format!("at {loc}")) as Box<dyn fmt::Display + 'a>),
            }
        }

        fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
            if self.ctx.code.is_none() {
                return None;
            }
            let primary = SourceSpan::new(SourceOffset::from(self.offset), self.primary_len);
            let primary_label = self
                .ctx
                .message
                .as_ref()
                .map(|m| m.as_ref().to_owned())
                .or_else(|| self.ctx.expected.map(|e| format!("expected {e}")))
                .unwrap_or_else(|| self.ctx.code.as_str().to_owned());

            let mut labels = vec![LabeledSpan::new_primary_with_span(
                Some(primary_label),
                primary,
            )];

            // Related: whole source line (Glaze context window role).
            if self.line_end > self.line_start {
                let line_span = SourceSpan::new(
                    SourceOffset::from(self.line_start),
                    self.line_end - self.line_start,
                );
                labels.push(LabeledSpan::new_with_span(
                    Some(format!("line {}", self.line)),
                    line_span,
                ));
            }

            Some(Box::new(labels.into_iter()))
        }

        fn source_code(&self) -> Option<&dyn SourceCode> {
            // `Report::with_source_code(NamedSource<Arc<String>>)` below owns
            // the one source copy. Returning `None` lets that named wrapper win
            // instead of silently masking it with an unnamed inner source.
            None
        }

        fn url<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
            None
        }

        fn severity(&self) -> Option<miette::Severity> {
            Some(miette::Severity::Error)
        }
    }

    impl Diagnostic for ErrorCtx {
        fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
            Some(Box::new(self.code.as_str()))
        }

        fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
            self.expected
                .map(|e| Box::new(format!("expected {e}")) as Box<dyn fmt::Display + 'a>)
        }
    }

    impl Diagnostic for Error {
        fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
            self.ctx.code()
        }

        fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
            Diagnostic::help(&self.ctx)
        }
    }

    pub(super) fn report_from_ctx(
        err: &ErrorCtx,
        input: &str,
        name: Option<&str>,
    ) -> miette::Report {
        let diag = KdlDiagnostic::from_ctx(err.clone(), input, name);
        let source = Arc::new(input.to_owned());
        let source_name = name.unwrap_or("kdl");
        miette::Report::new(diag).with_source_code(NamedSource::new(source_name, source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_error_uses_glaze_eof_index_without_a_fabricated_caret() {
        let input = "ok\n";
        let err = ErrorCtx::new(ErrorCode::UnexpectedEof, input.len())
            .with_expected("`}`")
            .with_message("missing closing brace");

        assert_eq!(
            format_error(&err, input),
            "index 3: unexpected end of input (expected `}`) missing closing brace"
        );
        // Structured diagnostics still point to the intuitive insertion point.
        assert_eq!(err.line_col(input), (2, 1));
    }

    #[test]
    fn named_format_prefixes_line_and_eof_diagnostics() {
        let source = "broken";
        let syntax = ErrorCtx::new(ErrorCode::Syntax, 2);
        assert!(format_error_named(&syntax, source, "config.kdl").starts_with("config.kdl:1:3:"),);

        let eof = ErrorCtx::new(ErrorCode::UnexpectedEof, source.len());
        assert_eq!(
            format_error_named(&eof, source, "config.kdl"),
            "config.kdl:index 6: unexpected end of input"
        );
    }

    #[test]
    fn presentation_normalizes_invalid_utf8_byte_offsets() {
        let source = "éx\n";
        // Byte 1 is inside `é`; public formatting must remain safe for callers
        // constructing an ErrorCtx rather than coming from Parser.
        let err = ErrorCtx::new(ErrorCode::Syntax, 1);

        assert!(format_error(&err, source).contains("éx"));
        assert_eq!(err.line_col(source), (1, 1));
    }
}
