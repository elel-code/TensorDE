//! Single-pass KDL 2.0 reader (Glaze-style cursor over UTF-8 bytes).

mod multiline;
mod number;
mod space;
mod string;
mod visit;

use crate::context::Context;
use crate::error::{CtxResult, ErrorCode, ErrorCtx};
use crate::parse::chars::{is_newline_char, is_unicode_space};
use crate::value::{Document, Entry, KdlStr, Node, Value};

pub struct Parser<'a> {
    pub(super) input: &'a str,
    pub(super) bytes: &'a [u8],
    pub(super) index: usize,
    pub(super) ctx: Context,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            index: 0,
            ctx: Context::new(),
        }
    }

    pub fn with_context(input: &'a str, ctx: Context) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            index: 0,
            ctx,
        }
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.ctx
    }

    pub fn into_context(self) -> Context {
        self.ctx
    }

    pub fn offset(&self) -> usize {
        self.index
    }

    pub fn remaining(&self) -> &'a str {
        &self.input[self.index..]
    }

    pub(super) fn err(&self, code: ErrorCode) -> ErrorCtx {
        ErrorCtx::new(code, self.index).with_consumed(self.index)
    }

    pub(super) fn err_at(&self, code: ErrorCode, offset: usize) -> ErrorCtx {
        ErrorCtx::new(code, offset).with_consumed(self.index)
    }

    pub(super) fn eof(&self) -> bool {
        self.index >= self.bytes.len()
    }

    pub(super) fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    pub(super) fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    pub(super) fn bump_byte(&mut self) {
        if self.index < self.bytes.len() {
            self.index += 1;
        }
    }

    pub(super) fn bump_char(&mut self) -> Option<char> {
        let mut chars = self.remaining().chars();
        let c = chars.next()?;
        self.index += c.len_utf8();
        Some(c)
    }

    pub(super) fn starts_with(&self, s: &str) -> bool {
        self.remaining().starts_with(s)
    }

    pub(super) fn consume_exact(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.index += s.len();
            true
        } else {
            false
        }
    }

    /// Parse a full document into an owned DOM.
    pub fn parse_document(&mut self) -> CtxResult<Document<'a>> {
        let mut nodes = Vec::new();
        self.visit_document(crate::opts::Opts::new(), |node| {
            nodes.push(node);
            Ok(())
        })?;
        Ok(Document { nodes })
    }

    /// Stream top-level nodes to `visit` as each is parsed — Glaze primary path
    /// shape (`parse<Format>::op` fills `T` without retaining a full document).
    ///
    /// Cite: `references/glaze/include/glaze/core/read.hpp` (`read` → `parse::op`);
    /// unknown-key skip via `skip_value` when policy allows
    /// (`json/read.hpp` + `opts.error_on_unknown_keys`).
    ///
    /// `Opts::partial_read`: stop after the first successful `visit` (Glaze
    /// `partial_read` short-circuit after the structural object of interest).
    pub fn visit_document(
        &mut self,
        opts: crate::opts::Opts,
        mut visit: impl FnMut(Node<'a>) -> CtxResult<()>,
    ) -> CtxResult<()> {
        self.visit_document_at_nodes(opts, |parser| {
            let node = parser.parse_node()?;
            visit(node)
        })
    }

    /// Document loop without materializing a DOM [`Node`] per top-level item.
    ///
    /// At each top-level node start, `fill` must consume that node (e.g.
    /// [`Self::visit_node`] or [`crate::decode_node_visit`]). Slashdash nodes,
    /// BOM, version marker, `partial_read`, trailing validation, and
    /// `max_children` match [`Self::visit_document`].
    ///
    /// Glaze: array / sequence element loop calls `from::op` per element
    /// (`json/read.hpp`) without retaining a generic value.
    pub fn visit_document_at_nodes(
        &mut self,
        opts: crate::opts::Opts,
        fill: impl FnMut(&mut Self) -> CtxResult<()>,
    ) -> CtxResult<()> {
        self.visit_document_at_nodes_bits(opts.bits(), fill)
    }

    /// Const-generic document stream (Glaze `template <auto Opts>` monomorphization).
    ///
    /// `OPTS` is a packed bitset ([`crate::Opts::bits`] / [`crate::OPTS_DEFAULT`]).
    pub fn visit_document_at_nodes_const<const OPTS: u8>(
        &mut self,
        fill: impl FnMut(&mut Self) -> CtxResult<()>,
    ) -> CtxResult<()> {
        self.visit_document_at_nodes_bits(OPTS, fill)
    }

    fn visit_document_at_nodes_bits(
        &mut self,
        opts_bits: u8,
        mut fill: impl FnMut(&mut Self) -> CtxResult<()>,
    ) -> CtxResult<()> {
        use crate::opts::{flag_partial_read, flag_validate_trailing};

        self.skip_bom();
        self.skip_version_marker()?;
        let mut delivered = 0usize;
        loop {
            self.skip_line_space()?;
            if self.eof() {
                break;
            }
            if self.peek_byte() == Some(b'}') {
                return Err(self
                    .err(ErrorCode::UnexpectedToken)
                    .with_message("unmatched `}`"));
            }
            // Slashdash node — structural skip (Glaze skip_value role for
            // commented-out components).
            if self.starts_with("/-") {
                let start = self.index;
                self.index += 2;
                self.skip_line_space()?;
                self.skip_node()?;
                if self.index == start {
                    return Err(self
                        .err(ErrorCode::Syntax)
                        .with_message("invalid slashdash"));
                }
                continue;
            }
            fill(self)?;
            delivered += 1;
            if delivered > self.ctx.max_children {
                return Err(self
                    .err(ErrorCode::ExceededLimit)
                    .with_message("too many nodes"));
            }
            if flag_partial_read(opts_bits) {
                // Glaze partial_read: exit without parsing the rest.
                break;
            }
        }
        if flag_validate_trailing(opts_bits) && !flag_partial_read(opts_bits) {
            self.skip_line_space()?;
            if !self.eof() {
                return Err(self
                    .err(ErrorCode::UnexpectedToken)
                    .with_message("trailing content after document"));
            }
        }
        Ok(())
    }

    /// Collect top-level nodes with options (e.g. partial_read keeps only the first).
    pub fn parse_document_with_opts(&mut self, opts: crate::opts::Opts) -> CtxResult<Document<'a>> {
        let mut nodes = Vec::new();
        self.visit_document(opts, |node| {
            nodes.push(node);
            Ok(())
        })?;
        Ok(Document { nodes })
    }

    pub(super) fn skip_bom(&mut self) {
        if self.starts_with("\u{FEFF}") {
            self.index += "\u{FEFF}".len();
        }
    }

    /// Optional `/- kdl-version 2` (or 1) at document start.
    pub(super) fn skip_version_marker(&mut self) -> CtxResult<()> {
        self.skip_line_space()?;
        let saved = self.index;
        if !self.consume_exact("/-") {
            return Ok(());
        }
        self.skip_node_space()?;
        if !self.consume_exact("kdl-version") {
            self.index = saved;
            return Ok(());
        }
        self.skip_node_space()?;
        if !(self.consume_exact("2") || self.consume_exact("1")) {
            return Err(self
                .err(ErrorCode::Syntax)
                .with_message("kdl-version requires 1 or 2")
                .with_expected("1 or 2"));
        }
        self.skip_ws_only()?;
        // Must end with newline or eof per grammar.
        if self.eof() {
            return Ok(());
        }
        if self.consume_newline() {
            return Ok(());
        }
        // Single-line comment counts as terminator via newline.
        if self.starts_with("//") {
            self.skip_line_comment();
            return Ok(());
        }
        Err(self
            .err(ErrorCode::Syntax)
            .with_message("kdl-version marker must end with a newline"))
    }

    // Node visit / DOM: `reader/visit.rs`

    pub(super) fn parse_prop_or_arg(&mut self) -> CtxResult<Entry<'a>> {
        // Try type annotation then string; if followed by `=` it's a property key.
        let saved = self.index;
        let type_name = self.try_parse_type_annotation()?;
        self.skip_node_space()?;

        // Value keywords / numbers cannot be property keys.
        if self.at_value_keyword_or_number() {
            let value = self.parse_value_after_type(None)?;
            return Ok(Entry::Argument { type_name, value });
        }

        // Parse a string (ident / quoted / raw) as potential key or string arg.
        let string_start = self.index;
        let s = match self.parse_string_value() {
            Ok(s) => s,
            Err(_e) => {
                self.index = saved;
                let value = self.parse_value()?;
                return Ok(Entry::Argument { type_name, value });
            }
        };

        // `prop := string node-space* '=' ...` — probe for `=` without permanently
        // consuming the whitespace that separates this argument from the next entry.
        let after_string = self.index;
        self.skip_node_space()?;
        if self.peek_byte() == Some(b'=') {
            self.bump_byte();
            self.skip_node_space()?;
            let (val_ty, value) = self.parse_typed_value()?;
            if type_name.is_some() {
                return Err(self
                    .err_at(ErrorCode::Syntax, string_start)
                    .with_message("type annotation belongs on the property value, not the key"));
            }
            return Ok(Entry::Property {
                key: s,
                type_name: val_ty,
                value,
            });
        }
        self.index = after_string;

        Ok(Entry::Argument {
            type_name,
            value: Value::String(s),
        })
    }

    pub(super) fn skip_prop_or_arg(&mut self) -> CtxResult<()> {
        let _ = self.parse_prop_or_arg()?;
        Ok(())
    }

    pub(super) fn parse_typed_value(&mut self) -> CtxResult<(Option<KdlStr<'a>>, Value<'a>)> {
        let type_name = self.try_parse_type_annotation()?;
        self.skip_node_space()?;
        let value = self.parse_value_after_type(None)?;
        Ok((type_name, value))
    }

    pub(super) fn parse_value(&mut self) -> CtxResult<Value<'a>> {
        let type_name = self.try_parse_type_annotation()?;
        let _ = type_name; // type on bare parse_value is returned only via parse_typed_value
        self.skip_node_space()?;
        self.parse_value_after_type(None)
    }

    pub(super) fn parse_value_after_type(
        &mut self,
        _already: Option<KdlStr<'a>>,
    ) -> CtxResult<Value<'a>> {
        if self.eof() {
            return Err(self.err(ErrorCode::ExpectedValue).with_expected("value"));
        }

        // Keywords with #
        if self.peek_byte() == Some(b'#') {
            return self.parse_hash_keyword_or_raw_string();
        }

        // Quoted string
        if self.peek_byte() == Some(b'"') {
            return Ok(Value::String(self.parse_quoted_string()?));
        }

        // Number or identifier
        let c = self
            .peek_char()
            .ok_or_else(|| self.err(ErrorCode::ExpectedValue))?;
        if c == '+' || c == '-' || c == '.' || c.is_ascii_digit() {
            // Could still be ident like `.foo` or `+foo` per KDL2
            if self.looks_like_number() {
                return self.parse_number();
            }
        }

        Ok(Value::String(self.parse_string_value()?))
    }

    pub(super) fn at_value_keyword_or_number(&self) -> bool {
        match self.peek_byte() {
            Some(b'#') => true,
            Some(b'0'..=b'9') => true,
            Some(b'+') | Some(b'-') | Some(b'.') => self.looks_like_number(),
            _ => false,
        }
    }

    pub(super) fn looks_like_number(&self) -> bool {
        let s = self.remaining();
        let mut chars = s.chars();
        let first = match chars.next() {
            Some(c) => c,
            None => return false,
        };
        let rest = chars.as_str();
        match first {
            '+' | '-' => {
                let b = rest.as_bytes().first().copied();
                matches!(b, Some(b'0'..=b'9'))
                    || (b == Some(b'.')
                        && rest.as_bytes().get(1).is_some_and(|d| d.is_ascii_digit()))
                    || rest.starts_with("inf") // not valid without # in kdl2
            }
            '.' => rest.as_bytes().first().is_some_and(|d| d.is_ascii_digit()),
            '0'..='9' => true,
            _ => false,
        }
    }

    pub(super) fn parse_hash_keyword_or_raw_string(&mut self) -> CtxResult<Value<'a>> {
        // #true #false #null #inf #-inf #nan  or raw string #...# / ##"..."##
        if self.starts_with("#true") && self.hash_keyword_end("#true") {
            self.index += 5;
            return Ok(Value::Bool(true));
        }
        if self.starts_with("#false") && self.hash_keyword_end("#false") {
            self.index += 6;
            return Ok(Value::Bool(false));
        }
        if self.starts_with("#null") && self.hash_keyword_end("#null") {
            self.index += 5;
            return Ok(Value::Null);
        }
        if self.starts_with("#inf") && self.hash_keyword_end("#inf") {
            self.index += 4;
            return Ok(Value::float(f64::INFINITY));
        }
        if self.starts_with("#-inf") && self.hash_keyword_end("#-inf") {
            self.index += 5;
            return Ok(Value::float(f64::NEG_INFINITY));
        }
        if self.starts_with("#nan") && self.hash_keyword_end("#nan") {
            self.index += 4;
            return Ok(Value::float(f64::NAN));
        }
        // Raw string
        Ok(Value::String(self.parse_raw_string()?))
    }

    pub(super) fn hash_keyword_end(&self, kw: &str) -> bool {
        let after = self.input.get(self.index + kw.len()..).unwrap_or("");
        after.is_empty()
            || after.starts_with(|c: char| {
                is_unicode_space(c)
                    || is_newline_char(c)
                    || matches!(c, '\\' | '/' | '(' | ')' | '{' | '}' | ';' | '=' | '[')
            })
    }

    pub(super) fn try_parse_type_annotation(&mut self) -> CtxResult<Option<KdlStr<'a>>> {
        self.skip_node_space()?;
        if self.peek_byte() != Some(b'(') {
            return Ok(None);
        }
        self.bump_byte();
        self.skip_node_space()?;
        let name = self.parse_string_value()?;
        self.skip_node_space()?;
        if self.peek_byte() != Some(b')') {
            return Err(self
                .err(ErrorCode::Syntax)
                .with_message("unclosed type annotation")
                .with_expected("`)`"));
        }
        self.bump_byte();
        Ok(Some(name))
    }

    pub(super) fn parse_string_value(&mut self) -> CtxResult<KdlStr<'a>> {
        match self.peek_byte() {
            Some(b'"') => self.parse_quoted_string(),
            Some(b'#') => self.parse_raw_string(),
            Some(_) => self.parse_identifier_string(),
            None => Err(self.err(ErrorCode::UnexpectedEof).with_expected("string")),
        }
    }
}

/// Parse `input` into a document.
pub fn parse_document<'a>(input: &'a str) -> CtxResult<Document<'a>> {
    Parser::new(input).parse_document()
}

pub fn parse_document_with_context<'a>(input: &'a str, ctx: Context) -> CtxResult<Document<'a>> {
    Parser::with_context(input, ctx).parse_document()
}

/// Stream top-level nodes without requiring the caller to retain a full `Vec`
/// first (Glaze fill-as-you-parse). See [`Parser::visit_document`].
pub fn visit_document<'a>(
    input: &'a str,
    opts: crate::opts::Opts,
    visit: impl FnMut(Node<'a>) -> CtxResult<()>,
) -> CtxResult<()> {
    Parser::new(input).visit_document(opts, visit)
}

pub fn visit_document_with_context<'a>(
    input: &'a str,
    ctx: Context,
    opts: crate::opts::Opts,
    visit: impl FnMut(Node<'a>) -> CtxResult<()>,
) -> CtxResult<()> {
    Parser::with_context(input, ctx).visit_document(opts, visit)
}
