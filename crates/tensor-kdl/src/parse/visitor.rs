//! Node-level visitor — Glaze field-fill shape for one structural unit.
//!
//! Typed decode uses [`NodeVisitor::take_child_after_header`] (nested visit).
//! Building a [`Node`] tree is **feature `dom` only**.

use crate::error::CtxResult;
#[cfg(feature = "dom")]
use crate::value::Node;
use crate::value::{KdlStr, Value};

/// Callbacks while parsing a single KDL node (Glaze `from::op` per member).
pub trait NodeVisitor<'a> {
    fn on_header(&mut self, _type_name: Option<KdlStr<'a>>, _name: KdlStr<'a>) -> CtxResult<()> {
        Ok(())
    }

    fn on_argument(
        &mut self,
        _type_name: Option<KdlStr<'a>>,
        _value: Value<'a>,
    ) -> CtxResult<bool> {
        Ok(true)
    }

    fn on_property(
        &mut self,
        _key: KdlStr<'a>,
        _type_name: Option<KdlStr<'a>>,
        _value: Value<'a>,
    ) -> CtxResult<bool> {
        Ok(true)
    }

    /// Finished child as a [`Node`] (feature `dom` only).
    #[cfg(feature = "dom")]
    fn on_child(&mut self, _child: Node<'a>) -> CtxResult<bool> {
        Ok(true)
    }

    /// Take over the next child after its header (typed nested visit).
    ///
    /// Default `Ok(false)`: with `dom`, parser builds a [`Node`]; without `dom`,
    /// parser errors (no tree fallback on the Glaze primary path).
    fn take_child_after_header(
        &mut self,
        _parser: &mut crate::Parser<'a>,
        _opts: crate::opts::Opts,
        _type_name: Option<KdlStr<'a>>,
        _name: KdlStr<'a>,
    ) -> CtxResult<bool> {
        Ok(false)
    }

    fn on_children_begin(&mut self) -> CtxResult<()> {
        Ok(())
    }

    fn on_children_end(&mut self) -> CtxResult<()> {
        Ok(())
    }
}

/// Builds an owned [`Node`] from visitor events (**feature `dom` only**).
#[cfg(feature = "dom")]
#[derive(Debug, Default)]
pub struct DomNodeBuilder<'a> {
    type_name: Option<KdlStr<'a>>,
    name: Option<KdlStr<'a>>,
    entries: Vec<crate::value::Entry<'a>>,
    children: Vec<Node<'a>>,
}

#[cfg(feature = "dom")]
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

#[cfg(feature = "dom")]
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
    pub headers: u32,
    pub arguments: u32,
    pub properties: u32,
    pub children: u32,
}

impl<'a> NodeVisitor<'a> for CountingVisitor {
    fn on_header(&mut self, _type_name: Option<KdlStr<'a>>, _name: KdlStr<'a>) -> CtxResult<()> {
        self.headers += 1;
        Ok(())
    }

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

    #[cfg(feature = "dom")]
    fn on_child(&mut self, _child: Node<'a>) -> CtxResult<bool> {
        self.children += 1;
        Ok(true)
    }

    fn take_child_after_header(
        &mut self,
        parser: &mut crate::Parser<'a>,
        opts: crate::opts::Opts,
        type_name: Option<KdlStr<'a>>,
        name: KdlStr<'a>,
    ) -> CtxResult<bool> {
        let mut nested = CountingVisitor::default();
        nested.on_header(type_name, name)?;
        parser.finish_nested_child(opts, &mut nested)?;
        self.children += 1 + nested.children;
        self.arguments += nested.arguments;
        self.properties += nested.properties;
        Ok(true)
    }
}
