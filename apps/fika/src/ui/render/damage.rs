use fika_core::ViewRect;

use crate::ui::pane::ShellPaneProjection;
use crate::{
    FolderPreviewReady, FolderPreviewRoleChange, FolderPreviewRoleKey, ItemPixmapLayout,
    ShellScene, folder_preview_role_shell_rect, pane_content_rect_to_screen,
};

#[cfg(test)]
pub(crate) fn folder_preview_damage_rects_for_changed_keys(
    scene: &ShellScene,
    projections: &[ShellPaneProjection<'_>],
    keys: &[FolderPreviewRoleKey],
) -> Vec<ViewRect> {
    keys.iter()
        .filter_map(|key| folder_preview_damage_rect_for_changed_key(scene, projections, key, None))
        .collect()
}

pub(crate) fn folder_preview_damage_rects_for_changes(
    scene: &ShellScene,
    projections: &[ShellPaneProjection<'_>],
    changes: &[FolderPreviewRoleChange],
) -> Vec<ViewRect> {
    changes
        .iter()
        .filter_map(|change| {
            folder_preview_damage_rect_for_changed_key(
                scene,
                projections,
                &change.key,
                change.previous.as_ref(),
            )
        })
        .collect()
}

fn folder_preview_damage_rect_for_changed_key(
    scene: &ShellScene,
    projections: &[ShellPaneProjection<'_>],
    key: &FolderPreviewRoleKey,
    previous: Option<&FolderPreviewReady>,
) -> Option<ViewRect> {
    for projection in projections {
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
            if entry.modified_secs != Some(key.directory_modified_secs) {
                continue;
            }
            let Some(candidate) = scene.entry_path_for_pane_view(projection.view, entry_index)
            else {
                continue;
            };
            if candidate == key.path {
                let pixmap_layout =
                    ItemPixmapLayout::from_item_layout(projection.view.view_mode, item.layout);
                let requested_size = scene.folder_preview_role_size_px_for_item(pixmap_layout);
                let roles = scene.folder_preview_roles.borrow();
                let preview = roles.preview_or_closest(
                    &key.path,
                    key.directory_modified_secs,
                    requested_size,
                );
                let exact_preview_changed = preview
                    .filter(|preview| preview.size_px == key.size_px)
                    .is_some();
                let previous_preview_disappeared = previous.is_some()
                    && preview
                        .map(|preview| preview.size_px != requested_size)
                        .unwrap_or(true);
                let requested_preview_failed =
                    requested_size == key.size_px && roles.failed.contains(key);
                if !(exact_preview_changed
                    || previous_preview_disappeared
                    || requested_preview_failed)
                {
                    continue;
                }
                let damage_rect = folder_preview_role_shell_rect(pixmap_layout);
                return Some(pane_content_rect_to_screen(damage_rect, projection));
            }
        }
    }
    None
}
