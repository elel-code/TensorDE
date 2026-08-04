# Tensor Shell Architecture

Tensor Shell owns desktop-level panel, launcher, notification/OSD, control
center, overview, and lock surfaces. Application chrome remains in the owning
application.

The complete feature target and current implementation status are tracked in
[Functional Alignment](alignment.md). This architecture record defines the
boundaries used to reach that target; it is not itself a claim of feature
completion.

## Surface semantics

The shell keeps a value-only `ShellModel`. A surface is identified by the
`(OutputId, ShellComponent)` pair, and `ShellRuntime` reconciles live Wayland
objects against the model after each dispatched event batch:

- `Panel` is created once for each newly advertised output and reserves its
  panel height.
- output metadata updates preserve explicit visibility choices;
  disconnects destroy every surface on that output.
- launcher, notification-center, control-center, overview, and lock surfaces
  are interactive and mutually exclusive per output. Notification popups and
  OSD do not claim keyboard focus.
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

The current command path clears each configured surface to stable component
chrome, so it proves the real acquire, command, submit, and present lifecycle
but is not a completed panel or popup scene. `ShellRenderScene` contains one
surface's product semantics: ordinary direct draws, bounded scene-color
dependencies, and global history/consumer/compute/terminal facts.
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
