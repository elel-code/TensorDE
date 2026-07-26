# Startup

The startup sequence is a set of ordered gates:

1. Parse CLI arguments and environment overrides.
2. Resolve, parse, and validate the complete TOML configuration.
3. Initialize logging and diagnostics.
4. Create calloop, the Smithay display, and the Wayland listening socket.
5. Create Vulkan, validate the client-image SPIR-V and descriptor-heap pipeline, and reject
   devices without `VK_EXT_descriptor_heap` or a usable exportable native format/modifier.
6. Bind the private IPC socket.
7. Construct Bevy ECS resources, schedules, and the initial scene.
8. Register the Wayland display/socket, XWayland, signals, configuration watchers, and DRM notifier
   sources; submit the Compio IPC, udev, libinput, and libseat session completion services.
   Intersect every active output's KMS/GBM formats with the Vulkan capability snapshot before this
   gate completes.
9. Publish the session environment to the compositor-owned process launcher.
10. When systemd integration is active, synchronously publish the same values to the user manager.
11. Publish readiness after every required gate succeeds.
12. Authorize session autostart with a one-shot permit, then enter the event loop.

`StartupGates` makes the last four transitions explicit. A session autostart permit is unavailable
until the runtime is prepared, the child-process environment is installed, an active systemd user
manager has accepted that environment, and readiness has been published. The environment snapshot
contains the allocated XWayland `DISPLAY` when XWayland is enabled and explicitly clears inherited
session values first. `Compositor` requires the permit to launch configured commands, so moving a
function call cannot accidentally bypass the ordering. Check mode and non-session mode never issue
a permit.

Logging starts after configuration validation and before any calloop, renderer, or protocol state.
Every tracing destination uses the same bounded asynchronous path: callers format at most an 8 KiB
record and try to enqueue it, while a dedicated Compio drain thread owns the selected file or
`stderr`. `TENSOR_LOG_FILE` selects a compositor log file; without it, the drain writes to
`stderr`, which systemd captures as journal output. The queue is deliberately lossy under a burst
and records a later drop notice rather than ever blocking frame, input, or protocol dispatch.
Termination signals are blocked early and consumed by a submitted Compio read on Linux `signalfd`.
The completion produces a bounded value event; the compositor writes a best-effort line to stderr,
logs through tracing, and stops the transitional host loop. After the loop returns, Tensor
announces stop (including optional systemd `STOPPING=1`), records that the loop stopped, and joins
the log drain so queued shutdown lines are flushed before exit.

Client commands use the same resolved systemd policy after startup. `ProcessLauncher` performs a
double-fork without a shell. In an active systemd scope it waits for the transient-unit job before
releasing the client, and it terminates the still-blocked child if `systemd "enabled"` cannot create
that scope. `auto` reports the scope failure and permits a direct child; `disabled` always uses the
direct path. Those waits run on a dedicated launch worker thread: the compositor only enqueues a
value-only request and later observes a value-only outcome through a bounded Tensor bridge after a
submitted Compio eventfd read completes. Fork setup and systemd job completion therefore never
stall Wayland, input, or presentation dispatch.

`--check` executes the Vulkan device gate as well as the other initialization gates, reports the
selected physical device, and exits. It must never emit systemd readiness. Partial initialization
unwinds the logical device, instance, loader, sockets, and other owned handles in dependency order.

Niri is the primary lifecycle reference, particularly its ordering of configuration, calloop,
Wayland display/socket, IPC, environment publication, watcher registration, and final run loop.
Niri waits for both `systemctl --user import-environment` and
`dbus-update-activation-environment` before notifying readiness and spawning configured commands.
Tensor preserves that ordering without invoking a shell: it writes explicit values through
`systemctl --user set-environment`, waits for completion, updates D-Bus activation when available,
then notifies readiness. A failed user-manager update or readiness notification aborts startup
before any `spawn-at-startup` command runs.

`tensor-session` is a Rust session launcher modeled on `niri-session`. It auto-detects the systemd
user manager, imports the login environment, starts `tensor.service` with `--wait`, triggers
`tensor-shutdown.target` during teardown, and clears session-only variables. Without a usable user
manager it executes `tensor-compositor --session` directly. No shell is used for this policy.
Display managers launch `tensor-session` through the Wayland-only desktop entry in
`contrib/wayland-sessions`; an X11 `xsessions` entry is intentionally forbidden.

After readiness, session startup consumes its permit and queues every validated
`spawn-at-startup` argv entry on the asynchronous launch worker. Queue acceptance is logged
immediately; process creation and optional systemd scope setup complete off the compositor thread,
and success or failure is reported when the outcome channel is drained. A failed entry is logged
without tearing down an otherwise ready session; `systemd "enabled"` still prevents that
individual client from running outside a scope. Direct sessions use the same process environment
gate but do not require a systemd manager. Without the `systemd` Cargo feature, `auto` remains a
direct session even if a manager-shaped environment was inherited; explicitly requesting
`systemd "enabled"` fails closed instead of launching clients with stale manager state.
The imported systemd and D-Bus activation snapshot is owned for the compositor lifetime and is
cleared automatically on normal exit or any later startup failure.
