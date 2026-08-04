# Archive Reference

This document records the source references for Tensor Files's archive and Ark
integration. Dolphin is the behavioral reference for context actions and Ark
drag-and-drop interop; Tensor Files keeps the file-manager UI native and uses Ark as an
external archive tool when the user invokes archive actions.

## Dolphin Sources

- `references/fika/dolphin/src/dolphincontextmenu.cpp`
  - `addAdditionalActions()` delegates context-menu extensions to
    `KFileItemActions::addActionsTo(..., MenuActionSource::All, ...)`.
  - Local directory targets can add local-only actions such as Open Terminal
    before the service-menu actions.
- `references/fika/dolphin/src/dolphinviewcontainer.cpp`
  - Item activation asks `DolphinView::openItemAsFolderUrl()` before launching
    a file, so archives can be browsed as folders when the setting allows it.
  - Middle-click falls back to opening archives in a tab when there is no second
    or third associated application.
- `references/fika/dolphin/src/settings/dolphin_generalsettings.kcfg`
  - `BrowseThroughArchives` controls archive-as-folder browsing.
- `references/fika/dolphin/src/views/draganddrophelper.cpp`
  - Ark drag payloads carry a D-Bus service and object path.
  - Dolphin calls `org.kde.ark.DndExtract.extractSelectedFilesTo` with the drop
    destination when both Ark MIME payload fields are present.
- `references/fika/dolphin/src/views/draganddrophelper.h`
  - Ark DnD MIME names are
    `application/x-kde-ark-dndextract-service` and
    `application/x-kde-ark-dndextract-path`.

## Tensor Files Mapping

- `src/core/archive.rs`
  - Classifies common archive MIME types and extensions.
  - Parses and validates Ark DnD payloads and builds the structured D-Bus
    request for `extractSelectedFilesTo`.
- `src/core/launcher/ark.rs`
  - Builds Ark launch plans for the Dolphin Ark plugin actions: direct
    `tar.gz`/`zip` compression, `Compress to...`, `Extract here`, `Extract and
    trash archive`, and `Extract to...`.
  - Keeps Ark execution behind the same `DesktopLaunchPlan` and systemd-user
    launcher used by Open With and service-menu actions.
- `src/core/file_ops.rs`
  - Provides async/compio trash helpers so archive post-actions can move files
    to Trash through Tensor Files's io_uring-backed local file operation path.
- `src/main.rs`
  - Adds Tensor Files-owned context action IDs:
    `fika.builtin.ark.compress-tar-gz`, `fika.builtin.ark.compress-zip`,
    `fika.builtin.ark.compress`, `fika.builtin.ark.extract-here`,
    `fika.builtin.ark.extract-and-trash`, and `fika.builtin.ark.extract-to`.
  - Shows a root `Compress` submenu for local file/directory item selections,
    except a single archive, matching Ark's Dolphin plugin.
  - Shows a root `Extract` submenu for local archive selections, including
    multi-selection; extraction actions operate only on selected archive items.
  - Runs `Extract and trash archive` internally: Tensor Files waits for Ark with
    `async_process::Command`, then moves the original archive(s) to Trash with
    `file_ops::trash_paths_async`.
  - Does not expose Ark local actions for Trash or pure network URI targets.
  - De-duplicates by visible label and submenu when a discovered service menu
    already provides the same archive action.

## Remaining Work

- Add archive-as-folder browsing after the directory model can represent archive
  virtual folders cleanly.
- Surface completion/failure from launched Ark processes in the UI instead of
  only recording the start action.
- Revisit archive actions for remote locations after the network backend can
  provide a local mounted path or a backend-native archive operation.
