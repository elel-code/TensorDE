# Startup

The startup sequence is a set of ordered gates:

1. Parse CLI arguments and environment overrides.
2. Resolve, parse, and validate the complete KDL configuration.
3. Initialize logging and diagnostics.
4. Create calloop, the Smithay display, and the Wayland listening socket.
5. Create Vulkan and reject devices without `VK_EXT_descriptor_heap`.
6. Bind the private IPC socket.
7. Construct Bevy ECS resources, schedules, and the initial scene.
8. Register the Wayland display/socket, XWayland, signals, configuration watchers, IPC, input, and
   backend event sources.
9. Publish the session environment and notify optional systemd integration after every required
   gate succeeds.
10. Enter the event loop.

Client commands use the same resolved systemd policy after startup. `ProcessLauncher` performs a
double-fork without a shell. In an active systemd scope it waits for the transient-unit job before
releasing the client, and it terminates the still-blocked child if `systemd "enabled"` cannot create
that scope. `auto` reports the scope failure and permits a direct child; `disabled` always uses the
direct path.

`--check` executes the initialization gates that exist, reports their resolved state, and exits. It
must never emit systemd readiness. Partial initialization must unwind owned sockets and handles.

Niri is the primary lifecycle reference, particularly its ordering of configuration, calloop,
Wayland display/socket, IPC, environment publication, watcher registration, and final run loop.

`tensor-session` is a Rust session launcher modeled on `niri-session`. It auto-detects the systemd
user manager, imports the login environment, starts `tensor.service` with `--wait`, triggers
`tensor-shutdown.target` during teardown, and clears session-only variables. Without a usable user
manager it executes `tensor-compositor --session` directly. No shell is used for this policy.
Display managers launch `tensor-session` through the Wayland-only desktop entry in
`contrib/wayland-sessions`; an X11 `xsessions` entry is intentionally forbidden.
