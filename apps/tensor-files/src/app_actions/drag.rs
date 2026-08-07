#[cfg(test)]
use std::fs;
#[cfg(all(test, unix))]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::windowing::{
    ActiveEventLoop, AsyncRequestSerial, DataTransferId, DataTransferSendBuilder, DndAction,
    DragIcon, PhysicalPosition, PhysicalSize, SendData, TypeHint, TypedData,
};

use super::outcome::ShellActionOutcome;
use crate::ui::drag_preview_layout::{
    DragPreviewBackgroundStyle, DragPreviewLabelStyle, MultiDragPreviewLayout,
    SingleDragPreviewLayout, multi_drag_preview_layout,
};
use crate::ui::drop_menu::ShellDropTarget;
use crate::ui::file_item_view::style::{
    FileManagerItemPalette, item_background_color_for_palette,
    place_row_background_color_for_palette,
};
use crate::ui::icon_roles::{file_icon_path_cache_key, icon_cache_size};
use crate::ui::tasks::ShellTaskStatus;
use crate::{
    FolderPreviewCacheStats, IconDrawLayer, IconFrameBuilder, IconFrameConfig, IconFrameResources,
    IconGpuSource, IncomingDndTransfer, ItemPixmapLayout, OutgoingDndTransfer,
    ShellInternalDragPreviewSource, ShellViewMode, TensorFilesApp, TextFrameBuilder,
    TextFrameResources, ViewRect, decode_file_clipboard_text, entry_path_for_thumbnail,
    folder_preview_gpu_draw_rect, icon_emblem_kinds_for_path, icon_emblem_rects,
    normalized_scale_factor, path_uri_from_path, thumbnail_request_may_have_preview,
    view_point_from_physical_position,
};

const ACCEPTED_DND_ACTIONS: [DndAction; 3] = [DndAction::Ask, DndAction::Move, DndAction::Copy];
const DND_FALLBACK_ICON_SIZE: f32 = 128.0;

#[derive(Clone, Debug)]
struct OutgoingDndPayload {
    uris: Vec<String>,
    text: String,
}

impl TensorFilesApp {
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
        let (drag_icon, icon_texture) = self
            .renderer
            .as_mut()
            .and_then(|renderer| {
                let draws = outgoing_dnd_preview_gpu_draws(
                    renderer,
                    preview_source.as_ref(),
                    &paths,
                    preview_metrics,
                    preview_metrics.buffer_scale as f32,
                );
                outgoing_dnd_gpu_drag_icon(
                    renderer,
                    preview_metrics,
                    draws,
                    &preview_label,
                    label_color,
                )
            })
            .map(|(icon, texture)| (Some(icon), Some(texture)))
            .unwrap_or((None, None));
        let send_data = DataTransferSendBuilder::new(payload)
            .with_type(TypeHint::UriList, |payload, _| {
                Some(SendData::Uris(payload.uris.clone()))
            })
            .with_type(TypeHint::Plaintext, |payload, _| Some(payload.text.clone()))
            .build();
        match event_loop.start_drag(window_id, send_data, &ACCEPTED_DND_ACTIONS, drag_icon) {
            Ok(id) => {
                tensor_files_log!(
                    "[tensor-files] outgoing-dnd start id={} sources={}",
                    id.into_raw(),
                    paths.len()
                );
                self.outgoing_dnd_transfer = Some(OutgoingDndTransfer {
                    id,
                    paths,
                    source,
                    _icon_texture: icon_texture,
                });
            }
            Err(error) => {
                self.outgoing_dnd_start_failed = true;
                tensor_files_log!("[tensor-files] outgoing-dnd-unavailable {error}");
                if self.scene.clear_internal_drag()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
        }
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
                tensor_files_log!(
                    "[tensor-files] external-dnd data-transfer-error id={} {error}",
                    id.into_raw()
                );
                false
            });

        if !supports_uri_list {
            self.set_valid_dnd_actions(event_loop, id, false);
            self.incoming_dnd_transfer = None;
            let changed = self.scene.clear_external_drag();
            tensor_files_log!(
                "[tensor-files] external-dnd reject id={} reason=missing-uri-list",
                id.into_raw()
            );
            return ShellActionOutcome::redraw_if(changed);
        }

        match event_loop.fetch_data_transfer(id, &TypeHint::UriList) {
            Ok(serial) => {
                transfer.fetch_serial = Some(serial);
            }
            Err(error) => {
                tensor_files_log!(
                    "[tensor-files] external-dnd fetch-error id={} {error}",
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
        tensor_files_log!(
            "[tensor-files] external-dnd enter id={} target={}",
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
                tensor_files_log!(
                    "[tensor-files] external-dnd data-error id={} {error}",
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
            tensor_files_log!(
                "[tensor-files] external-dnd reject id={} reason=empty-uri-list",
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
        tensor_files_log!(
            "[tensor-files] external-dnd data id={} sources={}",
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
        tensor_files_log!(
            "[tensor-files] outgoing-dnd drop id={} action={:?} sources={}",
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
        tensor_files_log!(
            "[tensor-files] outgoing-dnd cancel id={} sources={}",
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
                tensor_files_log!(
                    "[tensor-files] external-dnd drop menu={} target={}",
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
                tensor_files_log!("[tensor-files] external-dnd-error {error}");
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
            tensor_files_log!(
                "[tensor-files] dnd-actions-error id={} accepted={} {error}",
                id.into_raw(),
                accepted as u8
            );
        }
    }
}

include!("drag/outgoing_preview.rs");

include!("drag/preview_geometry.rs");

#[cfg(test)]
#[path = "drag/tests.rs"]
mod tests;
