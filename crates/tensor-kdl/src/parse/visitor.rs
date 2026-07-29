//! Node-level visitor — Glaze field-fill shape for one structural unit.
//!
//! Cite:
//! - `references/glaze/include/glaze/json/read.hpp` `decode_index` →
//!   `from<JSON, V>::op(...)(get_member(value, ...), ctx, it, end)` writes **into
//!   the target member** without building a JSON object DOM.
//! - `references/glaze/include/glaze/json/skip.hpp` `skip_value` for unknown keys.
//!
//! KDL maps JSON object keys to **properties** and nested objects to **children**.
//! Positional **arguments** have no JSON-object analogue; they are delivered in order.
//!
//! P-G3b: callers implement [`NodeVisitor`] to sink events; DOM build is one visitor.
//! Full derive monomorphization (Glaze `decode_linear` tables) is the next increment.

use crate::error::CtxResult;
use crate::value::{KdlStr, Node, Value};

/// Callbacks while parsing a single KDL node (Glaze `from::op` per member).
///
/// Default methods no-op so visitors only override what they need.
pub trait NodeVisitor<'a> {
    /// After type annotation (if any) and node name are known.
    fn on_header(&mut self, _type_name: Option<KdlStr<'a>>, _name: KdlStr<'a>) -> CtxResult<()> {
        Ok(())
    }

    /// Positional argument (ordered). Return `true` if the value was consumed
    /// by the visitor (ownership taken); `false` if the parser should drop it.
    fn on_argument(
        &mut self,
        _type_name: Option<KdlStr<'a>>,
        _value: Value<'a>,
    ) -> CtxResult<bool> {
        Ok(true)
    }

    /// Named property. Unknown-key policy is applied by the parser using
    /// [`crate::Opts::error_on_unknown_keys`] when the visitor returns `false`
    /// **and** opts request errors — visitors that implement schemas should
    /// return whether the key was recognized (Glaze: match key → `decode_index`,
    /// else `skip_value` or `unknown_key` error).
    ///
    /// Return `Ok(true)` if handled, `Ok(false)` if unknown to this visitor.
    fn on_property(
        &mut self,
        _key: KdlStr<'a>,
        _type_name: Option<KdlStr<'a>>,
        _value: Value<'a>,
    ) -> CtxResult<bool> {
        Ok(true)
    }

    /// Child node fully parsed as DOM (fallback when nested visit is not used).
    fn on_child(&mut self, _child: Node<'a>) -> CtxResult<bool> {
        Ok(true)
    }

    /// P-G3d: take over the next child after its header was parsed.
    ///
    /// The parser has already read `(type_name, name)`. If this returns
    /// `Ok(true)`, the implementation **must** have consumed the rest of the
    /// child node (typically by calling `parser.finish_nested_child` on a
    /// nested visitor). Default `Ok(false)` → parser builds a DOM [`Node`] and
    /// calls [`Self::on_child`].
    ///
    /// Glaze: nested `from::op` on a sub-object (`json/read.hpp`).
    fn take_child_after_header(
        &mut self,
        _parser: &mut crate::Parser<'a>,
        _opts: crate::opts::Opts,
        _type_name: Option<KdlStr<'a>>,
        _name: KdlStr<'a>,
    ) -> CtxResult<bool> {
        Ok(false)
    }

    /// Called when a real (non-slashdashed) children block begins, before children.
    fn on_children_begin(&mut self) -> CtxResult<()> {
        Ok(())
    }

    /// Called after the children block closes.
    fn on_children_end(&mut self) -> CtxResult<()> {
        Ok(())
    }
}

/// Builds an owned [`Node`] from visitor events (DOM path).
#[derive(Debug, Default)]
pub struct DomNodeBuilder<'a> {
    type_name: Option<KdlStr<'a>>,
    name: Option<KdlStr<'a>>,
    entries: Vec<crate::value::Entry<'a>>,
    children: Vec<Node<'a>>,
}

impl<'a> DomNodeBuilder<'a> {
    pub fn finish(self) -> CtxResult<Node<'a>> {
        let name = self.name.ok_or_else(|| {
            crate::error::ErrorCtx::new(crate::error::ErrorCode::ExpectedNodeName, 0)
        })?;
        Ok(Node {
            type_name: self.type_name,
            name,
            entries: self.entries,
            children: self.children,
        })
    }
}

impl<'a> NodeVisitor<'a> for DomNodeBuilder<'a> {
    fn on_header(&mut self, type_name: Option<KdlStr<'a>>, name: KdlStr<'a>) -> CtxResult<()> {
        self.type_name = type_name;
        self.name = Some(name);
        Ok(())
    }

    fn on_argument(&mut self, type_name: Option<KdlStr<'a>>, value: Value<'a>) -> CtxResult<bool> {
        self.entries
            .push(crate::value::Entry::Argument { type_name, value });
        Ok(true)
    }

    fn on_property(
        &mut self,
        key: KdlStr<'a>,
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    ) -> CtxResult<bool> {
        self.entries.push(crate::value::Entry::Property {
            key,
            type_name,
            value,
        });
        Ok(true)
    }

    fn on_child(&mut self, child: Node<'a>) -> CtxResult<bool> {
        self.children.push(child);
        Ok(true)
    }
}

/// Counts events without retaining values (bench / structural validate).
#[derive(Debug, Default, Clone, Copy)]
pub struct CountingVisitor {
    pub arguments: usize,
    pub properties: usize,
    pub children: usize,
}

impl<'a> NodeVisitor<'a> for CountingVisitor {
    fn on_argument(
        &mut self,
        _type_name: Option<KdlStr<'a>>,
        _value: Value<'a>,
    ) -> CtxResult<bool> {
        self.arguments += 1;
        Ok(true)
    }

    fn on_property(
        &mut self,
        _key: KdlStr<'a>,
        _type_name: Option<KdlStr<'a>>,
        _value: Value<'a>,
    ) -> CtxResult<bool> {
        self.properties += 1;
        Ok(true)
    }

    fn on_child(&mut self, _child: Node<'a>) -> CtxResult<bool> {
        self.children += 1;
        Ok(true)
    }
}
