//! Tensor-owned text-input-v3 and input-method-v2 authority.
//!
//! Text clients and the single input method exchange only through this
//! compositor-thread state machine. Commit/done serials, UTF-8 byte indices,
//! activation, and focus are validated here rather than delegated to an
//! adapter library.

mod lifecycle;
mod wire;

use std::collections::{HashMap, HashSet};

use tensor_util::LogicalRect;
use tracing::info;
use wayland_protocols::wp::text_input::zv3::server::zwp_text_input_v3::{
    ChangeCause, ContentHint, ContentPurpose, ZwpTextInputV3,
};
use wayland_protocols_misc::zwp_input_method_v2::server::{
    zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2,
    zwp_input_method_v2::ZwpInputMethodV2, zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2,
};
use wayland_server::{
    DisplayHandle, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::{wl_keyboard, wl_surface::WlSurface},
};

use crate::protocol::{seat::ModifiersState, serial::Serial};

pub(in crate::protocol) const INPUT_POPUP_SURFACE_ROLE: &str = "zwp_input_popup_surface_v2";
const TEXT_INPUT_VERSION: u32 = 1;
const INPUT_METHOD_VERSION: u32 = 1;
const MAX_TEXT_BYTES: usize = 4_000;
const MAX_INPUT_POPUPS: usize = 16;
const MAX_INPUT_METHOD_KEYBOARD_GRABS: usize = 16;

#[derive(Clone, Debug)]
struct SurroundingText {
    text: Box<str>,
    cursor: u32,
    anchor: u32,
}

#[derive(Clone, Debug, Default)]
struct TextInputState {
    surrounding: Option<SurroundingText>,
    change_cause: Option<ChangeCause>,
    content_type: Option<(ContentHint, ContentPurpose)>,
    cursor_rectangle: Option<LogicalRect<i32>>,
}

#[derive(Clone, Copy, Debug, Default)]
struct TextStateChanges {
    surrounding: bool,
    change_cause: bool,
    content_type: bool,
    cursor_rectangle: bool,
}

impl TextStateChanges {
    fn present(state: &TextInputState) -> Self {
        Self {
            surrounding: state.surrounding.is_some(),
            change_cause: state.change_cause.is_some(),
            content_type: state.content_type.is_some(),
            cursor_rectangle: state.cursor_rectangle.is_some(),
        }
    }
}

#[derive(Debug, Default)]
struct PendingTextInputState {
    enabled: Option<bool>,
    state: TextInputState,
}

#[derive(Debug)]
struct TextInputInstance {
    resource: Weak<ZwpTextInputV3>,
    client: ClientId,
    serial: u32,
    pending: PendingTextInputState,
    current: TextInputState,
}

#[derive(Debug, Default)]
struct PendingInputMethodState {
    commit_string: Option<Box<str>>,
    preedit: Option<(Box<str>, i32, i32)>,
    delete_surrounding: Option<(u32, u32)>,
}

#[derive(Debug)]
struct InputMethodInstance {
    resource: Weak<ZwpInputMethodV2>,
    serial: u32,
    pending: PendingInputMethodState,
}

#[derive(Clone, Debug)]
pub(crate) struct InputPopupSurface {
    role: Weak<ZwpInputPopupSurfaceV2>,
    surface: WlSurface,
    owner: ObjectId,
}

#[derive(Clone, Debug)]
struct InputMethodKeyboardGrab {
    resource: Weak<ZwpInputMethodKeyboardGrabV2>,
    owner: ObjectId,
}

impl InputPopupSurface {
    pub(crate) fn wl_surface(&self) -> &WlSurface {
        &self.surface
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextCommitDisposition {
    Ignored,
    Activated,
    Deactivated,
    Updated,
}

#[derive(Debug)]
pub(crate) struct InputMethodProtocol {
    _text_input_global: GlobalId,
    _input_method_global: GlobalId,
    text_inputs: HashMap<ObjectId, TextInputInstance>,
    focused_surface: Option<Weak<WlSurface>>,
    active_text_input: Option<ObjectId>,
    input_method: Option<InputMethodInstance>,
    unavailable_input_methods: HashSet<ObjectId>,
    // Creation order is the popup stacking order. Lifecycle operations are
    // cold and the list is normally one element; startup reserves the hard
    // limit so role creation never grows this storage.
    popups: Vec<InputPopupSurface>,
    keyboard_grabs: Vec<InputMethodKeyboardGrab>,
    active_keyboard_grab: Option<Weak<ZwpInputMethodKeyboardGrabV2>>,
}

impl InputMethodProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _text_input_global: wire::create_text_input_global(display, TEXT_INPUT_VERSION),
            _input_method_global: wire::create_input_method_global(display, INPUT_METHOD_VERSION),
            text_inputs: HashMap::new(),
            focused_surface: None,
            active_text_input: None,
            input_method: None,
            unavailable_input_methods: HashSet::new(),
            popups: Vec::with_capacity(MAX_INPUT_POPUPS),
            keyboard_grabs: Vec::with_capacity(MAX_INPUT_METHOD_KEYBOARD_GRABS),
            active_keyboard_grab: None,
        }
    }

    pub(crate) fn forward_key(
        &self,
        key: u32,
        pressed: bool,
        serial: Serial,
        time: u32,
        modifiers: Option<ModifiersState>,
    ) -> bool {
        let Some(grab) = self
            .active_keyboard_grab
            .as_ref()
            .and_then(|grab| grab.upgrade().ok())
        else {
            return false;
        };
        if let Some(modifiers) = modifiers {
            let modifiers = modifiers.serialized;
            grab.modifiers(
                serial.into(),
                modifiers.depressed,
                modifiers.latched,
                modifiers.locked,
                modifiers.layout,
            );
        }
        grab.key(
            serial.into(),
            time,
            key,
            if pressed {
                wl_keyboard::KeyState::Pressed
            } else {
                wl_keyboard::KeyState::Released
            },
        );
        true
    }

    fn reset_text_pending(&mut self, resource: &ZwpTextInputV3, enabled: bool) {
        if let Some(instance) = self.text_inputs.get_mut(&resource.id()) {
            instance.pending = PendingTextInputState {
                enabled: Some(enabled),
                state: TextInputState::default(),
            };
        }
    }

    fn set_surrounding_text(
        &mut self,
        resource: &ZwpTextInputV3,
        text: String,
        cursor: i32,
        anchor: i32,
    ) -> bool {
        let Some((cursor, anchor)) = valid_surrounding_indices(&text, cursor, anchor) else {
            return false;
        };
        if let Some(instance) = self.text_inputs.get_mut(&resource.id()) {
            instance.pending.state.surrounding = Some(SurroundingText {
                text: text.into_boxed_str(),
                cursor,
                anchor,
            });
        }
        true
    }

    fn set_change_cause(&mut self, resource: &ZwpTextInputV3, cause: ChangeCause) {
        if let Some(instance) = self.text_inputs.get_mut(&resource.id()) {
            instance.pending.state.change_cause = Some(cause);
        }
    }

    fn set_content_type(
        &mut self,
        resource: &ZwpTextInputV3,
        hint: ContentHint,
        purpose: ContentPurpose,
    ) {
        if let Some(instance) = self.text_inputs.get_mut(&resource.id()) {
            instance.pending.state.content_type = Some((hint, purpose));
        }
    }

    fn set_cursor_rectangle(&mut self, resource: &ZwpTextInputV3, rect: LogicalRect<i32>) -> bool {
        if rect.size.w < 0 || rect.size.h < 0 {
            return false;
        }
        if let Some(instance) = self.text_inputs.get_mut(&resource.id()) {
            instance.pending.state.cursor_rectangle = Some(rect);
        }
        true
    }

    fn commit_text_input(&mut self, resource: &ZwpTextInputV3) -> TextCommitDisposition {
        let id = resource.id();
        let Some(focused_client) = self.focused_client() else {
            if let Some(instance) = self.text_inputs.get_mut(&id) {
                instance.serial = instance.serial.wrapping_add(1);
                instance.pending = PendingTextInputState::default();
            }
            return TextCommitDisposition::Ignored;
        };
        let Some(instance) = self.text_inputs.get_mut(&id) else {
            return TextCommitDisposition::Ignored;
        };
        instance.serial = instance.serial.wrapping_add(1);
        let pending = std::mem::take(&mut instance.pending);
        if instance.client != focused_client {
            return TextCommitDisposition::Ignored;
        }
        match pending.enabled {
            Some(true)
                if self
                    .active_text_input
                    .as_ref()
                    .is_some_and(|active| active != &id) =>
            {
                TextCommitDisposition::Ignored
            }
            Some(true) => {
                instance.current = pending.state;
                self.active_text_input = Some(id);
                let input_method_registered = self.input_method_resource().is_some();
                self.activate_input_method();
                if input_method_registered {
                    info!("text input activated for the registered input method");
                } else {
                    info!("text input activated without a registered input method");
                }
                TextCommitDisposition::Activated
            }
            Some(false) => {
                instance.current = TextInputState::default();
                if self.active_text_input.as_ref() == Some(&id) {
                    self.active_text_input = None;
                    self.deactivate_input_method();
                    TextCommitDisposition::Deactivated
                } else {
                    TextCommitDisposition::Ignored
                }
            }
            None if self.active_text_input.as_ref() == Some(&id) => {
                let changes = TextStateChanges::present(&pending.state);
                merge_text_state(&mut instance.current, pending.state);
                self.update_input_method(changes);
                TextCommitDisposition::Updated
            }
            None => TextCommitDisposition::Ignored,
        }
    }

    fn activate_input_method(&mut self) {
        let Some(resource) = self.input_method_resource() else {
            return;
        };
        if let Some(instance) = self.input_method.as_mut() {
            // Requests made while inactive must be accepted, but activation
            // resets them so they cannot affect the new text-input context.
            instance.pending = PendingInputMethodState::default();
        }
        resource.activate();
        let changes = self
            .active_text_input
            .as_ref()
            .and_then(|id| self.text_inputs.get(id))
            .map(|instance| TextStateChanges::present(&instance.current))
            .unwrap_or_default();
        self.send_current_text_state(&resource, changes);
        self.finish_input_method_update(&resource);
    }

    fn update_input_method(&mut self, changes: TextStateChanges) {
        let Some(resource) = self.input_method_resource() else {
            return;
        };
        self.send_current_text_state(&resource, changes);
        self.finish_input_method_update(&resource);
    }

    fn deactivate_input_method(&mut self) {
        let Some(resource) = self.input_method_resource() else {
            return;
        };
        resource.deactivate();
        self.finish_input_method_update(&resource);
    }

    fn send_current_text_state(&self, resource: &ZwpInputMethodV2, changes: TextStateChanges) {
        let Some(id) = self.active_text_input.as_ref() else {
            return;
        };
        let Some(state) = self.text_inputs.get(id).map(|instance| &instance.current) else {
            return;
        };
        if changes.surrounding
            && let Some(surrounding) = &state.surrounding
        {
            resource.surrounding_text(
                surrounding.text.to_string(),
                surrounding.cursor,
                surrounding.anchor,
            );
        }
        if changes.change_cause
            && let Some(cause) = state.change_cause
        {
            resource.text_change_cause(cause);
        }
        if changes.content_type
            && let Some((hint, purpose)) = state.content_type
        {
            resource.content_type(hint, purpose);
        }
        if changes.cursor_rectangle
            && let Some(rect) = state.cursor_rectangle
        {
            for popup in &self.popups {
                if let Ok(role) = popup.role.upgrade() {
                    role.text_input_rectangle(rect.loc.x, rect.loc.y, rect.size.w, rect.size.h);
                }
            }
        }
    }

    fn finish_input_method_update(&mut self, resource: &ZwpInputMethodV2) {
        resource.done();
        if let Some(instance) = self.input_method.as_mut() {
            instance.serial = instance.serial.wrapping_add(1);
        }
    }

    fn set_commit_string(&mut self, resource: &ZwpInputMethodV2, text: String) -> bool {
        if !self.input_method_available(resource) {
            return true;
        }
        if text.len() > MAX_TEXT_BYTES {
            return false;
        }
        if let Some(instance) = self.input_method.as_mut() {
            instance.pending.commit_string = Some(text.into_boxed_str());
        }
        true
    }

    fn set_preedit(
        &mut self,
        resource: &ZwpInputMethodV2,
        text: String,
        cursor_begin: i32,
        cursor_end: i32,
    ) -> bool {
        if !self.input_method_available(resource) {
            return true;
        }
        if !valid_preedit_indices(&text, cursor_begin, cursor_end) {
            return false;
        }
        if let Some(instance) = self.input_method.as_mut() {
            instance.pending.preedit = Some((text.into_boxed_str(), cursor_begin, cursor_end));
        }
        true
    }

    fn set_delete_surrounding(&mut self, resource: &ZwpInputMethodV2, before: u32, after: u32) {
        if self.input_method_available(resource)
            && let Some(instance) = self.input_method.as_mut()
        {
            instance.pending.delete_surrounding = Some((before, after));
        }
    }

    fn commit_input_method(&mut self, resource: &ZwpInputMethodV2, serial: u32) {
        if !self.input_method_available(resource) {
            return;
        }
        let Some(instance) = self.input_method.as_mut() else {
            return;
        };
        let current_serial = instance.serial;
        let pending = std::mem::take(&mut instance.pending);
        let Some(text_input) = self.active_text_input_resource() else {
            return;
        };
        if let Some((before, after)) = pending.delete_surrounding {
            text_input.delete_surrounding_text(before, after);
        }
        if let Some(text) = pending.commit_string {
            text_input.commit_string(Some(text.into_string()));
        }
        if let Some((text, begin, end)) = pending.preedit {
            text_input.preedit_string(Some(text.into_string()), begin, end);
        }
        let text_serial = self
            .text_inputs
            .get(&text_input.id())
            .map(|instance| instance.serial)
            .unwrap_or_default();
        let done_serial = if serial == current_serial {
            text_serial
        } else {
            text_serial.wrapping_add(1)
        };
        text_input.done(done_serial);
    }
}

fn merge_text_state(current: &mut TextInputState, pending: TextInputState) {
    if pending.surrounding.is_some() {
        current.surrounding = pending.surrounding;
    }
    if pending.change_cause.is_some() {
        current.change_cause = pending.change_cause;
    }
    if pending.content_type.is_some() {
        current.content_type = pending.content_type;
    }
    if pending.cursor_rectangle.is_some() {
        current.cursor_rectangle = pending.cursor_rectangle;
    }
}

fn valid_surrounding_indices(text: &str, cursor: i32, anchor: i32) -> Option<(u32, u32)> {
    if text.len() > MAX_TEXT_BYTES {
        return None;
    }
    let cursor = usize::try_from(cursor).ok()?;
    let anchor = usize::try_from(anchor).ok()?;
    if cursor > text.len()
        || anchor > text.len()
        || !text.is_char_boundary(cursor)
        || !text.is_char_boundary(anchor)
    {
        return None;
    }
    Some((u32::try_from(cursor).ok()?, u32::try_from(anchor).ok()?))
}

fn valid_preedit_indices(text: &str, begin: i32, end: i32) -> bool {
    if text.len() > MAX_TEXT_BYTES || (begin == -1 && end == -1) {
        return text.len() <= MAX_TEXT_BYTES && begin == -1 && end == -1;
    }
    let (Ok(begin), Ok(end)) = (usize::try_from(begin), usize::try_from(end)) else {
        return false;
    };
    begin <= text.len()
        && end <= text.len()
        && text.is_char_boundary(begin)
        && text.is_char_boundary(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surrounding_indices_are_utf8_byte_boundaries() {
        assert_eq!(valid_surrounding_indices("a中b", 4, 1), Some((4, 1)));
        assert_eq!(valid_surrounding_indices("a中b", 2, 1), None);
        assert_eq!(valid_surrounding_indices("a", -1, 0), None);
        assert!(valid_surrounding_indices(&"x".repeat(MAX_TEXT_BYTES), 0, 0).is_some());
        assert!(valid_surrounding_indices(&"x".repeat(MAX_TEXT_BYTES + 1), 0, 0).is_none());
    }

    #[test]
    fn preedit_cursor_accepts_hidden_or_valid_utf8_boundaries() {
        assert!(valid_preedit_indices("中", -1, -1));
        assert!(valid_preedit_indices("中", 0, 3));
        assert!(!valid_preedit_indices("中", 1, 3));
        assert!(!valid_preedit_indices("中", -1, 0));
        assert!(valid_preedit_indices(&"x".repeat(MAX_TEXT_BYTES), 0, 0));
        assert!(!valid_preedit_indices(
            &"x".repeat(MAX_TEXT_BYTES + 1),
            0,
            0
        ));
    }
}
