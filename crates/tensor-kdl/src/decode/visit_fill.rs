//! Fill `T` from [`NodeVisitor`] events — Glaze monomorphized member write.
//!
//! Cite: `references/glaze/include/glaze/json/read.hpp`
//! - `decode_linear`: linear search over `reflect<T>::keys`
//! - `decode_index`: on match, `from::op` into `get_member(value, …)`
//!
//! KDL roles: arguments (positional index), properties (linear key match),
//! children (linear name match). Derive emits [`DecodeFromVisit`];
//! [`VisitFill`] is the runtime visitor shell.

use crate::error::{CtxResult, ErrorCode, ErrorCtx};
use crate::opts::Opts;
use crate::parse::visitor::NodeVisitor;
use crate::value::{KdlStr, Node, Value};

/// Schema sink filled during [`crate::Parser::visit_node`].
///
/// Glaze writes directly into `T&`; we accumulate into a builder that
/// [`VisitBuilder::finish`] converts to `T` (allows `Default` field init +
/// presence checks).
pub trait DecodeFromVisit<'a>: Sized {
    type Builder: VisitBuilder<'a, Output = Self>;

    fn start_visit() -> Self::Builder;
}

/// Builder receiving visitor events (implemented by derive or by hand).
pub trait VisitBuilder<'a>: Sized {
    type Output;

    fn on_header(&mut self, type_name: Option<KdlStr<'a>>, name: KdlStr<'a>) -> CtxResult<()>;

    /// Glaze `decode_linear` role for positional slots.
    fn on_argument(&mut self, type_name: Option<KdlStr<'a>>, value: Value<'a>) -> CtxResult<bool>;

    /// Return whether `key` was recognized (Glaze: index &lt; N).
    fn on_property(
        &mut self,
        key: KdlStr<'a>,
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool>;

    /// DOM child fallback (when nested visit is not used).
    fn on_child(&mut self, child: Node<'a>) -> CtxResult<bool>;

    /// P-G3d: optional nested visit-fill for a child named `name`.
    ///
    /// Return `Ok(true)` after fully consuming the child body via
    /// `parser.finish_nested_child`. Default uses DOM + [`Self::on_child`].
    fn take_child_after_header(
        &mut self,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<bool> {
        let _ = (parser, opts, type_name, name);
        Ok(false)
    }

    fn finish(self) -> CtxResult<Self::Output>;
}

/// Adapts a [`VisitBuilder`] to [`NodeVisitor`] for `Parser::visit_node`.
pub struct VisitFill<'a, B: VisitBuilder<'a>> {
    pub builder: B,
    _phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a, B: VisitBuilder<'a>> VisitFill<'a, B> {
    pub fn new(builder: B) -> Self {
        Self {
            builder,
            _phantom: core::marker::PhantomData,
        }
    }

    pub fn finish(self) -> CtxResult<B::Output> {
        self.builder.finish()
    }
}

impl<'a, B: VisitBuilder<'a>> NodeVisitor<'a> for VisitFill<'a, B> {
    fn on_header(&mut self, type_name: Option<KdlStr<'a>>, name: KdlStr<'a>) -> CtxResult<()> {
        self.builder.on_header(type_name, name)
    }

    fn on_argument(&mut self, type_name: Option<KdlStr<'a>>, value: Value<'a>) -> CtxResult<bool> {
        self.builder.on_argument(type_name, value)
    }

    fn on_property(
        &mut self,
        key: KdlStr<'a>,
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool> {
        self.builder.on_property(key, type_name, value)
    }

    fn on_child(&mut self, child: Node<'a>) -> CtxResult<bool> {
        self.builder.on_child(child)
    }

    fn take_child_after_header(
        &mut self,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<bool> {
        self.builder
            .take_child_after_header(parser, opts, type_name, name)
    }
}

/// Parse one node from `input` into `T` via visit-fill (no intermediate DOM
/// [`Node`] for the root).
///
/// Nested children that implement [`DecodeFromVisit`] can be filled without a
/// parent retaining them only when the parent's `VisitBuilder` uses
/// [`VisitBuilder::take_child_after_header`]; the default derive still uses
/// DOM `on_child` for compatibility. Call [`decode_node_visit`] for the raw
/// parser path used by nested fills.
pub fn decode_node_str<'a, T: DecodeFromVisit<'a>>(input: &'a str, opts: Opts) -> CtxResult<T> {
    let mut parser = crate::Parser::new(input);
    decode_node_visit(&mut parser, opts)
}

/// Const-generic [`decode_node_str`] (P-G4 / Glaze `template <auto Opts>`).
pub fn decode_node_str_const<'a, T: DecodeFromVisit<'a>, const OPTS: u8>(
    input: &'a str,
) -> CtxResult<T> {
    let mut parser = crate::Parser::new(input);
    decode_node_visit_const::<T, OPTS>(&mut parser)
}

/// Fill `T` from the current parser position (one node) via visit-fill.
pub fn decode_node_visit<'a, T: DecodeFromVisit<'a>>(
    parser: &mut crate::Parser<'a>,
    opts: Opts,
) -> CtxResult<T> {
    let mut fill = VisitFill::new(T::start_visit());
    parser.visit_node(opts, &mut fill)?;
    fill.finish()
}

/// Const-generic [`decode_node_visit`].
pub fn decode_node_visit_const<'a, T: DecodeFromVisit<'a>, const OPTS: u8>(
    parser: &mut crate::Parser<'a>,
) -> CtxResult<T> {
    let mut fill = VisitFill::new(T::start_visit());
    parser.visit_node_const::<OPTS, _>(&mut fill)?;
    fill.finish()
}

/// After `parse_node_header`, finish the current node into `T` (nested child path).
pub fn decode_node_body_after_header<'a, T: DecodeFromVisit<'a>>(
    parser: &mut crate::Parser<'a>,
    opts: Opts,
    type_name: Option<KdlStr<'a>>,
    name: KdlStr<'a>,
) -> CtxResult<T> {
    let mut fill = VisitFill::new(T::start_visit());
    fill.builder.on_header(type_name, name)?;
    parser.finish_nested_child(opts, &mut fill)?;
    fill.finish()
}

/// Helper used by generated builders: linear property key match (Glaze `decode_linear`).
#[inline(always)]
pub fn linear_prop_index(keys: &[&str], key: &str) -> Option<usize> {
    keys.iter().position(|k| *k == key)
}

/// Finish helper when a required field was never set.
pub fn missing_field(name: &str) -> ErrorCtx {
    ErrorCtx::new(ErrorCode::MissingProperty, 0).with_message(format!("missing field `{name}`"))
}

pub fn missing_argument_at(index: usize) -> ErrorCtx {
    ErrorCtx::new(ErrorCode::MissingArgument, 0)
        .with_message(format!("missing argument at index {index}"))
}

pub fn missing_child_named(name: &str) -> ErrorCtx {
    ErrorCtx::new(ErrorCode::MissingChild, 0).with_message(format!("missing child `{name}`"))
}

/// Stream top-level nodes into `out` using visit-fill (no per-node DOM [`Node`]).
///
/// Glaze array path: each element `from::op` without retaining a generic value.
/// Cite: `core/read.hpp` + array element loop in `json/read.hpp`.
///
/// Equivalent to [`crate::read_into`] on `Vec<T>` when `T: DecodeFromVisit`
/// (P-G3e routes through [`crate::TopLevelFill`]). Kept as an explicit API for
/// call sites that want the visit-only bound in the type signature.
pub fn read_nodes_into_visit<'a, T: DecodeFromVisit<'a>>(
    out: &mut Vec<T>,
    input: &'a str,
    ctx: &mut crate::Context,
    opts: Opts,
) -> ErrorCtx {
    // Same body as Vec<T: Decode>::read_stream with visit-only monomorphization
    // forced by the trait bound (always hits TopLevelFill &&Probe arm).
    use super::nested_dispatch::{NestedProbe, TopLevelFill};

    ctx.clear_error();
    ctx.reset_depth();
    ctx.apply_opts(opts);
    out.clear();

    let owned = crate::take_context_for_parser(ctx);
    let mut parser = crate::Parser::with_context(input, owned);
    let visit_result = parser.visit_document_at_nodes(opts, |parser| {
        // Double-ref is required for autoref specialization (visit vs DOM).
        #[allow(clippy::needless_borrow)]
        let item = (&&NestedProbe::<T>::new()).fill_top(parser, opts)?;
        out.push(item);
        Ok(())
    });
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
