//! Nested / top-level fill — Glaze nested `from::op` (visit-fill).
//!
//! Without feature `dom`, only [`DecodeFromVisit`] children are supported
//! (no intermediate [`Node`]). With `dom`, Decode-only types fall back to a
//! temporary tree (Glaze does not do this on the primary path; keep it opt-in).

use core::marker::PhantomData;

use crate::error::CtxResult;
use crate::opts::Opts;
use crate::value::KdlStr;

use super::visit_fill::{
    DecodeFromVisit, decode_node_body_after_header, decode_node_body_after_header_at,
};

#[cfg(feature = "dom")]
use super::Decode;
#[cfg(feature = "dom")]
use crate::parse::visitor::{DomNodeBuilder, NodeVisitor};
#[cfg(feature = "dom")]
use crate::value::Node;

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
pub trait NestedFill<'a> {
    type Output;

    fn fill_nested(
        self,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<Self::Output>;

    fn fill_nested_at(
        self,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        node_offset: usize,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<Self::Output>
    where
        Self: Sized,
    {
        let _ = node_offset;
        self.fill_nested(parser, opts, type_name, name)
    }
}

/// Visit-fill path (Glaze nested `from::op`) — always available.
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

    #[inline(always)]
    fn fill_nested_at(
        self,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        node_offset: usize,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<T> {
        decode_node_body_after_header_at::<T>(parser, opts, node_offset, type_name, name)
    }
}

/// DOM fallback for Decode-only children (feature `dom` only).
#[cfg(feature = "dom")]
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

    #[inline(always)]
    fn fill_nested_at(
        self,
        parser: &mut crate::Parser<'a>,
        opts: Opts,
        node_offset: usize,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<T> {
        let mut child_dom = DomNodeBuilder::default();
        child_dom.on_header_at(node_offset, type_name, name)?;
        parser.finish_nested_child(opts, &mut child_dom)?;
        let child: Node<'a> = child_dom.finish()?;
        T::decode_node(&child)
    }
}

pub type NestedVisitTag = NestedProbe<()>;
pub use NestedFill as NestedViaVisit;

/// Top-level node → `T` (document element loop).
pub trait TopLevelFill<'a> {
    type Output;

    fn fill_top(self, parser: &mut crate::Parser<'a>, opts: Opts) -> CtxResult<Self::Output>;
}

impl<'a, T: DecodeFromVisit<'a>> TopLevelFill<'a> for &&NestedProbe<T> {
    type Output = T;

    #[inline(always)]
    fn fill_top(self, parser: &mut crate::Parser<'a>, opts: Opts) -> CtxResult<T> {
        super::visit_fill::decode_node_visit::<T>(parser, opts)
    }
}

#[cfg(feature = "dom")]
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
