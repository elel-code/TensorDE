//! text-input-v3 public state types. 

use std::ops::Range;

use bitflags::bitflags;

use crate::{LogicalRect, SurfaceId};

const MAX_SURROUNDING_TEXT_BYTES: usize = 4_000;

bitflags! {
    /// Hints that refine how an input method should handle an editable field.
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
    pub struct TextInputContentHint: u16 {
        const COMPLETION = 1 << 0;
        const SPELLCHECK = 1 << 1;
        const AUTO_CAPITALIZATION = 1 << 2;
        const LOWERCASE = 1 << 3;
        const UPPERCASE = 1 << 4;
        const TITLECASE = 1 << 5;
        const HIDDEN_TEXT = 1 << 6;
        const SENSITIVE_DATA = 1 << 7;
        const LATIN = 1 << 8;
        const MULTILINE = 1 << 9;
    }
}

/// Primary semantic purpose of an editable field.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextInputContentPurpose {
    #[default]
    Normal,
    Alpha,
    Digits,
    Number,
    Phone,
    Url,
    Email,
    Name,
    Password,
    Pin,
    Date,
    Time,
    DateTime,
    Terminal,
}

/// Cause of the latest surrounding-text update.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextInputChangeCause {
    #[default]
    InputMethod,
    Other,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct TextInputContentType {
    pub hints: TextInputContentHint,
    pub purpose: TextInputContentPurpose,
}

/// UTF-8 text around the editor cursor. Cursor and anchor are byte offsets,
/// following the text-input-v3 wire format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextInputSurroundingText {
    text: String,
    cursor: usize,
    anchor: usize,
}

impl TextInputSurroundingText {
    pub fn new(
        text: impl Into<String>,
        cursor: usize,
        anchor: usize,
    ) -> Result<Self, TextInputError> {
        let text = text.into();
        if text.len() > MAX_SURROUNDING_TEXT_BYTES {
            return Err(TextInputError::SurroundingTextTooLong);
        }
        if text.contains('\0') {
            return Err(TextInputError::SurroundingTextContainsNul);
        }
        if cursor > text.len() || anchor > text.len() {
            return Err(TextInputError::SurroundingOffsetOutOfBounds);
        }
        if !text.is_char_boundary(cursor) || !text.is_char_boundary(anchor) {
            return Err(TextInputError::SurroundingOffsetSplitsCodepoint);
        }
        Ok(Self {
            text,
            cursor,
            anchor,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    pub const fn anchor(&self) -> usize {
        self.anchor
    }
}

/// Complete client state applied atomically to one focused text-input object.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextInputState {
    surrounding_text: Option<TextInputSurroundingText>,
    content_type: Option<TextInputContentType>,
    cursor_rectangle: Option<LogicalRect>,
    change_cause: TextInputChangeCause,
}

impl TextInputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_surrounding_text(mut self, surrounding: TextInputSurroundingText) -> Self {
        self.surrounding_text = Some(surrounding);
        self
    }

    pub fn with_content_type(mut self, content_type: TextInputContentType) -> Self {
        self.content_type = Some(content_type);
        self
    }

    pub fn with_cursor_rectangle(mut self, rectangle: LogicalRect) -> Result<Self, TextInputError> {
        validate_cursor_rectangle(rectangle)?;
        self.cursor_rectangle = Some(rectangle);
        Ok(self)
    }

    pub fn with_change_cause(mut self, cause: TextInputChangeCause) -> Self {
        self.change_cause = cause;
        self
    }

    pub fn surrounding_text(&self) -> Option<&TextInputSurroundingText> {
        self.surrounding_text.as_ref()
    }

    pub const fn content_type(&self) -> Option<TextInputContentType> {
        self.content_type
    }

    pub const fn cursor_rectangle(&self) -> Option<LogicalRect> {
        self.cursor_rectangle
    }

    pub const fn change_cause(&self) -> TextInputChangeCause {
        self.change_cause
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TextInputError {
    #[error("text-input surrounding text exceeds the protocol limit of 4000 bytes")]
    SurroundingTextTooLong,
    #[error("text-input surrounding text must not contain NUL bytes")]
    SurroundingTextContainsNul,
    #[error("text-input surrounding cursor or anchor is outside the text")]
    SurroundingOffsetOutOfBounds,
    #[error("text-input surrounding cursor or anchor splits a UTF-8 codepoint")]
    SurroundingOffsetSplitsCodepoint,
    #[error("text-input cursor rectangle must have non-zero dimensions")]
    EmptyCursorRectangle,
    #[error("text-input cursor rectangle dimensions exceed Wayland integer limits")]
    CursorRectangleTooLarge,
}

/// Preedit text and its optional UTF-8 byte cursor/selection range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextInputPreedit {
    pub text: String,
    pub cursor_range: Option<Range<usize>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextInputDeleteSurrounding {
    pub before_bytes: usize,
    pub after_bytes: usize,
}

/// One atomic text-input-v3 `done` batch. Apply deletion, commit, and the new
/// preedit in that order after replacing any previous preedit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextInputDone {
    pub surface: SurfaceId,
    pub serial: u32,
    pub delete_surrounding: Option<TextInputDeleteSurrounding>,
    pub commit: Option<String>,
    pub preedit: Option<TextInputPreedit>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextInputEvent {
    Entered { surface: SurfaceId },
    Left { surface: SurfaceId },
    Done(TextInputDone),
}


fn validate_cursor_rectangle(rectangle: LogicalRect) -> Result<(), TextInputError> {
    if rectangle.is_empty() {
        return Err(TextInputError::EmptyCursorRectangle);
    }
    if rectangle.size.width > i32::MAX as u32 || rectangle.size.height > i32::MAX as u32 {
        return Err(TextInputError::CursorRectangleTooLarge);
    }
    Ok(())
}

