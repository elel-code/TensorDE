use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::windowing::{
    ActiveEventLoop, CursorIcon, Modifiers, PhysicalSize, Theme, Window, WindowAttributes,
    WindowEvent, WindowId,
};

use crate::FikaRenderer;
use crate::ui::window_semantics::{ShellDialogWindowRole, ShellWindowRole, apply_window_semantics};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ShellDialogWindowKind {
    Create,
    OpenWith,
    Properties,
    Rename,
    Settings,
    TaskDetail,
    TrashConflict,
}

impl ShellDialogWindowKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::OpenWith => "open-with",
            Self::Properties => "properties",
            Self::Rename => "rename",
            Self::Settings => "settings",
            Self::TaskDetail => "task-detail",
            Self::TrashConflict => "trash-conflict",
        }
    }

    fn window_role(self) -> ShellDialogWindowRole {
        match self {
            Self::Create => ShellDialogWindowRole::Create,
            Self::OpenWith => ShellDialogWindowRole::OpenWith,
            Self::Properties => ShellDialogWindowRole::Properties,
            Self::Rename => ShellDialogWindowRole::Rename,
            Self::Settings => ShellDialogWindowRole::Settings,
            Self::TaskDetail => ShellDialogWindowRole::TaskDetail,
            Self::TrashConflict => ShellDialogWindowRole::TrashConflict,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ShellDialogWindowSpec {
    title: String,
    surface_size: PhysicalSize<u32>,
    min_surface_size: Option<PhysicalSize<u32>>,
    max_surface_size: Option<PhysicalSize<u32>>,
    resizable: bool,
    theme: Option<Theme>,
}

impl ShellDialogWindowSpec {
    pub(crate) fn fixed(title: String, surface_size: PhysicalSize<u32>, theme: Theme) -> Self {
        Self {
            title,
            surface_size,
            min_surface_size: Some(surface_size),
            max_surface_size: Some(surface_size),
            resizable: false,
            theme: Some(theme),
        }
    }

    fn window_attributes(
        &self,
        event_loop: &ActiveEventLoop,
        kind: ShellDialogWindowKind,
    ) -> WindowAttributes {
        let mut attrs = WindowAttributes::default()
            .with_title(self.title.clone())
            .with_surface_size(self.surface_size)
            .with_resizable(self.resizable)
            .with_theme(self.theme);
        if let Some(min_surface_size) = self.min_surface_size {
            attrs = attrs.with_min_surface_size(min_surface_size);
        }
        if let Some(max_surface_size) = self.max_surface_size {
            attrs = attrs.with_max_surface_size(max_surface_size);
        }
        apply_window_semantics(
            event_loop,
            attrs,
            ShellWindowRole::Dialog(kind.window_role()),
        )
    }
}

pub(crate) struct ShellDetachedDialogWindow {
    kind: ShellDialogWindowKind,
    renderer: FikaRenderer,
    window: Arc<Window>,
    layout_size: PhysicalSize<u32>,
    cursor_icon: CursorIcon,
}

impl ShellDetachedDialogWindow {
    pub(crate) fn create(
        event_loop: &ActiveEventLoop,
        _shared_renderer: Option<&FikaRenderer>,
        kind: ShellDialogWindowKind,
        spec: &ShellDialogWindowSpec,
    ) -> Result<Self, String> {
        let window = event_loop
            .create_window(spec.window_attributes(event_loop, kind))
            .map_err(|error| format!("{} dialog window create failed: {error}", kind.as_str()))?;
        let renderer = FikaRenderer::new(window.clone())
            .map_err(|error| format!("{} dialog renderer init failed: {error}", kind.as_str()))?;
        fika_dialog_trace!(
            "[fika] dialog-window-created kind={} window={:?} surface={}x{}",
            kind.as_str(),
            window.id(),
            spec.surface_size.width,
            spec.surface_size.height
        );
        Ok(Self {
            kind,
            renderer,
            window,
            layout_size: spec.surface_size,
            cursor_icon: CursorIcon::Default,
        })
    }

    pub(crate) fn kind(&self) -> ShellDialogWindowKind {
        self.kind
    }

    pub(crate) fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub(crate) fn window(&self) -> &Window {
        self.window.as_ref()
    }

    pub(crate) fn renderer_size(&self) -> PhysicalSize<u32> {
        self.renderer.size
    }

    pub(crate) fn frame_count(&self) -> u64 {
        self.renderer.frame_count
    }

    pub(crate) fn layout_size(&self) -> PhysicalSize<u32> {
        self.layout_size
    }

    pub(crate) fn sync(&mut self, spec: &ShellDialogWindowSpec) {
        self.layout_size = spec.surface_size;
        self.window.set_title(&spec.title);
        self.window.set_theme(spec.theme);
        self.window.set_min_surface_size(spec.min_surface_size);
        self.window.set_max_surface_size(spec.max_surface_size);
        self.window.set_resizable(spec.resizable);
        if let Some(applied) = self.window.request_surface_size(spec.surface_size)
            && let Err(error) = self.renderer.resize(applied)
        {
            eprintln!(
                "[fika-vulkan] {} dialog resize failed: {error}",
                self.kind.as_str()
            );
        }
        self.request_redraw();
    }

    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) {
        if let Err(error) = self.renderer.resize(size) {
            eprintln!(
                "[fika-vulkan] {} dialog resize failed: {error}",
                self.kind.as_str()
            );
        }
    }

    pub(crate) fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub(crate) fn focus(&self) {
        self.window.focus_window();
        self.window.request_user_attention();
        self.window.request_redraw();
    }

    pub(crate) fn set_cursor(&mut self, cursor_icon: CursorIcon) {
        if self.cursor_icon == cursor_icon {
            return;
        }
        self.cursor_icon = cursor_icon;
        self.window.set_cursor(cursor_icon);
    }

    fn prepare_for_drop(&mut self) {
        fika_dialog_trace!(
            "[fika] dialog-window-renderer-idle kind={} window={:?}",
            self.kind.as_str(),
            self.window_id()
        );
        if let Err(error) = self.renderer.wait_idle("dialog-window-drop") {
            eprintln!(
                "[fika-vulkan] {} dialog idle failed: {error}",
                self.kind.as_str()
            );
        }
    }

    pub(crate) fn renderer_and_window_mut(&mut self) -> (&mut FikaRenderer, &Window) {
        (&mut self.renderer, self.window.as_ref())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ShellDialogWindowHostEvent {
    CloseRequested,
    SurfaceResized,
    ScaleFactorChanged {
        scale_factor: f32,
        renderer_size: PhysicalSize<u32>,
    },
    ModifiersChanged(Modifiers),
}

struct ShellDeferredDialogClose {
    kind: ShellDialogWindowKind,
    window_id: WindowId,
    window: ShellDetachedDialogWindow,
    drop_at: Instant,
}

#[derive(Default)]
pub(crate) struct ShellDialogWindows {
    create: Option<ShellDetachedDialogWindow>,
    open_with: Option<ShellDetachedDialogWindow>,
    properties: Option<ShellDetachedDialogWindow>,
    rename: Option<ShellDetachedDialogWindow>,
    settings: Option<ShellDetachedDialogWindow>,
    task_detail: Option<ShellDetachedDialogWindow>,
    trash_conflict: Option<ShellDetachedDialogWindow>,
    deferred_closes: VecDeque<ShellDeferredDialogClose>,
}

impl ShellDialogWindows {
    const DEFERRED_CLOSE_DELAY: Duration = Duration::from_millis(1);

    pub(crate) fn has_open_window(&self) -> bool {
        self.create.is_some()
            || self.open_with.is_some()
            || self.properties.is_some()
            || self.rename.is_some()
            || self.settings.is_some()
            || self.task_detail.is_some()
            || self.trash_conflict.is_some()
    }

    pub(crate) fn is_open(&self, kind: ShellDialogWindowKind) -> bool {
        self.get(kind).is_some()
    }

    pub(crate) fn get(&self, kind: ShellDialogWindowKind) -> Option<&ShellDetachedDialogWindow> {
        match kind {
            ShellDialogWindowKind::Create => self.create.as_ref(),
            ShellDialogWindowKind::OpenWith => self.open_with.as_ref(),
            ShellDialogWindowKind::Properties => self.properties.as_ref(),
            ShellDialogWindowKind::Rename => self.rename.as_ref(),
            ShellDialogWindowKind::Settings => self.settings.as_ref(),
            ShellDialogWindowKind::TaskDetail => self.task_detail.as_ref(),
            ShellDialogWindowKind::TrashConflict => self.trash_conflict.as_ref(),
        }
    }

    pub(crate) fn get_mut(
        &mut self,
        kind: ShellDialogWindowKind,
    ) -> Option<&mut ShellDetachedDialogWindow> {
        match kind {
            ShellDialogWindowKind::Create => self.create.as_mut(),
            ShellDialogWindowKind::OpenWith => self.open_with.as_mut(),
            ShellDialogWindowKind::Properties => self.properties.as_mut(),
            ShellDialogWindowKind::Rename => self.rename.as_mut(),
            ShellDialogWindowKind::Settings => self.settings.as_mut(),
            ShellDialogWindowKind::TaskDetail => self.task_detail.as_mut(),
            ShellDialogWindowKind::TrashConflict => self.trash_conflict.as_mut(),
        }
    }

    pub(crate) fn request_redraw(&self, kind: ShellDialogWindowKind) -> bool {
        self.get(kind).is_some_and(|window| {
            window.request_redraw();
            true
        })
    }

    pub(crate) fn resize(&mut self, kind: ShellDialogWindowKind, size: PhysicalSize<u32>) -> bool {
        self.get_mut(kind).is_some_and(|window| {
            window.resize(size);
            window.request_redraw();
            true
        })
    }

    pub(crate) fn set_cursor(
        &mut self,
        kind: ShellDialogWindowKind,
        cursor_icon: CursorIcon,
    ) -> bool {
        self.get_mut(kind).is_some_and(|window| {
            window.set_cursor(cursor_icon);
            true
        })
    }

    pub(crate) fn layout_size(&self, kind: ShellDialogWindowKind) -> Option<PhysicalSize<u32>> {
        self.get(kind).map(ShellDetachedDialogWindow::layout_size)
    }

    pub(crate) fn handle_window_event(
        &mut self,
        kind: ShellDialogWindowKind,
        event: &WindowEvent,
    ) -> Option<ShellDialogWindowHostEvent> {
        match event {
            WindowEvent::CloseRequested => Some(ShellDialogWindowHostEvent::CloseRequested),
            WindowEvent::SurfaceResized(size) => {
                self.resize(kind, *size);
                Some(ShellDialogWindowHostEvent::SurfaceResized)
            }
            WindowEvent::ScaleFactorChanged { scale_factor } => {
                self.get(kind)
                    .map(|window| ShellDialogWindowHostEvent::ScaleFactorChanged {
                        scale_factor: *scale_factor as f32,
                        renderer_size: window.renderer_size(),
                    })
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                Some(ShellDialogWindowHostEvent::ModifiersChanged(*modifiers))
            }
            _ => None,
        }
    }

    pub(crate) fn set(&mut self, kind: ShellDialogWindowKind, window: ShellDetachedDialogWindow) {
        debug_assert_eq!(window.kind(), kind);
        fika_dialog_trace!(
            "[fika] dialog-window-set kind={} window={:?}",
            kind.as_str(),
            window.window_id()
        );
        match kind {
            ShellDialogWindowKind::Create => self.create = Some(window),
            ShellDialogWindowKind::OpenWith => self.open_with = Some(window),
            ShellDialogWindowKind::Properties => self.properties = Some(window),
            ShellDialogWindowKind::Rename => self.rename = Some(window),
            ShellDialogWindowKind::Settings => self.settings = Some(window),
            ShellDialogWindowKind::TaskDetail => self.task_detail = Some(window),
            ShellDialogWindowKind::TrashConflict => self.trash_conflict = Some(window),
        }
    }

    pub(crate) fn close(&mut self, kind: ShellDialogWindowKind) -> bool {
        let closed = match kind {
            ShellDialogWindowKind::Create => self.create.take(),
            ShellDialogWindowKind::OpenWith => self.open_with.take(),
            ShellDialogWindowKind::Properties => self.properties.take(),
            ShellDialogWindowKind::Rename => self.rename.take(),
            ShellDialogWindowKind::Settings => self.settings.take(),
            ShellDialogWindowKind::TaskDetail => self.task_detail.take(),
            ShellDialogWindowKind::TrashConflict => self.trash_conflict.take(),
        };
        if let Some(window) = closed {
            let window_id = window.window_id();
            fika_dialog_trace!(
                "[fika] dialog-window-close-deferred kind={} window={:?}",
                kind.as_str(),
                window_id
            );
            self.deferred_closes.push_back(ShellDeferredDialogClose {
                kind,
                window_id,
                window,
                drop_at: Instant::now() + Self::DEFERRED_CLOSE_DELAY,
            });
            true
        } else {
            fika_dialog_trace!(
                "[fika] dialog-window-close kind={} window=<none>",
                kind.as_str()
            );
            false
        }
    }

    pub(crate) fn drain_ready_deferred_closes(&mut self) -> bool {
        let mut dropped_any = false;
        let now = Instant::now();
        let mut pending = VecDeque::new();
        while let Some(mut close) = self.deferred_closes.pop_front() {
            if close.drop_at > now {
                pending.push_back(close);
                continue;
            }
            fika_dialog_trace!(
                "[fika] dialog-window-drop-deferred kind={} window={:?}",
                close.kind.as_str(),
                close.window_id
            );
            close.window.prepare_for_drop();
            dropped_any = true;
        }
        self.deferred_closes = pending;
        dropped_any
    }

    pub(crate) fn next_deferred_close_deadline(&self) -> Option<Instant> {
        self.deferred_closes.iter().map(|close| close.drop_at).min()
    }

    pub(crate) fn close_all(&mut self) {
        for mut window in [
            self.create.take(),
            self.open_with.take(),
            self.properties.take(),
            self.rename.take(),
            self.settings.take(),
            self.task_detail.take(),
            self.trash_conflict.take(),
        ]
        .into_iter()
        .flatten()
        {
            fika_dialog_trace!(
                "[fika] dialog-window-close-all kind={} window={:?}",
                window.kind().as_str(),
                window.window_id()
            );
            window.prepare_for_drop();
        }
        while let Some(mut close) = self.deferred_closes.pop_front() {
            fika_dialog_trace!(
                "[fika] dialog-window-close-all-deferred kind={} window={:?}",
                close.kind.as_str(),
                close.window_id
            );
            close.window.prepare_for_drop();
        }
    }

    pub(crate) fn window_kind_for_id(&self, window_id: WindowId) -> Option<ShellDialogWindowKind> {
        [
            self.create.as_ref(),
            self.open_with.as_ref(),
            self.properties.as_ref(),
            self.rename.as_ref(),
            self.settings.as_ref(),
            self.task_detail.as_ref(),
            self.trash_conflict.as_ref(),
        ]
        .into_iter()
        .flatten()
        .find(|window| window.window_id() == window_id)
        .map(ShellDetachedDialogWindow::kind)
    }

    pub(crate) fn frame_count(&self, kind: ShellDialogWindowKind) -> Option<u64> {
        self.get(kind).map(ShellDetachedDialogWindow::frame_count)
    }
}
