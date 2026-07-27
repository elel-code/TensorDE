//! text-input-v3 methods for [`NativeShell`].

use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    self, ContentHint as WireContentHint, ContentPurpose as WireContentPurpose,
};

use super::api::NativeShell;
use super::types::NativeSurfaceId;
use crate::native::connection::NativeError;
use crate::text_input::{
    TextInputChangeCause, TextInputContentHint, TextInputContentPurpose, TextInputState,
};

impl NativeShell {
    pub fn has_text_input(&self) -> bool {
        self.state.text_input.is_some()
    }

    /// Enable text-input-v3 and push the full client editor state.
    ///
    /// Applies surrounding text, content type, change cause, and cursor
    /// rectangle (surface-local logical coordinates). The hard-coded
    /// `(0,0,1,1)` placeholder used during early native bring-up is gone —
    /// compositors were docking the IME popup at the surface origin.
    pub fn set_text_input_state(
        &mut self,
        surface: NativeSurfaceId,
        state: &TextInputState,
    ) -> Result<(), NativeError> {
        let ti = self
            .state
            .text_input
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("text_input_v3 missing".into()))?
            .clone();
        let wl = self
            .state
            .toplevels
            .get(&surface)
            .map(|t| t.wl.clone())
            .or_else(|| self.state.popups.get(&surface).map(|p| p.wl.clone()))
            .or_else(|| self.state.layers.get(&surface).map(|l| l.wl.clone()))
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {surface:?}")))?;

        ti.enable();
        self.state.text_input_surface = Some(surface);

        if let Some(surrounding) = state.surrounding_text() {
            ti.set_surrounding_text(
                surrounding.text().to_string(),
                surrounding.cursor() as i32,
                surrounding.anchor() as i32,
            );
        }
        ti.set_text_change_cause(match state.change_cause() {
            TextInputChangeCause::InputMethod => zwp_text_input_v3::ChangeCause::InputMethod,
            TextInputChangeCause::Other => zwp_text_input_v3::ChangeCause::Other,
        });
        if let Some(content) = state.content_type() {
            ti.set_content_type(
                content_hint_to_wire(content.hints),
                content_purpose_to_wire(content.purpose),
            );
        }
        if let Some(rect) = state.cursor_rectangle() {
            ti.set_cursor_rectangle(
                rect.origin.x,
                rect.origin.y,
                rect.size.width.max(1) as i32,
                rect.size.height.max(1) as i32,
            );
        }

        // Double-buffered: text-input commit applies state; on protocol v2+
        // the cursor rectangle is further applied on the next wl_surface.commit.
        ti.commit();
        wl.commit();
        self.connection.mark_dirty();
        Ok(())
    }

    /// Enable text-input-v3 on `surface` without editor state (legacy helper).
    pub fn enable_text_input(&mut self, surface: NativeSurfaceId) -> Result<(), NativeError> {
        self.set_text_input_state(surface, &TextInputState::new())
    }

    /// Disable text-input-v3 for the seat.
    pub fn disable_text_input(&mut self) -> Result<(), NativeError> {
        let ti = self
            .state
            .text_input
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("text_input_v3 missing".into()))?;
        ti.disable();
        ti.commit();
        self.state.text_input_surface = None;
        self.connection.mark_dirty();
        Ok(())
    }
}

fn content_hint_to_wire(hints: TextInputContentHint) -> WireContentHint {
    let mut wire = WireContentHint::empty();
    if hints.contains(TextInputContentHint::COMPLETION) {
        wire |= WireContentHint::Completion;
    }
    if hints.contains(TextInputContentHint::SPELLCHECK) {
        wire |= WireContentHint::Spellcheck;
    }
    if hints.contains(TextInputContentHint::AUTO_CAPITALIZATION) {
        wire |= WireContentHint::AutoCapitalization;
    }
    if hints.contains(TextInputContentHint::LOWERCASE) {
        wire |= WireContentHint::Lowercase;
    }
    if hints.contains(TextInputContentHint::UPPERCASE) {
        wire |= WireContentHint::Uppercase;
    }
    if hints.contains(TextInputContentHint::TITLECASE) {
        wire |= WireContentHint::Titlecase;
    }
    if hints.contains(TextInputContentHint::HIDDEN_TEXT) {
        wire |= WireContentHint::HiddenText;
    }
    if hints.contains(TextInputContentHint::SENSITIVE_DATA) {
        wire |= WireContentHint::SensitiveData;
    }
    if hints.contains(TextInputContentHint::LATIN) {
        wire |= WireContentHint::Latin;
    }
    if hints.contains(TextInputContentHint::MULTILINE) {
        wire |= WireContentHint::Multiline;
    }
    wire
}

fn content_purpose_to_wire(purpose: TextInputContentPurpose) -> WireContentPurpose {
    match purpose {
        TextInputContentPurpose::Normal => WireContentPurpose::Normal,
        TextInputContentPurpose::Alpha => WireContentPurpose::Alpha,
        TextInputContentPurpose::Digits => WireContentPurpose::Digits,
        TextInputContentPurpose::Number => WireContentPurpose::Number,
        TextInputContentPurpose::Phone => WireContentPurpose::Phone,
        TextInputContentPurpose::Url => WireContentPurpose::Url,
        TextInputContentPurpose::Email => WireContentPurpose::Email,
        TextInputContentPurpose::Name => WireContentPurpose::Name,
        TextInputContentPurpose::Password => WireContentPurpose::Password,
        TextInputContentPurpose::Pin => WireContentPurpose::Pin,
        TextInputContentPurpose::Date => WireContentPurpose::Date,
        TextInputContentPurpose::Time => WireContentPurpose::Time,
        TextInputContentPurpose::DateTime => WireContentPurpose::Datetime,
        TextInputContentPurpose::Terminal => WireContentPurpose::Terminal,
    }
}
