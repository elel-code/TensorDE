use super::{
    ButtonSource, CursorIcon, DndAction, ElementState, Key, KeyCode, KeyEvent, MouseButton,
    MouseScrollDelta, NamedKey, NativeKey, NativeKeyCode, PhysicalKey, PhysicalPosition,
    PhysicalSize, WindowState,
};
use wayland_client_runtime::{
    CursorIcon as RuntimeCursorIcon, DndAction as RuntimeDndAction,
    DndActions as RuntimeDndActions, KeyState, LogicalPosition, LogicalSize, PointerAxisValue,
};

pub(super) fn normalize_wayland_scale_factor(scale_factor: f64) -> f64 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

pub(super) fn logical_to_physical_rounded(
    size: LogicalSize,
    scale_factor: f64,
) -> PhysicalSize<u32> {
    let scale_factor = normalize_wayland_scale_factor(scale_factor);
    PhysicalSize::new(
        scaled_dimension(size.width, scale_factor),
        scaled_dimension(size.height, scale_factor),
    )
}

pub(super) fn apply_configured_logical_size(
    state: &mut WindowState,
    logical_size: LogicalSize,
) -> (PhysicalSize<u32>, bool, bool) {
    let physical_size = logical_to_physical_rounded(logical_size, state.scale_factor);
    let surface_state_changed = !state.configured
        || logical_size != state.logical_size
        || physical_size != state.physical_size;
    let resized = !state.configured || physical_size != state.physical_size;
    state.logical_size = logical_size;
    state.physical_size = physical_size;
    state.configured = true;
    state.redraw_requested = true;
    (physical_size, surface_state_changed, resized)
}

pub(super) fn physical_to_logical_rounded(
    size: PhysicalSize<u32>,
    scale_factor: f64,
) -> LogicalSize {
    let scale_factor = normalize_wayland_scale_factor(scale_factor);
    LogicalSize::new(
        scaled_dimension(size.width, scale_factor.recip()),
        scaled_dimension(size.height, scale_factor.recip()),
    )
}

pub(super) fn scaled_dimension(value: u32, scale_factor: f64) -> u32 {
    (f64::from(value) * scale_factor)
        .round()
        .clamp(1.0, f64::from(u32::MAX)) as u32
}

pub(super) fn integer_buffer_scale(scale_factor: f64) -> i32 {
    normalize_wayland_scale_factor(scale_factor)
        .round()
        .clamp(1.0, f64::from(i32::MAX)) as i32
}

pub(super) fn scale_dnd_position(position: LogicalPosition, scale: f64) -> PhysicalPosition<f64> {
    PhysicalPosition::new(position.x as f64 * scale, position.y as f64 * scale)
}

pub(super) fn runtime_dnd_actions(actions: &[DndAction]) -> RuntimeDndActions {
    let mut mapped = RuntimeDndActions::empty();
    for action in actions {
        mapped |= match action {
            DndAction::Copy => RuntimeDndActions::COPY,
            DndAction::Move => RuntimeDndActions::MOVE,
            DndAction::Ask => RuntimeDndActions::ASK,
        };
    }
    mapped
}

pub(super) fn preferred_runtime_dnd_action(actions: &[DndAction]) -> Option<RuntimeDndAction> {
    if actions.contains(&DndAction::Ask) {
        Some(RuntimeDndAction::Ask)
    } else if actions.contains(&DndAction::Move) {
        Some(RuntimeDndAction::Move)
    } else if actions.contains(&DndAction::Copy) {
        Some(RuntimeDndAction::Copy)
    } else {
        None
    }
}

pub(super) fn dnd_action_from_runtime(action: RuntimeDndAction) -> DndAction {
    match action {
        RuntimeDndAction::Copy => DndAction::Copy,
        RuntimeDndAction::Move => DndAction::Move,
        RuntimeDndAction::Ask => DndAction::Ask,
    }
}

pub(super) fn runtime_cursor_icon(icon: CursorIcon) -> RuntimeCursorIcon {
    match icon {
        CursorIcon::ColResize => RuntimeCursorIcon::ColResize,
        CursorIcon::Default => RuntimeCursorIcon::Default,
        CursorIcon::Pointer => RuntimeCursorIcon::Pointer,
        CursorIcon::Text => RuntimeCursorIcon::Text,
    }
}

/// Map a framed Wayland pointer axis into Tensor Files scroll vocabulary.
///
/// Prefer `axis_value120` / discrete logical steps (high-resolution wheels). Fall
/// back to continuous compositor coordinates scaled into physical pixels
/// (touchpads and continuous devices). Sign matches the historical continuous
/// path: UI consumers negate again to obtain content scroll direction.
pub(super) fn map_pointer_axis_to_scroll_delta(
    horizontal: PointerAxisValue,
    vertical: PointerAxisValue,
    scale_factor: f64,
) -> MouseScrollDelta {
    let scale_factor = normalize_wayland_scale_factor(scale_factor);
    let horizontal_steps = horizontal.logical_steps();
    let vertical_steps = vertical.logical_steps();
    if horizontal_steps.is_some() || vertical_steps.is_some() {
        return MouseScrollDelta::LineDelta {
            x: -horizontal_steps.unwrap_or(0.0),
            y: -vertical_steps.unwrap_or(0.0),
        };
    }
    MouseScrollDelta::PixelDelta(PhysicalPosition::new(
        -horizontal.continuous * scale_factor,
        -vertical.continuous * scale_factor,
    ))
}

pub(super) fn linux_button(button: u32) -> ButtonSource {
    let button = match button {
        0x110 => MouseButton::Left,
        0x111 => MouseButton::Right,
        0x112 => MouseButton::Middle,
        0x113 => MouseButton::Back,
        0x114 => MouseButton::Forward,
        value => return ButtonSource::Unknown(value),
    };
    ButtonSource::Mouse(button)
}

pub(super) fn translate_key_event(
    state: KeyState,
    raw_code: u32,
    keysym: u32,
    text: Option<String>,
) -> KeyEvent {
    let logical_key = logical_key(keysym, text.as_deref());
    KeyEvent {
        physical_key: physical_key(raw_code),
        key_without_modifiers: logical_key.clone(),
        logical_key,
        state: match state {
            KeyState::Pressed | KeyState::Repeated => ElementState::Pressed,
            KeyState::Released => ElementState::Released,
        },
        repeat: state == KeyState::Repeated,
        text,
    }
}

pub(super) fn logical_key(keysym: u32, text: Option<&str>) -> Key {
    use xkeysym::key;

    let named = match keysym {
        key::BackSpace => Some(NamedKey::Backspace),
        key::Tab | key::ISO_Left_Tab => Some(NamedKey::Tab),
        key::Return | key::KP_Enter => Some(NamedKey::Enter),
        key::Escape => Some(NamedKey::Escape),
        key::Delete | key::KP_Delete => Some(NamedKey::Delete),
        key::Home | key::KP_Home => Some(NamedKey::Home),
        key::Left | key::KP_Left => Some(NamedKey::ArrowLeft),
        key::Up | key::KP_Up => Some(NamedKey::ArrowUp),
        key::Right | key::KP_Right => Some(NamedKey::ArrowRight),
        key::Down | key::KP_Down => Some(NamedKey::ArrowDown),
        key::Page_Up | key::KP_Page_Up => Some(NamedKey::PageUp),
        key::Page_Down | key::KP_Page_Down => Some(NamedKey::PageDown),
        key::End | key::KP_End => Some(NamedKey::End),
        key::F1 => Some(NamedKey::F1),
        key::F2 => Some(NamedKey::F2),
        key::F3 => Some(NamedKey::F3),
        key::F5 => Some(NamedKey::F5),
        key::F6 => Some(NamedKey::F6),
        _ => None,
    };
    if let Some(named) = named {
        Key::Named(named)
    } else if let Some(text) = text.filter(|value| !value.is_empty()) {
        Key::Character(text.to_string())
    } else if let Some(character) = xkeysym::Keysym::new(keysym).key_char() {
        Key::Character(character.to_string())
    } else {
        Key::Unidentified(NativeKey::Unidentified)
    }
}

pub(super) fn physical_key(raw_code: u32) -> PhysicalKey {
    // SCTK / wl_keyboard deliver Linux evdev keycodes. Do not subtract 8 here:
    // that offset is only used when converting *to* XKB keycodes (see SCTK's
    // `KeyCode::new(raw_code + 8)`). Subtracting maps Ctrl+C (46) to KeyL (38)
    // and steals the address-bar shortcut.
    let code = match raw_code {
        1 => KeyCode::Escape,
        2 => KeyCode::Digit1,
        3 => KeyCode::Digit2,
        4 => KeyCode::Digit3,
        14 => KeyCode::Backspace,
        15 => KeyCode::Tab,
        19 => KeyCode::KeyR,
        30 => KeyCode::KeyA,
        32 => KeyCode::KeyD,
        33 => KeyCode::KeyF,
        35 => KeyCode::KeyH,
        38 => KeyCode::KeyL,
        45 => KeyCode::KeyX,
        46 => KeyCode::KeyC,
        47 => KeyCode::KeyV,
        59 => KeyCode::F1,
        60 => KeyCode::F2,
        61 => KeyCode::F3,
        63 => KeyCode::F5,
        64 => KeyCode::F6,
        79 => KeyCode::Numpad1,
        80 => KeyCode::Numpad2,
        81 => KeyCode::Numpad3,
        102 => KeyCode::Home,
        103 => KeyCode::ArrowUp,
        105 => KeyCode::ArrowLeft,
        106 => KeyCode::ArrowRight,
        107 => KeyCode::End,
        108 => KeyCode::ArrowDown,
        111 => KeyCode::Delete,
        _ => return PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
    };
    PhysicalKey::Code(code)
}
