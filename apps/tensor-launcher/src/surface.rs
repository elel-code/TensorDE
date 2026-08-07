use std::{sync::Arc, time::Duration};

use tensor_present::{ColorRect, SurfaceFrame, SurfacePresenter, SurfacePresenterError};
use vulkan_renderer::{Extent2D, Rect2D};
use wayland_client_runtime::{
    DecorationPreference, Event, KeyState, KeyboardEvent, LogicalPosition, LogicalRect,
    LogicalSize, NativeRuntime, PointerEvent, PointerEventKind, RuntimeError, SurfaceEvent,
    SurfaceId, TextInputEvent, ToplevelAttributes,
};

use crate::{
    LaunchError, LaunchPlan, LauncherCatalogError, LauncherCatalogWatcher, LauncherClient,
    LauncherSession, LauncherSessionError,
};

const INITIAL_SIZE: LogicalSize = LogicalSize::new(720, 540);
const MIN_SIZE: LogicalSize = LogicalSize::new(480, 320);
const SEARCH_MARGIN: u32 = 24;
const SEARCH_HEIGHT: u32 = 56;
const RESULT_GAP: u32 = 8;
const RESULT_HEIGHT: u32 = 48;
const BTN_LEFT: u32 = 0x110;

/// Ordinary Wayland/Vulkan launcher surface around [`LauncherSession`].
pub struct LauncherSurface {
    wayland: NativeRuntime,
    surface: SurfaceId,
    session: LauncherSession,
    watcher: Option<LauncherCatalogWatcher>,
    logical_size: LogicalSize,
    configured: bool,
    text_input_active: bool,
    pointer_pressed: Option<usize>,
    dirty: bool,
    exit: bool,
    pending_launch: Option<LaunchPlan>,
    events: Vec<Event>,
    draws: Vec<ColorRect>,
    presenter: Option<SurfacePresenter>,
}

impl LauncherSurface {
    pub fn open(session: LauncherSession) -> Result<Self, LauncherSurfaceError> {
        Self::open_with_watcher(session, None)
    }

    pub fn open_with_watcher(
        session: LauncherSession,
        watcher: Option<LauncherCatalogWatcher>,
    ) -> Result<Self, LauncherSurfaceError> {
        let mut wayland = NativeRuntime::connect()?;
        let surface = wayland.create_toplevel(ToplevelAttributes {
            title: "Tensor Launcher".into(),
            app_id: "tensor-launcher".into(),
            initial_size: Some(INITIAL_SIZE),
            min_size: Some(MIN_SIZE),
            max_size: None,
            decorations: DecorationPreference::Server,
        })?;
        Ok(Self {
            wayland,
            surface,
            session,
            watcher,
            logical_size: INITIAL_SIZE,
            configured: false,
            text_input_active: false,
            pointer_pressed: None,
            dirty: true,
            exit: false,
            pending_launch: None,
            events: Vec::with_capacity(64),
            draws: Vec::with_capacity(130),
            presenter: None,
        })
    }

    pub fn run(mut self, io: &compio::runtime::Runtime) -> Result<(), LauncherSurfaceError> {
        while !self.exit {
            let timeout = if self.dirty && self.configured {
                Some(Duration::ZERO)
            } else if self.watcher.is_some() {
                Some(Duration::from_millis(500))
            } else {
                None
            };
            self.wayland.dispatch(timeout)?;
            self.refresh_catalog()?;
            self.events.clear();
            self.wayland.drain_events_into(&mut self.events);
            let events = std::mem::take(&mut self.events);
            for event in &events {
                self.handle_event(event)?;
                if self.exit || self.pending_launch.is_some() {
                    break;
                }
            }
            self.events = events;
            if let Some(plan) = self.pending_launch.take() {
                io.block_on(async {
                    let mut client = LauncherClient::connect().await?;
                    client.submit(plan).await
                })?;
                self.exit = true;
            }
            if self.dirty && self.configured && !self.wayland.is_present_pending(self.surface) {
                self.present()?;
            }
        }
        self.shutdown()
    }

    fn refresh_catalog(&mut self) -> Result<(), LauncherSurfaceError> {
        let Some(watcher) = self.watcher.as_mut() else {
            return Ok(());
        };
        if watcher.refresh_if_changed()? {
            self.session.replace_catalog(watcher.catalog().clone());
            self.dirty = true;
        }
        Ok(())
    }

    fn handle_event(&mut self, event: &Event) -> Result<(), LauncherSurfaceError> {
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
                self.exit = true;
            }
            Event::Keyboard(keyboard) => self.handle_keyboard(keyboard)?,
            Event::TextInput(text_input) => self.handle_text_input(text_input)?,
            Event::Pointer(pointer) if pointer.surface == self.surface => {
                self.handle_pointer(pointer)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_keyboard(&mut self, event: &KeyboardEvent) -> Result<(), LauncherSurfaceError> {
        match event {
            KeyboardEvent::Enter { surface, .. } if *surface == self.surface => {
                self.sync_text_input()?;
            }
            KeyboardEvent::Leave { surface, .. } if *surface == self.surface => {
                self.text_input_active = false;
            }
            KeyboardEvent::Key {
                surface,
                state,
                keysym,
                text,
                ..
            } if *surface == self.surface
                && matches!(state, KeyState::Pressed | KeyState::Repeated) =>
            {
                self.apply_key(*keysym, text.as_deref())?;
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_key(&mut self, keysym: u32, text: Option<&str>) -> Result<(), LauncherSurfaceError> {
        use xkeysym::key;

        match keysym {
            key::Escape => self.exit = true,
            key::Return | key::KP_Enter => {
                self.pending_launch = self.session.selected_launch_plan()?;
            }
            key::Up | key::KP_Up => {
                self.session.select_previous();
                self.dirty = true;
            }
            key::Down | key::KP_Down => {
                self.session.select_next();
                self.dirty = true;
            }
            key::BackSpace if self.session.backspace()? => {
                self.sync_text_input()?;
                self.dirty = true;
            }
            _ if !self.text_input_active => {
                if let Some(text) = text.filter(|text| {
                    !text.is_empty() && text.chars().all(|character| !character.is_control())
                }) {
                    self.session.insert_text(text)?;
                    self.sync_text_input()?;
                    self.dirty = true;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_text_input(&mut self, event: &TextInputEvent) -> Result<(), LauncherSurfaceError> {
        match event {
            TextInputEvent::Entered { surface } if *surface == self.surface => {
                self.text_input_active = true;
                self.sync_text_input()?;
            }
            TextInputEvent::Left { surface } if *surface == self.surface => {
                self.text_input_active = false;
            }
            TextInputEvent::Done(done) if done.surface == self.surface => {
                self.session.apply_text_input(done)?;
                self.sync_text_input()?;
                self.dirty = true;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_pointer(&mut self, event: &PointerEvent) -> Result<(), LauncherSurfaceError> {
        match event.kind {
            PointerEventKind::Press {
                button: BTN_LEFT, ..
            } => {
                self.pointer_pressed = result_at(
                    self.logical_size,
                    self.session.results().len(),
                    event.position,
                );
                if let Some(index) = self.pointer_pressed {
                    self.session.select_index(index);
                    self.dirty = true;
                }
            }
            PointerEventKind::Release {
                button: BTN_LEFT, ..
            } => {
                let released = result_at(
                    self.logical_size,
                    self.session.results().len(),
                    event.position,
                );
                if released.is_some() && released == self.pointer_pressed {
                    self.pending_launch = self.session.selected_launch_plan()?;
                }
                self.pointer_pressed = None;
            }
            PointerEventKind::Leave => self.pointer_pressed = None,
            _ => {}
        }
        Ok(())
    }

    fn sync_text_input(&mut self) -> Result<(), LauncherSurfaceError> {
        let state = self.session.text_input_state()?;
        match self
            .wayland
            .set_text_input_state(self.surface, Some(&state))
        {
            Ok(()) | Err(RuntimeError::Unsupported(_)) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn apply_surface_geometry(&mut self) -> Result<(), LauncherSurfaceError> {
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

    fn present(&mut self) -> Result<(), LauncherSurfaceError> {
        let (width, height) = self
            .wayland
            .buffer_size(self.surface)
            .ok_or(LauncherSurfaceError::MissingBufferExtent)?;
        let extent = Extent2D::new(width, height);
        let handle = Arc::new(
            self.wayland
                .surface_handle(self.surface)
                .ok_or(LauncherSurfaceError::MissingSurfaceHandle)?,
        );
        match self.presenter.as_mut() {
            Some(presenter) => {
                presenter.ensure_surface(self.surface, handle, extent, "tensor-launcher")?
            }
            None => {
                self.presenter = Some(SurfacePresenter::new(
                    self.surface,
                    handle,
                    extent,
                    "tensor-launcher",
                )?);
            }
        }
        build_draws(
            &mut self.draws,
            self.logical_size,
            extent,
            self.session.results().len(),
            self.session.selected_index(),
            !self.session.query().is_empty(),
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
                    clear: [0.035, 0.039, 0.045, 0.98],
                    rectangles: &self.draws,
                },
            )?;
        self.dirty = false;
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), LauncherSurfaceError> {
        if self.text_input_active {
            let _ = self.wayland.set_text_input_state(self.surface, None);
        }
        if let Some(presenter) = self.presenter.as_mut() {
            presenter.remove_surface(self.surface)?;
        }
        self.wayland.destroy_surface(self.surface)?;
        Ok(())
    }
}

fn result_rect(index: usize, extent: LogicalSize) -> Option<LogicalRect> {
    let width = extent.width.checked_sub(SEARCH_MARGIN * 2)?;
    let top = SEARCH_MARGIN + SEARCH_HEIGHT + RESULT_GAP;
    let stride = RESULT_HEIGHT + RESULT_GAP;
    let y = top.checked_add(u32::try_from(index).ok()?.checked_mul(stride)?)?;
    (y + RESULT_HEIGHT <= extent.height)
        .then(|| LogicalRect::new(SEARCH_MARGIN as i32, y as i32, width, RESULT_HEIGHT))
}

fn result_at(extent: LogicalSize, count: usize, position: (f64, f64)) -> Option<usize> {
    (0..count)
        .find(|index| result_rect(*index, extent).is_some_and(|rect| contains(rect, position)))
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

fn build_draws(
    target: &mut Vec<ColorRect>,
    logical: LogicalSize,
    physical: Extent2D,
    result_count: usize,
    selected: Option<usize>,
    query_present: bool,
) {
    target.clear();
    if let Some(search) = physical_rect(
        LogicalRect::new(
            SEARCH_MARGIN as i32,
            SEARCH_MARGIN as i32,
            logical.width.saturating_sub(SEARCH_MARGIN * 2),
            SEARCH_HEIGHT,
        ),
        logical,
        physical,
    ) {
        target.push(ColorRect {
            rect: search,
            color: if query_present {
                [0.11, 0.13, 0.15, 1.0]
            } else {
                [0.075, 0.085, 0.095, 1.0]
            },
        });
    }
    for index in 0..result_count {
        let Some(logical_rect) = result_rect(index, logical) else {
            break;
        };
        let Some(rect) = physical_rect(logical_rect, logical, physical) else {
            continue;
        };
        target.push(ColorRect {
            rect,
            color: if selected == Some(index) {
                [0.10, 0.36, 0.48, 1.0]
            } else {
                [0.065, 0.073, 0.083, 1.0]
            },
        });
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
pub enum LauncherSurfaceError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Present(#[from] SurfacePresenterError),
    #[error(transparent)]
    Session(#[from] LauncherSessionError),
    #[error(transparent)]
    TextInput(#[from] wayland_client_runtime::TextInputError),
    #[error(transparent)]
    Launch(#[from] LaunchError),
    #[error(transparent)]
    Catalog(#[from] LauncherCatalogError),
    #[error("Tensor Launcher Wayland surface has no renderer handle")]
    MissingSurfaceHandle,
    #[error("Tensor Launcher Wayland surface has no physical buffer extent")]
    MissingBufferExtent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_geometry_is_bounded_and_hit_tested() {
        let extent = LogicalSize::new(720, 540);
        assert_eq!(result_at(extent, 10, (30.0, 100.0)), Some(0));
        assert_eq!(result_at(extent, 10, (30.0, 539.0)), None);
        assert_eq!(result_at(extent, 10, (f64::NAN, 100.0)), None);
    }

    #[test]
    fn retained_draws_mark_exactly_one_selected_row() {
        let mut draws = Vec::new();
        build_draws(
            &mut draws,
            LogicalSize::new(720, 540),
            Extent2D::new(1_440, 1_080),
            4,
            Some(2),
            true,
        );
        assert_eq!(draws.len(), 5);
        assert_eq!(draws[0].rect, Rect2D::new(48, 48, 1_344, 112));
        assert_eq!(draws.iter().filter(|draw| draw.color[2] == 0.48).count(), 1);
    }
}
