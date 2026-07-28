# Network Reference

This document records the source references for Fika's future network
filesystem model. Dolphin is the behavioral reference for remote URLs and KIO
integration. cosmic-files is the Rust/system-integration reference for
GIO/GVfs-backed network discovery, authentication, mounting, and scanning.

## Dolphin Sources

- `references/fika/dolphin/src/dolphinpart.cpp`
  - The Go menu adds `go_network_folders` with icon `folder-remote`, text
    `Network Folders`, and URL `remote:/`.
  - `openUrl()` asks KIO for `KIO::mostLocalUrl(url)` before updating the view,
    so protocols that can expose a local path use that bridge while still
    retaining the remote URL model.
  - Non-local URLs disable local-only tools such as Find and Open Terminal.
  - Item activation uses the item's `targetUrl()`, including `network:/` items
    that redirect to another URL.
- `references/fika/dolphin/src/dolphinnavigatorswidgetaction.cpp`
  - When the current scheme is `remote`, the navigator switches to editable
    text and shows a server URL placeholder such as `smb://[ip address]`.
  - The `Add Network Folder` affordance uses icon `folder-add`.
  - The button launches `org.kde.knetattach` through a
    `KIO::ApplicationLauncherJob` and is only shown on `remote:/` when the
    service exists.
- `references/fika/dolphin/src/kitemviews/kfileitemmodel.cpp`
  - Directory loading is delegated to `KCoreDirLister::openUrl(url)`.
  - `KCoreDirLister::redirection` is forwarded as `directoryRedirection`, which
    covers remote URL redirects such as `fish://localhost`.
  - Slow KIO slaves periodically dispatch pending inserted items before the
    final completed/canceled signal, avoiding a blank view during long remote
    scans.
- `references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`
  - Remote files get unknown size (`-1`) instead of recursive local size
    counting.
  - Directory content counts use `KIO::listDir(url, HideProgressInfo, flags)`
    when counting is needed, so remote counting remains an async KIO job.
- `references/fika/dolphin/src/views/dolphinview.cpp`
  - Empty placeholders are protocol-aware: `smb` roots show
    `No shared folders found`, while `network` shows
    `No relevant network resources found`.
  - The view listens to the model's directory redirection signal and reloads
    through the same model/view pipeline.
- `references/fika/dolphin/src/views/dolphinremoteencoding.cpp`
  - Remote charset actions are enabled for non-local filesystem-style KIO
    protocols (`KProtocolInfo::T_FILESYSTEM`).
  - Charset choices are stored in `kio_<scheme>rc` by host and Dolphin asks
    `org.kde.KIO.Scheduler` to reparse slave configuration before reload.
- `references/fika/dolphin/src/panels/terminal/terminalpanel.cpp`
  - The terminal panel first tries `KIO::mostLocalUrl()` for `:local`
    protocols.
  - If no local path exists, it calls the `org.kde.KIOFuse.VFS.mountUrl` D-Bus
    method and changes the terminal to the returned KIOFuse path.
  - When Dolphin later detects that the view is inside a KIOFuse mount, it asks
    `remoteUrl()` for the original remote URL to avoid browsing the FUSE mount
    path directly.
- `references/fika/dolphin/src/userfeedback/placesdatasource.cpp`
  - Dolphin detects network shares through Solid `NetworkShare` devices.
  - SSHFS, Samba/CIFS, and NFS are distinguished from ordinary local devices;
    kdeconnect SSHFS mounts are explicitly ignored in that telemetry path.

## Cosmic Files Sources

- `references/fika/cosmic-files/src/mounter/mod.rs`
  - Defines backend-neutral `MounterAuth`, `MounterItem`, `MounterMessage`, and
    the `Mounter` trait.
  - `MounterAuth` carries username, domain, password, remember, and anonymous
    state. Its `Debug` implementation hides passwords.
  - The trait exposes `items`, `mount`, `network_drive`, `network_scan`,
    `dir_info`, `unmount`, and `subscription`, so UI code does not own the
    network backend.
- `references/fika/cosmic-files/src/mounter/gvfs.rs`
  - Uses `gio::VolumeMonitor` to enumerate mounts and volumes, including URI,
    icon, local path when available, mount state, and remote flag.
  - `network_scan(uri, sizes)` resolves the URI, enumerates children with GIO,
    maps every child to `Location::Network(uri, display_name, local_path)`, and
    avoids expensive local metadata work when the filesystem is remote.
  - `mount_op()` converts GIO password prompts into `NetworkAuth` messages and
    waits for the UI to return credentials or cancellation.
  - `NetworkDrive` calls `gio::File::mount_enclosing_volume()`.
  - `NetworkScan` checks `find_enclosing_mount()`, mounts if needed, then scans.
  - The backend runs a GLib main loop on its own thread and emits changed events
    for mount/volume add, remove, and change signals.
  - Unmount ejects when possible, otherwise it unmounts the GIO mount.
- `references/fika/cosmic-files/src/tab.rs`
  - `FsKind::{Local, Remote, Gvfs}` classifies mounted filesystems using Linux
    mountinfo filesystem types.
  - Remote classes include SMB/CIFS, NFS, SSHFS, WebDAV, rclone, S3/GCS FUSE,
    Ceph/Gluster/Lustre, and GVfs (`fuse.gvfsd-fuse`).
  - `Location::Network(String, String, Option<PathBuf>)` is the UI-visible
    network location model.
  - `Location::scan()` dispatches network locations through
    `MOUNTERS.network_scan()`.
  - Remote/GVfs entries degrade expensive roles: MIME guessing, thumbnails, and
    directory child stats are avoided or simplified.
  - `network:///` is treated as the network root and exposes an Add Network
    Drive action.
- `references/fika/cosmic-files/src/app.rs`
  - Adds a Network root row to the sidebar when mounters are available.
  - Projects mounter items into either local `Location::Path` rows or remote
    `Location::Network` rows based on the mounter item path/remote state.
  - Routes `NetworkAuth`, network drive input, connection state, and
    `NetworkResult` through application messages and dialogs.
  - Persists network favorites as `Favorite::Network { uri, name, path }`.

## Fika Mapping

- `src/core/network.rs`
  - Owns remote URL parsing and normalization for supported schemes such as
    `remote`, `network`, `smb`, `sftp`, `fish`, `ftp`, `ftps`, `nfs`, `dav`,
    and `davs`.
  - Canonicalizes Dolphin `remote:/` and cosmic-files `network:///` to Fika's
    `network:///` root model.
  - Defines backend-neutral `NetworkLocation { uri, display_name, local_path,
    scheme, icon_name }` snapshots. `local_path` remains optional so GVfs,
    KIOFuse, or another backend can attach a mounted path later without
    changing the UI-facing model.
  - Provides `NetworkAuth` with a redacted `Debug` implementation so passwords
    cannot leak into logs or test failures.
  - Keeps authentication state structured and never logs passwords.
  - Classifies remote/GVfs filesystem types from mountinfo/backend metadata
    through `classify_network_filesystem()` and `filesystem_type_is_remote()` so
    expensive MIME, thumbnail, directory-size, watcher, and recursive metadata
    work can be disabled or throttled on remote locations.
  - Future scan APIs must expose async, cancellable results routed by
    `PaneId + generation`.
  - Future asynchronous work and D-Bus calls must use the existing
    Tokio/shared bus infrastructure. Do not introduce `async-io` into the main
    project for this layer.
- `DirectoryLister`
  - Local mounted paths may reuse the local listing path, but the request still
    carries remote/GVfs classification so role updates and watcher behavior can
    be degraded.
  - Pure URI locations should be listed through a network backend and converted
    into the same core model deltas used by local directories.
  - Slow remote scans should deliver incremental batches instead of clearing the
    pane to an empty view while waiting for completion.
- Places sidebar
  - Adds a Network root equivalent to Dolphin `remote:/` and cosmic-files
    `network:///`. It is represented by the canonical `network:///` pseudo path,
    uses `folder-remote` icon candidates, is not persisted to
    Fika's Places file, and is not treated as a local mounted directory until a
    network backend is attached.
  - The Network root context menu exposes Dolphin's `Add Network Folder`
    affordance, but implements it inside Fika: the action opens Fika's own
    location input prefilled for a network URI, validates the URI through
    `src/core/network.rs`, saves it as a Network bookmark, and navigates to it
    through the same pane/generation loading path. Fika does not depend on
    `org.kde.knetattach` for this flow.
  - Show saved network bookmarks and discovered mounted network locations as
    locations, not as duplicated text-only hints.
  - Network places keep icon names/handles from the backend when available.
- File operations
  - Operations on network locations should reuse core file-operation result
    routing once the location has a local mounted path.
  - Pure URI operations need a backend boundary similar to Dolphin/KIO rather
    than ad-hoc UI commands.
  - Until that backend exists, remote context menus hide local write actions
    such as Create, Paste, Cut, Rename, and Trash instead of exposing commands
    that fail later.
- Terminal/open-here behavior
  - Local-only actions must be disabled or bridged through a local path resolver
    (`mostLocalUrl`/KIOFuse-style behavior) before execution.

## Remaining Work

- Decide the backend boundary: GVfs/GIO, KIOFuse, or a small abstraction that
  can use either when present.
- Add a fuller network bookmark editor after the network backend boundary is
  settled.
- Add authentication, cancellation, and structured error reporting.
- Integrate network scans with `DirectoryLister` without pane flicker.
- Add remote/GVfs metadata degradation for MIME, thumbnail, size, and watcher
  work.
- Add file-operation and DnD semantics for remote locations.
