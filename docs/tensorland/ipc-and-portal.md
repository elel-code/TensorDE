# IPC and Portal Gates

The compositor control protocol uses a Unix socket, a little-endian `u32` frame length, and a JSON
envelope. Every request carries a protocol version and request ID. Frames are capped at 1 MiB, the
socket is mode `0600`, bind never removes an existing path, and drop removes only the inode owned by
that server instance.

The server runs on a dedicated Compio runtime. The listener submits `accept`; each accepted client
submits reads and `write_all`, and only completed operations produce value messages for the
compositor. This is an io_uring-first completion path, not a calloop/readiness registry. Client
count, frame size, and the request bridge are fixed-capacity. Each connection awaits a one-shot
compositor reply without blocking the runtime thread, so a slow IPC peer cannot stall Wayland
dispatch or grow compositor memory without limit. Runtime failure and graceful-shutdown completion
use a reserved control slot that cannot be consumed by request load.
`tensorctl` exposes `ping`, `get-state`, `get-outputs`, `get-workspaces`, `get-overview`,
`get-config-status`, `reload-config`, layout/workspace/output controls, `minimize-focused`,
`restore-minimized`, stable-view activation/movement/close, `spawn`, and `quit` using the same codec.

`get-state` returns a value-only snapshot: active layout kind, view count on the active regular
workspace, mapped output count, focused view ID, regular/hidden workspace counts, and minimized-view
family count. Attached dialogs move and restore with their root and do not inflate that count.
`get-workspaces` includes hidden/overview/minimize-target metadata while normal workspace
indices and `ext-workspace-v1` remain limited to the regular pool.

`tensorctl minimize-focused` moves the focused view family to the configured minimize target.
`tensorctl restore-minimized <view-id>` restores it and follows its recorded regular workspace;
`--stay` restores without switching away from the current workspace. These operations reuse the
retained protocol window and renderer resource identity rather than asking the client to remap.
Restore reports `unknown_view` for a stale stable ID and `not_minimized` only for a live view that
has no retained minimize origin.

Version 6 `get-overview` returns a deterministic back-to-front plan for every regular workspace and
each hidden workspace whose KDL policy permits overview display. The response names the primary
work area, every workspace-card rectangle, and each view's current-or-last-valid source rectangle,
transformed destination, and clip. Each view also names its root family, placement kind, focus state,
stacking order, and stable `ext-foreign-toplevel-list-v1` identifier used to join title/app-id
metadata without duplicating an unbounded client string in IPC. The response is capped at 2,048 view
records across the topology and sets `truncated` when it returns only the stable prefix. If no output
work area exists yet, inventory and source rectangles remain available while plan area/card/view
destinations are `null`. Tensor Shell consumes these values for presentation; Tensorland's hit
tester consumes the same internal plan front-to-back.

Overview interaction returns those stable IDs through `activate-view <view-id>` and
`move-view-to-workspace <view-id> <regular-index> [--follow]`, and `close-view <view-id>`. Tensor
Shell never receives a Bevy entity, Wayland object, or Vulkan handle. Activating an attached dialog
switches to its root family's regular workspace but focuses the requested dialog. Activating a
minimized member restores the complete retained family to its recorded origin before applying that
same focus rule. Moving any member moves the root and every attachment; `--follow` then selects the
destination and requested member. Close deliberately targets the requested member, sends
`xdg_toplevel.close` or the X11 close request, and waits for normal client-owned teardown instead of
removing ECS state eagerly. Unknown IDs, unmapped views, non-minimize hidden workspaces, invalid
regular indices, active popup grabs, and lifecycle failures remain distinct structured errors. A
move without `--follow` transfers or clears seat focus before the family becomes invisible, so
keyboard focus never remains on a hidden surface.

`get-outputs` returns the current output topology as sorted value-only records (name, logical
geometry, fractional scale, mode size/refresh, and whether the head hosts the default workspace
viewport). Clients never receive Wayland output resources.

`spawn` accepts a direct argv array (one program and optional arguments). The compositor queues the
command on the asynchronous launch worker and returns `accepted` as soon as the request is enqueued;
process creation and optional systemd scope setup complete off the compositor thread. Each launch mints
an external `xdg-activation` token and exports it as `XDG_ACTIVATION_TOKEN` (and
`DESKTOP_STARTUP_ID` for legacy clients) on the child environment. Empty argv is rejected with
`invalid_argument`. A saturated launch queue returns `queue_full`. Tensorland never invokes a shell for
this path.

Connection handling verifies peer credentials before dispatch. Completed reads cross the bounded
Tensorland worker bridge; requests are validated and dispatched on the compositor thread, and replies
cross a one-shot value channel back to the Compio connection task. `quit` stops the compositor only
after the accepted response write completes. A saturated request bridge returns `queue_full` and
keeps the connection usable; a stopped bridge flushes `service_unavailable` before closing it.
External clients never receive Wayland resources, Vulkan handles, or mutable ECS access.

Configuration control uses the same request/reply transport. `reload-config` non-blockingly queues
the configured path on the one-entry cold worker and returns `accepted`; saturation and stopped
workers are structured errors. `get-config-status` returns the active transaction generation and
the last source-free bounded failure metadata. Filesystem changes reach that same worker through the
configuration watcher, and completed candidates commit on the compositor thread before the turn's
IPC requests are answered.

The current version-6 transport is still request/reply only. The next protocol slice is an explicit,
versioned subscription/event extension rather than unsolicited frames to existing clients. A failed
reload event is bounded to diagnostic category, path, error code, line, column, short summary, and a
config-validation command; the full KDL source remains in Tensorland's retained diagnostic and logs.
`tensor-shell` consumes this future event and owns visual/accessibility notification.

XDP means xdg-desktop-portal in this repository. The future portal implementation is an optional
D-Bus/PipeWire adapter for controlled capture and sharing. It follows the same command gate as IPC
and cannot bypass renderer extraction policy.

systemd is separately optional. Readiness is an output of the completed startup state, not a
prerequisite or an owner of the compositor lifecycle.
