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
`tensorctl` exposes `ping`, `get-state`, `get-outputs`, `get-workspaces`, `get-config-status`,
`reload-config`, layout/workspace/output controls, `minimize-focused`, `restore-minimized`, `spawn`,
and `quit` using the same codec.

`get-state` returns a value-only snapshot: active layout kind, view count on the active regular
workspace, mapped output count, focused view ID, regular/hidden workspace counts, and minimized-view
family count. Attached dialogs move and restore with their root and do not inflate that count.
`get-workspaces` includes hidden/overview/minimize-target metadata while normal workspace
indices and `ext-workspace-v1` remain limited to the regular pool.

`tensorctl minimize-focused` moves the focused view family to the configured minimize target.
`tensorctl restore-minimized <view-id>` restores it and follows its recorded regular workspace;
`--stay` restores without switching away from the current workspace. These operations reuse the
retained protocol window and renderer resource identity rather than asking the client to remap.

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
after the accepted response write completes. External clients never receive Wayland resources,
Vulkan handles, or mutable ECS access.

Configuration control uses the same request/reply transport. `reload-config` non-blockingly queues
the configured path on the one-entry cold worker and returns `accepted`; saturation and stopped
workers are structured errors. `get-config-status` returns the active transaction generation and
the last source-free bounded failure metadata. Filesystem changes reach that same worker through the
configuration watcher, and completed candidates commit on the compositor thread before the turn's
IPC requests are answered.

The current version-1 transport is still request/reply only. The next protocol slice is an explicit,
versioned subscription/event extension rather than unsolicited frames to existing clients. A failed
reload event is bounded to diagnostic category, path, error code, line, column, short summary, and a
config-validation command; the full KDL source remains in Tensorland's retained diagnostic and logs.
`tensor-shell` consumes this future event and owns visual/accessibility notification.

XDP means xdg-desktop-portal in this repository. The future portal implementation is an optional
D-Bus/PipeWire adapter for controlled capture and sharing. It follows the same command gate as IPC
and cannot bypass renderer extraction policy.

systemd is separately optional. Readiness is an output of the completed startup state, not a
prerequisite or an owner of the compositor lifecycle.
