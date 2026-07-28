use std::hash::{Hash, Hasher};

use crate::windowing::PhysicalSize;
use fika_core::{ViewPoint, is_network_path};

use crate::shell::context_menu::ShellContextMenu;
use crate::shell::drop_menu::{ShellDropMenu, ShellDropTarget};
use crate::shell::location::ShellLocationDraft;
use crate::shell::options::ShellViewMode;
use crate::shell::pane::{ShellPaneId, ShellPaneProjection, ShellPaneState};
use crate::{
    FolderPreviewRoleKey, ItemPixmapLayout, RubberBand, ShellFolderPreviewRoleRuntime,
    ShellInternalDrag, ShellInternalDragSource, ShellPaneItemTarget, ShellScene,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellRenderDirtyKey {
    pub(crate) values: Box<[u64]>,
}

pub(crate) struct ShellRenderDirtyKeyContext {
    pane_entries_hashes: [u64; 2],
    folder_preview_roles_hash: u64,
}

impl ShellRenderDirtyKeyContext {
    pub(crate) fn from_scene(scene: &ShellScene, projections: &[ShellPaneProjection<'_>]) -> Self {
        let mut pane_entries_hashes = [0; 2];
        for pane_id in ShellPaneId::ALL {
            if let Some(pane) = scene.panes.get(pane_id) {
                pane_entries_hashes[pane_id.index()] =
                    pane_entries_dirty_hash(pane_id, pane, projections);
            }
        }
        Self {
            pane_entries_hashes,
            folder_preview_roles_hash: folder_preview_roles_dirty_hash(scene, projections),
        }
    }
}

impl ShellRenderDirtyKey {
    #[cfg(test)]
    pub(crate) fn from_scene(scene: &ShellScene, size: PhysicalSize<u32>) -> Self {
        let context = dirty_key_context_from_scene_lookup(scene, size);
        Self::from_scene_with_context(scene, size, &context)
    }

    pub(crate) fn from_scene_with_context(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
        context: &ShellRenderDirtyKeyContext,
    ) -> Self {
        Self::from_scene_with_options(scene, size, ShellRenderDirtyKeyOptions::default(), context)
    }

    #[cfg(test)]
    pub(crate) fn from_scene_ignoring_hover(scene: &ShellScene, size: PhysicalSize<u32>) -> Self {
        let context = dirty_key_context_from_scene_lookup(scene, size);
        Self::from_scene_ignoring_hover_with_context(scene, size, &context)
    }

    pub(crate) fn from_scene_ignoring_hover_with_context(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
        context: &ShellRenderDirtyKeyContext,
    ) -> Self {
        Self::from_scene_with_options(
            scene,
            size,
            ShellRenderDirtyKeyOptions::ignoring_hover(),
            context,
        )
    }

    #[cfg(test)]
    pub(crate) fn from_scene_ignoring_folder_preview_roles(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
    ) -> Self {
        let context = dirty_key_context_from_scene_lookup(scene, size);
        Self::from_scene_ignoring_folder_preview_roles_with_context(scene, size, &context)
    }

    pub(crate) fn from_scene_ignoring_folder_preview_roles_with_context(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
        context: &ShellRenderDirtyKeyContext,
    ) -> Self {
        Self::from_scene_with_options(
            scene,
            size,
            ShellRenderDirtyKeyOptions::ignoring_folder_preview_roles(),
            context,
        )
    }

    #[cfg(test)]
    pub(crate) fn from_scene_ignoring_hover_and_folder_preview_roles(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
    ) -> Self {
        let context = dirty_key_context_from_scene_lookup(scene, size);
        Self::from_scene_ignoring_hover_and_folder_preview_roles_with_context(scene, size, &context)
    }

    pub(crate) fn from_scene_ignoring_hover_and_folder_preview_roles_with_context(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
        context: &ShellRenderDirtyKeyContext,
    ) -> Self {
        Self::from_scene_with_options(
            scene,
            size,
            ShellRenderDirtyKeyOptions::ignoring_hover_and_folder_preview_roles(),
            context,
        )
    }

    fn from_scene_with_options(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
        options: ShellRenderDirtyKeyOptions,
        context: &ShellRenderDirtyKeyContext,
    ) -> Self {
        let mut values = Vec::with_capacity(128);
        push_u64(&mut values, size.width as u64);
        push_u64(&mut values, size.height as u64);
        push_f32(&mut values, scene.scale_factor);
        push_u64(&mut values, scene.active_pane.index() as u64);
        push_bool(&mut values, scene.places_visible);
        push_f32(&mut values, scene.places_width);
        push_f32(
            &mut values,
            if options.include_places_scroll {
                scene.places_scroll_y
            } else {
                0.0
            },
        );
        push_bool(&mut values, scene.filter_active);
        push_hash(&mut values, &scene.filter_pattern);
        push_bool(&mut values, scene.show_hidden);
        push_bool(&mut values, scene.dark_mode);
        push_bool(&mut values, scene.background_blur);
        push_f32(&mut values, scene.background_opacity);
        push_f32(&mut values, scene.split_pane_left_fraction);
        push_u64(
            &mut values,
            scene.animation_dirty_value_with_hover(options.include_hover),
        );
        push_u64(
            &mut values,
            if options.include_text_caret_blink {
                scene.location_text_caret_dirty_value()
            } else {
                0
            },
        );

        for pane_id in ShellPaneId::ALL {
            match scene.panes.get(pane_id) {
                Some(pane) => {
                    push_bool(&mut values, true);
                    push_hash(&mut values, &pane.path);
                    push_hash(&mut values, &pane.pending_path);
                    push_hash(&mut values, pane.view_mode.as_str());
                    push_u64(&mut values, pane.zoom_step as i64 as u64);
                    push_u64(&mut values, pane.entries.len() as u64);
                    push_u64(&mut values, pane.dir_count as u64);
                    push_u64(&mut values, pane.filtered_indexes.len() as u64);
                    push_u64(&mut values, context.pane_entries_hashes[pane_id.index()]);
                    push_u64(&mut values, pane.selection.len() as u64);
                    push_option_usize(&mut values, pane.selection.anchor);
                    push_option_usize(&mut values, pane.selection.focus);
                    push_hash(&mut values, &pane.selection.selected);
                    push_f32(&mut values, pane.scroll_x);
                    push_f32(&mut values, pane.scroll_y);
                }
                None => push_bool(&mut values, false),
            }
        }
        if options.include_folder_preview_roles {
            push_u64(&mut values, context.folder_preview_roles_hash);
        }

        push_pane_item_target(
            &mut values,
            if options.include_hover {
                scene.hovered_item
            } else {
                None
            },
        );
        push_option_usize(
            &mut values,
            if options.include_hover {
                scene.hovered_place
            } else {
                None
            },
        );
        push_drop_target(
            &mut values,
            if options.include_dnd_hover {
                scene.dnd_hover_target.as_ref()
            } else {
                None
            },
        );
        push_internal_drag(
            &mut values,
            if options.include_internal_drag {
                scene.internal_drag.as_ref()
            } else {
                None
            },
        );
        push_bool(&mut values, scene.pending_drop_request.is_some());
        push_rubber_band(
            &mut values,
            if options.include_rubber_band {
                scene.rubber_band.as_ref()
            } else {
                None
            },
        );
        push_location_draft(
            &mut values,
            if options.include_location_draft {
                scene.location_draft.as_ref()
            } else {
                None
            },
        );
        push_context_menu(
            &mut values,
            if options.include_context_menu {
                scene.context_menu.as_ref()
            } else {
                None
            },
            options.include_context_menu_hover,
        );
        push_drop_menu(
            &mut values,
            if options.include_drop_menu {
                scene.drop_menu.as_ref()
            } else {
                None
            },
            options.include_drop_menu_hover,
        );
        push_u64(&mut values, scene.task_statuses.len() as u64);

        for value in [
            if options.include_hover {
                scene.hit_tests
            } else {
                0
            },
            scene.selection_changes,
            scene.context_target_changes,
            scene.context_menu_actions,
            scene.open_changes,
            scene.copy_location_changes,
            scene.file_clipboard_changes,
            scene.paste_changes,
            scene.trash_changes,
            scene.places_changes,
            scene.places_resize_changes,
            if options.include_places_scroll {
                scene.places_scroll_changes
            } else {
                0
            },
            scene.keyboard_navigation,
            if options.include_rubber_band {
                scene.rubber_band_updates
            } else {
                0
            },
            scene.view_switches,
            scene.path_changes,
            scene.directory_reloads,
            if options.include_location_changes {
                scene.location_changes
            } else {
                0
            },
            scene.filter_changes,
            scene.hidden_changes,
            scene.appearance_changes,
            scene.zoom_changes,
            scene.split_pane_changes,
            if options.include_dnd_hover {
                scene.dnd_hover_changes
            } else {
                0
            },
            scene.dnd_drop_requests,
            if options.include_task_status_changes {
                scene.task_statuses.change_generation()
            } else {
                0
            },
        ] {
            push_u64(&mut values, value);
        }

        Self {
            values: values.into_boxed_slice(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ShellRenderDirtyKeyOptions {
    include_hover: bool,
    include_context_menu: bool,
    include_context_menu_hover: bool,
    include_drop_menu: bool,
    include_drop_menu_hover: bool,
    include_dnd_hover: bool,
    include_internal_drag: bool,
    include_places_scroll: bool,
    include_location_draft: bool,
    include_location_changes: bool,
    include_task_status_changes: bool,
    include_rubber_band: bool,
    include_folder_preview_roles: bool,
    include_text_caret_blink: bool,
}

impl Default for ShellRenderDirtyKeyOptions {
    fn default() -> Self {
        Self {
            include_hover: true,
            include_context_menu: true,
            include_context_menu_hover: true,
            include_drop_menu: true,
            include_drop_menu_hover: true,
            include_dnd_hover: true,
            include_internal_drag: true,
            include_places_scroll: true,
            include_location_draft: true,
            include_location_changes: true,
            include_task_status_changes: true,
            include_rubber_band: true,
            include_folder_preview_roles: true,
            include_text_caret_blink: true,
        }
    }
}

impl ShellRenderDirtyKeyOptions {
    fn ignoring_hover() -> Self {
        Self {
            include_hover: false,
            include_context_menu: false,
            include_context_menu_hover: false,
            include_drop_menu: false,
            include_drop_menu_hover: false,
            include_dnd_hover: false,
            include_internal_drag: false,
            include_places_scroll: false,
            include_location_draft: false,
            include_location_changes: false,
            include_task_status_changes: false,
            include_rubber_band: false,
            include_folder_preview_roles: true,
            include_text_caret_blink: false,
        }
    }

    fn ignoring_folder_preview_roles() -> Self {
        Self {
            include_folder_preview_roles: false,
            ..Self::default()
        }
    }

    fn ignoring_hover_and_folder_preview_roles() -> Self {
        Self {
            include_folder_preview_roles: false,
            ..Self::ignoring_hover()
        }
    }
}

fn push_u64(values: &mut Vec<u64>, value: u64) {
    values.push(value);
}

fn push_bool(values: &mut Vec<u64>, value: bool) {
    values.push(value as u64);
}

fn push_f32(values: &mut Vec<u64>, value: f32) {
    values.push(value.to_bits() as u64);
}

fn push_hash(values: &mut Vec<u64>, value: impl Hash) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    values.push(hasher.finish());
}

#[cfg(test)]
fn dirty_key_context_from_scene_lookup(
    scene: &ShellScene,
    size: PhysicalSize<u32>,
) -> ShellRenderDirtyKeyContext {
    let projections = ShellPaneId::ALL
        .into_iter()
        .filter_map(|pane_id| scene.pane_projection(pane_id, size))
        .collect::<Vec<_>>();
    ShellRenderDirtyKeyContext::from_scene(scene, &projections)
}

fn pane_entries_dirty_hash(
    pane_id: ShellPaneId,
    pane: &ShellPaneState,
    projections: &[ShellPaneProjection<'_>],
) -> u64 {
    if pane.view_mode == ShellViewMode::Details
        && let Some(projection) = projections
            .iter()
            .find(|projection| projection.geometry.kind == pane_id)
    {
        return pane_entries_visual_hash_for_indexes(
            pane,
            projection
                .visible_items
                .iter()
                .filter_map(|item| pane.filtered_indexes.get(item.layout.model_index).copied()),
        );
    }
    pane_entries_visual_hash(pane)
}

fn pane_entries_visual_hash(pane: &ShellPaneState) -> u64 {
    pane_entries_visual_hash_for_indexes(pane, pane.filtered_indexes.iter().copied())
}

fn pane_entries_visual_hash_for_indexes(
    pane: &ShellPaneState,
    indexes: impl IntoIterator<Item = usize>,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for entry_index in indexes {
        entry_index.hash(&mut hasher);
        match pane.entries.get(entry_index) {
            Some(entry) => {
                true.hash(&mut hasher);
                entry.name.hash(&mut hasher);
                entry.name_width_units.hash(&mut hasher);
                entry.target_path.hash(&mut hasher);
                entry.size_bytes.hash(&mut hasher);
                entry.modified_secs.hash(&mut hasher);
                entry.metadata_complete.hash(&mut hasher);
                entry.mime_type.as_deref().hash(&mut hasher);
                entry.mime_magic_checked.hash(&mut hasher);
                entry.trash_original_path.hash(&mut hasher);
                entry.trash_deletion_time.as_deref().hash(&mut hasher);
                entry.is_dir.hash(&mut hasher);
            }
            None => false.hash(&mut hasher),
        }
    }
    hasher.finish()
}

fn folder_preview_roles_dirty_hash(
    scene: &ShellScene,
    projections: &[ShellPaneProjection<'_>],
) -> u64 {
    let roles = scene.folder_preview_roles.borrow();
    let mut states = Vec::new();
    for projection in projections {
        push_folder_preview_role_states_for_projection(&mut states, scene, &roles, projection);
    }
    states.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    states.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct FolderPreviewRoleVisualState {
    pane_index: usize,
    path: std::path::PathBuf,
    directory_modified_secs: u64,
    requested_size: u16,
    status: FolderPreviewRoleVisualStatus,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct FolderPreviewRoleVisualStatus {
    kind: u8,
    size_px: u16,
    stamp: u64,
}

fn push_folder_preview_role_states_for_projection(
    states: &mut Vec<FolderPreviewRoleVisualState>,
    scene: &ShellScene,
    roles: &ShellFolderPreviewRoleRuntime,
    projection: &ShellPaneProjection<'_>,
) {
    let pane_id = projection.geometry.kind;
    for item in &projection.visible_items {
        let Some(entry_index) = projection
            .view
            .filtered_indexes
            .get(item.layout.model_index)
            .copied()
        else {
            continue;
        };
        let Some(entry) = projection.view.entries.get(entry_index) else {
            continue;
        };
        if !entry.is_dir || !entry.metadata_complete {
            continue;
        }
        let Some(modified_secs) = entry.modified_secs else {
            continue;
        };
        let Some(path) = scene.entry_path_for_pane_view(projection.view, entry_index) else {
            continue;
        };
        if is_network_path(&path) {
            continue;
        }
        let pixmap_layout =
            ItemPixmapLayout::from_item_layout(projection.view.view_mode, item.layout);
        let requested_size = scene.folder_preview_role_size_px_for_item(pixmap_layout);
        let requested_key = FolderPreviewRoleKey::new(path.clone(), modified_secs, requested_size);
        let state = roles
            .preview_or_closest(&path, modified_secs, requested_size)
            .map(|preview| FolderPreviewRoleVisualStatus {
                kind: 1,
                size_px: preview.size_px,
                stamp: preview.stamp,
            })
            .or_else(|| {
                roles
                    .failed
                    .contains(&requested_key)
                    .then_some(FolderPreviewRoleVisualStatus {
                        kind: 2,
                        size_px: requested_size,
                        stamp: 0,
                    })
            })
            .unwrap_or(FolderPreviewRoleVisualStatus {
                kind: 0,
                size_px: requested_size,
                stamp: 0,
            });
        states.push(FolderPreviewRoleVisualState {
            pane_index: pane_id.index(),
            path,
            directory_modified_secs: modified_secs,
            requested_size,
            status: state,
        });
    }
}

fn push_option_usize(values: &mut Vec<u64>, value: Option<usize>) {
    match value {
        Some(value) => {
            push_bool(values, true);
            push_u64(values, value as u64);
        }
        None => push_bool(values, false),
    }
}

fn push_pane_item_target(values: &mut Vec<u64>, target: Option<ShellPaneItemTarget>) {
    match target {
        Some(target) => {
            push_bool(values, true);
            push_u64(values, target.pane.index() as u64);
            push_u64(values, target.index as u64);
        }
        None => push_bool(values, false),
    }
}

fn push_view_point(values: &mut Vec<u64>, point: ViewPoint) {
    push_f32(values, point.x);
    push_f32(values, point.y);
}

fn push_drop_target(values: &mut Vec<u64>, target: Option<&ShellDropTarget>) {
    match target {
        Some(ShellDropTarget::PaneItem {
            pane,
            index,
            path,
            is_dir,
        }) => {
            push_u64(values, 1);
            push_u64(values, pane.index() as u64);
            push_u64(values, *index as u64);
            push_hash(values, path);
            push_bool(values, *is_dir);
        }
        Some(ShellDropTarget::PaneBlank { pane, path }) => {
            push_u64(values, 2);
            push_u64(values, pane.index() as u64);
            push_hash(values, path);
        }
        Some(ShellDropTarget::Place { index, path }) => {
            push_u64(values, 3);
            push_u64(values, *index as u64);
            push_hash(values, path);
        }
        Some(ShellDropTarget::PlacesGap { index }) => {
            push_u64(values, 4);
            push_u64(values, *index as u64);
        }
        Some(ShellDropTarget::PlacesBlank) => push_u64(values, 5),
        None => push_u64(values, 0),
    }
}

fn push_internal_drag(values: &mut Vec<u64>, drag: Option<&ShellInternalDrag>) {
    match drag {
        Some(drag) => {
            push_bool(values, true);
            match &drag.source {
                ShellInternalDragSource::PaneItem {
                    pane,
                    index,
                    source_path,
                    is_dir,
                } => {
                    push_u64(values, 1);
                    push_u64(values, pane.index() as u64);
                    push_u64(values, *index as u64);
                    push_hash(values, source_path);
                    push_bool(values, *is_dir);
                }
                ShellInternalDragSource::Place { index } => {
                    push_u64(values, 2);
                    push_u64(values, *index as u64);
                }
            }
            push_u64(values, drag.paths.len() as u64);
            for path in &drag.paths {
                push_hash(values, path);
            }
            push_hash(values, &drag.label);
            // Pointer motion no longer repaints an in-window drag overlay; the
            // compositor owns the Wayland DnD icon. Only source identity and
            // active state affect the window surface.
            push_view_point(values, drag.start);
            push_bool(values, drag.active);
        }
        None => push_bool(values, false),
    }
}

fn push_rubber_band(values: &mut Vec<u64>, rubber_band: Option<&RubberBand>) {
    match rubber_band {
        Some(rubber_band) => {
            push_bool(values, true);
            push_view_point(values, rubber_band.start);
            push_view_point(values, rubber_band.current);
            push_bool(values, rubber_band.active);
            push_hash(values, format!("{:?}", rubber_band.mode));
        }
        None => push_bool(values, false),
    }
}

fn push_location_draft(values: &mut Vec<u64>, draft: Option<&ShellLocationDraft>) {
    match draft {
        Some(draft) => {
            push_bool(values, true);
            push_u64(values, draft.pane.index() as u64);
            push_hash(values, &draft.draft.value);
            push_u64(values, draft.draft.cursor as u64);
            push_bool(values, draft.draft.replace_on_insert);
            push_hash(values, format!("{:?}", draft.purpose));
        }
        None => push_bool(values, false),
    }
}

fn push_context_menu(values: &mut Vec<u64>, menu: Option<&ShellContextMenu>, include_hover: bool) {
    match menu {
        Some(menu) => {
            push_bool(values, true);
            push_view_point(values, menu.position);
            if include_hover {
                push_option_usize(values, menu.hovered_row);
                push_option_usize(values, menu.hovered_submenu_row);
                push_hash(values, format!("{:?}", menu.active_submenu));
                push_option_usize(values, menu.active_submenu_row);
            }
        }
        None => push_bool(values, false),
    }
}

fn push_drop_menu(values: &mut Vec<u64>, menu: Option<&ShellDropMenu>, include_hover: bool) {
    match menu {
        Some(menu) => {
            push_bool(values, true);
            push_view_point(values, menu.position);
            if include_hover {
                push_option_usize(values, menu.hovered_row);
            }
            push_hash(values, &menu.target_dir);
            push_drop_target(values, Some(&menu.target));
        }
        None => push_bool(values, false),
    }
}
