impl FikaWgpuApp {
    fn queue_scene_change(&mut self, reason: &'static str, redraw_frames: u8) {
        self.pending_redraw_frames = self.pending_redraw_frames.max(redraw_frames);
        self.pending_render_reason = Some(reason);
        if let Some(window) = self.window.as_ref() {
            window.set_title(&window_title(&self.scene));
            window.request_redraw();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::TaskDetail) {
            self.sync_task_detail_dialog_window();
        }
    }

    fn present_scene_change(&mut self, event_loop: &ActiveEventLoop, reason: &'static str) {
        self.pending_redraw_frames = VIEW_SWITCH_REDRAW_FRAMES;
        self.pending_render_reason = None;
        if let Some(window) = self.window.as_ref() {
            window.set_title(&window_title(&self.scene));
            window.request_redraw();
        }
        if self.dialog_windows.is_open(ShellDialogWindowKind::TaskDetail) {
            self.sync_task_detail_dialog_window();
        }
        self.prewarm_current_scene_caches(reason);
        self.render_now(event_loop, reason, true);
    }

    fn prewarm_current_scene_caches(&mut self, reason: &'static str) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        renderer.prewarm_scene_caches(&mut self.scene, reason);
    }

    fn render_create_dialog_now(&mut self, reason: &'static str) {
        let Some(dialog_state) = self.scene.create_dialog.as_ref() else {
            self.close_create_dialog_window();
            return;
        };
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog_window) = self.dialog_windows.get_mut(ShellDialogWindowKind::Create) else {
            return;
        };
        let layout_size = dialog_window.layout_size();
        let (renderer, window) = dialog_window.renderer_and_window_mut();
        renderer.render_create_dialog(
            window,
            dialog_state,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn render_rename_dialog_now(&mut self, reason: &'static str) {
        let Some(dialog_state) = self.scene.rename_dialog.as_ref() else {
            self.close_rename_dialog_window();
            return;
        };
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog_window) = self.dialog_windows.get_mut(ShellDialogWindowKind::Rename) else {
            return;
        };
        let layout_size = dialog_window.layout_size();
        let (renderer, window) = dialog_window.renderer_and_window_mut();
        renderer.render_rename_dialog(
            window,
            dialog_state,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn render_open_with_dialog_now(
        &mut self,
        reason: &'static str,
    ) {
        let Some(chooser) = self.scene.open_with_chooser.as_ref() else {
            self.close_open_with_dialog_window();
            return;
        };
        let scale = self.scene.ui_scale();
        let caret_visible = self.scene.text_caret_visible();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog) = self.dialog_windows.get_mut(ShellDialogWindowKind::OpenWith) else {
            return;
        };
        let layout_size = dialog.layout_size();
        let (renderer, window) = dialog.renderer_and_window_mut();
        renderer.render_open_with_dialog(
            window,
            chooser,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            caret_visible,
            reason,
        );
    }

    fn render_properties_dialog_now(&mut self, reason: &'static str) {
        let Some(overlay) = self.scene.properties_overlay.as_ref() else {
            self.close_properties_dialog_window();
            return;
        };
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog) = self.dialog_windows.get_mut(ShellDialogWindowKind::Properties) else {
            return;
        };
        let layout_size = dialog.layout_size();
        let (renderer, window) = dialog.renderer_and_window_mut();
        renderer.render_properties_dialog(
            window,
            overlay,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn render_task_detail_dialog_now(&mut self, reason: &'static str) {
        if self.scene.task_detail_dialog.is_none() {
            self.close_task_detail_dialog_window();
            return;
        }
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog) = self.dialog_windows.get_mut(ShellDialogWindowKind::TaskDetail) else {
            return;
        };
        let layout_size = dialog.layout_size();
        let (renderer, window) = dialog.renderer_and_window_mut();
        renderer.render_task_detail_dialog(
            window,
            &self.scene.task_statuses,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn render_trash_conflict_dialog_now(&mut self, reason: &'static str) {
        let Some(dialog_state) = self.scene.trash_conflict_dialog.as_ref() else {
            self.close_trash_conflict_dialog_window();
            return;
        };
        let scale = self.scene.ui_scale();
        let popup_theme = PopupTheme::from_shell_theme(self.scene.theme());
        let Some(dialog) = self
            .dialog_windows
            .get_mut(ShellDialogWindowKind::TrashConflict)
        else {
            return;
        };
        let layout_size = dialog.layout_size();
        let (renderer, window) = dialog.renderer_and_window_mut();
        renderer.render_trash_conflict_dialog(
            window,
            dialog_state,
            DialogRenderViewport {
                popup_theme,
                scale,
                layout_size,
            },
            reason,
        );
    }

    fn render_now(
        &mut self,
        event_loop: &ActiveEventLoop,
        reason: &'static str,
        force_log: bool,
    ) {
        if self.scene.is_properties_overlay_open()
            && !self.dialog_windows.is_open(ShellDialogWindowKind::Properties)
        {
            self.ensure_properties_dialog_window(event_loop);
        }
        if self.scene.is_task_detail_dialog_open()
            && !self.dialog_windows.is_open(ShellDialogWindowKind::TaskDetail)
        {
            self.ensure_task_detail_dialog_window(event_loop);
        }
        if self.scene.is_trash_conflict_dialog_open()
            && !self
                .dialog_windows
                .is_open(ShellDialogWindowKind::TrashConflict)
        {
            self.ensure_trash_conflict_dialog_window(event_loop);
        }
        self.reconcile_dialog_window_lifecycle();
        let rendered = {
            let Some(window) = self.window.as_ref() else {
                return;
            };
            let Some(renderer) = self.renderer.as_mut() else {
                return;
            };

            renderer.render(
                window.as_ref(),
                event_loop,
                &mut self.scene,
                reason,
                force_log,
            )
        };

        if rendered.consumed_redraw_request() && self.pending_redraw_frames > 0 {
            self.pending_redraw_frames -= 1;
        }
        if rendered.presented() {
            self.drive_autosmoke_after_render();
        }
    }
}
fn scroll_delta_y(delta: MouseScrollDelta, scale_factor: f32) -> f32 {
    match delta {
        // Platform already scaled continuous axes into physical pixels and
        // applied the Wayland sign flip; negate once more for content scroll.
        MouseScrollDelta::PixelDelta(position) => -position.y as f32,
        // Logical wheel steps: one notch ≈ SCROLL_LINE_PX at scale 1.0.
        MouseScrollDelta::LineDelta { y, .. } => {
            -y as f32 * SCROLL_LINE_PX * scale_factor.max(f32::EPSILON)
        }
    }
}
#[derive(Clone, Copy, Debug)]
struct PaneClick {
    pane: ShellPaneId,
    index: usize,
    point: ViewPoint,
    time: Instant,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShellPaneItemTarget {
    pane: ShellPaneId,
    index: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellPlace {
    group: &'static str,
    marker: &'static str,
    icon_name: &'static str,
    label: String,
    path: PathBuf,
    device: Option<ShellDevicePlace>,
    network: bool,
    trash: bool,
    root: bool,
    editable: bool,
}
impl ShellPlace {
    fn new(
        group: &'static str,
        marker: &'static str,
        label: impl Into<String>,
        path: PathBuf,
        editable: bool,
    ) -> Self {
        let trash = file_ops::is_trash_files_dir(&path);
        let network = is_network_path(&path);
        let root = path == Path::new("/");
        let icon_name = shell_place_icon_name(marker, trash, network, root, editable);
        Self {
            group,
            marker,
            icon_name,
            label: label.into(),
            path,
            device: None,
            network,
            trash,
            root,
            editable,
        }
    }

    fn with_device(mut self, device: ShellDevicePlace) -> Self {
        self.icon_name = "drive-removable-media";
        self.device = Some(device);
        self
    }
}
fn place_icon_paint(place: &ShellPlace) -> PlaceIconPaint {
    PlaceIconPaint::from_flags(
        place.trash,
        place.network,
        place.root,
        place.editable,
        place.marker == "D" || place.marker == "/",
    )
}
fn shell_place_icon_name(
    marker: &str,
    trash: bool,
    network: bool,
    root: bool,
    editable: bool,
) -> &'static str {
    if trash {
        return "user-trash";
    }
    if network {
        return "folder-remote";
    }
    if root {
        return "drive-harddisk";
    }
    match marker {
        "H" => "user-home",
        "Desk" => "user-desktop",
        "Doc" => "folder-documents",
        "Down" => "folder-download",
        "Mus" => "folder-music",
        "Pic" => "folder-pictures",
        "Vid" => "folder-videos",
        "D" => "drive-removable-media",
        "/" => "drive-harddisk",
        _ if editable => "folder-bookmark",
        _ => "folder",
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum ShellItemActivation {
    Directory { pane: ShellPaneId, path: PathBuf },
    File(OpenFileRequest),
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct CopyLocationRequest {
    path: PathBuf,
    text: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct AddNetworkFolderRequest {
    pane: ShellPaneId,
    path: PathBuf,
    label: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct DeviceActionRequest {
    id: String,
    label: String,
    action: ShellContextMenuAction,
    operation: DevicePlaceOperation,
    pane: ShellPaneId,
    path: PathBuf,
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum ShellPlaceActivation {
    Open { pane: ShellPaneId, path: PathBuf },
    DeviceAction(DeviceActionRequest),
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellTrashResult {
    success_count: usize,
    failure_count: usize,
    trash_pairs: Vec<(PathBuf, PathBuf)>,
    privileged: bool,
    administrator_available: bool,
    first_error: Option<String>,
}
impl ShellTrashResult {
    fn changed(&self) -> bool {
        self.success_count > 0
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum ShellInternalDragSource {
    PaneItem {
        pane: ShellPaneId,
        index: usize,
        source_path: PathBuf,
        is_dir: bool,
    },
    Place {
        index: usize,
    },
}
#[derive(Clone, Debug, PartialEq)]
/// Local press/threshold/preview state before Wayland owns the transfer.
struct ShellInternalDrag {
    source: ShellInternalDragSource,
    paths: Vec<PathBuf>,
    label: String,
    start: ViewPoint,
    current: ViewPoint,
    active: bool,
}
impl ShellInternalDrag {
    fn new(
        source: ShellInternalDragSource,
        paths: Vec<PathBuf>,
        label: String,
        start: ViewPoint,
    ) -> Self {
        Self {
            source,
            paths,
            label,
            start,
            current: start,
            active: false,
        }
    }

    fn update(&mut self, current: ViewPoint) -> bool {
        let old_current = self.current;
        let old_active = self.active;
        self.current = current;
        if !self.active && point_distance(self.start, current) >= RUBBER_BAND_START_THRESHOLD {
            self.active = true;
        }
        old_current != self.current || old_active != self.active
    }

    fn source_place_index(&self) -> Option<usize> {
        match self.source {
            ShellInternalDragSource::Place { index } => Some(index),
            ShellInternalDragSource::PaneItem { .. } => None,
        }
    }

}
#[derive(Clone, Debug)]
struct ShellInternalDragPreviewItem {
    path: PathBuf,
    entry: Entry,
}

#[derive(Clone, Debug)]
enum ShellInternalDragPreviewSource {
    PaneItem {
        directory: PathBuf,
        entry: Entry,
        items: Vec<ShellInternalDragPreviewItem>,
        label: String,
        view_mode: ShellViewMode,
        layout: crate::shell::drag_preview_layout::SingleDragPreviewLayout,
        folder_preview: Option<FolderPreviewReady>,
    },
    Place {
        label: String,
        icon_name: String,
        layout: crate::shell::drag_preview_layout::SingleDragPreviewLayout,
    },
}
impl ShellInternalDragPreviewSource {
    fn label(&self) -> &str {
        match self {
            Self::PaneItem { label, .. } | Self::Place { label, .. } => label,
        }
    }

    fn layout(&self) -> crate::shell::drag_preview_layout::SingleDragPreviewLayout {
        match self {
            Self::PaneItem { layout, .. } | Self::Place { layout, .. } => *layout,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
/// One active incoming Wayland offer, including offers from this client.
struct ShellExternalDrag {
    sources: Vec<PathBuf>,
    local_source: Option<ShellInternalDragSource>,
}
impl ShellExternalDrag {
    fn new(
        sources: Vec<PathBuf>,
        local_source: Option<ShellInternalDragSource>,
    ) -> Option<Self> {
        let sources = normalized_external_drop_sources(sources);
        (!sources.is_empty()).then_some(Self {
            sources,
            local_source,
        })
    }
}
struct ShellPreparedPaneVisibleItem {
    layout: ItemLayout,
    path: Option<PathBuf>,
    slot_id: u64,
}
impl ShellVisibleSlotItem for ShellPreparedPaneVisibleItem {
    fn visible_slot_path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn visible_slot_id(&self) -> u64 {
        self.slot_id
    }

    fn set_visible_slot_id(&mut self, slot_id: u64) {
        self.slot_id = slot_id;
    }

    fn release_visible_slot_path(&mut self) {
        self.path = None;
    }
}
struct ShellPreparedPaneProjection {
    geometry: ShellPaneGeometry,
    visible_items: Vec<ShellPreparedPaneVisibleItem>,
    scroll_metrics: ShellPaneScrollMetrics,
}
struct ShellPreparedFrameProjectionLayouts {
    layouts: Vec<ShellPreparedPaneProjection>,
    layout_us: u128,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct ShellPlacePress {
    index: usize,
    point: ViewPoint,
}
