#[cfg(test)]
use std::fs;
#[cfg(all(test, unix))]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::platform::{
    ActiveEventLoop, AsyncRequestSerial, DataTransferId, DataTransferSendBuilder, DndAction,
    DragIcon, PhysicalPosition, RgbaIcon, SendData, TypeHint, TypedData,
};

use super::outcome::ShellActionOutcome;
use crate::shell::drag_preview_layout::{
    DragPreviewBackgroundStyle, DragPreviewLabelStyle, MultiDragPreviewLayout,
    SingleDragPreviewLayout, multi_drag_preview_layout,
};
use crate::shell::drop_menu::ShellDropTarget;
use crate::shell::file_item_view::style::{
    DolphinItemPalette, item_background_color_for_palette, place_row_background_color_for_palette,
};
use crate::shell::icon_roles::{
    FileIconKind, FileIconPathCacheKey, FileIconRoleCacheKey, NamedIconFallback,
    file_icon_path_cache_key, icon_cache_size,
};
use crate::shell::tasks::ShellTaskStatus;
use crate::{
    FikaWgpuApp, FolderPreviewReady, IconRaster, IconRasterCacheKey, IconRasterStyle,
    IncomingDndTransfer, ItemPixmapLayout, OutgoingDndTransfer, ShellInternalDragPreviewSource,
    ShellViewMode, ViewRect, decode_file_clipboard_text, entry_path_for_thumbnail,
    folder_preview_role_draw_rect, icon_emblem_kinds_for_path, icon_emblem_rects,
    normalized_scale_factor, path_uri_from_path, rasterize_icon,
    thumbnail_request_may_have_preview, view_point_from_physical_position,
};

const ACCEPTED_DND_ACTIONS: [DndAction; 3] = [DndAction::Ask, DndAction::Move, DndAction::Copy];
const DND_FALLBACK_ICON_SIZE: f32 = 128.0;

#[derive(Clone, Debug)]
struct OutgoingDndPayload {
    uris: Vec<String>,
    text: String,
}

#[derive(Clone, Debug)]
struct OutgoingDndPreviewRasters {
    icons: Vec<Option<IconRaster>>,
}

impl OutgoingDndPreviewRasters {
    fn icon(&self, index: usize) -> Option<&IconRaster> {
        self.icons.get(index).and_then(Option::as_ref)
    }
}

impl FikaWgpuApp {
    pub(crate) fn reset_outgoing_drag_tracking(&mut self) {
        self.outgoing_dnd_transfer = None;
        self.outgoing_dnd_start_failed = false;
    }

    pub(crate) fn start_outgoing_drag_if_needed(&mut self, event_loop: &ActiveEventLoop) {
        if self.outgoing_dnd_transfer.is_some()
            || self.outgoing_dnd_start_failed
            || !self.scene.internal_drag_active()
        {
            return;
        }
        let Some(window_id) = self.window.as_ref().map(|window| window.id()) else {
            return;
        };
        let Some(paths) = self.scene.active_internal_drag_paths() else {
            return;
        };
        let Some(source) = self.scene.active_internal_drag_source() else {
            return;
        };
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let payload = outgoing_dnd_payload(&paths);
        let scale = self.scene.ui_scale();
        let preview_source = self.scene.active_internal_drag_preview_source(size);
        let mut preview_metrics = if paths.len() == 1 {
            preview_source
                .as_ref()
                .map(|source| outgoing_dnd_preview_metrics_for_layout(source.layout(), scale))
                .unwrap_or_else(|| outgoing_dnd_fallback_preview_metrics(scale))
        } else {
            outgoing_dnd_preview_metrics_for_multi_layout(
                multi_drag_preview_layout(paths.len(), scale),
                scale,
            )
        };
        let preview_label = if paths.len() == 1 {
            preview_source
                .as_ref()
                .map(|source| source.label().to_string())
                .unwrap_or_else(|| outgoing_dnd_fallback_label(&paths))
        } else {
            String::new()
        };
        let (background_color, label_color, label_style) =
            outgoing_dnd_preview_colors(preview_source.as_ref(), self.scene.theme());
        preview_metrics.background_color = background_color;
        preview_metrics.label_style = label_style.or(preview_metrics.label_style);
        let preview_rasters = self.outgoing_dnd_preview_rasters(
            preview_source.as_ref(),
            &paths,
            preview_metrics.cache_icon_size,
            preview_metrics.buffer_scale as f32,
            preview_metrics.visible_icon_count(),
        );
        let label_raster =
            self.outgoing_dnd_preview_label_raster(&preview_label, preview_metrics, label_style);
        let drag_icon = outgoing_dnd_drag_icon(
            &paths,
            preview_metrics,
            Some(&preview_rasters),
            label_raster.as_ref(),
            label_color,
        );
        let send_data = DataTransferSendBuilder::new(payload)
            .with_type(TypeHint::UriList, |payload, _| {
                Some(SendData::Uris(payload.uris.clone()))
            })
            .with_type(TypeHint::Plaintext, |payload, _| Some(payload.text.clone()))
            .build();
        match event_loop.start_drag(window_id, send_data, &ACCEPTED_DND_ACTIONS, drag_icon) {
            Ok(id) => {
                fika_log!(
                    "[fika-wgpu] outgoing-dnd start id={} sources={}",
                    id.into_raw(),
                    paths.len()
                );
                self.outgoing_dnd_transfer = Some(OutgoingDndTransfer { id, paths, source });
            }
            Err(error) => {
                self.outgoing_dnd_start_failed = true;
                fika_log!("[fika-wgpu] outgoing-dnd-unavailable {error}");
                if self.scene.clear_internal_drag()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
        }
    }

    fn outgoing_dnd_preview_rasters(
        &mut self,
        source: Option<&ShellInternalDragPreviewSource>,
        paths: &[PathBuf],
        cache_icon_size: f32,
        scale: f32,
        visible_count: usize,
    ) -> OutgoingDndPreviewRasters {
        let Some(renderer) = self.renderer.as_mut() else {
            return OutgoingDndPreviewRasters { icons: Vec::new() };
        };
        // Use scene-space icon size so thumbnail cache keys match the live view.
        let icon_size_px = icon_cache_size(cache_icon_size);
        let icons = paths
            .iter()
            .take(visible_count)
            .map(|path| {
                outgoing_dnd_preview_raster_for_path(
                    renderer,
                    source,
                    path,
                    cache_icon_size,
                    icon_size_px,
                    scale,
                )
            })
            .collect();
        OutgoingDndPreviewRasters { icons }
    }

    fn outgoing_dnd_preview_label_raster(
        &mut self,
        label: &str,
        metrics: OutgoingDndPreviewMetrics,
        style: Option<DragPreviewLabelStyle>,
    ) -> Option<OutgoingDndPreviewLabelRaster> {
        let rect = metrics.label_rect?;
        let style = style.or(metrics.label_style)?;
        let renderer = self.renderer.as_mut()?;
        let alpha = renderer.text_renderer.rasterize_drag_label(
            label,
            rect.width as u32,
            rect.height as u32,
            metrics.buffer_scale as f32,
            style,
        );
        Some(OutgoingDndPreviewLabelRaster {
            alpha: Arc::from(alpha),
            width: rect.width as u32,
            height: rect.height as u32,
        })
    }

    pub(crate) fn external_drag_entered(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: DataTransferId,
        position: Option<PhysicalPosition<f64>>,
    ) -> ShellActionOutcome {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return ShellActionOutcome::None;
        };
        let local_drag = self
            .outgoing_dnd_transfer
            .as_ref()
            .map(|transfer| (transfer.paths.clone(), transfer.source.clone()));
        let mut transfer = IncomingDndTransfer::new(
            id,
            position,
            local_drag.as_ref().map(|(paths, _)| paths.clone()),
            local_drag.as_ref().map(|(_, source)| source.clone()),
        );
        let supports_uri_list = event_loop
            .data_transfer(id)
            .map(|data| data.has_type(&TypeHint::UriList))
            .unwrap_or_else(|error| {
                fika_log!(
                    "[fika-wgpu] external-dnd data-transfer-error id={} {error}",
                    id.into_raw()
                );
                false
            });

        if !supports_uri_list {
            self.set_valid_dnd_actions(event_loop, id, false);
            self.incoming_dnd_transfer = None;
            let changed = self.scene.clear_external_drag();
            fika_log!(
                "[fika-wgpu] external-dnd reject id={} reason=missing-uri-list",
                id.into_raw()
            );
            return ShellActionOutcome::redraw_if(changed);
        }

        match event_loop.fetch_data_transfer(id, &TypeHint::UriList) {
            Ok(serial) => {
                transfer.fetch_serial = Some(serial);
            }
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] external-dnd fetch-error id={} {error}",
                    id.into_raw()
                );
                self.set_valid_dnd_actions(event_loop, id, false);
                self.incoming_dnd_transfer = None;
                let changed = self.scene.clear_external_drag();
                return ShellActionOutcome::redraw_if(changed);
            }
        }

        let changed = position
            .map(|position| {
                let point = view_point_from_physical_position(position);
                self.scene.begin_data_transfer_drag(
                    local_drag
                        .as_ref()
                        .map(|(paths, _)| paths.clone())
                        .unwrap_or_default(),
                    local_drag.map(|(_, source)| source),
                    point,
                    size,
                )
            })
            .unwrap_or(false);
        self.incoming_dnd_transfer = Some(transfer);
        self.sync_external_dnd_actions(event_loop, id);
        fika_log!(
            "[fika-wgpu] external-dnd enter id={} target={}",
            id.into_raw(),
            self.scene
                .dnd_hover_target
                .as_ref()
                .map(ShellDropTarget::kind)
                .unwrap_or("none")
        );
        ShellActionOutcome::redraw_if(changed)
    }

    pub(crate) fn external_drag_position(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: DataTransferId,
        position: PhysicalPosition<f64>,
    ) -> ShellActionOutcome {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return ShellActionOutcome::None;
        };
        let Some(transfer) = self
            .incoming_dnd_transfer
            .as_mut()
            .filter(|transfer| transfer.id == id)
        else {
            return ShellActionOutcome::None;
        };
        transfer.last_position = Some(position);
        let point = view_point_from_physical_position(position);
        let changed = if let Some(paths) = transfer.paths.clone() {
            if self.scene.external_drag.is_some() {
                self.scene.update_external_drag(point, size)
            } else {
                self.scene.begin_data_transfer_drag(
                    paths,
                    transfer.local_source.clone(),
                    point,
                    size,
                )
            }
        } else {
            false
        };
        self.sync_external_dnd_actions(event_loop, id);
        ShellActionOutcome::redraw_if(changed)
    }

    pub(crate) fn external_drag_dropped(&mut self, id: DataTransferId) -> ShellActionOutcome {
        self.finish_external_drag_if_ready(id)
    }

    pub(crate) fn external_drag_left(&mut self, id: DataTransferId) -> ShellActionOutcome {
        let changed = if self
            .incoming_dnd_transfer
            .as_ref()
            .is_some_and(|transfer| transfer.id == id)
        {
            self.incoming_dnd_transfer = None;
            self.scene.clear_external_drag()
        } else {
            false
        };
        ShellActionOutcome::redraw_if(changed)
    }

    pub(crate) fn external_drag_data_received(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: DataTransferId,
        serial: AsyncRequestSerial,
        value: Arc<dyn TypedData>,
    ) -> ShellActionOutcome {
        let Some(transfer) = self
            .incoming_dnd_transfer
            .as_mut()
            .filter(|transfer| transfer.id == id)
        else {
            return ShellActionOutcome::None;
        };
        if transfer
            .fetch_serial
            .is_some_and(|fetch_serial| fetch_serial != serial)
        {
            return ShellActionOutcome::None;
        }

        let paths = match external_drag_paths_from_typed_data(value.as_ref()) {
            Ok(paths) => paths,
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] external-dnd data-error id={} {error}",
                    id.into_raw()
                );
                self.set_valid_dnd_actions(event_loop, id, false);
                self.incoming_dnd_transfer = None;
                let changed = self.scene.clear_external_drag();
                return ShellActionOutcome::redraw_if(changed);
            }
        };
        if paths.is_empty() {
            self.set_valid_dnd_actions(event_loop, id, false);
            self.incoming_dnd_transfer = None;
            let changed = self.scene.clear_external_drag();
            fika_log!(
                "[fika-wgpu] external-dnd reject id={} reason=empty-uri-list",
                id.into_raw()
            );
            return ShellActionOutcome::redraw_if(changed);
        }

        if transfer
            .paths
            .as_ref()
            .is_some_and(|provisional| provisional != &paths)
        {
            transfer.local_source = None;
        }
        transfer.paths = Some(paths.clone());
        transfer.data_received = true;
        let local_source = transfer.local_source.clone();
        let changed = if let (Some(position), Some(size)) = (
            transfer.last_position,
            self.renderer.as_ref().map(|renderer| renderer.size),
        ) {
            let point = view_point_from_physical_position(position);
            self.scene
                .begin_data_transfer_drag(paths, local_source, point, size)
        } else {
            false
        };
        let drop_pending = transfer.drop_pending;
        self.sync_external_dnd_actions(event_loop, id);
        fika_log!(
            "[fika-wgpu] external-dnd data id={} sources={}",
            id.into_raw(),
            self.incoming_dnd_transfer
                .as_ref()
                .and_then(|transfer| transfer.paths.as_ref())
                .map(Vec::len)
                .unwrap_or(0)
        );
        if drop_pending {
            return self.finish_external_drag_if_ready(id);
        }
        ShellActionOutcome::redraw_if(changed)
    }

    pub(crate) fn outgoing_drag_dropped(
        &mut self,
        id: DataTransferId,
        action: Option<DndAction>,
    ) -> ShellActionOutcome {
        if !self
            .outgoing_dnd_transfer
            .as_ref()
            .is_some_and(|transfer| transfer.id == id)
        {
            return ShellActionOutcome::None;
        }
        let source_count = self
            .outgoing_dnd_transfer
            .as_ref()
            .map(|transfer| transfer.paths.len())
            .unwrap_or(0);
        self.outgoing_dnd_transfer = None;
        self.outgoing_dnd_start_failed = false;
        let changed = self.scene.clear_internal_drag();
        fika_log!(
            "[fika-wgpu] outgoing-dnd drop id={} action={:?} sources={}",
            id.into_raw(),
            action,
            source_count
        );
        ShellActionOutcome::redraw_if(changed)
    }

    pub(crate) fn outgoing_drag_canceled(&mut self, id: DataTransferId) -> ShellActionOutcome {
        if !self
            .outgoing_dnd_transfer
            .as_ref()
            .is_some_and(|transfer| transfer.id == id)
        {
            return ShellActionOutcome::None;
        }
        let source_count = self
            .outgoing_dnd_transfer
            .as_ref()
            .map(|transfer| transfer.paths.len())
            .unwrap_or(0);
        self.outgoing_dnd_transfer = None;
        self.outgoing_dnd_start_failed = false;
        let changed = self.scene.clear_internal_drag();
        fika_log!(
            "[fika-wgpu] outgoing-dnd cancel id={} sources={}",
            id.into_raw(),
            source_count
        );
        ShellActionOutcome::redraw_if(changed)
    }

    fn finish_external_drag_if_ready(&mut self, id: DataTransferId) -> ShellActionOutcome {
        let Some(transfer) = self
            .incoming_dnd_transfer
            .as_mut()
            .filter(|transfer| transfer.id == id)
        else {
            return ShellActionOutcome::None;
        };
        transfer.drop_pending = true;
        if !transfer.data_received {
            return ShellActionOutcome::None;
        }
        let Some(paths) = transfer.paths.clone() else {
            return ShellActionOutcome::None;
        };
        let Some(position) = transfer.last_position else {
            return ShellActionOutcome::None;
        };
        self.incoming_dnd_transfer = None;
        self.finish_external_drag_paths(paths, position)
    }

    fn finish_external_drag_paths(
        &mut self,
        paths: Vec<PathBuf>,
        position: PhysicalPosition<f64>,
    ) -> ShellActionOutcome {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return ShellActionOutcome::None;
        };
        let point = view_point_from_physical_position(position);
        let sources = external_drag_drop_sources(paths, self.scene.external_drag_sources());
        match self.scene.finish_external_drag(sources, point, size) {
            Ok(changed) => {
                fika_log!(
                    "[fika-wgpu] external-dnd drop menu={} target={}",
                    self.scene.drop_menu.is_some() as u8,
                    self.scene
                        .drop_menu
                        .as_ref()
                        .map(|menu| menu.target.kind())
                        .unwrap_or("none")
                );
                ShellActionOutcome::redraw_if(changed)
            }
            Err(error) => {
                fika_log!("[fika-wgpu] external-dnd-error {error}");
                self.scene
                    .record_task_status(ShellTaskStatus::failed("Drop failed", error, false));
                ShellActionOutcome::Redraw
            }
        }
    }

    fn sync_external_dnd_actions(&self, event_loop: &ActiveEventLoop, id: DataTransferId) {
        let accepted = self
            .incoming_dnd_transfer
            .as_ref()
            .filter(|transfer| transfer.id == id)
            .is_some_and(|transfer| {
                if transfer.paths.is_none() || transfer.last_position.is_none() {
                    return true;
                }
                self.scene.dnd_hover_target.is_some()
            });
        self.set_valid_dnd_actions(event_loop, id, accepted);
    }

    fn set_valid_dnd_actions(
        &self,
        event_loop: &ActiveEventLoop,
        id: DataTransferId,
        accepted: bool,
    ) {
        let actions = if accepted {
            ACCEPTED_DND_ACTIONS.as_slice()
        } else {
            &[]
        };
        if let Err(error) = event_loop.set_valid_dnd_actions(id, actions) {
            fika_log!(
                "[fika-wgpu] dnd-actions-error id={} accepted={} {error}",
                id.into_raw(),
                accepted as u8
            );
        }
    }
}

fn outgoing_dnd_preview_colors(
    source: Option<&ShellInternalDragPreviewSource>,
    theme: crate::shell::theme::ShellTheme,
) -> ([u8; 4], [u8; 4], Option<DragPreviewLabelStyle>) {
    let Some(source) = source else {
        return (
            [0, 0, 0, 0],
            text_color_to_rgba8(theme.primary_text()),
            Some(DragPreviewLabelStyle::PlainSingleLine),
        );
    };
    let layout = source.layout();
    let background = match layout.background_style {
        DragPreviewBackgroundStyle::SelectedItem => item_background_color_for_palette(
            true,
            false,
            DolphinItemPalette::from_shell_theme(theme),
        ),
        DragPreviewBackgroundStyle::HoveredPlace => place_row_background_color_for_palette(
            false,
            true,
            DolphinItemPalette::from_shell_theme(theme),
        ),
    };
    let label_color = match source {
        ShellInternalDragPreviewSource::PaneItem { .. } => {
            if theme.is_dark() {
                [241, 245, 249, 255]
            } else {
                [15, 23, 42, 255]
            }
        }
        ShellInternalDragPreviewSource::Place { .. } => text_color_to_rgba8(theme.primary_text()),
    };
    (
        ui_color_to_rgba8(background),
        label_color,
        layout.label.map(|label| label.style),
    )
}

fn outgoing_dnd_payload(paths: &[PathBuf]) -> OutgoingDndPayload {
    let uris = paths
        .iter()
        .map(|path| path_uri_from_path(path))
        .collect::<Vec<_>>();
    let text = uris.join("\n");
    OutgoingDndPayload { uris, text }
}

fn outgoing_dnd_preview_raster_for_path(
    renderer: &mut crate::WgpuState,
    source: Option<&ShellInternalDragPreviewSource>,
    path: &Path,
    cache_icon_size: f32,
    icon_size_px: u16,
    scale: f32,
) -> Option<IconRaster> {
    match source {
        Some(ShellInternalDragPreviewSource::PaneItem {
            directory,
            entry,
            items,
            view_mode,
            folder_preview,
            ..
        }) => {
            if let Some(item) = items.iter().find(|item| item.path == path) {
                let source_path = entry_path_for_thumbnail(directory, entry);
                let item_folder_preview = (source_path == path)
                    .then_some(folder_preview.as_ref())
                    .flatten();
                return rasterize_entry_drag_icon(
                    renderer,
                    DragPreviewEntryRasterSource {
                        directory,
                        entry: &item.entry,
                        path,
                        folder_preview: item_folder_preview,
                        view_mode: *view_mode,
                    },
                    cache_icon_size,
                    scale,
                );
            }
            rasterize_path_drag_icon(renderer, path, cache_icon_size, icon_size_px, scale)
        }
        Some(ShellInternalDragPreviewSource::Place { icon_name, .. }) => {
            rasterize_named_drag_icon(renderer, icon_name, icon_size_px)
        }
        _ => rasterize_path_drag_icon(renderer, path, cache_icon_size, icon_size_px, scale),
    }
}

struct DragPreviewEntryRasterSource<'a> {
    directory: &'a Path,
    entry: &'a crate::Entry,
    path: &'a Path,
    folder_preview: Option<&'a FolderPreviewReady>,
    view_mode: ShellViewMode,
}

fn rasterize_entry_drag_icon(
    renderer: &mut crate::WgpuState,
    source: DragPreviewEntryRasterSource<'_>,
    cache_icon_size: f32,
    scale: f32,
) -> Option<IconRaster> {
    let DragPreviewEntryRasterSource {
        directory,
        entry,
        path,
        folder_preview,
        view_mode,
    } = source;
    let icon_size_px = icon_cache_size(cache_icon_size);
    let base = if entry.is_dir {
        let resolved = renderer.icon_renderer.resolver.resolve_entry_visible_fast(
            directory,
            entry,
            cache_icon_size,
        );
        let base = rasterize_resolved_drag_icon(renderer, resolved.path, icon_size_px)?;
        apply_folder_preview_to_drag_icon(base, folder_preview, view_mode)
    } else if let Some(raster) = ready_drag_thumbnail(
        &mut renderer.icon_renderer.raster_cache,
        &mut renderer.icon_renderer.thumbnails,
        directory,
        entry,
        icon_size_px,
    ) {
        raster
    } else {
        let resolved = renderer.icon_renderer.resolver.resolve_entry_visible_fast(
            directory,
            entry,
            cache_icon_size,
        );
        rasterize_resolved_drag_icon(renderer, resolved.path, icon_size_px)?
    };
    Some(apply_drag_emblems(renderer, base, path, scale))
}

fn rasterize_path_drag_icon(
    renderer: &mut crate::WgpuState,
    path: &Path,
    cache_icon_size: f32,
    icon_size_px: u16,
    scale: f32,
) -> Option<IconRaster> {
    let key = file_icon_path_cache_key(path, path.is_dir(), None, true, cache_icon_size);
    let resolved = renderer
        .icon_renderer
        .resolver
        .resolve_path_cache_key_fast(key);
    let base = rasterize_resolved_drag_icon(renderer, resolved.path, icon_size_px)?;
    Some(apply_drag_emblems(renderer, base, path, scale))
}

fn ready_drag_thumbnail(
    raster_cache: &mut crate::IconRasterCache,
    thumbnails: &mut crate::ThumbnailRasterResolver,
    directory: &Path,
    entry: &crate::Entry,
    size_px: u16,
) -> Option<IconRaster> {
    let path = entry_path_for_thumbnail(directory, entry);
    let modified_secs = entry.modified_secs?;
    if !thumbnail_request_may_have_preview(&path, entry.mime_type.as_deref()) {
        return None;
    }
    // Prefer the exact live-view size, then any ready neighboring size so the
    // outgoing drag surface still shows the item's thumbnail (Dolphin reads
    // model "iconPixmap", which already holds the preview when available).
    let exact = IconRasterCacheKey::thumbnail(path.clone(), size_px, modified_secs);
    if let Some(raster) = raster_cache.get(&exact) {
        return Some(raster);
    }
    thumbnails.drain_results();
    if let Some(entry) = thumbnails.ready.get_mut(&exact) {
        thumbnails.ready_frame = thumbnails.ready_frame.wrapping_add(1);
        entry.last_used_frame = thumbnails.ready_frame;
        return Some(raster_cache.insert(exact, entry.raster.clone()));
    }
    if let Some(raster) = closest_ready_thumbnail(raster_cache, &path, modified_secs, size_px) {
        return Some(raster);
    }
    closest_ready_thumbnail_from_resolver(thumbnails, raster_cache, &path, modified_secs, size_px)
}

fn closest_ready_thumbnail(
    raster_cache: &mut crate::IconRasterCache,
    path: &Path,
    modified_secs: u64,
    size_px: u16,
) -> Option<IconRaster> {
    let key = raster_cache
        .entries
        .keys()
        .filter(|key| {
            key.stamp == Some(modified_secs)
                && key.path == path
                && key.style == IconRasterStyle::Original
        })
        .min_by_key(|key| key.size_px.abs_diff(size_px))
        .cloned()?;
    raster_cache.get(&key)
}

fn closest_ready_thumbnail_from_resolver(
    thumbnails: &mut crate::ThumbnailRasterResolver,
    raster_cache: &mut crate::IconRasterCache,
    path: &Path,
    modified_secs: u64,
    size_px: u16,
) -> Option<IconRaster> {
    let key = thumbnails
        .ready
        .keys()
        .filter(|key| key.stamp == Some(modified_secs) && key.path == path)
        .min_by_key(|key| key.size_px.abs_diff(size_px))
        .cloned()?;
    let entry = thumbnails.ready.get_mut(&key)?;
    thumbnails.ready_frame = thumbnails.ready_frame.wrapping_add(1);
    entry.last_used_frame = thumbnails.ready_frame;
    Some(raster_cache.insert(key, entry.raster.clone()))
}

fn rasterize_resolved_drag_icon(
    renderer: &mut crate::WgpuState,
    icon_path: Option<PathBuf>,
    size_px: u16,
) -> Option<IconRaster> {
    let icon_path = icon_path?;
    let key = IconRasterCacheKey::icon(icon_path, size_px);
    if let Some(raster) = renderer.icon_renderer.raster_cache.get(&key) {
        return Some(raster);
    }
    let raster = rasterize_icon(&key.path, size_px as u32)?;
    Some(renderer.icon_renderer.raster_cache.insert(key, raster))
}

fn rasterize_named_drag_icon(
    renderer: &mut crate::WgpuState,
    icon_name: &str,
    size_px: u16,
) -> Option<IconRaster> {
    let key = FileIconPathCacheKey {
        role: FileIconRoleCacheKey {
            kind: FileIconKind::Named {
                icon_name: icon_name.to_string(),
                fallback: NamedIconFallback::Service,
            },
        },
        size_px,
    };
    let resolved = renderer
        .icon_renderer
        .resolver
        .resolve_path_cache_key_fast(key);
    rasterize_resolved_drag_icon(renderer, resolved.path, size_px)
}

fn rasterize_named_drag_icon_exact(
    renderer: &mut crate::WgpuState,
    icon_name: &str,
    size_px: u16,
) -> Option<IconRaster> {
    let path = renderer
        .icon_renderer
        .resolver
        .resolve_named_exact_fast(icon_name, size_px as f32)?;
    rasterize_resolved_drag_icon(renderer, Some(path), size_px)
}

fn apply_folder_preview_to_drag_icon(
    base: IconRaster,
    folder_preview: Option<&FolderPreviewReady>,
    view_mode: ShellViewMode,
) -> IconRaster {
    let Some(folder_preview) = folder_preview else {
        return base;
    };
    let layout = ItemPixmapLayout {
        view_mode,
        icon_rect: ViewRect {
            x: 0.0,
            y: 0.0,
            width: base.width as f32,
            height: base.height as f32,
        },
        text_rect: ViewRect {
            x: 0.0,
            y: 0.0,
            width: base.width as f32,
            height: base.height as f32,
        },
        text_midline_shift: 0.0,
    };
    let draw_rect = folder_preview_role_draw_rect(layout, &folder_preview.raster);
    let rect = PixelRect::new(
        draw_rect.x.round() as i32,
        draw_rect.y.round() as i32,
        draw_rect.width.round().max(1.0) as i32,
        draw_rect.height.round().max(1.0) as i32,
    );
    let mut pixels = base.pixels.to_vec();
    draw_raster_scaled(&mut pixels, base.width, &folder_preview.raster, rect, 1.0);
    IconRaster {
        pixels: Arc::from(pixels),
        width: base.width,
        height: base.height,
    }
}

fn apply_drag_emblems(
    renderer: &mut crate::WgpuState,
    base: IconRaster,
    path: &Path,
    scale: f32,
) -> IconRaster {
    let emblems = icon_emblem_kinds_for_path(path);
    if emblems.is_empty() {
        return base;
    }
    let rects = drag_emblem_pixel_rects(base.width, scale);
    let mut pixels = base.pixels.to_vec();
    for (index, emblem) in emblems.into_iter().take(rects.len()).enumerate() {
        let rect = rects[index];
        let size_px = icon_cache_size(rect.width.max(rect.height) as f32);
        for icon_name in emblem.theme_names() {
            if let Some(raster) = rasterize_named_drag_icon_exact(renderer, icon_name, size_px) {
                draw_raster_scaled(&mut pixels, base.width, &raster, rect, 1.0);
                break;
            }
        }
    }
    IconRaster {
        pixels: Arc::from(pixels),
        width: base.width,
        height: base.height,
    }
}

fn drag_emblem_pixel_rects(icon_size: u32, scale: f32) -> [PixelRect; 4] {
    let paint_area = ViewRect {
        x: 0.0,
        y: 0.0,
        width: icon_size as f32,
        height: icon_size as f32,
    };
    icon_emblem_rects(paint_area, scale).map(|rect| {
        PixelRect::new(
            rect.x.round() as i32,
            rect.y.round() as i32,
            rect.width.round().max(1.0) as i32,
            rect.height.round().max(1.0) as i32,
        )
    })
}

fn outgoing_dnd_drag_icon(
    paths: &[PathBuf],
    metrics: OutgoingDndPreviewMetrics,
    rasters: Option<&OutgoingDndPreviewRasters>,
    label: Option<&OutgoingDndPreviewLabelRaster>,
    label_color: [u8; 4],
) -> Option<DragIcon> {
    let pixels =
        outgoing_dnd_preview_pixels_with_label(paths, metrics, rasters, label, label_color);
    let icon = RgbaIcon::new(pixels, metrics.canvas_width, metrics.canvas_height).ok()?;
    // Runtime `DndIcon` offset is logical surface coords relative to the drag
    // hotspot (wayland-client-runtime::dnd). Metrics store that as
    // `hotspot_logical_*` (scene hotspot / ui_scale).
    Some(DragIcon {
        icon,
        buffer_scale: metrics.buffer_scale,
        offset_x: -metrics.hotspot_logical_x,
        offset_y: -metrics.hotspot_logical_y,
    })
}

fn ui_color_to_rgba8(color: [f32; 4]) -> [u8; 4] {
    color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn text_color_to_rgba8(color: cosmic_text::Color) -> [u8; 4] {
    let [r, g, b, a] = color.as_rgba();
    [r, g, b, a]
}

fn outgoing_dnd_fallback_label(paths: &[PathBuf]) -> String {
    if paths.len() > 1 {
        return format!("{} items", paths.len());
    }
    paths
        .first()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Item")
        .to_string()
}

include!("drag/preview_raster.rs");

#[cfg(test)]
#[path = "drag/tests.rs"]
mod tests;
