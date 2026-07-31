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
    /// Mirrors `validate.hpp` `get_source_info` line/column calculation intent.
    pub fn line_col(&self, input: &str) -> (usize, usize) {
        source_info(input, self.consumed).line_col()
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
    #[allow(dead_code)]
    rear_truncation: usize,
}

impl SourceInfo {
    fn line_col(&self) -> (usize, usize) {
        (self.line, self.column)
    }
}

/// Glaze `detail::get_source_info` (char buffers).
fn source_info(buffer: &str, index: usize) -> SourceInfo {
    if index >= buffer.len() && buffer.is_empty() {
        return SourceInfo {
            line: 1,
            column: 1,
            context: String::new(),
            index,
            front_truncation: 0,
            rear_truncation: 0,
        };
    }
    // If index == len (EOF), point at last byte for context when non-empty.
    let probe = if index >= buffer.len() {
        buffer.len().saturating_sub(1)
    } else {
        index
    };

    let prefix = &buffer[..probe.min(buffer.len())];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let column = probe.saturating_sub(line_start) + 1;

    let line_end = buffer[probe.min(buffer.len())..]
        .find('\n')
        .map(|i| probe + i)
        .unwrap_or(buffer.len());

    let mut context_begin = line_start;
    let mut context_end = line_end;
    let mut front_truncation = 0usize;
    let mut rear_truncation = 0usize;

    // Glaze: if context length > 64, truncate around the column.
    if context_end.saturating_sub(context_begin) > 64 {
        if column <= 32 {
            rear_truncation = 64;
            context_end = context_begin + rear_truncation;
            if context_end > buffer.len() {
                context_end = buffer.len();
            }
        } else {
            front_truncation = column - 32;
            context_begin = line_start + front_truncation;
            if context_end.saturating_sub(context_begin) > 64 {
                rear_truncation = front_truncation + 64;
                context_end = (line_start + rear_truncation).min(buffer.len());
            }
        }
    }

    let mut context: String = buffer
        .get(context_begin..context_end)
        .unwrap_or("")
        .to_owned();
    // Glaze convert_tabs_to_single_spaces (`validate.hpp`) — tab → single space.
    context = context.replace('\t', " ");

    SourceInfo {
        line,
        column,
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

/// Format an error against source text.
///
/// Glaze: `format_error(const error_ctx& pe, const auto& buffer)` uses **`pe.count`**
/// as the source index (`reflect.hpp`). We use [`ErrorCtx::consumed`].
pub fn format_error(err: &ErrorCtx, input: &str) -> String {
    if err.code.is_none() {
        return String::new();
    }
    let info = source_info(input, err.consumed);
    // Glaze generate_error_string shape:
    //   line:column: <error>\n   <context>\n   <pad>^
    let mut out = String::new();
    if info.context.is_empty() {
        out.push_str("index ");
        out.push_str(&info.index.to_string());
        out.push_str(": ");
        out.push_str(err.code.as_str());
    } else {
        out.push_str(&info.line.to_string());
        out.push(':');
        out.push_str(&info.column.to_string());
        out.push_str(": ");
        out.push_str(err.code.as_str());
        if let Some(expected) = err.expected {
            out.push_str(" (expected ");
            out.push_str(expected);
            out.push(')');
        }
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
    miette_support::report_from_ctx(err, input, "kdl")
}

/// Like [`report_error`] with an explicit source name (path or buffer id).
#[cfg(feature = "diagnostics")]
pub fn report_error_named(err: &ErrorCtx, input: &str, name: &str) -> miette::Report {
    miette_support::report_from_ctx(err, input, name)
}

#[cfg(feature = "diagnostics")]
mod miette_support {
    use super::*;
    use miette::{Diagnostic, LabeledSpan, SourceCode, SourceOffset, SourceSpan};
    use std::sync::Arc;

    /// Owned source + error for miette (spans need a named source).
    ///
    /// P-G9c: primary label at `consumed` (Glaze `count`); secondary label for
    /// the current line snippet window (Glaze `get_source_info` context).
    #[derive(Debug)]
    struct KdlDiagnostic {
        ctx: ErrorCtx,
        src: Arc<String>,
        name: String,
        line: usize,
        column: usize,
        line_start: usize,
        line_end: usize,
    }

    impl KdlDiagnostic {
        fn from_ctx(ctx: ErrorCtx, input: &str, name: &str) -> Self {
            let info = source_info(input, ctx.consumed);
            let (line, column) = info.line_col();
            // Recompute line byte range for related span (full line, not truncated).
            let probe = if ctx.consumed >= input.len() && !input.is_empty() {
                input.len().saturating_sub(1)
            } else {
                ctx.consumed
                    .min(input.len().saturating_sub(1).min(ctx.consumed))
            };
            let probe = probe.min(
                input
                    .len()
                    .saturating_sub(if input.is_empty() { 0 } else { 1 }),
            );
            let prefix = &input[..probe.min(input.len())];
            let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
            let line_end = input[probe.min(input.len())..]
                .find('\n')
                .map(|i| probe + i)
                .unwrap_or(input.len());
            Self {
                ctx,
                src: Arc::new(input.to_owned()),
                name: name.to_owned(),
                line,
                column,
                line_start,
                line_end: line_end.max(line_start),
            }
        }
    }

    impl fmt::Display for KdlDiagnostic {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            // Prefer Glaze-shaped multi-line format when source is present.
            let formatted = format_error(&self.ctx, self.src.as_str());
            if formatted.is_empty() {
                self.ctx.fmt(f)
            } else {
                f.write_str(&formatted)
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
            let offset = self.ctx.consumed.min(self.src.len());
            let len = if offset < self.src.len() {
                self.src[offset..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(1)
            } else {
                0
            };
            let primary = SourceSpan::new(SourceOffset::from(offset), len);
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
            if self.line_end > self.line_start && self.line_end <= self.src.len() {
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
            Some(self.src.as_ref() as &dyn SourceCode)
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

    pub(super) fn report_from_ctx(err: &ErrorCtx, input: &str, name: &str) -> miette::Report {
        let diag = KdlDiagnostic::from_ctx(err.clone(), input, name);
        let _ = diag.name.as_str(); // keep name for NamedSource consumers
        miette::Report::new(diag).with_source_code(Arc::new(input.to_owned()))
    }
}
