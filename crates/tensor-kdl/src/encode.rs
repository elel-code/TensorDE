//! Typed encode — reverse of [`crate::Decode`].
//!
//! **Primary path (Glaze-aligned):** monomorphized
//! [`Encode::write_node`] / [`EncodeDocument::write_document`] dump directly into
//! a [`WriteSink`] (`references/glaze/core/write.hpp` `to::op(value, ctx, b, ix)`;
//! dump primitives in `util/dump.hpp`). No intermediate [`Node`] / [`Document`]
//! on the success path.
//!
//! **DOM path:** [`Encode::encode_node`] remains for tooling / suite roundtrip
//! helpers; default implementations build a node then re-format (not hot).
//!
//! Public entrypoints mirror Glaze `writing.md`:
//! - [`write_into`] — resizable buffer; `ErrorCtx.consumed` = bytes written
//! - [`write_into_slice`] — fixed buffer + [`ErrorCode::BufferOverflow`]
//! - [`write`] — allocate `String`

mod sink;

pub use sink::{
    WriteSink, write_arg_node_line, write_argument_prefix, write_bool, write_children_close,
    write_children_open, write_f64, write_flag_line, write_i128, write_ident_or_string,
    write_indent, write_node_end_leaf, write_node_header, write_null, write_prop_node_line,
    write_property_key, write_quoted,
};

use crate::error::{CtxResult, ErrorCode, ErrorCtx};
use crate::value::{Document, Entry, KdlStr, Node, Value};
use crate::write::{format_document, format_document_into, format_node_into};

/// Encode `Self` as a single KDL node.
///
/// **Hot path:** override [`Self::write_node`]. Default DOM path is for tools.
pub trait Encode {
    /// Glaze `to::op` for a node — dump into `out` at `indent` (4-space levels).
    fn write_node(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
        let node = self.encode_node()?;
        // Fallback: format DOM node (not monomorphized; derive overrides this).
        let mut tmp = String::new();
        format_node_into(&node, &mut tmp);
        // format_node_into always ends with `\n` and uses indent 0; re-indent if needed.
        if indent == 0 {
            out.push_str(tmp.trim_end_matches('\n'))?;
            out.push_byte(b'\n')
        } else {
            for (i, line) in tmp.lines().enumerate() {
                if i > 0 {
                    out.push_byte(b'\n')?;
                }
                write_indent(out, indent)?;
                out.push_str(line)?;
            }
            out.push_byte(b'\n')
        }
    }

    fn encode_node(&self) -> CtxResult<Node<'static>>;
}

/// Encode `Self` as a full document (top-level nodes).
pub trait EncodeDocument {
    /// Glaze document write — sequence of top-level nodes into `out`.
    fn write_document(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        let doc = self.encode_document()?;
        let mut tmp = String::new();
        format_document_into(&doc, &mut tmp);
        out.push_str(&tmp)
    }

    fn encode_document(&self) -> CtxResult<Document<'static>>;
}

/// Encode a scalar as a KDL value **lexeme** (Glaze scalar `to::op`).
pub trait EncodeScalar {
    /// Write scalar text (no surrounding spaces).
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        let value = self.encode_scalar()?;
        write_value_lexeme(out, &value)
    }

    fn encode_scalar(&self) -> CtxResult<Value<'static>>;
}

/// Reverse of [`crate::DecodePartial`] for `#[kdl(flatten)]` on encode.
///
/// Policy: stream extra properties then extra children after the host's known
/// fields. Property key order for multi-key maps is sorted (suite Translation
/// Rules) when emitting via [`Self::write_partial`].
pub trait EncodePartial {
    /// Direct write of extra props/children (preferred).
    fn write_partial(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
        for entry in self.encode_entries()? {
            match entry {
                Entry::Argument { type_name, value } => {
                    write_argument_prefix(out)?;
                    if let Some(ty) = type_name {
                        out.push_byte(b'(')?;
                        write_ident_or_string(out, ty.as_str())?;
                        out.push_byte(b')')?;
                    }
                    write_value_lexeme(out, &value)?;
                }
                Entry::Property {
                    key,
                    type_name,
                    value,
                } => {
                    write_property_key(out, key.as_str())?;
                    if let Some(ty) = type_name {
                        out.push_byte(b'(')?;
                        write_ident_or_string(out, ty.as_str())?;
                        out.push_byte(b')')?;
                    }
                    write_value_lexeme(out, &value)?;
                }
            }
        }
        // Children need the host to open a block; EncodePartial children are
        // written by the host after `write_children_open`. Use encode_children
        // DOM fallback only when host collects children.
        let _ = indent;
        Ok(())
    }

    fn encode_entries(&self) -> CtxResult<Vec<Entry<'static>>> {
        Ok(Vec::new())
    }

    fn encode_children(&self) -> CtxResult<Vec<Node<'static>>> {
        Ok(Vec::new())
    }

    /// Stream child nodes at `indent` (inside an open children block).
    fn write_partial_children(
        &self,
        out: &mut WriteSink<'_>,
        indent: usize,
    ) -> Result<(), ErrorCtx> {
        for child in self.encode_children()? {
            let mut tmp = String::new();
            format_node_into(&child, &mut tmp);
            for line in tmp.lines() {
                write_indent(out, indent)?;
                out.push_str(line)?;
                out.push_byte(b'\n')?;
            }
        }
        Ok(())
    }
}

fn write_value_lexeme(out: &mut WriteSink<'_>, value: &Value<'_>) -> Result<(), ErrorCtx> {
    match value {
        Value::String(s) => write_ident_or_string(out, s.as_str()),
        Value::Bool(b) => write_bool(out, *b),
        Value::Null => write_null(out),
        Value::Int(n) => write_i128(out, *n),
        Value::Float { value, raw } => {
            if let Some(raw) = raw {
                out.push_str(raw.as_str())
            } else {
                write_f64(out, *value)
            }
        }
    }
}

impl EncodeScalar for String {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_ident_or_string(out, self)
    }

    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::String(KdlStr::owned(self.clone())))
    }
}

impl EncodeScalar for str {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_ident_or_string(out, self)
    }

    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::String(KdlStr::owned(self.to_owned())))
    }
}

impl EncodeScalar for bool {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_bool(out, *self)
    }

    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::Bool(*self))
    }
}

macro_rules! impl_int_encode_scalar {
    ($($t:ty),* $(,)?) => {$(
        impl EncodeScalar for $t {
            fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
                write_i128(out, i128::from(*self))
            }
            fn encode_scalar(&self) -> CtxResult<Value<'static>> {
                Ok(Value::Int(i128::from(*self)))
            }
        }
    )*};
}

impl_int_encode_scalar!(i8, i16, i32, i64, i128, u8, u16, u32, u64);

impl EncodeScalar for usize {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_i128(out, *self as i128)
    }
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::Int(*self as i128))
    }
}

impl EncodeScalar for isize {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_i128(out, *self as i128)
    }
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::Int(*self as i128))
    }
}

impl EncodeScalar for u128 {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        let n = i128::try_from(*self).map_err(|_| {
            ErrorCtx::new(ErrorCode::TypeMismatch, 0)
                .with_message("u128 value exceeds i128 range for KDL encode")
        })?;
        write_i128(out, n)
    }
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        i128::try_from(*self).map(Value::Int).map_err(|_| {
            ErrorCtx::new(ErrorCode::TypeMismatch, 0)
                .with_message("u128 value exceeds i128 range for KDL encode")
        })
    }
}

impl EncodeScalar for f64 {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_f64(out, *self)
    }
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::float(*self))
    }
}

impl EncodeScalar for f32 {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_f64(out, f64::from(*self))
    }
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        Ok(Value::float(f64::from(*self)))
    }
}

impl<T: EncodeScalar + ?Sized> EncodeScalar for &T {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        (*self).write_scalar(out)
    }
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        (*self).encode_scalar()
    }
}

impl<T: EncodeScalar> EncodeScalar for Option<T> {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        match self {
            Some(v) => v.write_scalar(out),
            None => write_null(out),
        }
    }
    fn encode_scalar(&self) -> CtxResult<Value<'static>> {
        match self {
            Some(v) => v.encode_scalar(),
            None => Ok(Value::Null),
        }
    }
}

impl Encode for crate::Flag {
    fn write_node(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
        write_flag_line(out, indent, "flag")
    }
    fn encode_node(&self) -> CtxResult<Node<'static>> {
        Ok(flag_node("flag"))
    }
}

impl<T: Encode> EncodeDocument for Vec<T> {
    fn write_document(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        for item in self {
            item.write_node(out, 0)?;
        }
        Ok(())
    }
    fn encode_document(&self) -> CtxResult<Document<'static>> {
        let mut nodes = Vec::with_capacity(self.len());
        for item in self {
            nodes.push(item.encode_node()?);
        }
        Ok(Document { nodes })
    }
}

impl EncodeDocument for Document<'static> {
    fn write_document(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        let mut tmp = String::new();
        format_document_into(self, &mut tmp);
        out.push_str(&tmp)
    }
    fn encode_document(&self) -> CtxResult<Document<'static>> {
        Ok(self.clone())
    }
}

impl Encode for Node<'static> {
    fn write_node(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
        write_dom_node(self, out, indent)
    }
    fn encode_node(&self) -> CtxResult<Node<'static>> {
        Ok(self.clone())
    }
}

fn write_dom_node(node: &Node<'_>, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
    write_node_header(
        out,
        indent,
        node.type_name.as_ref().map(|t| t.as_str()),
        node.name.as_str(),
    )?;
    for entry in &node.entries {
        if let Entry::Argument { type_name, value } = entry {
            write_argument_prefix(out)?;
            if let Some(ty) = type_name {
                out.push_byte(b'(')?;
                write_ident_or_string(out, ty.as_str())?;
                out.push_byte(b')')?;
            }
            write_value_lexeme(out, value)?;
        }
    }
    let mut props = std::collections::BTreeMap::<&str, (Option<&str>, &Value<'_>)>::new();
    for entry in &node.entries {
        if let Entry::Property {
            key,
            type_name,
            value,
        } = entry
        {
            props.insert(
                key.as_str(),
                (type_name.as_ref().map(|t| t.as_str()), value),
            );
        }
    }
    for (k, (ty, v)) in props {
        write_property_key(out, k)?;
        if let Some(ty) = ty {
            out.push_byte(b'(')?;
            write_ident_or_string(out, ty)?;
            out.push_byte(b')')?;
        }
        write_value_lexeme(out, v)?;
    }
    if node.children.is_empty() {
        return write_node_end_leaf(out);
    }
    write_children_open(out)?;
    for child in &node.children {
        write_dom_node(child, out, indent + 1)?;
    }
    write_children_close(out, indent)
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

/// Format `T` as KDL text via the **direct** write path.
pub fn to_string<T: EncodeDocument>(value: &T) -> CtxResult<String> {
    write(value)
}

/// Format a single node type as a one-node document (direct write).
pub fn to_string_node<T: Encode>(value: &T) -> CtxResult<String> {
    let mut buffer = String::new();
    let ec = write_node_into(value, &mut buffer);
    if ec.is_err() { Err(ec) } else { Ok(buffer) }
}

/// Glaze `write_json(T)` — allocate and return KDL text.
pub fn write<T: EncodeDocument>(value: &T) -> std::result::Result<String, ErrorCtx> {
    let mut buffer = String::new();
    let ec = write_into(value, &mut buffer);
    if ec.is_err() { Err(ec) } else { Ok(buffer) }
}

/// Glaze `write(T, buffer)` for resizable buffers — **direct** monomorphized dump.
///
/// Clears `buffer`, writes via [`EncodeDocument::write_document`], returns
/// [`ErrorCtx`] with `consumed == buffer.len()` on success.
pub fn write_into<T: EncodeDocument>(value: &T, buffer: &mut String) -> ErrorCtx {
    buffer.clear();
    let mut sink = WriteSink::string(buffer);
    match value.write_document(&mut sink) {
        Ok(()) => ErrorCtx::ok(sink.finish()),
        Err(error) => {
            // Preserve bytes already written as consumed (Glaze lower-bound count).
            let n = sink.written();
            ErrorCtx {
                consumed: n,
                code: error.code,
                message: error.message,
                expected: error.expected,
            }
        }
    }
}

/// Write a single node type into a resizable buffer (direct).
pub fn write_node_into<T: Encode>(value: &T, buffer: &mut String) -> ErrorCtx {
    buffer.clear();
    let mut sink = WriteSink::string(buffer);
    match value.write_node(&mut sink, 0) {
        Ok(()) => ErrorCtx::ok(sink.finish()),
        Err(error) => ErrorCtx {
            consumed: sink.written(),
            code: error.code,
            message: error.message,
            expected: error.expected,
        },
    }
}

/// Glaze fixed-buffer write (`std::span` / array role) — **direct** dump.
pub fn write_into_slice<T: EncodeDocument>(value: &T, buffer: &mut [u8]) -> ErrorCtx {
    let mut sink = WriteSink::slice(buffer);
    match value.write_document(&mut sink) {
        Ok(()) => ErrorCtx::ok(sink.finish()),
        Err(error) => ErrorCtx {
            consumed: sink.written(),
            code: if error.code == ErrorCode::None {
                ErrorCode::BufferOverflow
            } else {
                error.code
            },
            message: error.message,
            expected: error.expected,
        },
    }
}

/// Fixed-buffer write for a single node document (direct).
pub fn write_node_into_slice<T: Encode>(value: &T, buffer: &mut [u8]) -> ErrorCtx {
    let mut sink = WriteSink::slice(buffer);
    match value.write_node(&mut sink, 0) {
        Ok(()) => ErrorCtx::ok(sink.finish()),
        Err(error) => ErrorCtx {
            consumed: sink.written(),
            code: if error.code == ErrorCode::None {
                ErrorCode::BufferOverflow
            } else {
                error.code
            },
            message: error.message,
            expected: error.expected,
        },
    }
}

// Keep format_document import used by Document write fallback only.
#[allow(dead_code)]
fn _format_document_used(doc: &Document<'_>) -> String {
    format_document(doc)
}
