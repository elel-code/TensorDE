use crate::platform::{Key, KeyCode, KeyEvent, MouseButton, NamedKey, PhysicalKey};

use crate::shell::create_rename::CreateEntryKind;
use crate::shell::options::ShellViewMode;
use crate::shell::selection::NavigationAction;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathNavigationAction {
    Back,
    Forward,
    Parent,
}

impl PathNavigationAction {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Back => "history-back",
            Self::Forward => "history-forward",
            Self::Parent => "parent-directory",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ZoomAction {
    In,
    Out,
    Reset,
}

impl ZoomAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
            Self::Reset => "reset",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectionCommand {
    SelectAll,
    Clear,
}

impl SelectionCommand {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SelectAll => "select-all",
            Self::Clear => "clear",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileKeyboardCommand {
    Copy,
    Cut,
    Paste,
    Rename,
    Delete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FilterCommand {
    Activate,
    Insert(String),
    Backspace,
    ClearAndDeactivate,
    Deactivate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LocationCommand {
    Activate,
    Insert(String),
    Backspace,
    Delete,
    MoveLeft,
    MoveRight,
    MoveHome,
    MoveEnd,
    Cancel,
    Commit,
    Complete,
    Ignore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CreateCommand {
    Insert(String),
    Backspace,
    Cancel,
    Commit,
    SetKind(CreateEntryKind),
    Ignore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RenameCommand {
    Insert(String),
    Backspace,
    Cancel,
    Commit,
    Ignore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OpenWithCommand {
    Insert(String),
    Backspace,
    Delete,
    Cancel,
    Commit,
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    MoveHome,
    MoveEnd,
    Ignore,
}

pub(crate) fn zoom_action_for_scroll_delta(delta_y: f32) -> Option<ZoomAction> {
    if delta_y < -f32::EPSILON {
        Some(ZoomAction::In)
    } else if delta_y > f32::EPSILON {
        Some(ZoomAction::Out)
    } else {
        None
    }
}

/// Multiplicative finger-spacing change required for one icon zoom step.
///
/// Absolute pinch `scale` is relative to spacing at gesture begin (~1.0). About
/// 12% spacing change maps to one FileManager-style zoom step so continuous pinch
/// does not jump multiple levels on tiny motion.
pub(crate) const PINCH_ZOOM_STEP_RATIO: f64 = 1.12;

/// Tracks absolute pinch scale and emits discrete [`ZoomAction`] steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PinchZoomTracker {
    /// Scale at which the last zoom step was applied (starts at 1.0 on begin).
    last_applied_scale: f64,
}

impl PinchZoomTracker {
    pub(crate) fn begin() -> Self {
        Self {
            last_applied_scale: 1.0,
        }
    }

    /// Consume an absolute pinch scale update; may emit zero or more zoom steps.
    pub(crate) fn update(&mut self, scale: f64) -> Vec<ZoomAction> {
        if !scale.is_finite() || scale <= 0.0 {
            return Vec::new();
        }
        let mut actions = Vec::new();
        let step = PINCH_ZOOM_STEP_RATIO;
        // Fingers farther apart (scale grows) → zoom in; pinch together → out.
        while scale / self.last_applied_scale >= step {
            self.last_applied_scale *= step;
            actions.push(ZoomAction::In);
        }
        while self.last_applied_scale / scale >= step {
            self.last_applied_scale /= step;
            actions.push(ZoomAction::Out);
        }
        actions
    }
}

/// Minimum finger count for swipe → history navigation (2-finger is scroll).
pub(crate) const SWIPE_NAV_MIN_FINGERS: u32 = 3;

/// Minimum absolute horizontal travel (logical surface units) to commit navigation.
pub(crate) const SWIPE_NAV_DISTANCE: f64 = 96.0;

/// Accumulates a multi-finger swipe and resolves back/forward on end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SwipeNavigationTracker {
    fingers: u32,
    total_dx: f64,
    total_dy: f64,
}

impl SwipeNavigationTracker {
    pub(crate) fn begin(fingers: u32) -> Self {
        Self {
            fingers,
            total_dx: 0.0,
            total_dy: 0.0,
        }
    }

    pub(crate) fn update(&mut self, delta_x: f64, delta_y: f64) {
        if delta_x.is_finite() {
            self.total_dx += delta_x;
        }
        if delta_y.is_finite() {
            self.total_dy += delta_y;
        }
    }

    /// Resolve navigation when the gesture ends.
    ///
    /// Browser-style: swipe right (positive dx) → Back, swipe left → Forward.
    /// Requires enough horizontal travel, horizontal dominance, and ≥3 fingers.
    pub(crate) fn finish(self, cancelled: bool) -> Option<PathNavigationAction> {
        if cancelled || self.fingers < SWIPE_NAV_MIN_FINGERS {
            return None;
        }
        if self.total_dx.abs() < SWIPE_NAV_DISTANCE {
            return None;
        }
        if self.total_dx.abs() <= self.total_dy.abs() {
            return None;
        }
        if self.total_dx > 0.0 {
            Some(PathNavigationAction::Back)
        } else {
            Some(PathNavigationAction::Forward)
        }
    }
}

#[cfg(test)]
mod pinch_zoom_tests {
    use super::*;

    #[test]
    fn pinch_open_emits_zoom_in_steps() {
        let mut tracker = PinchZoomTracker::begin();
        assert!(tracker.update(1.05).is_empty());
        assert_eq!(tracker.update(1.12), vec![ZoomAction::In]);
        assert_eq!(tracker.update(1.12 * 1.12), vec![ZoomAction::In]);
    }

    #[test]
    fn pinch_close_emits_zoom_out_steps() {
        let mut tracker = PinchZoomTracker::begin();
        assert_eq!(tracker.update(1.0 / 1.12), vec![ZoomAction::Out]);
        assert_eq!(tracker.update(1.0 / (1.12 * 1.12)), vec![ZoomAction::Out]);
    }

    #[test]
    fn pinch_ignores_non_finite_scale() {
        let mut tracker = PinchZoomTracker::begin();
        assert!(tracker.update(f64::NAN).is_empty());
        assert!(tracker.update(0.0).is_empty());
        assert!(tracker.update(-1.0).is_empty());
    }

    #[test]
    fn large_pinch_jump_emits_multiple_steps() {
        let mut tracker = PinchZoomTracker::begin();
        // 1.12^3 ≈ 1.405; one large jump should yield three zoom-in steps.
        assert_eq!(
            tracker.update(PINCH_ZOOM_STEP_RATIO.powi(3)),
            vec![ZoomAction::In, ZoomAction::In, ZoomAction::In]
        );
    }
}

#[cfg(test)]
mod swipe_nav_tests {
    use super::*;

    #[test]
    fn three_finger_swipe_right_goes_back() {
        let mut tracker = SwipeNavigationTracker::begin(3);
        tracker.update(50.0, 5.0);
        tracker.update(60.0, -2.0);
        assert_eq!(tracker.finish(false), Some(PathNavigationAction::Back));
    }

    #[test]
    fn three_finger_swipe_left_goes_forward() {
        let mut tracker = SwipeNavigationTracker::begin(3);
        tracker.update(-120.0, 10.0);
        assert_eq!(tracker.finish(false), Some(PathNavigationAction::Forward));
    }

    #[test]
    fn two_finger_swipe_is_ignored() {
        let mut tracker = SwipeNavigationTracker::begin(2);
        tracker.update(200.0, 0.0);
        assert_eq!(tracker.finish(false), None);
    }

    #[test]
    fn vertical_dominant_swipe_is_ignored() {
        let mut tracker = SwipeNavigationTracker::begin(3);
        tracker.update(50.0, 200.0);
        assert_eq!(tracker.finish(false), None);
    }

    #[test]
    fn cancelled_swipe_is_ignored() {
        let mut tracker = SwipeNavigationTracker::begin(3);
        tracker.update(200.0, 0.0);
        assert_eq!(tracker.finish(true), None);
    }

    #[test]
    fn short_horizontal_swipe_is_ignored() {
        let mut tracker = SwipeNavigationTracker::begin(3);
        tracker.update(40.0, 0.0);
        assert_eq!(tracker.finish(false), None);
    }
}

pub(crate) fn is_activation_key(key: &Key) -> bool {
    matches!(key, Key::Named(NamedKey::Enter))
}

pub(crate) fn escape_requested_for_key_event(event: &KeyEvent) -> bool {
    matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(event.logical_key, Key::Named(NamedKey::Escape))
        || matches!(event.key_without_modifiers, Key::Named(NamedKey::Escape))
}

pub(crate) fn path_navigation_action_for_key(key: &Key, alt: bool) -> Option<PathNavigationAction> {
    if matches!(key, Key::Named(NamedKey::Backspace)) {
        return Some(PathNavigationAction::Parent);
    }
    if !alt {
        return None;
    }
    match key {
        Key::Named(NamedKey::ArrowLeft) => Some(PathNavigationAction::Back),
        Key::Named(NamedKey::ArrowRight) => Some(PathNavigationAction::Forward),
        Key::Named(NamedKey::ArrowUp) => Some(PathNavigationAction::Parent),
        _ => None,
    }
}

pub(crate) fn path_navigation_action_for_mouse_button(
    button: MouseButton,
) -> Option<PathNavigationAction> {
    match button {
        MouseButton::Back => Some(PathNavigationAction::Back),
        MouseButton::Forward => Some(PathNavigationAction::Forward),
        _ => None,
    }
}

pub(crate) fn zoom_action_for_key_event(event: &KeyEvent) -> Option<ZoomAction> {
    zoom_action_for_key(&event.key_without_modifiers)
        .or_else(|| zoom_action_for_key(&event.logical_key))
}

pub(crate) fn zoom_action_for_key(key: &Key) -> Option<ZoomAction> {
    match key {
        Key::Character(value) if value.as_str() == "+" || value.as_str() == "=" => {
            Some(ZoomAction::In)
        }
        Key::Character(value) if value.as_str() == "-" || value.as_str() == "_" => {
            Some(ZoomAction::Out)
        }
        Key::Character(value) if value.as_str() == "0" => Some(ZoomAction::Reset),
        _ => None,
    }
}

pub(crate) fn selection_command_for_key_event(
    event: &KeyEvent,
    shortcut: bool,
) -> Option<SelectionCommand> {
    selection_command_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn selection_command_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> Option<SelectionCommand> {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return Some(SelectionCommand::Clear);
    }
    if !shortcut {
        return None;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::KeyA))
        || key_character_eq(logical_key, "a")
        || key_character_eq(key_without_modifiers, "a")
    {
        return Some(SelectionCommand::SelectAll);
    }
    None
}

pub(crate) fn file_keyboard_command_for_key_event(
    event: &KeyEvent,
    shortcut: bool,
) -> Option<FileKeyboardCommand> {
    file_keyboard_command_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn file_keyboard_command_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> Option<FileKeyboardCommand> {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::F2))
        || matches!(logical_key, Key::Named(NamedKey::F2))
        || matches!(key_without_modifiers, Key::Named(NamedKey::F2))
    {
        return Some(FileKeyboardCommand::Rename);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Delete))
        || matches!(logical_key, Key::Named(NamedKey::Delete))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Delete))
    {
        return Some(FileKeyboardCommand::Delete);
    }
    if !shortcut {
        return None;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::KeyC))
        || key_character_eq(logical_key, "c")
        || key_character_eq(key_without_modifiers, "c")
    {
        return Some(FileKeyboardCommand::Copy);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::KeyX))
        || key_character_eq(logical_key, "x")
        || key_character_eq(key_without_modifiers, "x")
    {
        return Some(FileKeyboardCommand::Cut);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::KeyV))
        || key_character_eq(logical_key, "v")
        || key_character_eq(key_without_modifiers, "v")
    {
        return Some(FileKeyboardCommand::Paste);
    }
    None
}

pub(crate) fn reload_requested_for_key_event(event: &KeyEvent, shortcut: bool) -> bool {
    reload_requested_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn reload_requested_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> bool {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::F5))
        || matches!(logical_key, Key::Named(NamedKey::F5))
        || matches!(key_without_modifiers, Key::Named(NamedKey::F5))
    {
        return true;
    }
    shortcut
        && (matches!(physical_key, PhysicalKey::Code(KeyCode::KeyR))
            || key_character_eq(logical_key, "r")
            || key_character_eq(key_without_modifiers, "r"))
}

pub(crate) fn hidden_toggle_requested_for_key_event(event: &KeyEvent, shortcut: bool) -> bool {
    hidden_toggle_requested_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn hidden_toggle_requested_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> bool {
    shortcut
        && (matches!(physical_key, PhysicalKey::Code(KeyCode::KeyH))
            || key_character_eq(logical_key, "h")
            || key_character_eq(key_without_modifiers, "h"))
}

pub(crate) fn dark_mode_toggle_requested_for_key_event(
    event: &KeyEvent,
    shortcut: bool,
    shift: bool,
) -> bool {
    dark_mode_toggle_requested_for_key_parts(
        shortcut,
        shift,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn dark_mode_toggle_requested_for_key_parts(
    shortcut: bool,
    shift: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> bool {
    shortcut
        && shift
        && (matches!(physical_key, PhysicalKey::Code(KeyCode::KeyD))
            || key_character_eq(logical_key, "d")
            || key_character_eq(key_without_modifiers, "d"))
}

pub(crate) fn location_command_for_key_event(
    event: &KeyEvent,
    shortcut: bool,
    location_active: bool,
) -> Option<LocationCommand> {
    location_command_for_key_parts(
        shortcut,
        location_active,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn location_command_for_key_parts(
    shortcut: bool,
    location_active: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> Option<LocationCommand> {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::F6))
        || matches!(logical_key, Key::Named(NamedKey::F6))
        || matches!(key_without_modifiers, Key::Named(NamedKey::F6))
        || (shortcut
            && (matches!(physical_key, PhysicalKey::Code(KeyCode::KeyL))
                || matches!(physical_key, PhysicalKey::Code(KeyCode::KeyD))
                || key_character_eq(logical_key, "l")
                || key_character_eq(key_without_modifiers, "l")
                || key_character_eq(logical_key, "d")
                || key_character_eq(key_without_modifiers, "d")))
    {
        return Some(LocationCommand::Activate);
    }
    if !location_active {
        return None;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return Some(LocationCommand::Cancel);
    }
    if matches!(logical_key, Key::Named(NamedKey::Enter))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Enter))
    {
        return Some(LocationCommand::Commit);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Tab))
        || matches!(logical_key, Key::Named(NamedKey::Tab))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Tab))
    {
        return Some(LocationCommand::Complete);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || matches!(logical_key, Key::Named(NamedKey::Backspace))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Backspace))
    {
        return Some(LocationCommand::Backspace);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Delete))
        || matches!(logical_key, Key::Named(NamedKey::Delete))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Delete))
    {
        return Some(LocationCommand::Delete);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowLeft))
        || matches!(logical_key, Key::Named(NamedKey::ArrowLeft))
        || matches!(key_without_modifiers, Key::Named(NamedKey::ArrowLeft))
    {
        return Some(LocationCommand::MoveLeft);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowRight))
        || matches!(logical_key, Key::Named(NamedKey::ArrowRight))
        || matches!(key_without_modifiers, Key::Named(NamedKey::ArrowRight))
    {
        return Some(LocationCommand::MoveRight);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Home))
        || matches!(logical_key, Key::Named(NamedKey::Home))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Home))
    {
        return Some(LocationCommand::MoveHome);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::End))
        || matches!(logical_key, Key::Named(NamedKey::End))
        || matches!(key_without_modifiers, Key::Named(NamedKey::End))
    {
        return Some(LocationCommand::MoveEnd);
    }
    if shortcut {
        return Some(LocationCommand::Ignore);
    }
    match logical_key {
        Key::Character(value) if !value.chars().any(char::is_control) => {
            Some(LocationCommand::Insert(value.to_string()))
        }
        _ => Some(LocationCommand::Ignore),
    }
}

pub(crate) fn filter_command_for_key_event(
    event: &KeyEvent,
    shortcut: bool,
    filter_active: bool,
) -> Option<FilterCommand> {
    filter_command_for_key_parts(
        shortcut,
        filter_active,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn filter_command_for_key_parts(
    shortcut: bool,
    filter_active: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> Option<FilterCommand> {
    if shortcut
        && (matches!(physical_key, PhysicalKey::Code(KeyCode::KeyF))
            || key_character_eq(logical_key, "f")
            || key_character_eq(key_without_modifiers, "f"))
    {
        return Some(FilterCommand::Activate);
    }
    if !filter_active {
        return None;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return Some(FilterCommand::ClearAndDeactivate);
    }
    if matches!(logical_key, Key::Named(NamedKey::Enter))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Enter))
    {
        return Some(FilterCommand::Deactivate);
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || matches!(logical_key, Key::Named(NamedKey::Backspace))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Backspace))
    {
        return Some(FilterCommand::Backspace);
    }
    if shortcut {
        return None;
    }
    match logical_key {
        Key::Character(value) if !value.chars().any(char::is_control) => {
            Some(FilterCommand::Insert(value.to_string()))
        }
        _ => None,
    }
}

pub(crate) fn create_command_for_key_event(event: &KeyEvent, shortcut: bool) -> CreateCommand {
    create_command_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn create_command_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> CreateCommand {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return CreateCommand::Cancel;
    }
    if matches!(logical_key, Key::Named(NamedKey::Enter))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Enter))
    {
        return CreateCommand::Commit;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || matches!(logical_key, Key::Named(NamedKey::Backspace))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Backspace))
    {
        return CreateCommand::Backspace;
    }
    if shortcut {
        return CreateCommand::Ignore;
    }
    match logical_key {
        Key::Character(value) if !value.chars().any(char::is_control) => {
            CreateCommand::Insert(value.to_string())
        }
        _ => CreateCommand::Ignore,
    }
}

pub(crate) fn rename_command_for_key_event(event: &KeyEvent, shortcut: bool) -> RenameCommand {
    rename_command_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn rename_command_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> RenameCommand {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return RenameCommand::Cancel;
    }
    if matches!(logical_key, Key::Named(NamedKey::Enter))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Enter))
    {
        return RenameCommand::Commit;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || matches!(logical_key, Key::Named(NamedKey::Backspace))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Backspace))
    {
        return RenameCommand::Backspace;
    }
    if shortcut {
        return RenameCommand::Ignore;
    }
    match logical_key {
        Key::Character(value) if !value.chars().any(char::is_control) => {
            RenameCommand::Insert(value.to_string())
        }
        _ => RenameCommand::Ignore,
    }
}

pub(crate) fn open_with_command_for_key_event(event: &KeyEvent, shortcut: bool) -> OpenWithCommand {
    open_with_command_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn open_with_command_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> OpenWithCommand {
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape))
        || matches!(logical_key, Key::Named(NamedKey::Escape))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Escape))
    {
        return OpenWithCommand::Cancel;
    }
    if matches!(logical_key, Key::Named(NamedKey::Enter))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Enter))
    {
        return OpenWithCommand::Commit;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowUp))
        || matches!(logical_key, Key::Named(NamedKey::ArrowUp))
        || matches!(key_without_modifiers, Key::Named(NamedKey::ArrowUp))
    {
        return OpenWithCommand::MoveUp;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowDown))
        || matches!(logical_key, Key::Named(NamedKey::ArrowDown))
        || matches!(key_without_modifiers, Key::Named(NamedKey::ArrowDown))
    {
        return OpenWithCommand::MoveDown;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowLeft))
        || matches!(logical_key, Key::Named(NamedKey::ArrowLeft))
        || matches!(key_without_modifiers, Key::Named(NamedKey::ArrowLeft))
    {
        return OpenWithCommand::MoveLeft;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowRight))
        || matches!(logical_key, Key::Named(NamedKey::ArrowRight))
        || matches!(key_without_modifiers, Key::Named(NamedKey::ArrowRight))
    {
        return OpenWithCommand::MoveRight;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace))
        || matches!(logical_key, Key::Named(NamedKey::Backspace))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Backspace))
    {
        return OpenWithCommand::Backspace;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Delete))
        || matches!(logical_key, Key::Named(NamedKey::Delete))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Delete))
    {
        return OpenWithCommand::Delete;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::Home))
        || matches!(logical_key, Key::Named(NamedKey::Home))
        || matches!(key_without_modifiers, Key::Named(NamedKey::Home))
    {
        return OpenWithCommand::MoveHome;
    }
    if matches!(physical_key, PhysicalKey::Code(KeyCode::End))
        || matches!(logical_key, Key::Named(NamedKey::End))
        || matches!(key_without_modifiers, Key::Named(NamedKey::End))
    {
        return OpenWithCommand::MoveEnd;
    }
    if shortcut {
        return OpenWithCommand::Ignore;
    }
    match logical_key {
        Key::Character(value) if !value.chars().any(char::is_control) => {
            OpenWithCommand::Insert(value.to_string())
        }
        _ => OpenWithCommand::Ignore,
    }
}

fn key_character_eq(key: &Key, expected: &str) -> bool {
    matches!(key, Key::Character(value) if value.as_str().eq_ignore_ascii_case(expected))
}

pub(crate) fn navigation_action_for_key(key: &Key) -> Option<NavigationAction> {
    match key {
        Key::Named(NamedKey::ArrowLeft) => Some(NavigationAction::Left),
        Key::Named(NamedKey::ArrowRight) => Some(NavigationAction::Right),
        Key::Named(NamedKey::ArrowUp) => Some(NavigationAction::Up),
        Key::Named(NamedKey::ArrowDown) => Some(NavigationAction::Down),
        Key::Named(NamedKey::Home) => Some(NavigationAction::Home),
        Key::Named(NamedKey::End) => Some(NavigationAction::End),
        Key::Named(NamedKey::PageUp) => Some(NavigationAction::PageUp),
        Key::Named(NamedKey::PageDown) => Some(NavigationAction::PageDown),
        _ => None,
    }
}

pub(crate) fn view_mode_for_key_event(event: &KeyEvent, shortcut: bool) -> Option<ShellViewMode> {
    view_mode_for_key_parts(
        shortcut,
        &event.physical_key,
        &event.logical_key,
        &event.key_without_modifiers,
    )
}

pub(crate) fn view_mode_for_key_parts(
    shortcut: bool,
    physical_key: &PhysicalKey,
    logical_key: &Key,
    key_without_modifiers: &Key,
) -> Option<ShellViewMode> {
    let function_key_mode = match physical_key {
        PhysicalKey::Code(KeyCode::F1) => Some(ShellViewMode::Icons),
        PhysicalKey::Code(KeyCode::F3) => Some(ShellViewMode::Details),
        _ => function_key_view_mode(logical_key),
    };
    if function_key_mode.is_some() {
        return function_key_mode;
    }

    if !shortcut {
        return None;
    }
    physical_digit_view_mode(physical_key)
        .or_else(|| character_digit_view_mode(key_without_modifiers))
        .or_else(|| character_digit_view_mode(logical_key))
}

fn physical_digit_view_mode(physical_key: &PhysicalKey) -> Option<ShellViewMode> {
    match physical_key {
        PhysicalKey::Code(KeyCode::Digit1 | KeyCode::Numpad1) => Some(ShellViewMode::Icons),
        PhysicalKey::Code(KeyCode::Digit2 | KeyCode::Numpad2) => Some(ShellViewMode::Compact),
        PhysicalKey::Code(KeyCode::Digit3 | KeyCode::Numpad3) => Some(ShellViewMode::Details),
        _ => None,
    }
}

fn character_digit_view_mode(key: &Key) -> Option<ShellViewMode> {
    match key {
        Key::Character(value) if value.as_str() == "1" => Some(ShellViewMode::Icons),
        Key::Character(value) if value.as_str() == "2" => Some(ShellViewMode::Compact),
        Key::Character(value) if value.as_str() == "3" => Some(ShellViewMode::Details),
        _ => None,
    }
}

fn function_key_view_mode(key: &Key) -> Option<ShellViewMode> {
    match key {
        Key::Named(NamedKey::F1) => Some(ShellViewMode::Icons),
        Key::Named(NamedKey::F3) => Some(ShellViewMode::Details),
        _ => None,
    }
}
