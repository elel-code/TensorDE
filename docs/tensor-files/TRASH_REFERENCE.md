# Trash Reference

Tensor Files's Trash implementation follows Dolphin's `trash:/` model roles and
operation flow, while using the local XDG Trash layout as the backing store.

## Dolphin Sources

- `references/fika/dolphin/src/trash/dolphintrash.cpp`
  - Owns a `Trash` singleton with a `KDirLister` opened on `trash:/`.
  - Emits `emptinessChanged` from lister completion and deletion signals.
  - Refreshes `trash:/` when removable storage accessibility changes.
  - `Trash::empty()` runs `KIO::DeleteOrTrashJob` with `EmptyTrash`.
  - `Trash::isEmpty()` reads `trashrc` status for menu enablement.
- `references/fika/dolphin/src/views/dolphinview.cpp`
  - `trashSelectedItems()` sends selected URLs to `KIO::DeleteOrTrashJob`
    with `Trash`.
  - `deleteSelectedItems()` uses the same job type with `Delete`.
  - Both operations finish asynchronously and let the view keep the next item
    visible while the model changes.
- `references/fika/dolphin/src/kitemviews/kfileitemmodel.cpp`
  - In `trash:/`, PathRole is populated from `KIO::UDSEntry::UDS_EXTRA`.
  - DeletionTimeRole is populated from `KIO::UDSEntry::UDS_EXTRA + 1`.
  - DeletionTimeRole compares parsed date-time values as a model sort role.
- `references/fika/dolphin/src/kitemviews/kfileitemmodel.h`
  - Defines `DeletionTimeRole` as a first-class model role.
- `references/fika/dolphin/src/dolphincontextmenu.cpp`
  - Trash viewport context menu contains `Empty Trash`, enabled from
    `Trash::isEmpty()` and updated by `Trash::emptinessChanged`.
- `references/fika/dolphin/src/dolphinplacesmodelsingleton.cpp`
  - Places model listens to `Trash::emptinessChanged` and updates the Trash
    decoration role for the `trash:/` entry.
- `references/fika/dolphin/src/views/viewproperties.cpp`
  - Trash keeps a special-folder default view, with Details view semantics and
    trash-specific roles available for display/sort.

## Nautilus Sources

- `references/fika/nautilus/src/nautilus-files-view.c`
  - `files_view_remove_files()` maps changed files to existing view items
    before removal, preserving view-item identity until the model update.
  - `process_pending_files()` batches changed files that should no longer be
    shown and emits one remove operation per directory.
- `references/fika/nautilus/src/nautilus-view-model.c`
  - `nautilus_view_model_remove_items()` walks the directory store backwards
    and removes contiguous ranges with `g_list_store_splice()`, minimizing
    `items-changed` emissions.
  - The range-based update is the important behavior to mirror for smooth
    delete animations: keep stable item identity, remove in batches, and let
    the view animate surviving items from old rects to new rects.
- `references/fika/nautilus/src/nautilus-grid-view.c`
  - `on_model_changed()` binds `NautilusViewModel` to `GtkGridView`; GTK's
    list/grid machinery handles the visible reflow after model differences.

## Tensor Files Mapping

- Backing store:
  - `src/core/file_ops.rs` maps Trash to `$XDG_DATA_HOME/Trash/files` and
    `$XDG_DATA_HOME/Trash/info`.
  - `trash_path()` creates the XDG `.trashinfo` file with original `Path` and
    `DeletionDate`, then moves the item into `files/`.
  - `trashrc_path()`, `trash_status_empty()`, and
    `set_trash_status_empty()` maintain the Dolphin/KIO-style
    `$XDG_CONFIG_HOME/trashrc` `[Status] Empty=` state used for menu
    enablement.
  - `restore_trash_paths_with_policy()`, `permanently_delete_trash_paths()`,
    and `empty_trash()` are core file operations returning only summaries and
    affected directories.
- Model roles:
  - `src/core/entries.rs` decorates entries loaded from the Trash files
    directory with `trash_original_path` and `trash_deletion_time`.
  - `directory_entry_path()` maps watcher refreshes for `info/*.trashinfo`
    back to the matching item in `files/`, so metadata changes update the same
    model item.
  - `format_trash_original_location()` and `format_trash_deletion_time()`
    provide the display text used by the compact view and future details roles.
    `VisibleItemSnapshot` carries a role-derived detail label so Trash compact
    items expose both original location and deletion time without reading
    metadata from the renderer.
- Sorting and identity:
  - `src/core/model.rs` sorts Trash entries by deletion time role, then normal
    name order by default.
  - The model also exposes Dolphin-aligned Trash sort roles for original path
    and deletion time. Original path sorting uses the original parent
    directory, matching Dolphin's Trash `path` role rather than the local
    `$XDG_DATA_HOME/Trash/files` file name.
  - Trash full reloads reuse pane-local `ItemId` by trash file name instead of
    assuming the current sort order, matching Dolphin's role-based sorting
    where metadata changes can move an item without creating a new item.
  - Trash metadata refreshes keep the existing `ItemId`; if the deletion time
    changes the visible order, the model emits a reset rather than reporting
    changed roles at stale indexes.
- UI actions:
  - `src/main.rs` routes Delete in normal directories to move-to-trash.
  - Trash view context menus provide Restore, Delete Permanently, and Empty
    Trash actions.
  - Restore conflicts are reported as structured `TrashRestoreConflict`
    values. The pane-local conflict dialog lets the user skip or replace the
    occupied original path; replace reruns the same Trash restore operation
    with a replace policy, using a backup of the occupied target until the
    trash item has moved successfully.
  - Trash blank context menus use a Trash-specific Sort By submenu containing
    Name, Original Path, and Deletion Time, wired through pane-local
    `DirectoryModel` sort roles.
  - Completion refreshes the Trash directory and restored original directories
    through the lister path, keeping `PaneId + generation` routing.
- Places:
  - `src/core/places.rs` defines the Trash place that navigates to the Trash
    files directory.
  - `src/main.rs` owns the Trash empty/non-empty state, initializes it
    once, refreshes it after Trash-affecting operations, updates it from Trash
    pane lister events, and drains the core `TrashEmptinessMonitor` singleton
    watcher for external changes when no Trash pane is open. Places projection
    consumes that state and does not poll the filesystem.
  - The native Vulkan Places renderer displays the Trash state with the current
    shell marker style.
  - The Trash place context menu offers Open, Empty Trash, Copy Location, and
    Properties; Empty Trash uses the same app-owned state for enablement and
    runs through the focused pane's pane-local operation status.

## Remaining Gaps

- Tensor Files exposes Trash Original Path and Deletion Time through the pane-local
  Details view mode; compact items also show the same role-derived metadata.
- Tensor Files's local XDG Trash backend does not yet implement Dolphin/KIO's
  `trash:/` aggregation across storage devices or Solid removable-storage
  accessibility refresh for `.Trash-$uid` directories.
