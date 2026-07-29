//! Nested child fill — prefer visit-fill, fall back to DOM (autoref specialization).
//!
//! Glaze monomorphizes nested `from::op` when the member type is known
//! (`json/read.hpp`). In Rust we cannot require every child field to implement
//! [`super::DecodeFromVisit`] (e.g. `unwrap(property)` peels a scalar; some
//! types only implement [`crate::Decode`]).
//!
//! **Autoref specialization** (dtolnay): put `T` in the receiver type so method
//! resolution autoderefs from `&&Probe<T>` → `&Probe<T>` when the specialized
//! bound fails. A bare tag with `T` only as a trait parameter does **not** fall
//! through (rustc commits to the exact `&&Tag` candidate and errors).
//!
//! ```ignore
//! use tensor_kdl::{NestedFill, NestedProbe};
//! (&&NestedProbe::<Child>::new()).fill_nested(parser, opts, type_name, name)
//! ```

use core::marker::PhantomData;

use crate::error::CtxResult;
use crate::opts::Opts;
use crate::parse::visitor::{DomNodeBuilder, NodeVisitor};
use crate::value::{KdlStr, Node};

use super::Decode;
use super::visit_fill::{DecodeFromVisit, decode_node_body_after_header};

/// Probe carrying the child type for autoref specialization.
pub struct NestedProbe<T> {
    _ty: PhantomData<fn() -> T>,
}

impl<T> NestedProbe<T> {
    #[inline(always)]
    pub const fn new() -> Self {
        Self { _ty: PhantomData }
    }
}

impl<T> Default for NestedProbe<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Nested child body → `T` after the parent saw `(type_name, name)`.
///
/// Call `(&&NestedProbe::<T>::new()).fill_nested(...)`.
pub trait NestedFill<'a> {
    type Output;

    fn fill_nested(
        self,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<Self::Output>;
}

/// Specialized: visit-fill path (P-G3d / Glaze nested `from::op`).
impl<'a, T: DecodeFromVisit<'a>> NestedFill<'a> for &&NestedProbe<T> {
    type Output = T;

    #[inline(always)]
    fn fill_nested(
        self,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<T> {
        decode_node_body_after_header::<T>(parser, opts, type_name, name)
    }
}

/// Fallback: any [`Decode`] — finish body as DOM then `decode_node`.
impl<'a, T: Decode<'a>> NestedFill<'a> for &NestedProbe<T> {
    type Output = T;

    #[inline(always)]
    fn fill_nested(
        self,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<T> {
        let mut child_dom = DomNodeBuilder::default();
        child_dom.on_header(type_name, name)?;
        parser.finish_nested_child(opts, &mut child_dom)?;
        let child: Node<'a> = child_dom.finish()?;
        T::decode_node(&child)
    }
}

/// Compat: old tag name used as type alias documentation only.
pub type NestedVisitTag = NestedProbe<()>;

/// Compat aliases.
pub use NestedFill as NestedViaVisit;
pub use NestedFill as NestedViaDom;

/// Top-level node → `T` (document element loop).
///
/// Same autoref pattern as [`NestedFill`], but starts at a full node (header
/// not yet consumed). Call `(&&NestedProbe::<T>::new()).fill_top(...)`.
///
/// Cite: Glaze array element `from::op` without retaining a generic value
/// (`json/read.hpp` + `core/read.hpp`).
pub trait TopLevelFill<'a> {
    type Output;

    fn fill_top(self, parser: &mut crate::Parser<'a>, opts: Opts) -> CtxResult<Self::Output>;
}

/// Specialized: visit-fill (no intermediate [`Node`]).
impl<'a, T: DecodeFromVisit<'a>> TopLevelFill<'a> for &&NestedProbe<T> {
    type Output = T;

    #[inline(always)]
    fn fill_top(self, parser: &mut crate::Parser<'a>, opts: Opts) -> CtxResult<T> {
        super::visit_fill::decode_node_visit::<T>(parser, opts)
    }
}

/// Fallback: DOM node then [`Decode::decode_node`].
impl<'a, T: Decode<'a>> TopLevelFill<'a> for &NestedProbe<T> {
    type Output = T;

    #[inline(always)]
    fn fill_top(self, parser: &mut crate::Parser<'a>, opts: Opts) -> CtxResult<T> {
        let mut child_dom = DomNodeBuilder::default();
        parser.visit_node(opts, &mut child_dom)?;
        let node: Node<'a> = child_dom.finish()?;
        T::decode_node(&node)
    }
}
