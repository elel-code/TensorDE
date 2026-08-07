# Tensor Shell Configuration

Tensor Shell owns a separate typed KDL configuration from Tensorland. The
default path is `$XDG_CONFIG_HOME/tensor/shell.kdl`, falling back to
`$HOME/.config/tensor/shell.kdl` and then `/etc/tensor/shell.kdl`.
`TENSOR_SHELL_CONFIG` selects an explicit path. A missing file uses defaults;
an unreadable or invalid file is a startup error. The document is limited to
256 KiB.

`ShellRuntime::connect` starts a dedicated Compio/io_uring configuration
worker. It checks the file identity, size, mtime, and ctime once per second and
reads bytes only after that fingerprint changes. A new typed configuration is
published through one retained revisioned snapshot; unchanged bytes are not
parsed again. Invalid or unreadable runtime updates are reported once and keep
the last valid snapshot. Deleting the file restores defaults. The Wayland and
Vulkan owner thread consumes only validated values and never performs KDL I/O
or parsing. `connect_with_config` deliberately remains deterministic and does
not start the worker.

Runtime application follows ownership boundaries. Layout changes update and
commit existing layer-surface state, then wait for the compositor's configure
before presenting the resized surface. Panel ordering rebuilds retained panel
scenes and clears stale input state. Media policy updates the visible playback
OSD deadline or hides it immediately, while enabling it does not synthesize an
OSD from the current baseline player. Launcher argv is replaced in place. An
IPC socket change restarts only the Overview adapter; a Tensorland config-path
change only replaces the retained endpoint. Notification, media, network,
power, and session-lock services are not restarted.

The complete example is [../../apps/tensor-shell/examples/config.kdl](../../apps/tensor-shell/examples/config.kdl).

The `layout` node controls shell-owned layer-surface dimensions. The `panel`
node contains ordered `left`, `center`, and `right` widget arguments. Omitting
a section keeps its default; declaring an empty section hides it. Known widget
names are `launcher`, `workspaces`, `active-window`, `media`, `system-status`,
`clock`, `notifications`, and `control-center`. A widget may occur only once.

The optional `launcher` node replaces the default `tensor-launcher` argv:

```kdl
launcher {
    command "tensor-launcher" "--surface"
}
```

The command must contain a non-empty program, at most 64 arguments and at most
16 KiB of argument text. It is never parsed as a shell command. The panel sends
the argv through the bounded Compio Tensorland client; Tensorland generates the
Wayland activation token and starts the standalone Launcher product through its
process worker. Tensor Shell does not create an internal launcher surface.

The optional `tensorland` node identifies the compositor integration endpoint:

```kdl
tensorland {
    config-path "/etc/tensor/config.kdl"
    ipc-socket "/run/user/1000/tensor.sock"
}
```

Without overrides, the config path follows Tensorland's `TENSOR_CONFIG` and
XDG resolution, and the socket follows `TENSOR_IPC_SOCKET` then
`$XDG_RUNTIME_DIR/tensor.sock`. Tensor Shell settings remain in `shell.kdl`;
compositor output, layout, appearance, and effect settings remain in
Tensorland's `config.kdl`; idle deadlines remain in Tensor Idle's `idle.kdl`.
The standalone Tensor Settings application will edit these product-owned files
and use each product's versioned reload/status transaction rather than creating
a second configuration dialect inside Shell.

There is deliberately no Tensor settings daemon. Panel applets are built into
Tensor Shell and receive bounded snapshots from the system services that own
the underlying state. Tensor Settings only edits product-owned typed KDL and
requests the owning product's reload; it does not stay resident to keep panel
state alive.

When `system-status` is present in the panel order, Tensor Shell starts its
signal-driven UPower adapter on the system bus. There is no polling interval or
backend selector to configure. A machine without UPower remains a supported
configuration: the retained applet becomes `unavailable` while Shell startup
and the rest of the panel continue normally. Battery percentage is bounded to
0–100; charging, low, critical/action and no-battery states are lowered before
the renderer sees them.

Tensor Shell starts one signal-driven session-bus MPRIS adapter for the shared
Media applet and Control Center controls. It has no polling interval or backend
selector. Omitting `media` from the panel only removes that panel entry; the
Control Center can still expose Previous, PlayPause, and Next when a player
advertises the corresponding capabilities. No-player and unavailable-bus
states remain distinct retained values. The session service also owns the
versioned `org.tensor.Shell1.Media` control interface used by
`tensor-msg shell media previous`, `play-pause`, and `next`. Those commands use
the same active player, capability validation, retained action feedback, and
single-command queue as Shell UI; there is no separate media-command backend or
Shell KDL action key. Tensorland's separate `config.kdl` owns the physical
`media-keys` mapping and forwards matched actions to this interface without
discovering players itself.

The optional Shell-owned `media` node controls playback OSD policy:

```kdl
media {
    playback-osd #true
    playback-osd-timeout-ms 3000
}
```

`playback-osd` defaults to `#true`. The timeout defaults to 3000 milliseconds
and must be in the inclusive range 250–60000. These keys affect only Tensor
Shell presentation; player discovery and Control Center transport controls
remain active when the OSD is disabled. The first player snapshot establishes
a baseline without opening an OSD. Later metadata or playback changes open it
on every configured output, and pointer hover pauses expiration until the
pointer leaves.

Tensor Shell also starts one signal-driven NetworkManager adapter on the
system bus for the Control Center Network card. There is no polling interval,
command backend, or duplicate network policy in `shell.kdl`: NetworkManager
owns connection state and radio policy. Missing NetworkManager, a disabled
overall network stack, hardware rfkill, an in-flight write, and a typed ABI
failure remain distinct retained states. The current card controls only the
standard `WirelessEnabled` property; access-point discovery, saved connections,
credentials and VPN configuration will belong to the network detail workflow,
not to new Shell KDL keys.
