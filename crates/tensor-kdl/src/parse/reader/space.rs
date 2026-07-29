//! Whitespace, comments, and line continuations.

use crate::error::{CtxResult, ErrorCode};
use crate::parse::chars::{is_newline_char, is_unicode_space};
use crate::parse::reader::Parser;
use crate::parse::swar::skip_ascii_horizontal_ws;

impl<'a> Parser<'a> {
    pub(super) fn at_newline(&self) -> bool {
        matches!(self.peek_char(), Some(c) if is_newline_char(c))
    }

    pub(super) fn consume_newline(&mut self) -> bool {
        match self.peek_char() {
            Some('\r') => {
                self.bump_char();
                if self.peek_byte() == Some(b'\n') {
                    self.bump_byte();
                }
                true
            }
            Some(c) if is_newline_char(c) => {
                self.bump_char();
                true
            }
            _ => false,
        }
    }

    pub(super) fn skip_line_comment(&mut self) {
        debug_assert!(self.starts_with("//"));
        self.index += 2;
        while let Some(c) = self.peek_char() {
            if is_newline_char(c) {
                self.consume_newline();
                break;
            }
            self.bump_char();
        }
    }

    pub(super) fn skip_block_comment(&mut self) -> CtxResult<()> {
        debug_assert!(self.starts_with("/*"));
        self.index += 2;
        let mut depth = 1i32;
        while depth > 0 {
            if self.eof() {
                return Err(self
                    .err(ErrorCode::UnexpectedEof)
                    .with_message("unclosed block comment"));
            }
            if self.starts_with("/*") {
                self.index += 2;
                depth += 1;
            } else if self.starts_with("*/") {
                self.index += 2;
                depth -= 1;
            } else {
                self.bump_char();
            }
        }
        Ok(())
    }

    pub(super) fn skip_ws_only(&mut self) -> CtxResult<()> {
        loop {
            let before = self.index;
            self.index = skip_ascii_horizontal_ws(self.bytes, self.index);
            if let Some(c) = self.peek_char()
                && is_unicode_space(c)
            {
                self.bump_char();
                continue;
            }
            if self.starts_with("/*") {
                self.skip_block_comment()?;
                continue;
            }
            if self.index == before {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn skip_escline(&mut self) -> CtxResult<()> {
        if self.peek_byte() != Some(b'\\') {
            return Err(self.err(ErrorCode::Syntax));
        }
        self.bump_byte();
        self.skip_ws_only()?;
        if self.starts_with("//") {
            self.skip_line_comment();
            return Ok(());
        }
        if self.eof() {
            return Ok(());
        }
        if self.consume_newline() {
            return Ok(());
        }
        Err(self
            .err(ErrorCode::Syntax)
            .with_message("line continuation must end with newline or comment"))
    }

    /// Whitespace within a node (no bare newlines unless esclined).
    pub(super) fn skip_node_space(&mut self) -> CtxResult<()> {
        let _ = self.skip_node_space_counted()?;
        Ok(())
    }

    /// Like [`Self::skip_node_space`], but reports whether any space was consumed.
    pub(super) fn skip_node_space_counted(&mut self) -> CtxResult<bool> {
        let start = self.index;
        loop {
            let before = self.index;
            self.skip_ws_only()?;
            if self.peek_byte() == Some(b'\\') {
                self.skip_escline()?;
                continue;
            }
            if self.index == before {
                break;
            }
        }
        Ok(self.index > start)
    }

    /// Newlines, comments, and node-space (between nodes).
    pub(super) fn skip_line_space(&mut self) -> CtxResult<()> {
        loop {
            let before = self.index;
            self.skip_ws_only()?;
            if self.starts_with("//") {
                self.skip_line_comment();
                continue;
            }
            if self.consume_newline() {
                continue;
            }
            if self.peek_byte() == Some(b'\\') {
                // Lone escline at line-space level is odd but allow as node-space piece
                self.skip_escline()?;
                continue;
            }
            if self.index == before {
                break;
            }
        }
        Ok(())
    }
}
