//! KDL 2.0 single-pass parser.

pub(crate) mod chars;
mod reader;
mod simd;
mod swar;
pub mod visitor;

pub use reader::Parser;
#[cfg(feature = "dom")]
pub use reader::{
    parse_document, parse_document_with_context, visit_document, visit_document_with_context,
};
#[cfg(feature = "dom")]
pub use visitor::DomNodeBuilder;
pub use visitor::{CountingVisitor, NodeVisitor};
