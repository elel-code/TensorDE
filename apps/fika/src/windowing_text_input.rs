use std::ops::Range;

const MAX_SURROUNDING_TEXT_BYTES: usize = 4_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImeChangeCause {
    InputMethod,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImeCursorArea {
    pub position: PhysicalPosition<f64>,
    pub size: PhysicalSize<f64>,
}

impl ImeCursorArea {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            position: PhysicalPosition::new(x, y),
            size: PhysicalSize::new(width, height),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImeState {
    pub surrounding_text: String,
    pub cursor: usize,
    pub anchor: usize,
    pub cursor_area: Option<ImeCursorArea>,
    pub purpose: RuntimeTextInputContentPurpose,
    pub hints: RuntimeTextInputContentHint,
    pub change_cause: ImeChangeCause,
}

impl ImeState {
    pub fn new(
        surrounding_text: impl Into<String>,
        cursor: usize,
        anchor: usize,
        purpose: RuntimeTextInputContentPurpose,
    ) -> Self {
        let surrounding_text = surrounding_text.into();
        let (surrounding_text, cursor, anchor) =
            surrounding_text_window(surrounding_text, cursor, anchor);
        Self {
            surrounding_text,
            cursor,
            anchor,
            cursor_area: None,
            purpose,
            hints: RuntimeTextInputContentHint::empty(),
            change_cause: ImeChangeCause::Other,
        }
    }

    pub fn with_cursor_area(mut self, cursor_area: ImeCursorArea) -> Self {
        self.cursor_area = Some(cursor_area);
        self
    }

    pub fn with_change_cause(mut self, change_cause: ImeChangeCause) -> Self {
        self.change_cause = change_cause;
        self
    }

    pub(crate) fn same_client_state(&self, other: &Self) -> bool {
        self.surrounding_text == other.surrounding_text
            && self.cursor == other.cursor
            && self.anchor == other.anchor
            && self.cursor_area == other.cursor_area
            && self.purpose == other.purpose
            && self.hints == other.hints
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImePreedit {
    pub text: String,
    pub cursor_range: Option<Range<usize>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImeDeleteSurrounding {
    pub before_bytes: usize,
    pub after_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImeEvent {
    Enabled,
    Disabled,
    Done {
        serial: u32,
        delete_surrounding: Option<ImeDeleteSurrounding>,
        commit: Option<String>,
        preedit: Option<ImePreedit>,
    },
}

fn runtime_text_input_state(
    state: ImeState,
    scale_factor: f64,
) -> Result<RuntimeTextInputState, RuntimeError> {
    let surrounding = RuntimeTextInputSurroundingText::new(
        state.surrounding_text,
        state.cursor,
        state.anchor,
    )
    .map_err(|error| RuntimeError::Protocol(error.to_string()))?;
    let mut runtime = RuntimeTextInputState::new()
        .with_surrounding_text(surrounding)
        .with_content_type(RuntimeTextInputContentType {
            hints: state.hints,
            purpose: state.purpose,
        })
        .with_change_cause(match state.change_cause {
            ImeChangeCause::InputMethod => RuntimeTextInputChangeCause::InputMethod,
            ImeChangeCause::Other => RuntimeTextInputChangeCause::Other,
        });
    if let Some(area) = state.cursor_area {
        let factor = normalize_wayland_scale_factor(scale_factor);
        let x = scaled_ime_origin(area.position.x, factor);
        let y = scaled_ime_origin(area.position.y, factor);
        let width = scaled_ime_extent(area.size.width, factor);
        let height = scaled_ime_extent(area.size.height, factor);
        runtime = runtime
            .with_cursor_rectangle(wayland_client_runtime::LogicalRect::new(
                x, y, width, height,
            ))
            .map_err(|error| RuntimeError::Protocol(error.to_string()))?;
    }
    Ok(runtime)
}

fn scaled_ime_origin(value: f64, scale_factor: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    (value / scale_factor)
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn scaled_ime_extent(value: f64, scale_factor: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 1;
    }
    (value / scale_factor)
        .round()
        .clamp(1.0, f64::from(i32::MAX)) as u32
}

fn surrounding_text_window(
    text: String,
    cursor: usize,
    anchor: usize,
) -> (String, usize, usize) {
    let cursor = normalized_ime_offset(&text, cursor);
    let anchor = normalized_ime_offset(&text, anchor);
    if text.len() <= MAX_SURROUNDING_TEXT_BYTES {
        return (text, cursor, anchor);
    }

    let mut start = cursor.saturating_sub(MAX_SURROUNDING_TEXT_BYTES / 2);
    if text.len() - start < MAX_SURROUNDING_TEXT_BYTES {
        start = text.len() - MAX_SURROUNDING_TEXT_BYTES;
    }
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let mut end = (start + MAX_SURROUNDING_TEXT_BYTES).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }

    let cursor = cursor - start;
    let anchor = if (start..=end).contains(&anchor) {
        anchor - start
    } else {
        cursor
    };
    (text[start..end].to_string(), cursor, anchor)
}

fn normalized_ime_offset(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod text_input_tests {
    use super::*;

    #[test]
    fn surrounding_window_stays_wire_sized_and_preserves_utf8_cursor() {
        let text = format!("{}β{}", "a".repeat(3_000), "z".repeat(3_000));
        let cursor = 3_000 + "β".len();
        let state = ImeState::new(
            text,
            cursor,
            cursor,
            RuntimeTextInputContentPurpose::Normal,
        );

        assert!(state.surrounding_text.len() <= MAX_SURROUNDING_TEXT_BYTES);
        assert!(state.surrounding_text.is_char_boundary(state.cursor));
        assert_eq!(&state.surrounding_text[state.cursor - 2..state.cursor], "β");
    }

    #[test]
    fn surrounding_window_collapses_selection_outside_the_wire_window() {
        let state = ImeState::new(
            "x".repeat(5_000),
            4_900,
            0,
            RuntimeTextInputContentPurpose::Normal,
        );

        assert_eq!(state.cursor, state.anchor);
        assert_eq!(state.surrounding_text.len(), MAX_SURROUNDING_TEXT_BYTES);
    }
}
