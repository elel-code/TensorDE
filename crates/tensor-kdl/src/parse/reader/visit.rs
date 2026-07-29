//! Node visit / DOM build — Glaze field-fill loop for one KDL node.
//!
//! Cite: `json/read.hpp` `decode_index` / `from::op`; nested objects recurse
//! like nested `parse::op` (P-G3d).
//!
//! **P-G4:** `*_const::<OPTS>` monomorphizes policy bits (Glaze
//! `template <auto Opts>`). Runtime [`Opts`] wrappers call through
//! [`Opts::bits`].

use crate::error::{CtxResult, ErrorCode};
use crate::opts::{OPTS_DEFAULT, OPTS_LENIENT, Opts, flag_error_on_unknown};
use crate::parse::visitor::{CountingVisitor, DomNodeBuilder, NodeVisitor};
use crate::value::{Entry, KdlStr, Node};

use super::Parser;

impl<'a> Parser<'a> {
    /// Parse one node into an owned DOM [`Node`] via [`DomNodeBuilder`].
    pub(super) fn parse_node(&mut self) -> CtxResult<Node<'a>> {
        let mut builder = DomNodeBuilder::default();
        self.visit_node_const::<OPTS_DEFAULT, _>(&mut builder)?;
        builder.finish()
    }

    /// Parse one node, delivering fields to `visitor` as they are recognized.
    ///
    /// Glaze analogue: object body loop calls `decode_index` / `from::op` per key
    /// (`json/read.hpp`); unknown keys use `skip_value` or `unknown_key`
    /// (`opts.error_on_unknown_keys`).
    pub fn visit_node<V: NodeVisitor<'a>>(&mut self, opts: Opts, visitor: &mut V) -> CtxResult<()> {
        // Runtime opts: dispatch through bits so both APIs share one body.
        // Const sites should call [`Self::visit_node_const`] directly.
        self.visit_node_bits(opts.bits(), visitor)
    }

    /// Const-generic node visit (Glaze `parse<Opts>::op` monomorphization).
    ///
    /// `OPTS` is a packed bitset from [`Opts::bits`] / [`crate::OPTS_DEFAULT`].
    pub fn visit_node_const<const OPTS: u8, V: NodeVisitor<'a>>(
        &mut self,
        visitor: &mut V,
    ) -> CtxResult<()> {
        self.visit_node_bits(OPTS, visitor)
    }

    fn visit_node_bits<V: NodeVisitor<'a>>(
        &mut self,
        opts_bits: u8,
        visitor: &mut V,
    ) -> CtxResult<()> {
        let (type_name, name) = self.parse_node_header()?;
        visitor.on_header(type_name, name)?;
        self.visit_node_body_bits(opts_bits, visitor)
    }

    /// Type annotation + name only (for nested dispatch before choosing a child builder).
    pub fn parse_node_header(&mut self) -> CtxResult<(Option<KdlStr<'a>>, KdlStr<'a>)> {
        let type_name = self.try_parse_type_annotation()?;
        self.skip_node_space()?;
        let name = self.parse_string_value().map_err(|e| {
            if e.code == ErrorCode::UnexpectedEof || e.code == ErrorCode::InvalidIdent {
                e.with_expected("node name")
            } else {
                e
            }
        })?;
        Ok((type_name, name))
    }

    /// Body after header (args, props, children).
    pub fn visit_node_body<V: NodeVisitor<'a>>(
        &mut self,
        opts: Opts,
        visitor: &mut V,
    ) -> CtxResult<()> {
        self.visit_node_body_bits(opts.bits(), visitor)
    }

    /// Const-generic body after header.
    pub fn visit_node_body_const<const OPTS: u8, V: NodeVisitor<'a>>(
        &mut self,
        visitor: &mut V,
    ) -> CtxResult<()> {
        self.visit_node_body_bits(OPTS, visitor)
    }

    fn visit_node_body_bits<V: NodeVisitor<'a>>(
        &mut self,
        opts_bits: u8,
        visitor: &mut V,
    ) -> CtxResult<()> {
        let mut after_children = false;
        let mut has_real_children = false;
        let mut need_space_before_entry = true;

        loop {
            let space_before = self.skip_node_space_counted()?;

            if self.starts_with("/-") {
                self.index += 2;
                self.skip_line_space()?;
                if self.peek_byte() == Some(b'{') {
                    self.skip_children_block()?;
                    after_children = true;
                    need_space_before_entry = true;
                    continue;
                }
                if after_children {
                    return Err(self
                        .err(ErrorCode::Syntax)
                        .with_message("entries are not allowed after a children block"));
                }
                self.skip_prop_or_arg()?;
                need_space_before_entry = true;
                continue;
            }

            match self.peek_byte() {
                None => break,
                Some(b'}') => break,
                Some(b';') => {
                    self.bump_byte();
                    break;
                }
                Some(b'{') => {
                    if has_real_children {
                        return Err(self
                            .err(ErrorCode::Syntax)
                            .with_message("node already has a children block"));
                    }
                    visitor.on_children_begin()?;
                    self.visit_children_block_bits(opts_bits, visitor)?;
                    visitor.on_children_end()?;
                    has_real_children = true;
                    after_children = true;
                    need_space_before_entry = true;
                    continue;
                }
                Some(b'/') if self.starts_with("//") => {
                    self.skip_line_comment();
                    break;
                }
                Some(b'\\') => {
                    self.skip_escline()?;
                    need_space_before_entry = false;
                    continue;
                }
                _ => {
                    if self.at_newline() {
                        break;
                    }
                    if after_children {
                        return Err(self
                            .err(ErrorCode::Syntax)
                            .with_message("entries are not allowed after a children block"));
                    }
                    if need_space_before_entry && !space_before {
                        return Err(self
                            .err(ErrorCode::Syntax)
                            .with_message("expected whitespace before argument or property"));
                    }
                    self.visit_prop_or_arg_bits(opts_bits, visitor)?;
                    need_space_before_entry = true;
                }
            }
        }
        Ok(())
    }

    fn visit_prop_or_arg_bits<V: NodeVisitor<'a>>(
        &mut self,
        opts_bits: u8,
        visitor: &mut V,
    ) -> CtxResult<()> {
        let entry = self.parse_prop_or_arg()?;
        match entry {
            Entry::Argument { type_name, value } => {
                let _ = visitor.on_argument(type_name, value)?;
            }
            Entry::Property {
                key,
                type_name,
                value,
            } => {
                let handled = visitor.on_property(key, type_name, value)?;
                // Monomorphized when called via visit_node_const (opts_bits is const).
                if !handled && flag_error_on_unknown(opts_bits) {
                    return Err(self
                        .err(ErrorCode::UnknownProperty)
                        .with_message("property not recognized by visitor"));
                }
            }
        }
        Ok(())
    }

    fn visit_children_block_bits<V: NodeVisitor<'a>>(
        &mut self,
        opts_bits: u8,
        visitor: &mut V,
    ) -> CtxResult<()> {
        if self.peek_byte() != Some(b'{') {
            return Err(self.err(ErrorCode::ExpectedBrace).with_expected("`{`"));
        }
        let offset = self.index;
        self.bump_byte();
        self.ctx.enter_depth(offset)?;
        let opts = Opts::from_bits(opts_bits);
        let result = (|| {
            loop {
                self.skip_line_space()?;
                if self.eof() {
                    return Err(self.err(ErrorCode::ExpectedBrace).with_expected("`}`"));
                }
                if self.peek_byte() == Some(b'}') {
                    break;
                }
                if self.starts_with("/-") {
                    let start = self.index;
                    self.index += 2;
                    self.skip_line_space()?;
                    self.skip_node()?;
                    if self.index == start {
                        return Err(self
                            .err(ErrorCode::Syntax)
                            .with_message("invalid slashdash"));
                    }
                    continue;
                }
                // P-G3d: header then nested visit or DOM child (Glaze nested from::op).
                let (type_name, name) = self.parse_node_header()?;
                if visitor.take_child_after_header(self, opts, type_name.clone(), name.clone())? {
                    continue;
                }
                let mut child_dom = DomNodeBuilder::default();
                child_dom.on_header(type_name, name)?;
                self.visit_node_body_bits(opts_bits, &mut child_dom)?;
                let child = child_dom.finish()?;
                let _ = visitor.on_child(child)?;
            }
            self.skip_line_space()?;
            if self.peek_byte() != Some(b'}') {
                return Err(self.err(ErrorCode::ExpectedBrace).with_expected("`}`"));
            }
            self.bump_byte();
            Ok(())
        })();
        self.ctx.leave_depth();
        result
    }

    /// Drive a nested child's body into `child_visitor` after the parent already
    /// observed `(type_name, name)` via [`NodeVisitor::begin_child_visit`].
    pub fn finish_nested_child<V: NodeVisitor<'a>>(
        &mut self,
        opts: Opts,
        child_visitor: &mut V,
    ) -> CtxResult<()> {
        self.visit_node_body_bits(opts.bits(), child_visitor)
    }

    /// Const-generic nested child body (P-G4).
    pub fn finish_nested_child_const<const OPTS: u8, V: NodeVisitor<'a>>(
        &mut self,
        child_visitor: &mut V,
    ) -> CtxResult<()> {
        self.visit_node_body_bits(OPTS, child_visitor)
    }

    pub(super) fn parse_children_block(&mut self) -> CtxResult<Vec<Node<'a>>> {
        let mut children = Vec::new();
        struct Collect<'a, 'b> {
            out: &'b mut Vec<Node<'a>>,
        }
        impl<'a, 'b> NodeVisitor<'a> for Collect<'a, 'b> {
            fn on_child(&mut self, child: Node<'a>) -> CtxResult<bool> {
                self.out.push(child);
                Ok(true)
            }
        }
        if self.peek_byte() != Some(b'{') {
            return Err(self.err(ErrorCode::ExpectedBrace).with_expected("`{`"));
        }
        let mut v = Collect { out: &mut children };
        self.visit_children_block_bits(OPTS_DEFAULT, &mut v)?;
        Ok(children)
    }

    pub(super) fn skip_children_block(&mut self) -> CtxResult<()> {
        let _ = self.parse_children_block()?;
        Ok(())
    }

    pub(super) fn skip_node(&mut self) -> CtxResult<()> {
        let mut counter = CountingVisitor::default();
        self.visit_node_const::<OPTS_LENIENT, _>(&mut counter)
    }
}
