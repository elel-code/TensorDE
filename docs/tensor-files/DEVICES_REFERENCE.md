# Devices Reference

This document records the source references for Tensor Files's device discovery,
mount-state model, and Places sidebar integration.

## Dolphin Sources

- `references/fika/dolphin/src/dolphinplacesmodelsingleton.cpp`
  - Dolphin's Places model subclasses `KFilePlacesModel`.
  - Device entries, groups, icons, mount state, and hidden rows are provided by
    KDE Frameworks and Solid rather than by Dolphin's view code.
  - `deviceForIndex()` is used to find a Solid device for context menu and
    teardown actions.
  - `StorageAccess::teardownRequested` is forwarded to Dolphin's higher-level
    storage teardown flow so user-visible unmount/eject operations remain
    asynchronous.
- `references/fika/dolphin/src/panels/places/placespanel.cpp`
  - `PlacesPanel` is a `KFilePlacesView` bound to the singleton
    `DolphinPlacesModel`.
  - Drag move rejects non-writable place URLs for external drags while still
    allowing internal place reordering.

## Cosmic Files Sources

- `references/fika/cosmic-files/src/mounter/mod.rs`
  - Defines a backend-neutral `Mounter` trait that exposes items, mount,
    unmount, network scan, and a subscription for mounter events.
  - UI code consumes mounter items instead of owning the discovery backend.
- `references/fika/cosmic-files/src/mounter/gvfs.rs`
  - Uses `gio::VolumeMonitor` to enumerate mounts and volumes.
  - Hides shadowed mounts, maps mounted roots to local paths when possible, and
    exposes mount names, icons, URI, remote flag, mounted state, and path.
  - Connects `mount_added`, `mount_removed`, `mount_changed`,
    `volume_added`, `volume_removed`, and `volume_changed` to a single changed
    event so the UI can rescan the model.
  - Mount/unmount/eject operations are delegated to GIO `Volume`/`Mount`
    methods instead of parsing backend-specific block-device objects in the
    application layer.

## Tensor Files Mapping

- `src/core/devices.rs`
  - Uses `gio::VolumeMonitor` as the primary and only device backend.
  - Emits `DeviceInfo` snapshots with a stable opaque GIO device id, optional
    local mount point, URI, label, filesystem type, optional capacity, mounted
    state, and eject capability.
  - Builds mounted rows from non-shadowed `gio::Mount` objects and unmounted
    rows from `gio::Volume` objects that do not already have a mount.
  - Skips remote mounts that have no local path because the current Tensor Files pane
    model is still path-based. Remote/network browsing remains a separate
    backend.
  - Subscribes to GIO mount and volume change signals in `watch_devices()` and
    publishes fresh `DeviceMonitorMessage::Snapshot` values through the core
    channel boundary.
  - Resolves mount/unmount/eject operations by the opaque GIO device id at
    execution time. UI code does not pass `/dev/*` paths or backend object
    paths.
  - `mount_device()` calls `Volume::mount()` and returns the local mount point.
    `unmount_device()` calls `Mount::unmount_with_operation()` when available
    and falls back to `Mount::eject_with_operation()` for eject-only mounts.
    `eject_device()` calls the matching GIO eject method on `Mount` or `Volume`.
- `src/core/places.rs` and `src/main.rs`
  - Places has static built-ins, persisted user bookmarks, a dynamic
    "Removable Devices" section for removable `DeviceInfo` rows, and a static
    Root entry under Devices.
  - Device rows carry `device_id` and `device_mounted` separately from their
    display/navigation path. Unmounted rows use the opaque id only as a
    non-navigable placeholder path.
  - `replace_removable_device_places()` replaces only the dynamic removable
    device section, keeps user bookmarks before grouped sections, skips paths
    already covered by built-ins/user bookmarks, and gives device rows a drive
    icon instead of bookmark/folder styling.
  - Click on a mounted device opens its local mount point. Click on an unmounted
    device calls the GIO mount operation and navigates to the returned mount
    point.
- `src/main.rs`
  - Startup reads `read_gio_devices()` and starts the live `watch_devices()`
    monitor.
  - Successful device operations force a fresh device snapshot refresh in
    addition to relying on GIO monitor signals.
  - Device context menu actions pass the GIO `device_id` and user-visible label
    into `perform_device_place_operation()`.

## Remaining Work

- Verify mount/unmount/eject against real removable devices, including Polkit
  prompts, user cancellation, and failure paths.
- Add a path-independent network/GVfs browsing model before exposing remote
  mounts without local paths in Places.
  - Revisit "Safely Remove" once a backend-neutral drive-level teardown model is
  introduced; the current GIO path exposes eject but not a separate power-off
  capability.
