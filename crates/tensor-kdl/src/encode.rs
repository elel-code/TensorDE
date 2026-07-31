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

// `&T` covered by the blanket below (`str: EncodeScalar` ⇒ `&str: EncodeScalar`).

impl EncodeScalar for bool {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::Bool(*self))
    }
}

macro_rules! impl_int_encode_scalar {
    ($($t:ty),* $(,)?) => {$(
        impl EncodeScalar for $t {
            fn encode_scalar(&self) -> CtxResult<Value<'static>> {
                Ok(Value::Int(i128::from(*self)))
            }
        }
    )*};
}

impl_int_encode_scalar!(i8, i16, i32, i64, i128, u8, u16, u32, u64);

impl EncodeScalar for usize {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::Int(*self as i128))
    }
}

impl EncodeScalar for isize {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::Int(*self as i128))
    }
}

impl EncodeScalar for u128 {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        i128::try_from(*self).map(Value::Int).map_err(|_| {
            crate::ErrorCtx::new(crate::ErrorCode::TypeMismatch, 0)
                .with_message("u128 value exceeds i128 range for KDL encode")
        })
    }
}

impl EncodeScalar for f64 {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::float(*self))
    }
}

impl EncodeScalar for f32 {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::float(f64::from(*self)))
    }
}

impl<T: EncodeScalar + ?Sized> EncodeScalar for &T {
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        (*self).encode_scalar()
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

impl Encode for crate::Flag {
    fn encode_node(&self) -> CtxResult<Node<'static>> {
        Ok(flag_node("flag"))
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
