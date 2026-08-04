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
#[cfg(feature = "dom")]
use crate::value::Node;
use crate::value::{KdlStr, Value};

use super::DecodeScalar;

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

    fn on_header_at(
        &mut self,
        offset: usize,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<()> {
        self.on_header(type_name, name).map_err(|mut error| {
            if error.consumed == 0 {
                error.consumed = offset;
            }
            error
        })
    }

    /// Glaze `decode_linear` role for positional slots.
    fn on_argument(&mut self, type_name: Option<KdlStr<'a>>, value: Value<'a>) -> CtxResult<bool>;

    fn on_argument_at(
        &mut self,
        offset: usize,
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool> {
        self.on_argument(type_name, value).map_err(|mut error| {
            if error.consumed == 0 {
                error.consumed = offset;
            }
            error
        })
    }

    /// Return whether `key` was recognized (Glaze: index &lt; N).
    fn on_property(
        &mut self,
        key: KdlStr<'a>,
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool>;

    fn on_property_at(
        &mut self,
        offset: usize,
        key: KdlStr<'a>,
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool> {
        self.on_property(key, type_name, value)
            .map_err(|mut error| {
                if error.consumed == 0 {
                    error.consumed = offset;
                }
                error
            })
    }

    /// DOM child fallback (feature `dom` only).
    #[cfg(feature = "dom")]
    fn on_child(&mut self, child: Node<'a>) -> CtxResult<bool> {
        let _ = child;
        Ok(false)
    }

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

    fn take_child_after_header_at(
        &mut self,
        offset: usize,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<bool> {
        self.take_child_after_header(parser, opts, type_name, name)
            .map_err(|mut error| {
                if error.consumed == 0 {
                    error.consumed = offset;
                }
                error
            })
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

    fn on_header_at(
        &mut self,
        offset: usize,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<()> {
        self.builder.on_header_at(offset, type_name, name)
    }

    fn on_argument(&mut self, type_name: Option<KdlStr<'a>>, value: Value<'a>) -> CtxResult<bool> {
        self.builder.on_argument(type_name, value)
    }

    fn on_argument_at(
        &mut self,
        offset: usize,
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool> {
        self.builder.on_argument_at(offset, type_name, value)
    }

    fn on_property(
        &mut self,
        key: KdlStr<'a>,
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool> {
        self.builder.on_property(key, type_name, value)
    }

    fn on_property_at(
        &mut self,
        offset: usize,
        key: KdlStr<'a>,
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool> {
        self.builder.on_property_at(offset, key, type_name, value)
    }

    #[cfg(feature = "dom")]
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

    fn take_child_after_header_at(
        &mut self,
        offset: usize,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<bool> {
        self.builder
            .take_child_after_header_at(offset, parser, opts, type_name, name)
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
    decode_node_body_after_header_at(parser, opts, 0, type_name, name)
}

/// Source-aware nested variant of [`decode_node_body_after_header`].
pub fn decode_node_body_after_header_at<'a, T: DecodeFromVisit<'a>>(
    parser: &mut crate::Parser<'a>,
    opts: Opts,
    node_offset: usize,
    type_name: Option<KdlStr<'a>>,
    name: KdlStr<'a>,
) -> CtxResult<T> {
    let mut fill = VisitFill::new(T::start_visit());
    fill.builder.on_header_at(node_offset, type_name, name)?;
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

/// Peel helpers for `unwrap(argument|property)` without a [`Node`] tree (P-G12).
///
/// Glaze: nested scalar `from::op` without building a sub-object DOM.
struct PeelArgumentBuilder<T> {
    value: Option<T>,
    extra: bool,
    _ty: core::marker::PhantomData<T>,
}

impl<'a, T: DecodeScalar<'a>> NodeVisitor<'a> for PeelArgumentBuilder<T> {
    fn on_argument(&mut self, _type_name: Option<KdlStr<'a>>, value: Value<'a>) -> CtxResult<bool> {
        if self.value.is_some() {
            self.extra = true;
            return Ok(true);
        }
        self.value = Some(T::decode_scalar(&value)?);
        Ok(true)
    }

    fn on_argument_at(
        &mut self,
        offset: usize,
        _type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool> {
        if self.value.is_some() {
            self.extra = true;
            return Ok(true);
        }
        self.value = Some(T::decode_scalar_at(&value, offset)?);
        Ok(true)
    }

    fn on_property(
        &mut self,
        _key: KdlStr<'a>,
        _type_name: Option<KdlStr<'a>>,
        _value: Value<'a>,
    ) -> CtxResult<bool> {
        Ok(false)
    }
}

/// Peel first argument after header (live parser path).
pub fn peel_argument_after_header<'a, T: DecodeScalar<'a>>(
    parser: &mut crate::Parser<'a>,
    opts: Opts,
    type_name: Option<KdlStr<'a>>,
    name: KdlStr<'a>,
) -> CtxResult<T> {
    let mut b = PeelArgumentBuilder {
        value: None,
        extra: false,
        _ty: core::marker::PhantomData,
    };
    let _ = (type_name, name);
    // Header already consumed by caller; only body remains.
    parser.finish_nested_child(opts, &mut b)?;
    if b.extra {
        return Err(ErrorCtx::new(ErrorCode::Syntax, 0).with_message("too many arguments"));
    }
    b.value
        .ok_or_else(|| ErrorCtx::new(ErrorCode::MissingArgument, 0).with_expected("argument"))
}

/// Optional peel: missing argument → `None`.
pub fn peel_opt_argument_after_header<'a, T: DecodeScalar<'a>>(
    parser: &mut crate::Parser<'a>,
    opts: Opts,
    type_name: Option<KdlStr<'a>>,
    name: KdlStr<'a>,
) -> CtxResult<Option<T>> {
    let mut b = PeelArgumentBuilder {
        value: None,
        extra: false,
        _ty: core::marker::PhantomData,
    };
    let _ = (type_name, name);
    parser.finish_nested_child(opts, &mut b)?;
    if b.extra {
        return Err(ErrorCtx::new(ErrorCode::Syntax, 0).with_message("too many arguments"));
    }
    Ok(b.value)
}

/// Property peel visitor — key is borrowed (derive emits `&'static str`).
///
/// Glaze nested scalar fill does not allocate key storage on the hot path.
struct PeelPropertyBuilder<'k, T> {
    key: &'k str,
    value: Option<T>,
    _ty: core::marker::PhantomData<T>,
}

impl<'a, 'k, T: DecodeScalar<'a>> NodeVisitor<'a> for PeelPropertyBuilder<'k, T> {
    fn on_argument(
        &mut self,
        _type_name: Option<KdlStr<'a>>,
        _value: Value<'a>,
    ) -> CtxResult<bool> {
        Ok(true)
    }

    fn on_property(
        &mut self,
        key: KdlStr<'a>,
        _type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool> {
        if key.as_str() == self.key {
            self.value = Some(T::decode_scalar(&value)?);
            return Ok(true);
        }
        Ok(false)
    }

    fn on_property_at(
        &mut self,
        offset: usize,
        key: KdlStr<'a>,
        _type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool> {
        if key.as_str() == self.key {
            self.value = Some(T::decode_scalar_at(&value, offset)?);
            return Ok(true);
        }
        Ok(false)
    }
}

/// Peel named property from current node body (no [`Node`] tree).
pub fn peel_property_after_header<'a, T: DecodeScalar<'a>>(
    parser: &mut crate::Parser<'a>,
    opts: Opts,
    type_name: Option<KdlStr<'a>>,
    name: KdlStr<'a>,
    prop_key: &str,
) -> CtxResult<T> {
    let _ = (type_name, name);
    let mut b = PeelPropertyBuilder {
        key: prop_key,
        value: None,
        _ty: core::marker::PhantomData,
    };
    parser.finish_nested_child(opts, &mut b)?;
    b.value.ok_or_else(|| {
        ErrorCtx::new(ErrorCode::MissingProperty, 0)
            .with_message(format!("missing property `{prop_key}`"))
    })
}

/// Optional property peel.
pub fn peel_opt_property_after_header<'a, T: DecodeScalar<'a>>(
    parser: &mut crate::Parser<'a>,
    opts: Opts,
    type_name: Option<KdlStr<'a>>,
    name: KdlStr<'a>,
    prop_key: &str,
) -> CtxResult<Option<T>> {
    let _ = (type_name, name);
    let mut b = PeelPropertyBuilder {
        key: prop_key,
        value: None,
        _ty: core::marker::PhantomData,
    };
    parser.finish_nested_child(opts, &mut b)?;
    Ok(b.value)
}

/// Skip the remainder of the current node body (unknown top-level name).
pub fn skip_node_after_header<'a>(
    parser: &mut crate::Parser<'a>,
    opts: Opts,
    type_name: Option<KdlStr<'a>>,
    name: KdlStr<'a>,
) -> CtxResult<()> {
    struct Skip;
    impl<'a> NodeVisitor<'a> for Skip {}
    let mut skip = Skip;
    let _ = (type_name, name);
    parser.finish_nested_child(opts, &mut skip)
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
