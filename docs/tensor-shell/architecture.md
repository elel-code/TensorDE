# Tensor Shell Architecture

Tensor Shell owns desktop-level panel, notification/OSD, control center,
overview, and lock surfaces. Tensor Launcher owns the standalone launcher
surface and retained search model; Tensor Shell owns its panel entry and
visibility coordination. Tensor Settings owns the standalone settings surface,
schema-backed editors, and validation UI; Shell owns only its entry point.
Tensor Idle owns idle/power policy and drives the lock request boundary rather
than living in the Shell process. Application chrome remains in the owning
application.

Tensor Greeter is also a separate product. It runs as an ordinary greetd
Wayland client before a Tensorland session exists. Tensor Shell's lock workflow
instead binds Tensorland's `ext-session-lock-v1`; login and unlock must never be
represented as the same privileged process or protocol state.

The complete feature target and current implementation status are tracked in
[Functional Alignment](alignment.md). This architecture record defines the
boundaries used to reach that target; it is not itself a claim of feature
completion.

## Configuration boundaries

Tensor Shell loads its own typed KDL for shell-owned layout, widget ordering,
theme, motion, and service policy. Tensor Settings may edit that file but does
not own or reinterpret Shell policy. Tensorland retains authority over output,
workspace/layout, compositor appearance, and render-effect policy in its own
`config.kdl`. Shell configuration resolves the Tensorland config path and IPC
socket so settings surfaces can read compositor policy, persist requested
changes, request the existing transactional reload, and confirm its generation
or bounded diagnostic. The two products do not duplicate configuration keys or
IPC wire definitions. Idle deadlines and power-source policy live only in
Tensor Idle's `idle.kdl`.

## Surface semantics

The shell keeps a value-only `ShellModel`. A surface is identified by the
`(OutputId, ShellComponent)` pair, and `ShellRuntime` reconciles live Wayland
objects against the model after each dispatched event batch:

- `Panel` is created once for each newly advertised output and reserves its
  panel height.
- output metadata updates preserve explicit visibility choices;
  disconnects destroy every surface on that output.
- notification-center, control-center, overview, and lock surfaces are
  interactive and mutually exclusive per output. Notification popups and OSD
  do not claim keyboard focus. The current internal launcher surface is a
  migration placeholder and will be removed when the standalone launcher owns
  the visible surface end to end.
- a compositor-closed surface is removed from the live map and recreated when
  the model still requires it.

## Rendering boundary

`ShellPresenter` owns one Roadmap 2026 instance, adapter, device, and graphics
queue for the process. Each configured layer surface owns only its Vulkan
surface, swapchain, three retained acquire frame slots, per-image present
semaphores, and initialization state. Initial and retained image-layout graphs
are compiled once when the shared device root is created. Configure and scale
events select an exact physical buffer extent; close and replacement are
infrequent queue-idle lifecycle boundaries. Presentation requires FIFO
latest-ready and premultiplied alpha rather than silently selecting weaker
surface modes.

Non-panel surfaces currently clear to stable component chrome, so they prove
the real acquire, command, submit, and present lifecycle but are not completed
popup scenes. Panel surfaces additionally retain a responsive logical widget
scene for launcher, workspaces, active window, media, system status, clock,
notifications, and control center. The scene is rebuilt only on configure,
lowered to physical rectangles on extent or interaction changes, and recorded
inside the existing dynamic-rendering pass. Pointer and touch hit testing use
the same logical geometry; backed entry widgets toggle the model-owned launcher,
notification-center, and control-center surfaces. Text, icons, live service
snapshots, and configurable ordering remain future scene inputs rather than
placeholder values synthesized in the renderer.

Panel service completions first lower into a fixed-size `PanelAppletStore`.
Duplicate snapshots do not advance its revision, badge and meter values are
bounded, and one dirty batch triggers one repaint of each configured panel.
The renderer consumes only the retained revision and validated render-facing
state; D-Bus arrival order never directly records GPU commands. Notification
count, critical attention, and do-not-disturb state already use this path.
The system-status applet now consumes the shared `tensor-dbus` typed UPower
display-device monitor through the same boundary. Its Compio io_uring worker
retains one complete snapshot, subscribes before the initial property read,
rebuilds after owner changes or property invalidation, and has an explicit
stop path for every connection, initialization and receive wait. Missing
UPower or a missing system bus is `unavailable`; transport or typed ABI faults
after connection are `failed`. NetworkManager, BlueZ, PipeWire/WirePlumber and
MPRIS adapters remain to be connected with the same lifecycle.

`ShellRenderScene` contains one surface's broader product semantics: ordinary
direct draws, bounded scene-color dependencies, and global
history/consumer/compute/terminal facts.
`compile_frame_plan` follows the same boundary as Tensor Files and Tensorland:
Tensor Shell owns semantic lowering, effect order, and region-local pass
planning, while the shared presentation planner automatically selects
direct-surface or offscreen output.
An offscreen decision retains the same product-local effect list; it changes
the generic target topology, not the meaning or ordering of shell effects.
Synchronization2 states, descriptor heaps, retained generic intermediates,
command recording primitives, and presentation remain owned by
`vulkan-renderer`. Tensor Shell must not add a second backend.

Frame planning is a strict cold-path boundary. Empty surface extents and empty,
negative-origin, overflowing, or out-of-surface scene-color regions are rejected
before command recording. Errors retain the source node index so invalid shell
effect geometry never becomes a late Vulkan validation failure.

## Reference implementations

The local ignored checkouts used for behavior comparison are pinned at:

- DankMaterialShell `6de5593216548551db507cecde581558475972a6`
- StatIndet/quickshell `c94c62ad7131dbd2bd162c9c9adef6076c6c6e47`

Their per-screen surface repeaters, output reconnect recovery, popout/modal
ownership, and explicit layer-shell focus policies informed the value-model
contracts above. The checkouts are reference material only and are never
included in TensorDE builds or commits.
