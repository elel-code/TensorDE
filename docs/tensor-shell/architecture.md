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

Shell configuration I/O and typed KDL parsing run on a dedicated Compio
io_uring worker. The worker uses a bounded document size, fingerprints before
reading, compares bytes before parsing, and publishes only a fully validated
single-slot revisioned snapshot. Runtime parse/read failures retain the last
valid configuration and are log-deduplicated. The Wayland owner applies a new
revision by domain: layer state is recommitted in place, retained panel scenes
and interaction state are rebuilt, media OSD policy and launcher argv are
replaced directly, and only the Overview adapter is restarted when its
Tensorland IPC socket changes. Other D-Bus services and Vulkan device state
remain live.

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
  do not claim keyboard focus. Launcher is not a `ShellComponent`: the panel
  submits the configured standalone `tensor-launcher` argv through Tensorland's
  bounded activation-aware `Spawn` command, so Shell never owns a duplicate
  launcher layer surface.
- a compositor-closed surface is removed from the live map and recreated when
  the model still requires it.
- a Shell layout reload changes double-buffered layer state on the existing
  surface identity. Presentation pauses for that surface until the matching
  compositor configure supplies its new extent; unrelated surfaces remain
  live.

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
the real acquire, command, submit, and present lifecycle but are not all
completed popup scenes. Panel surfaces retain a responsive logical widget scene
for launcher, workspaces, active window, media, system status, clock,
notifications, and control center. Workspaces opens an Overview scene whose
bounded Compio `GetOverview` snapshot is lowered into workspace/window cards;
the same retained geometry drives pointer hit testing and
`ActivateView`/`SetWorkspace` commands. The panel and Overview scenes are
rebuilt only on configure or snapshot changes, lowered to physical rectangles
on extent or interaction changes, and recorded inside the existing
dynamic-rendering pass. Text, icons, remaining live services, and richer popup
scenes remain future scene inputs rather than placeholder values synthesized in
the renderer.

Notification center and popup scenes consume the same locked
`NotificationStore` revision. A revision change rebuilds only the affected
bounded card scene; pointer or keyboard dismissal closes the value and emits
the standard `NotificationClosed` signal, while an action queues
`ActionInvoked` on the Compio D-Bus service thread. The center keeps a bounded
focus ring over each card's action and dismiss controls, with Tab and arrow
navigation, Enter activation, and Delete/Backspace dismissal. Popups remain
pointer-only and never claim keyboard focus. The rendering path never holds
the store lock and does not perform D-Bus work in the frame submission path.

The control-center scene uses the same retained boundary. It combines typed
NetworkManager, UPower and MPRIS snapshots, notification-store DND state, and
revisioned network and session action states into stable hit geometry and draw
values. Lock and Suspend are bounded commands to the existing Compio
system-bus worker, which calls logind without blocking Wayland dispatch or
frame submission. The Network card toggles the Wi-Fi radio only when overall
networking and wireless hardware are available, remains open after activation,
and rejects another activation while its bounded action is pending. Previous,
PlayPause, and Next likewise remain in the open Control Center and are enabled
strictly from the active player's advertised MPRIS capabilities. DND is updated
in the notification store and immediately rebuilds the scene. Closing
Overview, Notification Center, or Control Center repaints configured panels in
the same event cycle so retained active-state feedback cannot outlive modal
ownership.

Overview uses the same retained geometry for both drawing and input. Clicking a
view activates it and closes the modal; dragging a view beyond an 8-logical-pixel
threshold onto another workspace card queues `MoveViewToWorkspace` while keeping
Overview open, and each sufficiently large card has a bounded close control that
queues `CloseView`. A structured stale-view rejection only refreshes the snapshot;
it does not tear down the healthy Tensorland connection. The bounded command queue
remains off the Wayland and Vulkan paths.

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
after connection are `failed`.

The Network card consumes a separate typed NetworkManager details snapshot. Its
single-owner Compio system-bus worker installs one namespace match before the
initial root/device/AP reads, pipelines each cold `GetAll` batch, atomically
validates overall state, connectivity, primary-connection kind, Wi-Fi devices,
raw SSIDs, active AP, signal and security flags, and wakes Wayland only when
the retained revision changes. Invalidated root, wireless topology, device
topology, and scan-completion properties are refreshed as one complete
snapshot; AP strength and active-AP changes update the retained copy
atomically. If NetworkManager is absent, the worker retains only its exact
owner-change subscription and resumes when the service appears, without
polling. Wi-Fi writes use the standard variant-valued
`org.freedesktop.DBus.Properties.Set` call through a single-command bounded
queue. Pending, success and failure remain explicit render states, and input or
Vulkan paths never own or await the D-Bus connection. Credential workflows,
saved connections, VPN state, and connection activation remain later network
detail work until a complete secret-handling path exists.

The media applet and Control Center share one session-bus MPRIS service. Its
single-owner Compio worker installs name-owner and player-property matches
plus the MPRIS `Seeked` match before bounded initial discovery, keeps the active
player stable until a playing player takes precedence, and publishes complete
snapshots only after typed metadata, position, duration, and capability
validation. There is no media polling loop: property, seek, and owner signals
update the retained revision and wake the Wayland dispatch directly. The OSD
may extrapolate a visible Playing position locally between those signals, but
never writes that estimate back to D-Bus. Ready
snapshots are shared by `Arc`, so the Shell's 20 ms reconciliation turn does
not deep-copy player strings. Playing lowers to active panel emphasis; Previous,
PlayPause, and Next reserve a single-command bounded queue and execute against
the currently retained player. Pending, succeeded, and failed actions are
retained independently from playback state; capability or player changes clear
stale action feedback. The same worker owns the versioned
`org.tensor.Shell1`/`org.tensor.Shell1.Media` control object, so
`tensor-msg shell media` and Tensorland KDL media-key bindings reuse the
retained player and queue instead of opening a competing MPRIS policy path. The
Tensorland input turn only performs a bounded enqueue; its dedicated Compio
worker retains the Shell D-Bus connection. Neither renderer nor input path
owns a D-Bus connection or awaits a player.
BlueZ and PipeWire/WirePlumber remain to be connected with the same lifecycle.

Control Center uses the same retained action geometry for pointer and keyboard
input. Its focus ring includes only currently executable controls, so pending,
unavailable, and failed media/network states cannot be activated by stale
keyboard focus. Tab and arrow keys cycle through the bounded controls, Enter or
Space activates the focused action, and Escape releases modal ownership.

The playback OSD consumes that same cached snapshot and command queue rather
than opening another bus connection. The first complete player snapshot after
Shell startup establishes a baseline without raising a popup; later title,
artist, album, or playback-status changes raise the OSD on every configured
output. Capability-only changes refresh visible button state without extending
the deadline, and empty titles suppress the popup. The default three-second
deadline is monotonic and configurable: pointer hover pauses expiration, while
leaving restarts the full timeout. Previous, PlayPause, and Next are sent only
when the retained player advertises the capability, then dismiss the popup;
background activation dismisses it without sending a command. Valid position
and duration values lower to a bounded retained progress track; seek and
position changes refresh a visible OSD without reopening it or extending its
deadline, while local Playing extrapolation only updates that track and
remains clamped to the advertised duration.

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
