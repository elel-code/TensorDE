use std::{sync::Arc, time::Duration};

use tensor_ipc::land::CompioClient;
use tensor_present::{ColorRect, SurfaceFrame, SurfacePresenter, SurfacePresenterError};
use vulkan_renderer::{Extent2D, Rect2D};
use wayland_client_runtime::{
    DecorationPreference, Event, KeyState, KeyboardEvent, LogicalPosition, LogicalRect,
    LogicalSize, Modifiers, NativeRuntime, PointerEvent, PointerEventKind, RuntimeError,
    SurfaceEvent, SurfaceId, TextInputChangeCause, TextInputContentHint, TextInputContentPurpose,
    TextInputContentType, TextInputDone, TextInputEvent, TextInputPreedit, TextInputState,
    TextInputSurroundingText, ToplevelAttributes,
};

use crate::{
    ConfigDocumentError, ConfigDocumentState, ProductKind, SaveAndReloadOutcome, SaveConfirmation,
    SettingsWorkspace,
};

mod text;
use text::surrounding_window;

const INITIAL_SIZE: LogicalSize = LogicalSize::new(980, 680);
const MIN_SIZE: LogicalSize = LogicalSize::new(700, 480);
const MARGIN: u32 = 20;
const SEARCH_HEIGHT: u32 = 48;
const SIDEBAR_WIDTH: u32 = 260;
const ROW_HEIGHT: u32 = 48;
const GAP: u32 = 8;
const BTN_LEFT: u32 = 0x110;
const EDITOR_X_OFFSET: u32 = 288;
const EDITOR_Y_OFFSET: u32 = 84;

/// Native product navigator for the settings workspace.
pub struct SettingsSurface {
    wayland: NativeRuntime,
    surface: SurfaceId,
    workspace: SettingsWorkspace,
    visible_products: Vec<ProductKind>,
    logical_size: LogicalSize,
    configured: bool,
    editor_focused: bool,
    text_input_active: bool,
    editor_cursor: usize,
    preedit: Option<TextInputPreedit>,
    modifiers: Modifiers,
    land_client: Option<CompioClient>,
    save_confirmation_pending: bool,
    status: Option<String>,
    dirty: bool,
    exit: bool,
    events: Vec<Event>,
    draws: Vec<ColorRect>,
    presenter: Option<SurfacePresenter>,
}

impl SettingsSurface {
    pub fn open(workspace: SettingsWorkspace) -> Result<Self, SettingsSurfaceError> {
        let mut wayland = NativeRuntime::connect()?;
        let surface = wayland.create_toplevel(ToplevelAttributes {
            title: "Tensor Settings".into(),
            app_id: "tensor-settings".into(),
            initial_size: Some(INITIAL_SIZE),
            min_size: Some(MIN_SIZE),
            max_size: None,
            decorations: DecorationPreference::Server,
        })?;
        let visible_products = workspace.filtered_products().collect();
        let editor_cursor = workspace.selected_document().draft().len();
        Ok(Self {
            wayland,
            surface,
            workspace,
            visible_products,
            logical_size: INITIAL_SIZE,
            configured: false,
            editor_focused: false,
            text_input_active: false,
            editor_cursor,
            preedit: None,
            modifiers: Modifiers::default(),
            land_client: None,
            save_confirmation_pending: false,
            status: None,
            dirty: true,
            exit: false,
            events: Vec::with_capacity(64),
            draws: Vec::with_capacity(16),
            presenter: None,
        })
    }

    pub fn run(mut self, io: &compio::runtime::Runtime) -> Result<(), SettingsSurfaceError> {
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
        self.shutdown()
    }

    fn handle_event(
        &mut self,
        event: &Event,
        io: &compio::runtime::Runtime,
    ) -> Result<(), SettingsSurfaceError> {
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
            Event::Keyboard(KeyboardEvent::Enter { surface, .. }) if *surface == self.surface => {
                self.sync_text_input()?;
            }
            Event::Keyboard(KeyboardEvent::Leave { surface, .. }) if *surface == self.surface => {
                self.text_input_active = false;
            }
            Event::Keyboard(KeyboardEvent::Modifiers { modifiers, .. }) => {
                self.modifiers = *modifiers;
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
            Event::TextInput(TextInputEvent::Entered { surface }) if *surface == self.surface => {
                self.text_input_active = true;
                self.sync_text_input()?;
            }
            Event::TextInput(TextInputEvent::Left { surface }) if *surface == self.surface => {
                self.text_input_active = false;
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
    ) -> Result<(), SettingsSurfaceError> {
        use xkeysym::key;

        if self.modifiers.ctrl && matches!(keysym, key::s | key::S) {
            self.save_selected(io);
            return Ok(());
        }
        if self.editor_focused {
            match keysym {
                key::Escape | key::Tab => {
                    self.editor_focused = false;
                    self.preedit = None;
                    self.disable_text_input();
                    self.dirty = true;
                }
                key::BackSpace => self.edit_draft(1, 0, "")?,
                key::Delete | key::KP_Delete => self.edit_draft(0, 1, "")?,
                key::Left | key::KP_Left => self.move_cursor_left(),
                key::Right | key::KP_Right => self.move_cursor_right(),
                key::Home | key::KP_Home => self.move_cursor_line_start(),
                key::End | key::KP_End => self.move_cursor_line_end(),
                key::Up | key::KP_Up => self.move_cursor_vertical(-1),
                key::Down | key::KP_Down => self.move_cursor_vertical(1),
                key::Return | key::KP_Enter => self.edit_draft(0, 0, "\n")?,
                key::F5 => self.reload_selected()?,
                _ if !self.text_input_active => {
                    if let Some(text) = text.filter(|value| valid_text(value)) {
                        self.edit_draft(0, 0, text)?;
                    }
                }
                _ => {}
            }
            return Ok(());
        }

        match keysym {
            key::Escape if self.workspace.query().is_empty() => self.exit = true,
            key::Escape => self.set_query(String::new()),
            key::Tab => self.focus_editor(),
            key::Up | key::KP_Up => self.select_relative(-1),
            key::Down | key::KP_Down => self.select_relative(1),
            key::BackSpace => {
                let mut query = self.workspace.query().to_owned();
                if query.pop().is_some() {
                    self.set_query(query);
                }
            }
            key::F5 => {
                self.reload_selected()?;
            }
            _ => {
                if let Some(text) = text.filter(|value| {
                    !value.is_empty() && value.chars().all(|character| !character.is_control())
                }) {
                    let mut query = self.workspace.query().to_owned();
                    query.push_str(text);
                    self.set_query(query);
                }
            }
        }
        Ok(())
    }

    fn apply_text_input(&mut self, done: &TextInputDone) -> Result<(), SettingsSurfaceError> {
        if !self.editor_focused {
            return Ok(());
        }
        let delete = done.delete_surrounding;
        let cursor = self.editor_cursor;
        let commit = done.commit.as_deref().unwrap_or_default();
        self.editor_cursor = self.workspace.selected_document_mut().apply_edit(
            cursor,
            delete.map_or(0, |value| value.before_bytes),
            delete.map_or(0, |value| value.after_bytes),
            commit,
        )?;
        self.preedit = done.preedit.clone();
        self.sync_text_input()?;
        self.dirty = true;
        Ok(())
    }

    fn edit_draft(
        &mut self,
        delete_before: usize,
        delete_after: usize,
        commit: &str,
    ) -> Result<(), SettingsSurfaceError> {
        self.editor_cursor = self.workspace.selected_document_mut().apply_edit(
            self.editor_cursor,
            delete_before,
            delete_after,
            commit,
        )?;
        self.preedit = None;
        self.sync_text_input()?;
        self.dirty = true;
        Ok(())
    }

    fn focus_editor(&mut self) {
        match self.workspace.selected_document().state() {
            ConfigDocumentState::ReadOnly => {
                self.status = Some("Selected configuration is read-only".into());
                self.dirty = true;
                return;
            }
            ConfigDocumentState::Unsupported => {
                self.status = Some("Selected product has no editable KDL document".into());
                self.dirty = true;
                return;
            }
            ConfigDocumentState::Clean
            | ConfigDocumentState::Dirty
            | ConfigDocumentState::Invalid => {}
        }
        self.editor_focused = true;
        self.editor_cursor = self.workspace.selected_document().draft().len();
        self.preedit = None;
        self.status = None;
        let _ = self.sync_text_input();
        self.dirty = true;
    }

    fn reload_selected(&mut self) -> Result<(), SettingsSurfaceError> {
        self.workspace.selected_document_mut().reload()?;
        self.editor_cursor = self.workspace.selected_document().draft().len();
        self.preedit = None;
        self.save_confirmation_pending = false;
        self.status = None;
        self.sync_text_input()?;
        self.dirty = true;
        Ok(())
    }

    fn save_selected(&mut self, io: &compio::runtime::Runtime) {
        let endpoint = self.workspace.selected_document().endpoint().clone();
        if endpoint.reload == crate::ReloadRoute::TensorMsgLand
            && self.workspace.selected_document().is_dirty()
            && self.land_client.is_none()
        {
            let Some(socket) = endpoint.socket_path.clone() else {
                self.status = Some("Tensorland IPC socket is not configured".into());
                self.dirty = true;
                return;
            };
            match io.block_on(CompioClient::connect(socket)) {
                Ok(client) => self.land_client = Some(client),
                Err(error) => {
                    self.status = Some(format!("Tensorland reload unavailable: {error}"));
                    self.dirty = true;
                    return;
                }
            }
        }
        let confirmation = if self.save_confirmation_pending {
            SaveConfirmation::PrivilegedConfirmed
        } else {
            SaveConfirmation::Ordinary
        };
        let result = io.block_on(
            self.workspace
                .save_selected_and_reload(confirmation, self.land_client.as_mut()),
        );
        match result {
            Ok(SaveAndReloadOutcome::Unchanged) => {
                self.status = Some("No configuration changes".into());
                self.save_confirmation_pending = false;
            }
            Ok(SaveAndReloadOutcome::Saved { reload_requested }) => {
                self.status = Some(if reload_requested {
                    "Saved; Tensorland reload accepted".into()
                } else {
                    "Configuration saved".into()
                });
                self.save_confirmation_pending = false;
            }
            Err(ConfigDocumentError::PrivilegedConfirmationRequired { .. }) => {
                self.save_confirmation_pending = true;
                self.status = Some("Press Ctrl+S again to confirm privileged change".into());
            }
            Err(error) => self.status = Some(error.to_string()),
        }
        self.dirty = true;
    }

    fn sync_text_input(&mut self) -> Result<(), SettingsSurfaceError> {
        if !self.editor_focused {
            self.disable_text_input();
            return Ok(());
        }
        let draft = self.workspace.selected_document().draft();
        let (window_start, window_end) = surrounding_window(draft, self.editor_cursor);
        let surrounding = TextInputSurroundingText::new(
            draft[window_start..window_end].to_owned(),
            self.editor_cursor - window_start,
            self.editor_cursor - window_start,
        )?;
        let cursor = u32::try_from(self.editor_cursor).unwrap_or(u32::MAX);
        let cursor_line = cursor / 96;
        let cursor_x = EDITOR_X_OFFSET + (cursor % 96) * 6;
        let cursor_y = EDITOR_Y_OFFSET + cursor_line.saturating_mul(18);
        let state = TextInputState::new()
            .with_surrounding_text(surrounding)
            .with_content_type(TextInputContentType {
                hints: TextInputContentHint::MULTILINE,
                purpose: TextInputContentPurpose::Normal,
            })
            .with_change_cause(TextInputChangeCause::Other)
            .with_cursor_rectangle(LogicalRect::new(cursor_x as i32, cursor_y as i32, 2, 16))?;
        match self
            .wayland
            .set_text_input_state(self.surface, Some(&state))
        {
            Ok(()) => self.text_input_active = true,
            Err(RuntimeError::Unsupported(_)) => self.text_input_active = false,
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    fn disable_text_input(&mut self) {
        let _ = self.wayland.set_text_input_state(self.surface, None);
        self.text_input_active = false;
    }

    fn move_cursor_left(&mut self) {
        if let Some(character) = self.workspace.selected_document().draft()[..self.editor_cursor]
            .chars()
            .next_back()
        {
            self.editor_cursor -= character.len_utf8();
            self.dirty = true;
            let _ = self.sync_text_input();
        }
    }

    fn move_cursor_right(&mut self) {
        if let Some(character) = self.workspace.selected_document().draft()[self.editor_cursor..]
            .chars()
            .next()
        {
            self.editor_cursor += character.len_utf8();
            self.dirty = true;
            let _ = self.sync_text_input();
        }
    }

    fn move_cursor_line_start(&mut self) {
        let draft = self.workspace.selected_document().draft();
        self.editor_cursor = draft[..self.editor_cursor]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        self.dirty = true;
        let _ = self.sync_text_input();
    }

    fn move_cursor_line_end(&mut self) {
        let draft = self.workspace.selected_document().draft();
        self.editor_cursor = draft[self.editor_cursor..]
            .find('\n')
            .map_or(draft.len(), |index| self.editor_cursor + index);
        self.dirty = true;
        let _ = self.sync_text_input();
    }

    fn move_cursor_vertical(&mut self, direction: isize) {
        let draft = self.workspace.selected_document().draft();
        let line_start = draft[..self.editor_cursor]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let column = self.editor_cursor - line_start;
        let target_start = if direction < 0 {
            let previous_end = line_start.saturating_sub(1);
            draft[..previous_end]
                .rfind('\n')
                .map_or(0, |index| index + 1)
        } else {
            let Some(next) = draft[self.editor_cursor..].find('\n') else {
                return;
            };
            self.editor_cursor + next + 1
        };
        let target_end = draft[target_start..]
            .find('\n')
            .map_or(draft.len(), |index| target_start + index);
        self.editor_cursor = (target_start + column).min(target_end);
        self.dirty = true;
        let _ = self.sync_text_input();
    }

    fn set_query(&mut self, query: String) {
        self.workspace.set_query(query);
        self.visible_products.clear();
        self.visible_products
            .extend(self.workspace.filtered_products());
        if !self.visible_products.contains(&self.workspace.selected())
            && let Some(product) = self.visible_products.first().copied()
        {
            self.workspace.select(product);
        }
        self.dirty = true;
    }

    fn select_relative(&mut self, direction: isize) {
        if self.visible_products.is_empty() {
            return;
        }
        let current = self
            .visible_products
            .iter()
            .position(|product| *product == self.workspace.selected())
            .unwrap_or(0);
        let len = self.visible_products.len() as isize;
        let next = (current as isize + direction).rem_euclid(len) as usize;
        self.workspace.select(self.visible_products[next]);
        self.editor_focused = false;
        self.disable_text_input();
        self.editor_cursor = self.workspace.selected_document().draft().len();
        self.preedit = None;
        self.save_confirmation_pending = false;
        self.status = None;
        let _ = self.sync_text_input();
        self.dirty = true;
    }

    fn handle_pointer(&mut self, event: &PointerEvent) {
        if !matches!(
            event.kind,
            PointerEventKind::Press {
                button: BTN_LEFT,
                ..
            }
        ) {
            return;
        }
        if let Some(index) = product_at(
            self.logical_size,
            self.visible_products.len(),
            event.position,
        ) {
            self.workspace.select(self.visible_products[index]);
            self.editor_focused = false;
            self.disable_text_input();
            self.editor_cursor = self.workspace.selected_document().draft().len();
            self.preedit = None;
            self.save_confirmation_pending = false;
            self.status = None;
            let _ = self.sync_text_input();
            self.dirty = true;
        }
    }

    fn apply_surface_geometry(&mut self) -> Result<(), SettingsSurfaceError> {
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

    fn present(&mut self) -> Result<(), SettingsSurfaceError> {
        let (width, height) = self
            .wayland
            .buffer_size(self.surface)
            .ok_or(SettingsSurfaceError::MissingBufferExtent)?;
        let extent = Extent2D::new(width, height);
        let handle = Arc::new(
            self.wayland
                .surface_handle(self.surface)
                .ok_or(SettingsSurfaceError::MissingSurfaceHandle)?,
        );
        match self.presenter.as_mut() {
            Some(presenter) => {
                presenter.ensure_surface(self.surface, handle, extent, "tensor-settings")?
            }
            None => {
                self.presenter = Some(SurfacePresenter::new(
                    self.surface,
                    handle,
                    extent,
                    "tensor-settings",
                )?);
            }
        }
        build_draws(
            &mut self.draws,
            self.logical_size,
            extent,
            SettingsDrawState {
                visible: &self.visible_products,
                selected: self.workspace.selected(),
                state: self.workspace.selected_document().state(),
                query_present: !self.workspace.query().is_empty(),
                editor_focused: self.editor_focused,
                editor_cursor: self.editor_cursor,
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
                    clear: [0.044, 0.047, 0.052, 1.0],
                    rectangles: &self.draws,
                },
            )?;
        self.dirty = false;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), SettingsSurfaceError> {
        if let Some(presenter) = self.presenter.as_mut() {
            presenter.remove_surface(self.surface)?;
        }
        self.wayland.destroy_surface(self.surface)?;
        Ok(())
    }
}

fn product_rect(index: usize, extent: LogicalSize) -> Option<LogicalRect> {
    let width = SIDEBAR_WIDTH.min(extent.width.saturating_sub(MARGIN * 2));
    let y =
        MARGIN + SEARCH_HEIGHT + GAP + u32::try_from(index).ok()?.checked_mul(ROW_HEIGHT + GAP)?;
    (width > 0 && y + ROW_HEIGHT <= extent.height)
        .then(|| LogicalRect::new(MARGIN as i32, y as i32, width, ROW_HEIGHT))
}

fn product_at(extent: LogicalSize, count: usize, position: (f64, f64)) -> Option<usize> {
    (0..count)
        .find(|index| product_rect(*index, extent).is_some_and(|rect| contains(rect, position)))
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

struct SettingsDrawState<'a> {
    visible: &'a [ProductKind],
    selected: ProductKind,
    state: ConfigDocumentState,
    query_present: bool,
    editor_focused: bool,
    editor_cursor: usize,
}

fn build_draws(
    target: &mut Vec<ColorRect>,
    logical: LogicalSize,
    physical: Extent2D,
    visual: SettingsDrawState<'_>,
) {
    target.clear();
    push_draw(
        target,
        LogicalRect::new(MARGIN as i32, MARGIN as i32, SIDEBAR_WIDTH, SEARCH_HEIGHT),
        logical,
        physical,
        if visual.query_present {
            [0.11, 0.13, 0.15, 1.0]
        } else {
            [0.075, 0.08, 0.09, 1.0]
        },
    );
    for (index, product) in visual.visible.iter().enumerate() {
        let Some(rect) = product_rect(index, logical) else {
            break;
        };
        push_draw(
            target,
            rect,
            logical,
            physical,
            if *product == visual.selected {
                [0.11, 0.33, 0.42, 1.0]
            } else {
                [0.065, 0.071, 0.08, 1.0]
            },
        );
    }
    let main_x = MARGIN + SIDEBAR_WIDTH + GAP * 2;
    push_draw(
        target,
        LogicalRect::new(
            main_x as i32,
            MARGIN as i32,
            logical.width.saturating_sub(main_x + MARGIN),
            logical.height.saturating_sub(MARGIN * 2),
        ),
        logical,
        physical,
        state_color(visual.state),
    );
    if visual.editor_focused {
        // A retained caret gives the editor a visible focus affordance while
        // the text atlas remains owned by the shared renderer.
        let cursor = u32::try_from(visual.editor_cursor).unwrap_or(u32::MAX);
        let line = cursor / 96;
        let x = EDITOR_X_OFFSET + (cursor % 96) * 6;
        let y = EDITOR_Y_OFFSET + line.saturating_mul(18);
        push_draw(
            target,
            LogicalRect::new(x as i32, y as i32, 2, 16),
            logical,
            physical,
            [0.55, 0.86, 0.92, 1.0],
        );
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

const fn state_color(state: ConfigDocumentState) -> [f32; 4] {
    match state {
        ConfigDocumentState::Clean => [0.06, 0.10, 0.09, 1.0],
        ConfigDocumentState::Dirty => [0.16, 0.13, 0.05, 1.0],
        ConfigDocumentState::Invalid => [0.20, 0.06, 0.07, 1.0],
        ConfigDocumentState::ReadOnly => [0.08, 0.09, 0.12, 1.0],
        ConfigDocumentState::Unsupported => [0.10, 0.075, 0.11, 1.0],
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
pub enum SettingsSurfaceError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Present(#[from] SurfacePresenterError),
    #[error(transparent)]
    Document(#[from] ConfigDocumentError),
    #[error(transparent)]
    TextInput(#[from] wayland_client_runtime::TextInputError),
    #[error("Tensor Settings Wayland surface has no renderer handle")]
    MissingSurfaceHandle,
    #[error("Tensor Settings Wayland surface has no physical buffer extent")]
    MissingBufferExtent,
}

fn valid_text(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\0')
        && value
            .chars()
            .all(|character| !character.is_control() || matches!(character, '\n' | '\t'))
}

#[cfg(test)]
mod tests;
