//! High-performance KDL 2.0 parser and typed decode for TensorDE.
//!
//! Design: [`docs/kdl/design.md`](../../docs/kdl/design.md).  
//! Glaze mechanical contract: [`docs/kdl/glaze-alignment.md`](../../docs/kdl/glaze-alignment.md).
//!
//! # Quick start
//!
//! ```
//! use tensor_kdl::from_str;
//!
//! let doc = from_str(r#"
//!     node 1 key="value" {
//!         child
//!     }
//! "#).unwrap();
//! assert_eq!(doc.nodes[0].name.as_str(), "node");
//! ```
//!
//! # Glaze-aligned read API
//!
//! Mirrors `glz::read_json` (`references/glaze/include/glaze/json/read.hpp`):
//!
//! - [`read_into`] — fill an existing `T` (Glaze `read_json(T&, buffer)`)
//! - [`read`] — allocate `T` (Glaze `expected<T, error_ctx> read_json(buffer)`)
//! - [`read_with_context`] — reuse [`Context`] / scratch (perf doc)
//! - [`visit_document`] — stream top-level nodes as parsed (Glaze `parse::op` shape)
//! - [`Opts`] — policy flags (Glaze `glz::opts`)
//!
//! # Performance notes
//!
//! - Single-pass recursive descent over UTF-8 bytes
//! - SWAR helpers for ASCII whitespace and quoted-string scans (Glaze `util/parse.hpp`)
//! - Reusable [`Context`] scratch buffer for unescaping
//! - Depth guard against pathological nesting (Glaze `depth_guard`)
//! - Document roots implementing [`Decode`] can be filled node-by-node via
//!   [`read_nodes_into`] without retaining the full `Document` vec first

#![forbid(unsafe_code)]

mod context;
mod decode;
mod encode;
mod error;
mod opts;
mod pad;
mod parse;
mod query;
mod value;
mod write;

pub use context::{Context, DEFAULT_MAX_DEPTH, DepthGuard};
pub use decode::{
    Decode, DecodeChildren, DecodeDocument, DecodeFromVisit, DecodePartial, DecodeScalar, Flag,
    NestedFill, NestedProbe, NestedViaDom, NestedViaVisit, NestedVisitTag, TopLevelFill,
    VisitBuilder, VisitFill, argument_or_flag, child, child_in, children, children_in,
    decode_node_body_after_header, decode_node_str, decode_node_str_const, decode_node_visit,
    decode_node_visit_const, linear_prop_index, missing_argument_at, missing_child_named,
    missing_field, one_argument, one_argument_in, one_property, opt_argument, opt_child,
    opt_child_in, opt_one_argument_in, opt_one_property, opt_property, property,
    read_nodes_into_visit,
};
pub use encode::{
    Encode, EncodeDocument, EncodePartial, EncodeScalar, WriteSink, arg_entry, arg_node, flag_node,
    prop_entry, to_string, to_string_node, write, write_arg_node_line, write_argument_prefix,
    write_bool, write_children_close, write_children_open, write_f64, write_flag_line, write_i128,
    write_ident_or_string, write_indent, write_into, write_into_slice, write_node_end_leaf,
    write_node_header, write_node_into, write_node_into_slice, write_null, write_prop_node_line,
    write_property_key, write_quoted,
};
pub use error::{CtxResult, Error, ErrorCode, ErrorCtx, Result, format_error, format_error_code};
#[cfg(feature = "diagnostics")]
pub use error::{report_error, report_error_named};
pub use opts::{
    FLAG_ERROR_ON_MISSING, FLAG_ERROR_ON_UNKNOWN, FLAG_PARTIAL_READ, FLAG_VALIDATE_TRAILING,
    OPTS_DEFAULT, OPTS_LENIENT, OPTS_PARTIAL, Opts, flag_error_on_missing, flag_error_on_unknown,
    flag_partial_read, flag_validate_trailing,
};
pub use pad::{PADDING_BYTES, PaddedInput, load_u64_for_scan, pad_string, unpad_string};
pub use parse::{
    CountingVisitor, DomNodeBuilder, NodeVisitor, Parser, parse_document,
    parse_document_with_context, visit_document, visit_document_with_context,
};
pub use query::{query, query_node};
pub use value::{Document, Entry, KdlStr, Node, Value};
pub use write::{format_document, format_document_into, format_node_into};

#[cfg(feature = "derive")]
pub use tensor_kdl_macros::{
    Decode as DecodeMacro, DecodeScalar as DecodeScalarMacro, Encode as EncodeMacro,
    EncodeScalar as EncodeScalarMacro,
};

#[cfg(feature = "derive")]
pub use tensor_kdl_macros::{Decode, DecodeScalar, Encode, EncodeScalar};

/// Parse a KDL 2.0 document from `input` into a DOM.
///
/// **KDL vs Glaze:** Glaze `read` rejects empty JSON buffers (`no_read_input`)
/// because JSON-text requires a value. KDL documents may be empty (zero nodes);
/// see official suite `empty.kdl`. Empty input therefore succeeds as an empty
/// [`Document`] (`docs/kdl/glaze-alignment.md`: KDL wins on syntax).
pub fn from_str(input: &str) -> Result<Document<'_>> {
    parse_document(input).map_err(Error::from)
}

/// Parse from a [`PaddedInput`] (Glaze padded `std::string` pattern).
///
/// **P-G10a:** uses [`Parser::from_padded`] so SWAR may over-read into the
/// 16 trailing zero bytes; logical EOF stays at content length.
pub fn from_padded(input: &PaddedInput) -> Result<Document<'_>> {
    Parser::from_padded(input)
        .parse_document()
        .map_err(Error::from)
}

/// [`read_into`] from a [`PaddedInput`] (padded parser path).
pub fn read_into_padded<'a, T: DecodeDocument<'a>>(
    value: &mut T,
    input: &'a PaddedInput,
) -> ErrorCtx {
    read_into_padded_with_opts(value, input, &mut Context::new(), Opts::new())
}

/// Like [`read_into_padded`] with reused [`Context`].
///
/// The parser keeps the logical `&str` for borrowed decoded values while its
/// scanners see the complete padded allocation.
pub fn read_into_padded_with_context<'a, T: DecodeDocument<'a>>(
    value: &mut T,
    input: &'a PaddedInput,
    ctx: &mut Context,
) -> ErrorCtx {
    read_into_padded_with_opts(value, input, ctx, Opts::new())
}

/// Padded [`read_into_with_opts`] using one live [`Parser`].
pub fn read_into_padded_with_opts<'a, T: DecodeDocument<'a>>(
    value: &mut T,
    input: &'a PaddedInput,
    ctx: &mut Context,
    opts: Opts,
) -> ErrorCtx {
    ctx.clear_error();
    ctx.reset_depth();
    ctx.apply_opts(opts);

    let owned = take_context_for_parser(ctx);
    let mut parser = Parser::from_padded_with_context(input, owned);
    let result = T::read_stream_parser(value, &mut parser, opts);
    let consumed = parser.offset();
    restore_context_from_parser(ctx, parser);

    match result {
        Ok(()) => ErrorCtx::ok(consumed),
        Err(error) => {
            ctx.error = error.code;
            ctx.custom_error_message = error.message.clone();
            error
        }
    }
}

/// Const-generic padded read (Glaze `template <auto Opts>` + padded buffer).
pub fn read_into_padded_const<'a, T: DecodeDocument<'a>, const OPTS: u8>(
    value: &mut T,
    input: &'a PaddedInput,
) -> ErrorCtx {
    read_into_padded_with_opts(value, input, &mut Context::new(), Opts::from_bits(OPTS))
}

/// Parse with an explicit reusable [`Context`] (Glaze ctx reuse).
pub fn from_str_with_context<'a>(input: &'a str, ctx: Context) -> Result<Document<'a>> {
    parse_document_with_context(input, ctx).map_err(Error::from)
}

/// Parse a document and decode it into `T` (allocating).
///
/// Prefer [`read_into`] to reuse `T` (Glaze in-place), or [`read`] when `T: Default`.
pub fn from_str_decode<'a, T: DecodeDocument<'a>>(input: &'a str) -> Result<T> {
    let doc = from_str(input)?;
    T::decode_document(&doc).map_err(Error::from)
}

/// Glaze `read_json(T&, buffer)` — parse and decode **into** an existing value.
///
/// Returns [`ErrorCtx`] always (Glaze `error_ctx`). Success: `!ec.is_err()`.
/// `ec.consumed` is bytes processed (Glaze `count`).
///
/// **P-G3e:** dispatches to [`DecodeDocument::read_stream`]. `Vec<T>` streams
/// element-by-element via [`TopLevelFill`] (visit-fill when `T: DecodeFromVisit`).
/// Named children-only roots still buffer top-level [`Node`]s for lookup.
pub fn read_into<'a, T: DecodeDocument<'a>>(value: &mut T, input: &'a str) -> ErrorCtx {
    read_into_with_context(value, input, &mut Context::new())
}

/// Like [`read_into`] with a reused [`Context`] (Glaze `read(..., ctx)`).
pub fn read_into_with_context<'a, T: DecodeDocument<'a>>(
    value: &mut T,
    input: &'a str,
    ctx: &mut Context,
) -> ErrorCtx {
    read_into_with_opts(value, input, ctx, Opts::new())
}

/// [`read_into`] with explicit [`Opts`] (Glaze `read<Opts>(...)`).
pub fn read_into_with_opts<'a, T: DecodeDocument<'a>>(
    value: &mut T,
    input: &'a str,
    ctx: &mut Context,
    opts: Opts,
) -> ErrorCtx {
    T::read_stream(value, input, ctx, opts)
}

/// Const-generic [`read_into`] (Glaze `template <auto Opts> read(...)`).
///
/// `OPTS` is a packed bitset — use [`OPTS_DEFAULT`], [`OPTS_LENIENT`],
/// [`OPTS_PARTIAL`], or [`Opts::bits`]. Policy branches monomorphize at the
/// call site (P-G4; Rust cannot use a struct as a const generic).
pub fn read_into_const<'a, T: DecodeDocument<'a>, const OPTS: u8>(
    value: &mut T,
    input: &'a str,
) -> ErrorCtx {
    read_into_const_with_context::<T, OPTS>(value, input, &mut Context::new())
}

/// Like [`read_into_const`] with a reused [`Context`].
pub fn read_into_const_with_context<'a, T: DecodeDocument<'a>, const OPTS: u8>(
    value: &mut T,
    input: &'a str,
    ctx: &mut Context,
) -> ErrorCtx {
    T::read_stream(value, input, ctx, Opts::from_bits(OPTS))
}

/// Default [`DecodeDocument::read_stream`]: buffer top-level nodes, then
/// [`DecodeDocument::decode_document`]. Used by named children-only roots.
pub(crate) fn read_document_buffered<'a, T: DecodeDocument<'a>>(
    value: &mut T,
    input: &'a str,
    ctx: &mut Context,
    opts: Opts,
) -> ErrorCtx {
    ctx.clear_error();
    ctx.reset_depth();
    ctx.apply_opts(opts);

    let owned = take_context_for_parser(ctx);
    let mut parser = Parser::with_context(input, owned);
    let mut nodes = Vec::new();
    let visit_result = parser.visit_document(opts, |node| {
        nodes.push(node);
        Ok(())
    });
    let consumed = parser.offset();
    restore_context_from_parser(ctx, parser);

    if let Err(e) = visit_result {
        ctx.error = e.code;
        ctx.custom_error_message = e.message.clone();
        return e;
    }

    let doc = Document { nodes };
    match T::decode_document(&doc) {
        Ok(decoded) => {
            *value = decoded;
            ErrorCtx::ok(consumed)
        }
        Err(e) => {
            ctx.error = e.code;
            ctx.custom_error_message = e.message.clone();
            e.with_consumed(consumed)
        }
    }
}

/// Stream-parse top-level nodes into a `Vec`, decoding each with `Decode` as it
/// arrives when `partial_read` is off (Glaze array element fill pattern).
///
/// For `partial_read`, only the first node is parsed and decoded.
///
/// Prefer [`read_nodes_into_visit`] when `T: DecodeFromVisit` to avoid per-node
/// DOM [`Node`] allocation (P-G3d; Glaze element `from::op`).
pub fn read_nodes_into<'a, T: Decode<'a>>(
    out: &mut Vec<T>,
    input: &'a str,
    ctx: &mut Context,
    opts: Opts,
) -> ErrorCtx {
    ctx.clear_error();
    ctx.reset_depth();
    ctx.apply_opts(opts);
    out.clear();

    let owned = take_context_for_parser(ctx);
    let mut parser = Parser::with_context(input, owned);
    let visit_result = parser.visit_document(opts, |node| {
        out.push(T::decode_node(&node)?);
        Ok(())
    });
    let consumed = parser.offset();
    restore_context_from_parser(ctx, parser);

    match visit_result {
        Ok(()) => ErrorCtx::ok(consumed),
        Err(e) => {
            ctx.error = e.code;
            ctx.custom_error_message = e.message.clone();
            e
        }
    }
}

/// Glaze `expected<T, error_ctx> read_json(buffer)` — allocate and fill `T`.
pub fn read<'a, T: DecodeDocument<'a> + Default>(
    input: &'a str,
) -> std::result::Result<T, ErrorCtx> {
    let mut value = T::default();
    let ec = read_into(&mut value, input);
    if ec.is_err() { Err(ec) } else { Ok(value) }
}

/// Like [`read`] with reused context.
pub fn read_with_context<'a, T: DecodeDocument<'a> + Default>(
    input: &'a str,
    ctx: &mut Context,
) -> std::result::Result<T, ErrorCtx> {
    let mut value = T::default();
    let ec = read_into_with_context(&mut value, input, ctx);
    if ec.is_err() { Err(ec) } else { Ok(value) }
}

/// Parse DOM only, returning Glaze-shaped [`ErrorCtx`] on failure.
///
/// Empty input is a valid empty document (KDL), not `NoReadInput`.
pub fn read_document(input: &str) -> std::result::Result<Document<'_>, ErrorCtx> {
    let mut parser = Parser::new(input);
    parser.parse_document()
}

/// Parse DOM with [`Opts`] (e.g. [`Opts::partial`] keeps only the first node).
pub fn read_document_with_opts(
    input: &str,
    opts: Opts,
) -> std::result::Result<Document<'_>, ErrorCtx> {
    let mut parser = Parser::new(input);
    parser.parse_document_with_opts(opts)
}

/// Split scratch/limits out of `ctx` for a temporary [`Parser`] (derive + lib).
pub fn take_context_for_parser(ctx: &mut Context) -> Context {
    Context {
        error: ctx.error,
        custom_error_message: ctx.custom_error_message.clone(),
        depth: 0,
        current_file: std::mem::take(&mut ctx.current_file),
        scratch: std::mem::take(&mut ctx.scratch),
        max_string_len: ctx.max_string_len,
        max_children: ctx.max_children,
        max_depth: ctx.max_depth,
        error_on_unknown_keys: ctx.error_on_unknown_keys,
        error_on_missing_keys: ctx.error_on_missing_keys,
    }
}

/// Restore scratch/depth from a temporary [`Parser`] back into `ctx`.
pub fn restore_context_from_parser(ctx: &mut Context, parser: Parser<'_>) {
    let back = parser.into_context();
    ctx.scratch = back.scratch;
    ctx.current_file = back.current_file;
    ctx.depth = back.depth;
    ctx.error = back.error;
    ctx.custom_error_message = back.custom_error_message;
}
