//! Typed encode — reverse of [`crate::Decode`].
//!
//! **Glaze only** (`references/glaze/core/write.hpp`):
//! monomorphized [`Encode::write_node`] / [`EncodeDocument::write_document`] dump
//! into [`WriteSink`] (`util/dump.hpp`). User type `T` → bytes. No parse tree,
//! no `Node`/`Document` on this path.
//!
//! Parsed-tree pretty-print for the official suite lives in [`crate::write`]
//! (Glaze `generic` / tooling role) and does **not** implement these traits.
//!
//! Public entrypoints (`references/glaze/docs/writing.md`):
//! - [`write_into`] — resizable buffer; `ErrorCtx.consumed` = bytes written
//! - [`write_into_slice`] — fixed buffer + [`ErrorCode::BufferOverflow`]
//! - [`write`] — allocate `String`

pub(crate) mod sink;

pub use sink::{
    WRITE_PADDING_BYTES, WriteSink, write_arg_node_line, write_argument_prefix, write_bool,
    write_children_close, write_children_open, write_f64, write_flag_line, write_i128,
    write_ident_or_string, write_indent, write_node_end_leaf, write_node_header, write_null,
    write_prop_node_line, write_property_key, write_quoted, write_u128,
};

use crate::context::Context;
use crate::error::{CtxResult, ErrorCode, ErrorCtx};

/// Encode `Self` as a single KDL node (Glaze `to::op`).
pub trait Encode {
    /// Dump a full node line/block at `indent` (4-space levels).
    fn write_node(&self, out: &mut WriteSink<'_>, indent: usize) -> Result<(), ErrorCtx>;

    /// Dump arguments, properties, and children **after** the caller already
    /// wrote the node header (`(type)? name`). Used when a parent forces a
    /// child name (`#[kdl(child(name = "..."))]`).
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
/// Direct stream only — no entry/node vectors.
pub trait EncodePartial {
    /// Extra properties after host known fields.
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
    fn has_partial_children(&self) -> bool {
        false
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
        // Full unsigned range via stack digits (Glaze write_chars).
        write_u128(out, *self)
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
///
/// Reuses `buffer` capacity across calls (perf doc: prefer write-into over allocate).
pub fn write_into<T: EncodeDocument>(value: &T, buffer: &mut String) -> ErrorCtx {
    write_into_with_context(value, buffer, &mut Context::new())
}

/// Glaze `write(T, buffer, ctx)` — reuse [`Context`] error/scratch across hot writes.
///
/// Cite: `core/write.hpp` `write(T, Buffer&, ctx)` + perf doc context reuse.
pub fn write_into_with_context<T: EncodeDocument>(
    value: &T,
    buffer: &mut String,
    ctx: &mut Context,
) -> ErrorCtx {
    ctx.clear_error();
    // Keep capacity; clear length only (Glaze overwrites from ix=0 then finalize).
    buffer.clear();
    let mut sink = WriteSink::string(buffer);
    match value.write_document(&mut sink) {
        Ok(()) => ErrorCtx::ok(sink.finish()),
        Err(error) => {
            ctx.error = error.code;
            ctx.custom_error_message = error.message.clone();
            ErrorCtx {
                consumed: sink.written(),
                code: error.code,
                message: error.message,
                expected: error.expected,
            }
        }
    }
}

/// Write a single node type into a resizable buffer.
pub fn write_node_into<T: Encode>(value: &T, buffer: &mut String) -> ErrorCtx {
    write_node_into_with_context(value, buffer, &mut Context::new())
}

/// Like [`write_node_into`] with reused [`Context`].
pub fn write_node_into_with_context<T: Encode>(
    value: &T,
    buffer: &mut String,
    ctx: &mut Context,
) -> ErrorCtx {
    ctx.clear_error();
    buffer.clear();
    let mut sink = WriteSink::string(buffer);
    match value.write_node(&mut sink, 0) {
        Ok(()) => ErrorCtx::ok(sink.finish()),
        Err(error) => {
            ctx.error = error.code;
            ctx.custom_error_message = error.message.clone();
            ErrorCtx {
                consumed: sink.written(),
                code: error.code,
                message: error.message,
                expected: error.expected,
            }
        }
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
