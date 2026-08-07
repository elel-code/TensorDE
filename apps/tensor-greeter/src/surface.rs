use std::{path::PathBuf, sync::Arc, time::Duration};

use tensor_present::{ColorRect, SurfaceFrame, SurfacePresenter, SurfacePresenterError};
use vulkan_renderer::{Extent2D, Rect2D};
use wayland_client_runtime::{
    DecorationPreference, Event, KeyState, KeyboardEvent, LogicalPosition, LogicalRect,
    LogicalSize, NativeRuntime, PointerEvent, PointerEventKind, RuntimeError, SurfaceEvent,
    SurfaceId, TextInputContentHint, TextInputContentPurpose, TextInputContentType, TextInputDone,
    TextInputEvent, TextInputState, ToplevelAttributes,
};

use crate::{
    AuthPhase, AuthPromptKind, AuthUpdate, GreetdClient, GreeterModel, GreeterModelError,
    GreeterTransaction, GreeterTransactionError, UserAccount,
};

const INITIAL_SIZE: LogicalSize = LogicalSize::new(900, 620);
const MIN_SIZE: LogicalSize = LogicalSize::new(640, 440);
const MARGIN: u32 = 28;
const USER_ROW_HEIGHT: u32 = 54;
const SESSION_ROW_HEIGHT: u32 = 46;
const GAP: u32 = 8;
const BTN_LEFT: u32 = 0x110;

/// Native login surface around the transport and generation-tagged model.
pub struct GreeterSurface {
    wayland: NativeRuntime,
    surface: SurfaceId,
    model: Option<GreeterModel>,
    socket: PathBuf,
    transaction: Option<GreeterTransaction>,
    response: SecretBuffer,
    logical_size: LogicalSize,
    configured: bool,
    dirty: bool,
    exit: bool,
    events: Vec<Event>,
    draws: Vec<ColorRect>,
    presenter: Option<SurfacePresenter>,
}

impl GreeterSurface {
    pub fn open(model: GreeterModel, socket: PathBuf) -> Result<Self, GreeterSurfaceError> {
        let mut wayland = NativeRuntime::connect()?;
        let surface = wayland.create_toplevel(ToplevelAttributes {
            title: "Tensor Greeter".into(),
            app_id: "tensor-greeter".into(),
            initial_size: Some(INITIAL_SIZE),
            min_size: Some(MIN_SIZE),
            max_size: None,
            decorations: DecorationPreference::None,
        })?;
        Ok(Self {
            wayland,
            surface,
            model: Some(model),
            socket,
            transaction: None,
            response: SecretBuffer::default(),
            logical_size: INITIAL_SIZE,
            configured: false,
            dirty: true,
            exit: false,
            events: Vec::with_capacity(64),
            draws: Vec::with_capacity(20),
            presenter: None,
        })
    }

    pub fn run(mut self, io: &compio::runtime::Runtime) -> Result<(), GreeterSurfaceError> {
        let client = io.block_on(GreetdClient::connect(&self.socket))?;
        let model = self
            .model
            .take()
            .expect("greeter model is initialized once");
        self.transaction = Some(GreeterTransaction::new(model, client));
        while !self.exit {
            self.wayland.dispatch(if self.dirty && self.configured {
                Some(Duration::ZERO)
            } else {
                None
            })?;
            self.events.clear();
            self.wayland.drain_events_into(&mut self.events);
            let events = std::mem::take(&mut self.events);
            for event in &events {
                self.handle_event(event, io)?;
                if self.exit {
                    break;
                }
            }
            self.events = events;
            if self.dirty && self.configured && !self.wayland.is_present_pending(self.surface) {
                self.present()?;
            }
        }
        self.shutdown(io)
    }

    fn handle_event(
        &mut self,
        event: &Event,
        io: &compio::runtime::Runtime,
    ) -> Result<(), GreeterSurfaceError> {
        match event {
            Event::Surface(SurfaceEvent::Configure {
                surface,
                suggested_size,
                ..
            }) if *surface == self.surface => {
                self.logical_size = LogicalSize::new(
                    suggested_size
                        .width
                        .unwrap_or(self.logical_size.width)
                        .max(1),
                    suggested_size
                        .height
                        .unwrap_or(self.logical_size.height)
                        .max(1),
                );
                self.configured = true;
                self.apply_surface_geometry()?;
                self.dirty = true;
            }
            Event::Surface(SurfaceEvent::ScaleFactorChanged { surface, .. })
                if *surface == self.surface =>
            {
                if let Some(size) = self.wayland.logical_size(self.surface) {
                    self.logical_size = size;
                }
                self.apply_surface_geometry()?;
                self.dirty = true;
            }
            Event::Surface(SurfaceEvent::CloseRequested { surface })
                if *surface == self.surface =>
            {
                self.exit = true
            }
            Event::Keyboard(KeyboardEvent::Key {
                surface,
                state,
                keysym,
                text,
                ..
            }) if *surface == self.surface
                && matches!(state, KeyState::Pressed | KeyState::Repeated) =>
            {
                self.apply_key(*keysym, text.as_deref(), io)?;
            }
            Event::TextInput(TextInputEvent::Done(done)) if done.surface == self.surface => {
                self.apply_text_input(done)?;
            }
            Event::Pointer(pointer) if pointer.surface == self.surface => {
                self.handle_pointer(pointer);
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_key(
        &mut self,
        keysym: u32,
        text: Option<&str>,
        io: &compio::runtime::Runtime,
    ) -> Result<(), GreeterSurfaceError> {
        use xkeysym::key;

        match keysym {
            key::Escape => {
                if self.in_authentication() {
                    self.cancel(io)?;
                } else {
                    self.exit = true;
                }
            }
            key::Up | key::KP_Up => self.select_user(-1)?,
            key::Down | key::KP_Down => self.select_user(1)?,
            key::Left | key::KP_Left => self.select_session(-1)?,
            key::Right | key::KP_Right => self.select_session(1)?,
            key::Page_Up | key::KP_Page_Up => self.select_session(-1)?,
            key::Page_Down | key::KP_Page_Down => self.select_session(1)?,
            key::BackSpace if self.prompt_requires_response() => {
                self.response.backspace();
                self.dirty = true;
            }
            key::Return | key::KP_Enter => self.activate(io)?,
            _ if self.prompt_requires_response() => {
                if let Some(text) = text.filter(|value| {
                    !value.is_empty() && value.chars().all(|character| !character.is_control())
                }) {
                    self.response.push(text);
                    self.dirty = true;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn activate(&mut self, io: &compio::runtime::Runtime) -> Result<(), GreeterSurfaceError> {
        let Some(transaction) = self.transaction.as_mut() else {
            return Ok(());
        };
        let update = match transaction.model().phase() {
            AuthPhase::Idle | AuthPhase::Failed { .. } => {
                io.block_on(transaction.begin_authentication())?
            }
            AuthPhase::Prompt { attempt, prompt } if prompt.kind.requires_response() => {
                let attempt = *attempt;
                let mut answer = self.response.take_string();
                let result = io.block_on(transaction.respond(attempt, Some(&answer)));
                wipe_string(&mut answer);
                result?
            }
            AuthPhase::Prompt { attempt, .. } => {
                io.block_on(transaction.respond(*attempt, None))?
            }
            AuthPhase::Authenticated { attempt } => {
                io.block_on(transaction.start_session(*attempt))?;
                self.exit = true;
                return Ok(());
            }
            AuthPhase::Waiting { .. } | AuthPhase::SessionStarted { .. } => return Ok(()),
        };
        self.apply_update(update, io)
    }

    fn apply_update(
        &mut self,
        update: AuthUpdate,
        io: &compio::runtime::Runtime,
    ) -> Result<(), GreeterSurfaceError> {
        match update {
            AuthUpdate::Prompt { prompt, .. } => {
                self.response.clear();
                if prompt.kind.requires_response() {
                    self.enable_text_input(prompt.kind)?;
                } else {
                    self.disable_text_input();
                    let attempt = match self.transaction.as_ref().map(|tx| tx.model().phase()) {
                        Some(AuthPhase::Prompt { attempt, .. }) => *attempt,
                        _ => return Ok(()),
                    };
                    let next = {
                        let transaction = self.transaction.as_mut().expect("transaction exists");
                        io.block_on(transaction.respond(attempt, None))?
                    };
                    self.apply_update(next, io)?;
                }
                self.dirty = true;
            }
            AuthUpdate::Authenticated { .. } => {
                self.disable_text_input();
                self.dirty = true;
            }
            AuthUpdate::Failed { .. } => {
                self.response.clear();
                self.disable_text_input();
                self.dirty = true;
            }
        }
        Ok(())
    }

    fn apply_text_input(&mut self, done: &TextInputDone) -> Result<(), GreeterSurfaceError> {
        if !self.prompt_requires_response() {
            return Ok(());
        }
        if let Some(delete) = done.delete_surrounding {
            self.response
                .delete(delete.before_bytes, delete.after_bytes);
        }
        if let Some(commit) = &done.commit {
            self.response.push(commit);
        }
        self.dirty = true;
        Ok(())
    }

    fn select_user(&mut self, direction: isize) -> Result<(), GreeterSurfaceError> {
        if self.in_authentication() || self.model_users().is_empty() {
            return Ok(());
        }
        let selected = self
            .transaction
            .as_ref()
            .and_then(|transaction| transaction.model().selected_user())
            .map(|user| user.username.as_str());
        let current = self
            .model_users()
            .iter()
            .position(|user| Some(user.username.as_str()) == selected)
            .unwrap_or(0);
        let len = self.model_users().len() as isize;
        let index = (current as isize + direction).rem_euclid(len) as usize;
        if let Some(transaction) = self.transaction.as_mut() {
            transaction.model_mut().select_user(index)?;
        }
        self.dirty = true;
        Ok(())
    }

    fn select_session(&mut self, direction: isize) -> Result<(), GreeterSurfaceError> {
        if self.in_authentication() {
            return Ok(());
        }
        let Some(transaction) = self.transaction.as_mut() else {
            return Ok(());
        };
        let count = transaction.model().sessions().len();
        if count == 0 {
            return Ok(());
        }
        let current = transaction.model().selected_session_index() as isize;
        let index = (current + direction).rem_euclid(count as isize) as usize;
        transaction.model_mut().select_session(index)?;
        self.dirty = true;
        Ok(())
    }

    fn handle_pointer(&mut self, event: &PointerEvent) {
        if !matches!(
            event.kind,
            PointerEventKind::Press {
                button: BTN_LEFT,
                ..
            }
        ) || self.in_authentication()
        {
            return;
        }
        if let Some(index) = user_at(self.logical_size, self.model_users().len(), event.position)
            && let Some(transaction) = self.transaction.as_mut()
            && transaction.model_mut().select_user(index).is_ok()
        {
            self.dirty = true;
            return;
        }
        if let Some(index) = session_at(
            self.logical_size,
            self.transaction
                .as_ref()
                .map_or(0, |transaction| transaction.model().sessions().len()),
            event.position,
        ) && let Some(transaction) = self.transaction.as_mut()
            && transaction.model_mut().select_session(index).is_ok()
        {
            self.dirty = true;
        }
    }

    fn enable_text_input(&mut self, kind: AuthPromptKind) -> Result<(), GreeterSurfaceError> {
        let content = TextInputContentType {
            hints: if kind == AuthPromptKind::Secret {
                TextInputContentHint::HIDDEN_TEXT | TextInputContentHint::SENSITIVE_DATA
            } else {
                TextInputContentHint::empty()
            },
            purpose: if kind == AuthPromptKind::Secret {
                TextInputContentPurpose::Password
            } else {
                TextInputContentPurpose::Normal
            },
        };
        let state = TextInputState::new().with_content_type(content);
        match self
            .wayland
            .set_text_input_state(self.surface, Some(&state))
        {
            Ok(()) | Err(RuntimeError::Unsupported(_)) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn disable_text_input(&mut self) {
        let _ = self.wayland.set_text_input_state(self.surface, None);
    }

    fn cancel(&mut self, io: &compio::runtime::Runtime) -> Result<(), GreeterSurfaceError> {
        if let Some(transaction) = self.transaction.as_mut() {
            io.block_on(transaction.cancel())?;
        }
        self.response.clear();
        self.disable_text_input();
        self.dirty = true;
        Ok(())
    }

    fn in_authentication(&self) -> bool {
        self.transaction.as_ref().is_some_and(|transaction| {
            !matches!(
                transaction.model().phase(),
                AuthPhase::Idle | AuthPhase::Failed { .. }
            )
        })
    }

    fn prompt_requires_response(&self) -> bool {
        self.transaction.as_ref().is_some_and(|transaction| {
            matches!(transaction.model().phase(), AuthPhase::Prompt { prompt, .. } if prompt.kind.requires_response())
        })
    }

    fn model_users(&self) -> &[UserAccount] {
        self.transaction
            .as_ref()
            .map_or(&[], |transaction| transaction.model().users())
    }

    fn apply_surface_geometry(&mut self) -> Result<(), GreeterSurfaceError> {
        self.wayland
            .set_window_geometry(self.surface, LogicalPosition::ZERO, self.logical_size)?;
        if self.wayland.capabilities().fractional_scale {
            self.wayland.set_buffer_scale(self.surface, 1)?;
            self.wayland
                .set_viewport_destination(self.surface, Some(self.logical_size))?;
        } else {
            let scale = self
                .wayland
                .scale_factor(self.surface)
                .unwrap_or(1.0)
                .round()
                .clamp(1.0, f64::from(i32::MAX)) as i32;
            self.wayland.set_buffer_scale(self.surface, scale)?;
        }
        self.wayland.commit(self.surface)?;
        Ok(())
    }

    fn present(&mut self) -> Result<(), GreeterSurfaceError> {
        let (width, height) = self
            .wayland
            .buffer_size(self.surface)
            .ok_or(GreeterSurfaceError::MissingBufferExtent)?;
        let extent = Extent2D::new(width, height);
        let handle = Arc::new(
            self.wayland
                .surface_handle(self.surface)
                .ok_or(GreeterSurfaceError::MissingSurfaceHandle)?,
        );
        match self.presenter.as_mut() {
            Some(presenter) => {
                presenter.ensure_surface(self.surface, handle, extent, "tensor-greeter")?
            }
            None => {
                self.presenter = Some(SurfacePresenter::new(
                    self.surface,
                    handle,
                    extent,
                    "tensor-greeter",
                )?);
            }
        }
        let selected = self
            .transaction
            .as_ref()
            .and_then(|transaction| transaction.model().selected_user())
            .map(|user| user.username.as_str());
        let selected_index = self
            .model_users()
            .iter()
            .position(|user| Some(user.username.as_str()) == selected);
        let user_count = self.model_users().len();
        let (response_active, prompt_kind, selected_session, session_count) = self
            .transaction
            .as_ref()
            .map_or((false, None, 0, 0), |transaction| {
                let model = transaction.model();
                (
                    self.prompt_requires_response(),
                    match model.phase() {
                        AuthPhase::Prompt { prompt, .. } => Some(prompt.kind),
                        _ => None,
                    },
                    model.selected_session_index(),
                    model.sessions().len(),
                )
            });
        build_draws(
            &mut self.draws,
            self.logical_size,
            extent,
            GreeterDrawState {
                users: user_count,
                selected_user: selected_index,
                sessions: session_count,
                selected_session,
                response_active,
                prompt_kind,
            },
        );
        self.wayland.arm_present_notify(self.surface)?;
        self.wayland.flush()?;
        self.presenter
            .as_mut()
            .expect("presenter initialized above")
            .present(
                self.surface,
                extent,
                SurfaceFrame {
                    clear: [0.025, 0.03, 0.038, 1.0],
                    rectangles: &self.draws,
                },
            )?;
        self.dirty = false;
        Ok(())
    }

    fn shutdown(&mut self, io: &compio::runtime::Runtime) -> Result<(), GreeterSurfaceError> {
        if self.in_authentication() {
            let _ = self.cancel(io);
        }
        if let Some(presenter) = self.presenter.as_mut() {
            presenter.remove_surface(self.surface)?;
        }
        self.wayland.destroy_surface(self.surface)?;
        Ok(())
    }
}

struct SecretBuffer {
    bytes: Vec<u8>,
}

impl Default for SecretBuffer {
    fn default() -> Self {
        Self {
            bytes: Vec::with_capacity(128),
        }
    }
}

impl SecretBuffer {
    fn push(&mut self, text: &str) {
        self.bytes.extend_from_slice(text.as_bytes());
    }

    fn backspace(&mut self) {
        let Some(index) = std::str::from_utf8(&self.bytes)
            .ok()
            .and_then(|text| text.char_indices().next_back().map(|(index, _)| index))
        else {
            return;
        };
        self.bytes.truncate(index);
    }

    fn delete(&mut self, before: usize, after: usize) {
        if after != 0 || before > self.bytes.len() {
            return;
        }
        let Ok(text) = std::str::from_utf8(&self.bytes) else {
            return;
        };
        let start = text.len().saturating_sub(before);
        if text.is_char_boundary(start) {
            self.bytes.drain(start..);
        }
    }

    fn take_string(&mut self) -> String {
        String::from_utf8(std::mem::take(&mut self.bytes)).expect("secret input remains UTF-8")
    }

    fn clear(&mut self) {
        wipe_bytes(&mut self.bytes);
        self.bytes.clear();
    }
}

impl Drop for SecretBuffer {
    fn drop(&mut self) {
        self.clear();
    }
}

fn wipe_string(value: &mut String) {
    wipe_bytes(unsafe { value.as_bytes_mut() });
    value.clear();
}

fn wipe_bytes(bytes: &mut [u8]) {
    for byte in bytes {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

fn user_rect(index: usize, extent: LogicalSize) -> Option<LogicalRect> {
    let width = extent.width / 2;
    let y = MARGIN
        + u32::try_from(index)
            .ok()?
            .checked_mul(USER_ROW_HEIGHT + GAP)?;
    (width > 0 && y + USER_ROW_HEIGHT <= extent.height)
        .then(|| LogicalRect::new(MARGIN as i32, y as i32, width, USER_ROW_HEIGHT))
}

fn user_at(extent: LogicalSize, count: usize, position: (f64, f64)) -> Option<usize> {
    (0..count).find(|index| user_rect(*index, extent).is_some_and(|rect| contains(rect, position)))
}

fn session_rect(index: usize, extent: LogicalSize) -> Option<LogicalRect> {
    let left = extent.width / 2 + MARGIN;
    let width = extent.width.checked_sub(left + MARGIN)?;
    let y = MARGIN.checked_add(
        u32::try_from(index)
            .ok()?
            .checked_mul(SESSION_ROW_HEIGHT + GAP)?,
    )?;
    (width > 0 && y + SESSION_ROW_HEIGHT <= extent.height)
        .then(|| LogicalRect::new(left as i32, y as i32, width, SESSION_ROW_HEIGHT))
}

fn session_at(extent: LogicalSize, count: usize, position: (f64, f64)) -> Option<usize> {
    (0..count)
        .find(|index| session_rect(*index, extent).is_some_and(|rect| contains(rect, position)))
}

fn contains(rect: LogicalRect, position: (f64, f64)) -> bool {
    if !position.0.is_finite() || !position.1.is_finite() {
        return false;
    }
    let x = f64::from(rect.origin.x);
    let y = f64::from(rect.origin.y);
    position.0 >= x
        && position.1 >= y
        && position.0 < x + f64::from(rect.size.width)
        && position.1 < y + f64::from(rect.size.height)
}

struct GreeterDrawState {
    users: usize,
    selected_user: Option<usize>,
    sessions: usize,
    selected_session: usize,
    response_active: bool,
    prompt_kind: Option<AuthPromptKind>,
}

fn build_draws(
    target: &mut Vec<ColorRect>,
    logical: LogicalSize,
    physical: Extent2D,
    visual: GreeterDrawState,
) {
    target.clear();
    for index in 0..visual.users {
        let Some(rect) = user_rect(index, logical) else {
            break;
        };
        let selected = visual.selected_user == Some(index);
        push_draw(
            target,
            rect,
            logical,
            physical,
            if selected {
                [0.10, 0.34, 0.44, 1.0]
            } else {
                [0.065, 0.075, 0.09, 1.0]
            },
        );
    }
    let right_x = logical.width / 2 + MARGIN;
    push_draw(
        target,
        LogicalRect::new(
            right_x as i32,
            MARGIN as i32,
            logical.width.saturating_sub(right_x + MARGIN),
            logical.height.saturating_sub(MARGIN * 2),
        ),
        logical,
        physical,
        prompt_color(visual.prompt_kind, visual.response_active),
    );
    for index in 0..visual.sessions {
        let Some(rect) = session_rect(index, logical) else {
            break;
        };
        push_draw(
            target,
            rect,
            logical,
            physical,
            if index == visual.selected_session {
                [0.11, 0.30, 0.39, 1.0]
            } else {
                [0.055, 0.07, 0.085, 1.0]
            },
        );
    }
}

const fn prompt_color(kind: Option<AuthPromptKind>, response_active: bool) -> [f32; 4] {
    match kind {
        Some(AuthPromptKind::Error) => [0.22, 0.06, 0.07, 1.0],
        Some(AuthPromptKind::Info) => [0.08, 0.13, 0.18, 1.0],
        Some(AuthPromptKind::Visible) => [0.12, 0.14, 0.08, 1.0],
        Some(AuthPromptKind::Secret) if response_active => [0.14, 0.11, 0.05, 1.0],
        _ => [0.06, 0.075, 0.10, 1.0],
    }
}

fn push_draw(
    target: &mut Vec<ColorRect>,
    rect: LogicalRect,
    logical: LogicalSize,
    physical: Extent2D,
    color: [f32; 4],
) {
    if let Some(rect) = physical_rect(rect, logical, physical) {
        target.push(ColorRect { rect, color });
    }
}

fn physical_rect(rect: LogicalRect, logical: LogicalSize, physical: Extent2D) -> Option<Rect2D> {
    if rect.is_empty() || logical.is_empty() || physical.is_empty() {
        return None;
    }
    let x = u32::try_from(rect.origin.x).ok()?;
    let y = u32::try_from(rect.origin.y).ok()?;
    let scale = |value: u32, source: u32, target: u32| {
        u32::try_from(u64::from(value) * u64::from(target) / u64::from(source)).unwrap_or(u32::MAX)
    };
    Some(Rect2D::new(
        i32::try_from(scale(x, logical.width, physical.width)).unwrap_or(i32::MAX),
        i32::try_from(scale(y, logical.height, physical.height)).unwrap_or(i32::MAX),
        scale(rect.size.width, logical.width, physical.width),
        scale(rect.size.height, logical.height, physical.height),
    ))
}

#[derive(Debug, thiserror::Error)]
pub enum GreeterSurfaceError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Present(#[from] SurfacePresenterError),
    #[error(transparent)]
    Transaction(#[from] GreeterTransactionError),
    #[error(transparent)]
    Model(#[from] GreeterModelError),
    #[error(transparent)]
    Client(#[from] crate::GreetdClientError),
    #[error("Tensor Greeter Wayland surface has no renderer handle")]
    MissingSurfaceHandle,
    #[error("Tensor Greeter Wayland surface has no physical buffer extent")]
    MissingBufferExtent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_buffer_is_utf8_boundary_safe() {
        let mut buffer = SecretBuffer::default();
        buffer.push("密码x");
        buffer.backspace();
        assert_eq!(buffer.take_string(), "密码");
    }

    #[test]
    fn user_hit_testing_stays_inside_surface() {
        let extent = LogicalSize::new(900, 620);
        assert_eq!(user_at(extent, 2, (40.0, 40.0)), Some(0));
        assert_eq!(user_at(extent, 2, (899.0, 619.0)), None);
        assert_eq!(session_at(extent, 2, (500.0, 40.0)), Some(0));
        assert_eq!(session_at(extent, 2, (40.0, 40.0)), None);
    }

    #[test]
    fn session_rows_are_drawn_after_the_prompt_panel() {
        let mut draws = Vec::new();
        build_draws(
            &mut draws,
            LogicalSize::new(900, 620),
            Extent2D::new(900, 620),
            GreeterDrawState {
                users: 1,
                selected_user: Some(0),
                sessions: 2,
                selected_session: 1,
                response_active: false,
                prompt_kind: None,
            },
        );
        assert_eq!(draws.len(), 4);
        assert_eq!(draws[1].color, [0.06, 0.075, 0.10, 1.0]);
        assert_eq!(draws[3].color, [0.11, 0.30, 0.39, 1.0]);
    }
}
