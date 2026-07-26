use std::ops::Range;
use std::sync::Mutex;

use bitflags::bitflags;
use smithay_client_toolkit::dispatch2::Dispatch2;
use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::{wl_seat, wl_surface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    ChangeCause, ContentHint, ContentPurpose, Event as ProtocolEvent, ZwpTextInputV3,
};

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

pub(crate) trait TextInputHandler {
    fn text_input_entered(
        &mut self,
        seat_id: u32,
        text_input: &ZwpTextInputV3,
        surface: &wl_surface::WlSurface,
    );
    fn text_input_left(
        &mut self,
        seat_id: u32,
        text_input: &ZwpTextInputV3,
        surface: &wl_surface::WlSurface,
    );
    fn text_input_done(
        &mut self,
        seat_id: u32,
        text_input: &ZwpTextInputV3,
        surface: &wl_surface::WlSurface,
        serial: u32,
        batch: PendingBatch,
    );
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ManagerData;

impl<D> Dispatch2<ZwpTextInputManagerV3, D> for ManagerData {
    fn event(
        &self,
        _: &mut D,
        _: &ZwpTextInputManagerV3,
        _: <ZwpTextInputManagerV3 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        unreachable!("zwp_text_input_manager_v3 has no events");
    }
}

#[derive(Debug, Default)]
struct InputMetadata {
    surface: Option<wl_surface::WlSurface>,
    pending: PendingBatch,
}

#[derive(Debug)]
pub(crate) struct InputData {
    seat_id: u32,
    metadata: Mutex<InputMetadata>,
}

impl InputData {
    fn new(seat_id: u32) -> Self {
        Self {
            seat_id,
            metadata: Mutex::new(InputMetadata::default()),
        }
    }
}

impl<D> Dispatch2<ZwpTextInputV3, D> for InputData
where
    D: TextInputHandler,
{
    fn event(
        &self,
        state: &mut D,
        proxy: &ZwpTextInputV3,
        event: ProtocolEvent,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        match event {
            ProtocolEvent::Enter { surface } => {
                let mut metadata = self.metadata.lock().expect("text input metadata poisoned");
                metadata.surface = Some(surface.clone());
                metadata.pending = PendingBatch::default();
                drop(metadata);
                state.text_input_entered(self.seat_id, proxy, &surface);
            }
            ProtocolEvent::Leave { surface } => {
                let mut metadata = self.metadata.lock().expect("text input metadata poisoned");
                metadata.surface = None;
                metadata.pending = PendingBatch::default();
                drop(metadata);
                state.text_input_left(self.seat_id, proxy, &surface);
            }
            ProtocolEvent::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                self.metadata
                    .lock()
                    .expect("text input metadata poisoned")
                    .pending
                    .set_preedit(text.unwrap_or_default(), cursor_begin, cursor_end);
            }
            ProtocolEvent::CommitString { text } => {
                let mut metadata = self.metadata.lock().expect("text input metadata poisoned");
                metadata.pending.preedit = None;
                metadata.pending.commit = text;
            }
            ProtocolEvent::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                self.metadata
                    .lock()
                    .expect("text input metadata poisoned")
                    .pending
                    .delete_surrounding = Some(TextInputDeleteSurrounding {
                    before_bytes: before_length as usize,
                    after_bytes: after_length as usize,
                });
            }
            ProtocolEvent::Done { serial } => {
                let (surface, batch) = {
                    let mut metadata = self.metadata.lock().expect("text input metadata poisoned");
                    (
                        metadata.surface.clone(),
                        std::mem::take(&mut metadata.pending),
                    )
                };
                if let Some(surface) = surface {
                    state.text_input_done(self.seat_id, proxy, &surface, serial, batch);
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PendingBatch {
    delete_surrounding: Option<TextInputDeleteSurrounding>,
    commit: Option<String>,
    preedit: Option<TextInputPreedit>,
}

impl PendingBatch {
    fn set_preedit(&mut self, text: String, cursor_begin: i32, cursor_end: i32) {
        let begin = valid_preedit_offset(&text, cursor_begin);
        let end = valid_preedit_offset(&text, cursor_end);
        let cursor_range = begin.zip(end).map(|(begin, end)| begin..end);
        self.preedit = Some(TextInputPreedit { text, cursor_range });
    }

    pub(crate) fn into_done(self, surface: SurfaceId, serial: u32) -> TextInputDone {
        TextInputDone {
            surface,
            serial,
            delete_surrounding: self.delete_surrounding,
            commit: self.commit,
            preedit: self.preedit,
        }
    }
}

#[derive(Debug)]
pub(crate) struct TextInputManager {
    manager: ZwpTextInputManagerV3,
}

impl TextInputManager {
    pub(crate) fn bind<D>(
        globals: &GlobalList,
        queue_handle: &QueueHandle<D>,
    ) -> Result<Self, BindError>
    where
        D: Dispatch<ZwpTextInputManagerV3, ManagerData> + 'static,
    {
        let manager = globals.bind(queue_handle, 1..=1, ManagerData)?;
        Ok(Self { manager })
    }

    pub(crate) fn get_text_input<D>(
        &self,
        seat: &wl_seat::WlSeat,
        queue_handle: &QueueHandle<D>,
    ) -> ZwpTextInputV3
    where
        D: Dispatch<ZwpTextInputV3, InputData> + 'static,
    {
        self.manager
            .get_text_input(seat, queue_handle, InputData::new(seat.id().protocol_id()))
    }
}

impl Drop for TextInputManager {
    fn drop(&mut self) {
        if self.manager.is_alive() {
            self.manager.destroy();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionTransition {
    None,
    Apply { enable: bool },
    Disable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SessionState {
    focus: Option<SurfaceId>,
    enabled: bool,
}

impl SessionState {
    fn enter(&mut self, surface: SurfaceId, has_state: bool) -> SessionTransition {
        self.focus = Some(surface);
        self.enabled = has_state;
        if has_state {
            SessionTransition::Apply { enable: true }
        } else {
            SessionTransition::None
        }
    }

    fn leave(&mut self) -> SessionTransition {
        self.focus = None;
        self.enabled = false;
        // A disable on every leave also resets all double-buffered fields.
        SessionTransition::Disable
    }

    fn update(&mut self, surface: SurfaceId, has_state: bool) -> SessionTransition {
        if self.focus != Some(surface) {
            return SessionTransition::None;
        }
        match (has_state, self.enabled) {
            (true, enabled) => {
                self.enabled = true;
                SessionTransition::Apply { enable: !enabled }
            }
            (false, true) => {
                self.enabled = false;
                SessionTransition::Disable
            }
            (false, false) => SessionTransition::None,
        }
    }

    fn remove_surface(&mut self, surface: SurfaceId) -> SessionTransition {
        if self.focus != Some(surface) {
            return SessionTransition::None;
        }
        let transition = if self.enabled {
            SessionTransition::Disable
        } else {
            SessionTransition::None
        };
        self.focus = None;
        self.enabled = false;
        transition
    }

    fn accepts_done(self, surface: SurfaceId) -> bool {
        self.enabled && self.focus == Some(surface)
    }
}

/// One seat-scoped text-input-v3 session.
///
/// Keeping the proxy and its state together prevents the runtime orchestrator
/// from duplicating focus, enable and destruction rules.
#[derive(Debug)]
pub(crate) struct SeatTextInput {
    proxy: ZwpTextInputV3,
    state: SessionState,
}

impl SeatTextInput {
    pub(crate) fn new(proxy: ZwpTextInputV3) -> Self {
        Self {
            proxy,
            state: SessionState::default(),
        }
    }

    pub(crate) fn matches(&self, proxy: &ZwpTextInputV3) -> bool {
        self.proxy.id() == proxy.id()
    }

    pub(crate) fn enter(&mut self, surface: SurfaceId, state: Option<&TextInputState>) {
        let transition = self.state.enter(surface, state.is_some());
        self.apply_transition(transition, state);
    }

    pub(crate) fn leave(&mut self) {
        let transition = self.state.leave();
        self.apply_transition(transition, None);
    }

    pub(crate) fn update(&mut self, surface: SurfaceId, state: Option<&TextInputState>) {
        let transition = self.state.update(surface, state.is_some());
        self.apply_transition(transition, state);
    }

    pub(crate) fn remove_surface(&mut self, surface: SurfaceId) {
        let transition = self.state.remove_surface(surface);
        self.apply_transition(transition, None);
    }

    pub(crate) fn accepts_done(&self, proxy: &ZwpTextInputV3, surface: SurfaceId) -> bool {
        self.matches(proxy) && self.state.accepts_done(surface)
    }

    fn apply_transition(&self, transition: SessionTransition, state: Option<&TextInputState>) {
        match transition {
            SessionTransition::None => {}
            SessionTransition::Apply { enable } => apply_state(
                &self.proxy,
                state.expect("apply transition requires text input state"),
                enable,
            ),
            SessionTransition::Disable => disable(&self.proxy),
        }
    }
}

impl Drop for SeatTextInput {
    fn drop(&mut self) {
        if self.proxy.is_alive() {
            self.proxy.destroy();
        }
    }
}

fn apply_state(text_input: &ZwpTextInputV3, state: &TextInputState, enable: bool) {
    if enable {
        text_input.enable();
    }
    if let Some(surrounding) = state.surrounding_text() {
        text_input.set_surrounding_text(
            surrounding.text().to_string(),
            surrounding.cursor() as i32,
            surrounding.anchor() as i32,
        );
    }
    text_input.set_text_change_cause(protocol_change_cause(state.change_cause()));
    if let Some(content_type) = state.content_type() {
        text_input.set_content_type(
            protocol_content_hint(content_type.hints),
            protocol_content_purpose(content_type.purpose),
        );
    }
    if let Some(rectangle) = state.cursor_rectangle() {
        text_input.set_cursor_rectangle(
            rectangle.origin.x,
            rectangle.origin.y,
            rectangle.size.width as i32,
            rectangle.size.height as i32,
        );
    }
    text_input.commit();
}

fn disable(text_input: &ZwpTextInputV3) {
    text_input.disable();
    text_input.commit();
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

fn valid_preedit_offset(text: &str, offset: i32) -> Option<usize> {
    usize::try_from(offset)
        .ok()
        .filter(|offset| *offset <= text.len() && text.is_char_boundary(*offset))
}

fn protocol_change_cause(cause: TextInputChangeCause) -> ChangeCause {
    match cause {
        TextInputChangeCause::InputMethod => ChangeCause::InputMethod,
        TextInputChangeCause::Other => ChangeCause::Other,
    }
}

fn protocol_content_purpose(purpose: TextInputContentPurpose) -> ContentPurpose {
    match purpose {
        TextInputContentPurpose::Normal => ContentPurpose::Normal,
        TextInputContentPurpose::Alpha => ContentPurpose::Alpha,
        TextInputContentPurpose::Digits => ContentPurpose::Digits,
        TextInputContentPurpose::Number => ContentPurpose::Number,
        TextInputContentPurpose::Phone => ContentPurpose::Phone,
        TextInputContentPurpose::Url => ContentPurpose::Url,
        TextInputContentPurpose::Email => ContentPurpose::Email,
        TextInputContentPurpose::Name => ContentPurpose::Name,
        TextInputContentPurpose::Password => ContentPurpose::Password,
        TextInputContentPurpose::Pin => ContentPurpose::Pin,
        TextInputContentPurpose::Date => ContentPurpose::Date,
        TextInputContentPurpose::Time => ContentPurpose::Time,
        TextInputContentPurpose::DateTime => ContentPurpose::Datetime,
        TextInputContentPurpose::Terminal => ContentPurpose::Terminal,
    }
}

fn protocol_content_hint(hints: TextInputContentHint) -> ContentHint {
    let mut protocol = ContentHint::None;
    for (hint, value) in [
        (TextInputContentHint::COMPLETION, ContentHint::Completion),
        (TextInputContentHint::SPELLCHECK, ContentHint::Spellcheck),
        (
            TextInputContentHint::AUTO_CAPITALIZATION,
            ContentHint::AutoCapitalization,
        ),
        (TextInputContentHint::LOWERCASE, ContentHint::Lowercase),
        (TextInputContentHint::UPPERCASE, ContentHint::Uppercase),
        (TextInputContentHint::TITLECASE, ContentHint::Titlecase),
        (TextInputContentHint::HIDDEN_TEXT, ContentHint::HiddenText),
        (
            TextInputContentHint::SENSITIVE_DATA,
            ContentHint::SensitiveData,
        ),
        (TextInputContentHint::LATIN, ContentHint::Latin),
        (TextInputContentHint::MULTILINE, ContentHint::Multiline),
    ] {
        if hints.contains(hint) {
            protocol |= value;
        }
    }
    protocol
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surrounding_text_enforces_wire_length_and_utf8_boundaries() {
        assert!(TextInputSurroundingText::new("aβc", 3, 3).is_ok());
        assert_eq!(
            TextInputSurroundingText::new("aβc", 2, 3),
            Err(TextInputError::SurroundingOffsetSplitsCodepoint)
        );
        assert_eq!(
            TextInputSurroundingText::new("x".repeat(4_001), 0, 0),
            Err(TextInputError::SurroundingTextTooLong)
        );
    }

    #[test]
    fn preedit_cursor_offsets_are_validated_as_utf8_bytes() {
        let mut batch = PendingBatch::default();
        batch.set_preedit("aβc".to_string(), 1, 3);
        assert_eq!(batch.preedit.unwrap().cursor_range, Some(1..3));

        let mut batch = PendingBatch::default();
        batch.set_preedit("aβc".to_string(), 2, 3);
        assert_eq!(batch.preedit.unwrap().cursor_range, None);

        let mut batch = PendingBatch::default();
        batch.set_preedit("aβc".to_string(), 1, 2);
        assert_eq!(batch.preedit.unwrap().cursor_range, None);
    }

    #[test]
    fn cursor_rectangle_must_be_nonempty_and_wire_sized() {
        assert_eq!(
            TextInputState::new().with_cursor_rectangle(LogicalRect::new(0, 0, 0, 1)),
            Err(TextInputError::EmptyCursorRectangle)
        );
        assert!(
            TextInputState::new()
                .with_cursor_rectangle(LogicalRect::new(10, 20, 2, 18))
                .is_ok()
        );
    }

    #[test]
    fn content_hints_preserve_all_public_flags() {
        let all = TextInputContentHint::all();
        assert_eq!(protocol_content_hint(all).bits(), u32::from(all.bits()));
    }

    #[test]
    fn session_state_enables_updates_and_disables_without_redundant_transitions() {
        let surface = SurfaceId(7);
        let other = SurfaceId(8);
        let mut session = SessionState::default();

        assert_eq!(session.enter(surface, false), SessionTransition::None);
        assert_eq!(session.update(other, true), SessionTransition::None);
        assert_eq!(
            session.update(surface, true),
            SessionTransition::Apply { enable: true }
        );
        assert_eq!(
            session.update(surface, true),
            SessionTransition::Apply { enable: false }
        );
        assert!(session.accepts_done(surface));
        assert_eq!(session.update(surface, false), SessionTransition::Disable);
        assert_eq!(session.update(surface, false), SessionTransition::None);
        assert!(!session.accepts_done(surface));
    }

    #[test]
    fn session_state_resets_on_leave_and_surface_removal() {
        let surface = SurfaceId(3);
        let mut session = SessionState::default();

        assert_eq!(
            session.enter(surface, true),
            SessionTransition::Apply { enable: true }
        );
        assert_eq!(session.leave(), SessionTransition::Disable);
        assert_eq!(session.focus, None);

        assert_eq!(session.enter(surface, false), SessionTransition::None);
        assert_eq!(session.remove_surface(surface), SessionTransition::None);
        assert_eq!(session.focus, None);
    }
}
