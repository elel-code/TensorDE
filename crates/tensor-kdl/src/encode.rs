//! Typed encode — reverse of [`crate::Decode`] (Glaze write symmetry).
//!
//! Builds a DOM [`Node`]/ [`Document`] then pretty-prints via
//! [`crate::format_document`]. Not a monomorphized write path (Glaze has both
//! DOM and direct write); sufficient for config emit and suite roundtrip helpers.

use crate::error::CtxResult;
use crate::value::{Document, Entry, KdlStr, Node, Value};
use crate::write::format_document;

/// Encode `Self` as a single KDL node.
pub trait Encode {
    fn encode_node(&self) -> CtxResult<Node<'static>>;
}

/// Encode `Self` as a full document (top-level nodes).
pub trait EncodeDocument {
    fn encode_document(&self) -> CtxResult<Document<'static>>;
}

/// Encode a scalar as a KDL [`Value`].
pub trait EncodeScalar {
    fn encode_scalar(&self) -> CtxResult<Value<'static>>;
}

impl EncodeScalar for String {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::String(KdlStr::owned(self.clone())))
    }
}

impl EncodeScalar for str {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::String(KdlStr::owned(self.to_owned())))
    }
}

impl EncodeScalar for &str {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::String(KdlStr::owned((*self).to_owned())))
    }
}

impl EncodeScalar for bool {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::Bool(*self))
    }
}

impl EncodeScalar for i64 {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::Int(i128::from(*self)))
    }
}

impl EncodeScalar for i32 {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::Int(i128::from(*self)))
    }
}

impl EncodeScalar for u32 {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::Int(i128::from(*self)))
    }
}

impl EncodeScalar for f64 {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::float(*self))
    }
}

impl<T: EncodeScalar> EncodeScalar for Option<T> {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        match self {
            Some(v) => v.encode_scalar(),
            None => Ok(Value::Null),
        }
    }
}

impl<T: Encode> EncodeDocument for Vec<T> {
    fn encode_document(&self) -> CtxResult<Document<'static>> {
        let mut nodes = Vec::with_capacity(self.len());
        for item in self {
            nodes.push(item.encode_node()?);
        }
        Ok(Document { nodes })
    }
}

impl EncodeDocument for Document<'static> {
    fn encode_document(&self) -> CtxResult<Document<'static>> {
        Ok(self.clone())
    }
}

/// Helper: argument entry.
pub fn arg_entry(value: Value<'static>) -> Entry<'static> {
    Entry::Argument {
        type_name: None,
        value,
    }
}

/// Helper: property entry.
pub fn prop_entry(key: impl Into<String>, value: Value<'static>) -> Entry<'static> {
    Entry::Property {
        key: KdlStr::owned(key.into()),
        type_name: None,
        value,
    }
}

/// Helper: bare named node (flag / empty).
pub fn flag_node(name: impl Into<String>) -> Node<'static> {
    Node {
        type_name: None,
        name: KdlStr::owned(name.into()),
        entries: Vec::new(),
        children: Vec::new(),
    }
}

/// Helper: node with one argument.
pub fn arg_node(name: impl Into<String>, value: Value<'static>) -> Node<'static> {
    Node {
        type_name: None,
        name: KdlStr::owned(name.into()),
        entries: vec![arg_entry(value)],
        children: Vec::new(),
    }
}

/// Format `T` as KDL text (allocate document then pretty-print).
pub fn to_string<T: EncodeDocument>(value: &T) -> CtxResult<String> {
    let doc = value.encode_document()?;
    Ok(format_document(&doc))
}

/// Format a single node type as a one-node document.
pub fn to_string_node<T: Encode>(value: &T) -> CtxResult<String> {
    let node = value.encode_node()?;
    Ok(format_document(&Document { nodes: vec![node] }))
}
