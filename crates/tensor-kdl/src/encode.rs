//! Typed encode — reverse of [`crate::Decode`] (Glaze write symmetry).
//!
//! Builds a DOM [`Node`]/ [`Document`] then pretty-prints via
//! [`crate::format_document`] / [`crate::format_document_into`].
//!
//! Glaze write API shape (`references/glaze/docs/writing.md`):
//! - [`write_into`] — resizable buffer, `ErrorCtx.consumed` = bytes written
//! - [`write_into_slice`] — fixed buffer + [`ErrorCode::BufferOverflow`]
//! - [`write`] — allocate `String` (`expected<T, error_ctx>` role)
//!
//! Field-by-field monomorphized streaming (no intermediate `Node`) remains a
//! later optimization; the public write entrypoints already match Glaze's
//! buffer/`error_ctx` contract.

use crate::error::{CtxResult, ErrorCode, ErrorCtx};
use crate::value::{Document, Entry, KdlStr, Node, Value};
use crate::write::{format_document, format_document_into, format_node_into};

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

/// Reverse of [`crate::DecodePartial`] for `#[kdl(flatten)]` on encode.
///
/// Policy (`docs/kdl/design.md` §11): emit extra properties then extra children
/// after the host struct's known fields. Property key order is normalized by
/// the canonical formatter (suite Translation Rules).
pub trait EncodePartial {
    /// Extra properties / arguments to merge into the host node.
    fn encode_entries(&self) -> CtxResult<Vec<Entry<'static>>> {
        Ok(Vec::new())
    }

    /// Extra child nodes to append after the host's known children.
    fn encode_children(&self) -> CtxResult<Vec<Node<'static>>> {
        Ok(Vec::new())
    }
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

/// Glaze `write_json(T)` — allocate and return KDL text.
///
/// On success `Ok(String)`; on failure `Err(ErrorCtx)` (encode path).
pub fn write<T: EncodeDocument>(value: &T) -> std::result::Result<String, ErrorCtx> {
    let mut buffer = String::new();
    let ec = write_into(value, &mut buffer);
    if ec.is_err() { Err(ec) } else { Ok(buffer) }
}

/// Glaze `write_json(T, buffer)` for resizable buffers.
///
/// Clears `buffer`, formats into it, returns [`ErrorCtx`] with
/// `consumed == buffer.len()` on success (Glaze: `buffer.size() == ec.count`).
pub fn write_into<T: EncodeDocument>(value: &T, buffer: &mut String) -> ErrorCtx {
    buffer.clear();
    match value.encode_document() {
        Ok(doc) => {
            format_document_into(&doc, buffer);
            ErrorCtx::ok(buffer.len())
        }
        Err(error) => error,
    }
}

/// Write a single node type into a resizable buffer (one-node document).
pub fn write_node_into<T: Encode>(value: &T, buffer: &mut String) -> ErrorCtx {
    buffer.clear();
    match value.encode_node() {
        Ok(node) => {
            format_node_into(&node, buffer);
            ErrorCtx::ok(buffer.len())
        }
        Err(error) => error,
    }
}

/// Glaze fixed-buffer write (`std::span` / array role).
///
/// On success writes `consumed` bytes starting at `buffer[0]`. On overflow
/// copies a prefix (lower bound on required size per Glaze docs) and returns
/// [`ErrorCode::BufferOverflow`] with `consumed` = bytes stored.
pub fn write_into_slice<T: EncodeDocument>(value: &T, buffer: &mut [u8]) -> ErrorCtx {
    match value.encode_document() {
        Ok(doc) => {
            let text = format_document(&doc);
            write_bytes_into_slice(text.as_bytes(), buffer)
        }
        Err(error) => error,
    }
}

/// Fixed-buffer write for a single node document.
pub fn write_node_into_slice<T: Encode>(value: &T, buffer: &mut [u8]) -> ErrorCtx {
    match value.encode_node() {
        Ok(node) => {
            let mut text = String::new();
            format_node_into(&node, &mut text);
            write_bytes_into_slice(text.as_bytes(), buffer)
        }
        Err(error) => error,
    }
}

fn write_bytes_into_slice(bytes: &[u8], buffer: &mut [u8]) -> ErrorCtx {
    if bytes.len() > buffer.len() {
        let n = buffer.len();
        buffer.copy_from_slice(&bytes[..n]);
        return ErrorCtx::new(ErrorCode::BufferOverflow, n)
            .with_message("fixed write buffer too small");
    }
    buffer[..bytes.len()].copy_from_slice(bytes);
    ErrorCtx::ok(bytes.len())
}
