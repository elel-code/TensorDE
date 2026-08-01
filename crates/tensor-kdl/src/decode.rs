//! Typed decode traits (knus-inspired surface; Glaze-aligned read entrypoints live in `lib`).
//!
//! **P-G2 (see `docs/kdl/glaze-alignment.md`):** [`DecodeChildren`] avoids
//! `doc.nodes.clone()` when filling children-only document roots — Glaze never
//! builds a DOM to fill `T`; we still parse a `Document` today, but roots must
//! not clone the node list.
//!
//! **P-G3c:** [`DecodeFromVisit`] / [`VisitFill`] — monomorphized field sink
//! during `visit_node` (Glaze `decode_linear` + `from::op`).
//!
//! **P-G3d:** nested [`NestedFill`] / [`NestedProbe`] — child `from::op` without
//! intermediate [`Node`] when the child implements [`DecodeFromVisit`]; DOM
//! fallback otherwise. Top-level: [`read_nodes_into_visit`].
//!
//! **P-G3e:** [`DecodeDocument::read_stream`] — `read_into` entry; `Vec<T>`
//! streams via [`TopLevelFill`] (visit-fill when available). Named children-only
//! document roots still buffer top-level nodes for lookup.

mod nested_dispatch;
mod visit_fill;

pub use nested_dispatch::{NestedFill, NestedProbe, NestedViaVisit, NestedVisitTag, TopLevelFill};
pub use visit_fill::{
    DecodeFromVisit, VisitBuilder, VisitFill, decode_node_body_after_header, decode_node_str,
    decode_node_str_const, decode_node_visit, decode_node_visit_const, linear_prop_index,
    missing_argument_at, missing_child_named, missing_field, read_nodes_into_visit,
};

use crate::error::{CtxResult, ErrorCode, ErrorCtx};
#[cfg(feature = "dom")]
use crate::value::{Document, Entry, Node};
use crate::value::{KdlStr, Value};

/// Decode a scalar KDL value into `Self`.
pub trait DecodeScalar<'a>: Sized {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self>;
}

/// Decode a single KDL node from a parse tree (feature `dom` only).
///
/// Typed hot path uses [`DecodeFromVisit`] / [`DecodeDocument::read_stream`] instead.
#[cfg(feature = "dom")]
pub trait Decode<'a>: Sized {
    fn decode_node(node: &Node<'a>) -> CtxResult<Self>;
}

/// Decode from a slice of nodes (feature `dom` only).
#[cfg(feature = "dom")]
pub trait DecodeChildren<'a>: Sized {
    fn decode_children(nodes: &[Node<'a>]) -> CtxResult<Self>;
}

/// Decode a document (top-level nodes) into `Self`.
///
/// **P-G3e:** [`Self::read_stream`] is the Glaze primary path for
/// [`crate::read_into`] — override to avoid buffering a full [`Document`] when
/// the shape allows (e.g. [`Vec`] element loop).
///
/// **P-G11:** [`Self::read_stream_parser`] fills from an existing
/// [`crate::Parser`] (including [`crate::Parser::from_padded`]) so SWAR may
/// over-read padding.
pub trait DecodeDocument<'a>: Sized {
    /// Fill from a parse tree (feature `dom` only).
    #[cfg(feature = "dom")]
    fn decode_document(doc: &Document<'a>) -> CtxResult<Self>;

    /// Parse `input` and fill `out` in place (Glaze `read(T&, buffer, ctx)`).
    ///
    /// Default: stream top-level nodes into a temporary [`Document`], then
    /// [`Self::decode_document`]. Types that can fill without named multi-node
    /// lookup should override (see `Vec` + [`TopLevelFill`]).
    fn read_stream(
        out: &mut Self,
        input: &'a str,
        ctx: &mut crate::Context,
        opts: crate::Opts,
    ) -> ErrorCtx {
        #[cfg(feature = "dom")]
        {
            crate::read_document_buffered(out, input, ctx, opts)
        }
        #[cfg(not(feature = "dom"))]
        {
            ctx.clear_error();
            ctx.reset_depth();
            ctx.apply_opts(opts);
            let owned = crate::take_context_for_parser(ctx);
            let mut parser = crate::Parser::with_context(input, owned);
            let visit_result = Self::read_stream_parser(out, &mut parser, opts);
            let consumed = parser.offset();
            crate::restore_context_from_parser(ctx, parser);
            match visit_result {
                Ok(()) => ErrorCtx::ok(consumed),
                Err(e) => {
                    ctx.error = e.code;
                    ctx.custom_error_message = e.message.clone();
                    e
                }
            }
        }
    }

    /// Fill from a live [`crate::Parser`] (Glaze cursor already positioned).
    ///
    /// Default: collect top-level DOM nodes then [`Self::decode_document`].
    /// Override for streaming (see `Vec`).
    fn read_stream_parser(
        out: &mut Self,
        parser: &mut crate::Parser<'a>,
        opts: crate::Opts,
    ) -> CtxResult<()> {
        // Default buffers a Document (feature `dom`). Typed roots should
        // override with visit-fill streaming (Glaze primary path).
        #[cfg(feature = "dom")]
        {
            let mut nodes = Vec::new();
            parser.visit_document(opts, |node| {
                nodes.push(node);
                Ok(())
            })?;
            *out = Self::decode_document(&Document { nodes })?;
            Ok(())
        }
        #[cfg(not(feature = "dom"))]
        {
            let _ = (out, parser, opts);
            Err(ErrorCtx::new(ErrorCode::Syntax, 0).with_message(
                "this document root requires feature `dom` or a streaming DecodeDocument override",
            ))
        }
    }
}

#[cfg(feature = "dom")]
impl<'a, T: Decode<'a>> DecodeDocument<'a> for Vec<T> {
    fn decode_document(doc: &Document<'a>) -> CtxResult<Self> {
        doc.nodes.iter().map(T::decode_node).collect()
    }

    /// Stream each top-level node into the vec (Glaze array `from::op` loop).
    fn read_stream(
        out: &mut Self,
        input: &'a str,
        ctx: &mut crate::Context,
        opts: crate::Opts,
    ) -> ErrorCtx {
        ctx.clear_error();
        ctx.reset_depth();
        ctx.apply_opts(opts);
        out.clear();

        let owned = crate::take_context_for_parser(ctx);
        let mut parser = crate::Parser::with_context(input, owned);
        let visit_result = Self::read_stream_parser(out, &mut parser, opts);
        let consumed = parser.offset();
        crate::restore_context_from_parser(ctx, parser);

        match visit_result {
            Ok(()) => ErrorCtx::ok(consumed),
            Err(e) => {
                ctx.error = e.code;
                ctx.custom_error_message = e.message.clone();
                e
            }
        }
    }

    fn read_stream_parser(
        out: &mut Self,
        parser: &mut crate::Parser<'a>,
        opts: crate::Opts,
    ) -> CtxResult<()> {
        use nested_dispatch::{NestedProbe, TopLevelFill};

        out.clear();
        parser.visit_document_at_nodes(opts, |parser| {
            #[allow(clippy::needless_borrow)]
            let item = (&&NestedProbe::<T>::new()).fill_top(parser, opts)?;
            out.push(item);
            Ok(())
        })
    }
}

#[cfg(feature = "dom")]
impl<'a, T: Decode<'a>> DecodeChildren<'a> for Vec<T> {
    fn decode_children(nodes: &[Node<'a>]) -> CtxResult<Self> {
        nodes.iter().map(T::decode_node).collect()
    }
}

/// Helper: require exactly one argument and decode it.

#[cfg(not(feature = "dom"))]
impl<'a, T: DecodeFromVisit<'a>> DecodeDocument<'a> for Vec<T> {
    fn read_stream(
        out: &mut Self,
        input: &'a str,
        ctx: &mut crate::Context,
        opts: crate::Opts,
    ) -> ErrorCtx {
        ctx.clear_error();
        ctx.reset_depth();
        ctx.apply_opts(opts);
        out.clear();
        let owned = crate::take_context_for_parser(ctx);
        let mut parser = crate::Parser::with_context(input, owned);
        let visit_result = Self::read_stream_parser(out, &mut parser, opts);
        let consumed = parser.offset();
        crate::restore_context_from_parser(ctx, parser);
        match visit_result {
            Ok(()) => ErrorCtx::ok(consumed),
            Err(e) => {
                ctx.error = e.code;
                ctx.custom_error_message = e.message.clone();
                e
            }
        }
    }

    fn read_stream_parser(
        out: &mut Self,
        parser: &mut crate::Parser<'a>,
        opts: crate::Opts,
    ) -> CtxResult<()> {
        use nested_dispatch::{NestedProbe, TopLevelFill};
        out.clear();
        parser.visit_document_at_nodes(opts, |parser| {
            #[allow(clippy::needless_borrow)]
            let item = (&&NestedProbe::<T>::new()).fill_top(parser, opts)?;
            out.push(item);
            Ok(())
        })
    }
}

#[cfg(feature = "dom")]
pub fn one_argument<'a, T: DecodeScalar<'a>>(node: &Node<'a>) -> CtxResult<T> {
    let mut args = node.arguments();
    let first = args
        .next()
        .ok_or_else(|| ErrorCtx::new(ErrorCode::MissingArgument, 0).with_expected("argument"))?;
    if args.next().is_some() {
        return Err(ErrorCtx::new(ErrorCode::Syntax, 0).with_message("too many arguments"));
    }
    T::decode_scalar(first)
}

/// Helper: optional single argument.
#[cfg(feature = "dom")]
pub fn opt_argument<'a, T: DecodeScalar<'a>>(node: &Node<'a>) -> CtxResult<Option<T>> {
    let mut args = node.arguments();
    match args.next() {
        None => Ok(None),
        Some(v) => {
            if args.next().is_some() {
                return Err(ErrorCtx::new(ErrorCode::Syntax, 0).with_message("too many arguments"));
            }
            T::decode_scalar(v).map(Some)
        }
    }
}

/// Helper: required property by name.
#[cfg(feature = "dom")]
pub fn property<'a, T: DecodeScalar<'a>>(node: &Node<'a>, name: &str) -> CtxResult<T> {
    let value = node.property(name).ok_or_else(|| {
        ErrorCtx::new(ErrorCode::MissingProperty, 0)
            .with_message(format!("missing property `{name}`"))
    })?;
    T::decode_scalar(value)
}

/// Helper: optional property by name (`null` → `None`).
#[cfg(feature = "dom")]
pub fn opt_property<'a, T: DecodeScalar<'a>>(node: &Node<'a>, name: &str) -> CtxResult<Option<T>> {
    match node.property(name) {
        None => Ok(None),
        Some(Value::Null) => Ok(None),
        Some(v) => T::decode_scalar(v).map(Some),
    }
}

/// Helper: required child by name.
#[cfg(feature = "dom")]
pub fn child<'a, T: Decode<'a>>(node: &Node<'a>, name: &str) -> CtxResult<T> {
    child_in(node.children.as_slice(), name)
}

/// Helper: optional child by name.
#[cfg(feature = "dom")]
pub fn opt_child<'a, T: Decode<'a>>(node: &Node<'a>, name: &str) -> CtxResult<Option<T>> {
    opt_child_in(node.children.as_slice(), name)
}

/// Helper: children, optionally filtered by name.
#[cfg(feature = "dom")]
pub fn children<'a, T: Decode<'a>>(node: &Node<'a>, name: Option<&str>) -> CtxResult<Vec<T>> {
    children_in(node.children.as_slice(), name)
}

/// Required child in a node slice (document root / children block) — no parent `Node`.
#[cfg(feature = "dom")]
pub fn child_in<'a, T: Decode<'a>>(nodes: &[Node<'a>], name: &str) -> CtxResult<T> {
    let child = nodes
        .iter()
        .find(|n| n.name.as_str() == name)
        .ok_or_else(|| {
            ErrorCtx::new(ErrorCode::MissingChild, 0)
                .with_message(format!("missing child `{name}`"))
        })?;
    T::decode_node(child)
}

/// Optional child in a node slice.
#[cfg(feature = "dom")]
pub fn opt_child_in<'a, T: Decode<'a>>(nodes: &[Node<'a>], name: &str) -> CtxResult<Option<T>> {
    match nodes.iter().find(|n| n.name.as_str() == name) {
        None => Ok(None),
        Some(c) => T::decode_node(c).map(Some),
    }
}

/// Children from a node slice, optionally filtered by name.
#[cfg(feature = "dom")]
pub fn children_in<'a, T: Decode<'a>>(nodes: &[Node<'a>], name: Option<&str>) -> CtxResult<Vec<T>> {
    match name {
        Some(n) => nodes
            .iter()
            .filter(|node| node.name.as_str() == n)
            .map(T::decode_node)
            .collect(),
        None => nodes.iter().map(T::decode_node).collect(),
    }
}

/// First argument of a child found by name in a slice (unwrap(argument) for roots).
#[cfg(feature = "dom")]
pub fn one_argument_in<'a, T: DecodeScalar<'a>>(nodes: &[Node<'a>], name: &str) -> CtxResult<T> {
    let child = nodes
        .iter()
        .find(|n| n.name.as_str() == name)
        .ok_or_else(|| {
            ErrorCtx::new(ErrorCode::MissingChild, 0)
                .with_message(format!("missing child `{name}`"))
        })?;
    one_argument(child)
}

/// Optional first-argument peel from a named child in a slice.
#[cfg(feature = "dom")]
pub fn opt_one_argument_in<'a, T: DecodeScalar<'a>>(
    nodes: &[Node<'a>],
    name: &str,
) -> CtxResult<Option<T>> {
    match nodes.iter().find(|n| n.name.as_str() == name) {
        None => Ok(None),
        Some(c) => opt_argument(c),
    }
}

/// Partial structure that can absorb unknown children/properties (`flatten`).
#[cfg(feature = "dom")]
pub trait DecodePartial<'a>: Default {
    /// Try to store `node` as a child. Returns `Ok(true)` if consumed.
    fn insert_child(&mut self, node: &Node<'a>) -> CtxResult<bool>;

    /// Try to store a property. Returns `Ok(true)` if consumed.
    fn insert_property(&mut self, key: &str, value: &Value<'a>) -> CtxResult<bool>;
}

/// Unwrap a single-property child node: `width 0.5` / `width prop=...` style peels.
#[cfg(feature = "dom")]
pub fn one_property<'a, T: DecodeScalar<'a>>(node: &Node<'a>, name: &str) -> CtxResult<T> {
    property(node, name)
}

/// Optional single property peel.
#[cfg(feature = "dom")]
pub fn opt_one_property<'a, T: DecodeScalar<'a>>(
    node: &Node<'a>,
    name: &str,
) -> CtxResult<Option<T>> {
    opt_property(node, name)
}

/// Decode the first argument, or if none, treat a bare flag-like node as defaultable.
#[cfg(feature = "dom")]
pub fn argument_or_flag<'a, T: DecodeScalar<'a> + Default>(node: &Node<'a>) -> CtxResult<T> {
    match opt_argument::<T>(node)? {
        Some(v) => Ok(v),
        None => Ok(T::default()),
    }
}

// --- DecodeScalar impls for std types ---

impl<'a> DecodeScalar<'a> for String {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        match value {
            Value::String(s) => Ok(s.as_str().to_owned()),
            Value::Int(n) => Ok(n.to_string()),
            Value::Float { value, raw } => Ok(raw
                .as_ref()
                .map(|r| r.as_str().to_owned())
                .unwrap_or_else(|| value.to_string())),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Null => Err(ErrorCtx::new(ErrorCode::TypeMismatch, 0)
                .with_message("expected string, found null")),
        }
    }
}

impl<'a> DecodeScalar<'a> for bool {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        match value {
            Value::Bool(b) => Ok(*b),
            _ => Err(ErrorCtx::new(ErrorCode::TypeMismatch, 0).with_expected("bool")),
        }
    }
}

macro_rules! impl_int_scalar {
    ($($t:ty),* $(,)?) => {$(
        impl<'a> DecodeScalar<'a> for $t {
            fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
                match value {
                    Value::Int(n) => (*n).try_into().map_err(|_| {
                        ErrorCtx::new(ErrorCode::TypeMismatch, 0)
                            .with_message(concat!("integer out of range for ", stringify!($t)))
                    }),
                    _ => Err(ErrorCtx::new(ErrorCode::TypeMismatch, 0).with_expected("integer")),
                }
            }
        }
    )*};
}

impl_int_scalar!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

impl<'a> DecodeScalar<'a> for f64 {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        value
            .as_f64()
            .ok_or_else(|| ErrorCtx::new(ErrorCode::TypeMismatch, 0).with_expected("number"))
    }
}

impl<'a> DecodeScalar<'a> for f32 {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        value
            .as_f64()
            .map(|n| n as f32)
            .ok_or_else(|| ErrorCtx::new(ErrorCode::TypeMismatch, 0).with_expected("number"))
    }
}

impl<'a, T: DecodeScalar<'a>> DecodeScalar<'a> for Option<T> {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        match value {
            Value::Null => Ok(None),
            other => T::decode_scalar(other).map(Some),
        }
    }
}

impl<'a> DecodeScalar<'a> for () {
    fn decode_scalar(value: &Value<'a>) -> CtxResult<Self> {
        match value {
            Value::Null => Ok(()),
            _ => Err(ErrorCtx::new(ErrorCode::TypeMismatch, 0).with_expected("null")),
        }
    }
}

/// Presence-only child node (no arguments).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flag;

#[cfg(feature = "dom")]
#[cfg(feature = "dom")]
impl<'a> Decode<'a> for Flag {
    fn decode_node(node: &Node<'a>) -> CtxResult<Self> {
        if node
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Argument { .. }))
        {
            return Err(ErrorCtx::new(ErrorCode::Syntax, 0)
                .with_message("flag node must not have arguments"));
        }
        Ok(Flag)
    }
}

impl<'a> DecodeFromVisit<'a> for Flag {
    type Builder = FlagVisitBuilder;

    fn start_visit() -> Self::Builder {
        FlagVisitBuilder::default()
    }
}

#[derive(Default)]
pub struct FlagVisitBuilder {
    saw_arg: bool,
}

impl<'a> VisitBuilder<'a> for FlagVisitBuilder {
    type Output = Flag;

    fn on_header(&mut self, _type_name: Option<KdlStr<'a>>, _name: KdlStr<'a>) -> CtxResult<()> {
        Ok(())
    }

    fn on_argument(
        &mut self,
        _type_name: Option<KdlStr<'a>>,
        _value: Value<'a>,
    ) -> CtxResult<bool> {
        self.saw_arg = true;
        Err(ErrorCtx::new(ErrorCode::Syntax, 0).with_message("flag node must not have arguments"))
    }

    fn on_property(
        &mut self,
        _key: KdlStr<'a>,
        _type_name: Option<KdlStr<'a>>,
        _value: Value<'a>,
    ) -> CtxResult<bool> {
        Ok(false)
    }

    fn finish(self) -> CtxResult<Flag> {
        if self.saw_arg {
            return Err(ErrorCtx::new(ErrorCode::Syntax, 0)
                .with_message("flag node must not have arguments"));
        }
        Ok(Flag)
    }
}
