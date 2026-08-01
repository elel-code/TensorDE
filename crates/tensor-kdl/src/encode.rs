//! Typed encode — reverse of [`crate::Decode`].
//!
//! **Glaze primary path only** (`references/glaze/core/write.hpp`):
//! monomorphized [`Encode::write_node`] / [`EncodeDocument::write_document`] dump
//! into [`WriteSink`] (`util/dump.hpp` role). There is no intermediate
//! [`Node`] / [`Document`] on the typed write path — Glaze does not keep a
//! parallel “build DOM then print” encode API.
//!
//! Public entrypoints (`references/glaze/docs/writing.md`):
//! - [`write_into`] — resizable buffer; `ErrorCtx.consumed` = bytes written
//! - [`write_into_slice`] — fixed buffer + [`ErrorCode::BufferOverflow`]
//! - [`write`] — allocate `String`

mod sink;

pub use sink::{
    WriteSink, write_arg_node_line, write_argument_prefix, write_bool, write_children_close,
    write_children_open, write_f64, write_f64_lexical, write_flag_line, write_i128,
    write_ident_or_string, write_indent, write_node_end_leaf, write_node_header, write_null,
    write_prop_node_line, write_property_key, write_quoted,
};

use crate::error::{CtxResult, ErrorCode, ErrorCtx};
use crate::value::{Document, Entry, Node, Value};

/// Encode `Self` as a single KDL node (Glaze `to::op`).
pub trait Encode {
    /// Dump a full node line/block at `indent` (4-space levels).
    fn write_node(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx>;

    /// Dump arguments, properties, and children **after** the caller already
    /// wrote the node header (`(type)? name`). Used when a parent forces a
    /// child name (`#[kdl(child(name = "..."))]`).
    ///
    /// Default: write a full node at `indent` (safe but may re-emit a name).
    /// Derived types override with a true body-only dump.
    fn write_node_body(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
        self.write_node(out, indent)
    }

    /// Full node under an explicit name (parent rename).
    fn write_node_named(
        &self,
        out: &mut WriteSink<'_>,
        indent: usize,
        name: &str,
    ) -> Result<(), ErrorCtx> {
        write_node_header(out, indent, None, name)?;
        self.write_node_body(out, indent)
    }
}

/// Encode `Self` as a full document (top-level nodes).
pub trait EncodeDocument {
    fn write_document(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx>;
}

/// Encode a scalar as a KDL value **lexeme** (Glaze scalar `to::op`).
pub trait EncodeScalar {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx>;
}

/// Reverse of [`crate::DecodePartial`] for `#[kdl(flatten)]` on encode.
///
/// Direct stream only — no entry/node vectors on the hot path.
pub trait EncodePartial {
    /// Extra properties (and rare extra arguments) after host known fields.
    fn write_partial(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
        let _ = (out, indent);
        Ok(())
    }

    /// Extra children inside an open children block.
    fn write_partial_children(
        &self,
        out: &mut WriteSink<'_>,
        indent: usize,
    ) -> Result<(), ErrorCtx> {
        let _ = (out, indent);
        Ok(())
    }

    /// Whether [`Self::write_partial_children`] will emit at least one child.
    /// Used to avoid empty `{}` blocks when only flatten may contribute children.
    fn has_partial_children(&self) -> bool {
        false
    }
}

pub(crate) fn write_value_lexeme(
    out: &mut WriteSink<'_>,
    value: &Value<'_>,
) -> Result<(), ErrorCtx> {
    match value {
        Value::String(s) => write_ident_or_string(out, s.as_str()),
        Value::Bool(b) => write_bool(out, *b),
        Value::Null => write_null(out),
        Value::Int(n) => write_i128(out, *n),
        Value::Float { value, raw } => {
            if let Some(raw) = raw {
                write_f64_lexical(out, raw.as_str(), *value)
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
}

impl EncodeScalar for str {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_ident_or_string(out, self)
    }
}

impl EncodeScalar for bool {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_bool(out, *self)
    }
}

macro_rules! impl_int_encode_scalar {
    ($($t:ty),* $(,)?) => {$(
        impl EncodeScalar for $t {
            fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
                write_i128(out, i128::from(*self))
            }
        }
    )*};
}

impl_int_encode_scalar!(i8, i16, i32, i64, i128, u8, u16, u32, u64);

impl EncodeScalar for usize {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_i128(out, *self as i128)
    }
}

impl EncodeScalar for isize {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_i128(out, *self as i128)
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
}

impl EncodeScalar for f64 {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_f64(out, *self)
    }
}

impl EncodeScalar for f32 {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        write_f64(out, f64::from(*self))
    }
}

impl<T: EncodeScalar + ?Sized> EncodeScalar for &T {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        (*self).write_scalar(out)
    }
}

impl<T: EncodeScalar> EncodeScalar for Option<T> {
    fn write_scalar(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        match self {
            Some(v) => v.write_scalar(out),
            None => write_null(out),
        }
    }
}

impl Encode for crate::Flag {
    fn write_node(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
        write_flag_line(out, indent, "flag")
    }

    fn write_node_body(&self, out: &mut WriteSink<'_>, _indent: usize) -> Result<(), ErrorCtx> {
        write_node_end_leaf(out)
    }
}

impl<T: Encode> EncodeDocument for Vec<T> {
    fn write_document(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        for item in self {
            item.write_node(out, 0)?;
        }
        Ok(())
    }
}

impl EncodeDocument for Document<'_> {
    fn write_document(&self, out: &mut WriteSink<'_>) -> Result<(), ErrorCtx> {
        for node in &self.nodes {
            write_dom_node(node, out, 0)?;
        }
        Ok(())
    }
}

impl Encode for Node<'_> {
    fn write_node(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
        write_dom_node(self, out, indent)
    }

    fn write_node_body(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx> {
        write_dom_node_body(self, out, indent)
    }

    fn write_node_named(
        &self,
        out: &mut WriteSink<'_>,
        indent: usize,
        name: &str,
    ) -> Result<(), ErrorCtx> {
        write_node_header(
            out,
            indent,
            self.type_name.as_ref().map(|t| t.as_str()),
            name,
        )?;
        write_dom_node_body(self, out, indent)
    }
}

/// Format a parsed DOM node into the sink (suite / query tooling only).
pub fn write_dom_node(
    node: &Node<'_>,
    out: &mut WriteSink<'_>,
    indent: usize,
) -> Result<(), ErrorCtx> {
    write_node_header(
        out,
        indent,
        node.type_name.as_ref().map(|t| t.as_str()),
        node.name.as_str(),
    )?;
    write_dom_node_body(node, out, indent)
}

fn write_dom_node_body(
    node: &Node<'_>,
    out: &mut WriteSink<'_>,
    indent: usize,
) -> Result<(), ErrorCtx> {
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

/// Format `T` as KDL text via the direct write path.
pub fn to_string<T: EncodeDocument>(value: &T) -> CtxResult<String> {
    write(value)
}

/// Format a single node type as a one-node document.
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

/// Glaze `write(T, buffer)` for resizable buffers.
pub fn write_into<T: EncodeDocument>(value: &T, buffer: &mut String) -> ErrorCtx {
    buffer.clear();
    let mut sink = WriteSink::string(buffer);
    match value.write_document(&mut sink) {
        Ok(()) => ErrorCtx::ok(sink.finish()),
        Err(error) => ErrorCtx {
            consumed: sink.written(),
            code: error.code,
            message: error.message,
            expected: error.expected,
        },
    }
}

/// Write a single node type into a resizable buffer.
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

/// Glaze fixed-buffer write.
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

/// Fixed-buffer write for a single node.
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
