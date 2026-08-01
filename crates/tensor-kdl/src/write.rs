//! Pretty-printer for DOM documents (official suite Translation Rules).
//!
//! Implemented by dumping through [`crate::WriteSink`] / [`crate::write_dom_node`]
//! so suite tooling shares the same dump primitives as typed write.
//! Typed encode does **not** go through this module.

use crate::encode::{WriteSink, write_dom_node};
use crate::value::{Document, Node};

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
        let _ = write_dom_node(node, &mut sink, 0);
    }
}

/// Format a single node into `out` (append).
pub fn format_node_into(node: &Node<'_>, out: &mut String) {
    let mut sink = WriteSink::string(out);
    let _ = write_dom_node(node, &mut sink, 0);
}
