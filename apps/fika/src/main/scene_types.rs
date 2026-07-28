use crate::*;

/// Content-space scroll deltas `(dx, dy)` from a window-runtime wheel event.
///
/// The window runtime already applied the protocol sign flip for continuous
/// axes; negate once more so positive dy scrolls content down.
pub(crate) fn scroll_delta_xy(delta: MouseScrollDelta, scale_factor: f32) -> (f32, f32) {
    match delta {
        MouseScrollDelta::PixelDelta(position) => (-position.x as f32, -position.y as f32),
        MouseScrollDelta::LineDelta { x, y } => {
            let line = SCROLL_LINE_PX * scale_factor.max(f32::EPSILON);
            (-x as f32 * line, -y as f32 * line)
        }
    }
}

pub(crate) fn scroll_delta_y(delta: MouseScrollDelta, scale_factor: f32) -> f32 {
    scroll_delta_xy(delta, scale_factor).1
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct PaneClick {
    pub(crate) pane: ShellPaneId,
    pub(crate) index: usize,
    pub(crate) point: ViewPoint,
    pub(crate) time: Instant,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShellPaneItemTarget {
    pub(crate) pane: ShellPaneId,
    pub(crate) index: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellPlace {
    pub(crate) group: &'static str,
    pub(crate) marker: &'static str,
    pub(crate) icon_name: &'static str,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) device: Option<ShellDevicePlace>,
    pub(crate) network: bool,
    pub(crate) trash: bool,
    pub(crate) root: bool,
    pub(crate) editable: bool,
}
impl ShellPlace {
    pub(crate) fn new(
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

    pub(crate) fn with_device(mut self, device: ShellDevicePlace) -> Self {
        self.icon_name = "drive-removable-media";
        self.device = Some(device);
        self
    }
}
pub(crate) fn place_icon_paint(place: &ShellPlace) -> PlaceIconPaint {
    PlaceIconPaint::from_flags(
        place.trash,
        place.network,
        place.root,
        place.editable,
        place.marker == "D" || place.marker == "/",
    )
}
pub(crate) fn shell_place_icon_name(
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
pub(crate) enum ShellItemActivation {
    Directory { pane: ShellPaneId, path: PathBuf },
    File(OpenFileRequest),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CopyLocationRequest {
    pub(crate) path: PathBuf,
    pub(crate) text: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AddNetworkFolderRequest {
    pub(crate) pane: ShellPaneId,
    pub(crate) path: PathBuf,
    pub(crate) label: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DeviceActionRequest {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) action: ShellContextMenuAction,
    pub(crate) operation: DevicePlaceOperation,
    pub(crate) pane: ShellPaneId,
    pub(crate) path: PathBuf,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShellPlaceActivation {
    Open { pane: ShellPaneId, path: PathBuf },
    DeviceAction(DeviceActionRequest),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellTrashResult {
    pub(crate) success_count: usize,
    pub(crate) failure_count: usize,
    pub(crate) trash_pairs: Vec<(PathBuf, PathBuf)>,
    pub(crate) privileged: bool,
    pub(crate) administrator_available: bool,
    pub(crate) first_error: Option<String>,
}
impl ShellTrashResult {
    pub(crate) fn changed(&self) -> bool {
        self.success_count > 0
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShellInternalDragSource {
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
pub(crate) struct ShellInternalDrag {
    pub(crate) source: ShellInternalDragSource,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) label: String,
    pub(crate) start: ViewPoint,
    pub(crate) current: ViewPoint,
    pub(crate) active: bool,
}
impl ShellInternalDrag {
    pub(crate) fn new(
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

    pub(crate) fn update(&mut self, current: ViewPoint) -> bool {
        let old_current = self.current;
        let old_active = self.active;
        self.current = current;
        if !self.active && point_distance(self.start, current) >= RUBBER_BAND_START_THRESHOLD {
            self.active = true;
        }
        old_current != self.current || old_active != self.active
    }

    pub(crate) fn source_place_index(&self) -> Option<usize> {
        match self.source {
            ShellInternalDragSource::Place { index } => Some(index),
            ShellInternalDragSource::PaneItem { .. } => None,
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct ShellInternalDragPreviewItem {
    pub(crate) path: PathBuf,
    pub(crate) entry: Entry,
}

#[derive(Clone, Debug)]
pub(crate) enum ShellInternalDragPreviewSource {
    PaneItem {
        directory: PathBuf,
        entry: Entry,
        items: Vec<ShellInternalDragPreviewItem>,
        label: String,
        view_mode: ShellViewMode,
        layout: crate::ui::drag_preview_layout::SingleDragPreviewLayout,
        folder_preview: Option<FolderPreviewReady>,
    },
    Place {
        label: String,
        icon_name: String,
        layout: crate::ui::drag_preview_layout::SingleDragPreviewLayout,
    },
}
impl ShellInternalDragPreviewSource {
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::PaneItem { label, .. } | Self::Place { label, .. } => label,
        }
    }

    pub(crate) fn layout(&self) -> crate::ui::drag_preview_layout::SingleDragPreviewLayout {
        match self {
            Self::PaneItem { layout, .. } | Self::Place { layout, .. } => *layout,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
/// One active incoming Wayland offer, including offers from this client.
pub(crate) struct ShellExternalDrag {
    pub(crate) sources: Vec<PathBuf>,
    pub(crate) local_source: Option<ShellInternalDragSource>,
}
impl ShellExternalDrag {
    pub(crate) fn new(
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
pub(crate) struct ShellPreparedPaneVisibleItem {
    pub(crate) layout: ItemLayout,
    pub(crate) path: Option<PathBuf>,
    pub(crate) slot_id: u64,
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
pub(crate) struct ShellPreparedPaneProjection {
    pub(crate) geometry: ShellPaneGeometry,
    pub(crate) visible_items: Vec<ShellPreparedPaneVisibleItem>,
    pub(crate) scroll_metrics: ShellPaneScrollMetrics,
}
pub(crate) struct ShellPreparedFrameProjectionLayouts {
    pub(crate) layouts: Vec<ShellPreparedPaneProjection>,
    pub(crate) layout_us: u128,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellPlacePress {
    pub(crate) index: usize,
    pub(crate) point: ViewPoint,
}
