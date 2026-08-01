//! Parse-tree pretty-print (official suite Translation Rules).
//!
//! **Not** Glaze typed write. Glaze separates:
//! - `to::op<T>` — monomorphized user-type dump ([`crate::encode`])
//! - `glz::generic` / value trees — only when the schema is unknown
//!
//! This module is the KDL analogue of printing an already-parsed value tree
//! (`Document` / `Node` from [`crate::from_str`]). Suite roundtrip and examples
//! use it. Typed `#[derive(Encode)]` never goes through here.

use std::collections::BTreeMap;

use crate::encode::sink::{
    WriteSink, write_argument_prefix, write_bool, write_children_close, write_children_open,
    write_f64, write_f64_lexical, write_i128, write_ident_or_string, write_node_end_leaf,
    write_node_header, write_null, write_property_key,
};
use crate::error::ErrorCtx;
use crate::value::{Document, Entry, Node, Value};

/// Write a document using the official test-suite pretty-print conventions.
pub fn format_document(doc: &Document<'_>) -> String {
    let mut out = String::new();
    format_document_into(doc, &mut out);
    out
}

/// Format `doc` into `out` (append). Does not clear `out`.
pub fn format_document_into(doc: &Document<'_>, out: &mut String) {
    let mut sink = WriteSink::string(out);
    for node in &doc.nodes {
        let _ = format_node(&mut sink, node, 0);
    }
}

/// Format a single node into `out` (append).
pub fn format_node_into(node: &Node<'_>, out: &mut String) {
    let mut sink = WriteSink::string(out);
    let _ = format_node(&mut sink, node, 0);
}

fn format_node(out: &mut WriteSink<'_>, node: &Node<'_>, indent: usize) -> Result<(), ErrorCtx> {
    write_node_header(
        out,
        indent,
        node.type_name.as_ref().map(|t| t.as_str()),
        node.name.as_str(),
    )?;
    format_node_body(out, node, indent)
}

fn format_node_body(
    out: &mut WriteSink<'_>,
    node: &Node<'_>,
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
            format_value(out, value)?;
        }
    }
    // Rightmost property wins, then alphabetical by key (suite Translation Rules).
    let mut props = BTreeMap::<&str, (Option<&str>, &Value<'_>)>::new();
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
        format_value(out, v)?;
    }
    if node.children.is_empty() {
        return write_node_end_leaf(out);
    }
    write_children_open(out)?;
    for child in &node.children {
        format_node(out, child, indent + 1)?;
    }
    write_children_close(out, indent)
}

fn format_value(out: &mut WriteSink<'_>, value: &Value<'_>) -> Result<(), ErrorCtx> {
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
