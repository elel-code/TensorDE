# Architecture

Tensor is a Rust Wayland compositor built around four ownership domains:

1. Smithay owns protocol objects, input/session state, the calloop event loop, DRM/KMS, GBM,
   connectors, CRTCs, page flips, and scanout policy.
2. Bevy ECS owns compositor intent: stable IDs, lifecycle, workspace membership, focus, and geometry.
3. Vulkanalia owns Vulkan GPU handles, queues, descriptor heaps, frame extraction, rendering, and
   synchronization. It returns dma-bufs and fences to Smithay instead of owning KMS state.
4. IPC and portal adapters translate external requests into validated ECS commands.

Smithay and Vulkan objects do not become ordinary ECS components. Thread-affine Smithay state stays
in the protocol owner or a Bevy `NonSend` resource. `RuntimeState` is the calloop data object and
serializes Smithay dispatch, popup/seat state, surface-to-view indexing, layout intent, and ECS
lifecycle changes. The renderer consumes a compact scene extracted from ECS once per frame rather
than issuing ECS queries in GPU submission loops.

Layout computation is split into immutable policy and workspace-local state. ECS provides stable
`ViewId` order, focus, min/max constraints, and optional fixed/proportional primary sizes. The
layout returns one snapshot containing full geometry, viewport intersections, content bounds, and
the resolved scrolling offset. Rendering, effects, damage, and hit testing consume the same
snapshot; they must not maintain competing geometry calculations. Switching layout families
clears stored viewport offsets but retains configured gaps and width policy.

ECS retains the last valid snapshot for each workspace and invalidates it on view, focus,
constraint, or policy changes. Runtime reflow is event-driven rather than tied to every surface
commit: initial XDG configuration, committed min/max changes, output topology, and explicit layout
commands recompute geometry. The resulting snapshot relocates Smithay elements without changing
their stacking order and supplies both the XDG suggested size and output-relative bounds.

Wayland and IPC boundaries address views by compositor-owned stable IDs, never Bevy `Entity`
values. The ECS owner maintains the ID-to-entity index, rejects duplicate IDs, and is solely
responsible for lifecycle, workspace membership, focus uniqueness, and geometry updates.

The renderer requires Vulkan 1.4 plus `VK_EXT_descriptor_heap`. Descriptor sets and descriptor
buffers are not alternative backends. A device that lacks the heap capability fails startup before
any long-lived renderer state is created.

Physical-device ranking lives in `render/device.rs`. The policy is configurable but the default
prefers a discrete GPU, then integrated/virtual hardware, with CPU devices last. Vulkanalia probing
in `render/vulkan.rs` creates a Vulkan 1.4 instance, verifies both the descriptor-heap extension and
feature bit, requires a graphics queue and a complete primary/render node pair reported through
`VK_EXT_physical_device_drm`, and requires external dma-buf memory, explicit DRM modifiers, foreign
queue-family ownership transfer, and bidirectional binary `SYNC_FD` semaphore support. The logical
device enables only the extensions for this native path and descriptor heap; there is no
descriptor-set or descriptor-buffer fallback.
Before ranking, each otherwise eligible device must also expose at least one explicit DRM modifier
that supports the real native image usage and exportable dma-buf memory. Import and export support
remain separate values. After Smithay opens the selected DRM pair, `render/format.rs` intersects
that Vulkan snapshot with each active output's primary-plane `FormatSet` and GBM capability. Only
fourcc, modifier, plane count, and capability flags cross this boundary; neither side receives the
other subsystem's handles.
The selected DRM identity is then passed to Smithay as the sole native device choice. An explicit
`render-device` filters Vulkan candidates by major/minor before ranking; Vulkan and the tty backend
never choose devices independently. Pure ranking remains testable without a GPU. The complete
buffer and synchronization contract is recorded in `docs/rendering.md`.

Session-manager selection uses one `SystemdMode` policy for startup and child supervision. `auto`
follows the detected user-manager environment, while `enabled` and `disabled` are explicit.
`ProcessLauncher` is the compositor-owned client boundary. It accepts an executable and argument
list, never a shell string, and uses a double-fork so the compositor does not retain client
children. When systemd is active it creates an `app-tensor-*.scope` through the D-Bus
`StartTransientUnit` API, holding both forked PIDs until the job is ready. A direct path remains
available when systemd integration is inactive; `enabled` mode fails closed if the scope cannot be
created.

XWayland is a rootless compatibility server for individual applications, never a compositor
backend. Tensor ships only a Wayland session entry and rejects an inherited X11-only session.

Modules use `foo.rs` plus `foo/*.rs`; `mod.rs` is prohibited. Shared dependency-light primitives
belong in `crates/tensor-util`, while protocol, renderer, and compositor-specific types stay in their
own crates/modules.

The protocol layer owns long-lived globals as a single `ProtocolGlobals` capability set. Alongside
compositor/subcompositor, xdg-shell, SHM, xdg-output, seat, data-device, and popup tracking, Tensor
advertises viewporter, fractional-scale, xdg-decoration, primary selection, relative pointer, and
pointer gestures. Preferred integer/fractional scale and transform follow the output selected by
the authoritative layout placement. Decoration policy currently requires client-side decoration;
server-side mode will only be exposed when decoration geometry and rendering share the scene
snapshot. Presentation-time, alpha-modifier, and background-effect are likewise not advertised
until the Vulkan frame path can honor their feedback and pixels.

A toplevel is assigned a stable `ViewId` at creation and removed idempotently from both Smithay's
`Space<Window>` and ECS when either the shell or surface destruction callback fires.
Tensor does not reimplement DRM/KMS, GBM, libinput, udev, or libseat. The tty backend owns session
activation, udev hotplug reconciliation, libinput seat assignment, DRM notifier tokens, GBM
lifetime, and per-output native-format validation. It opens the primary/render pair selected during
Vulkan probing and requires that pair to be available through the active libseat session. Future
surface creation, modesetting, page flips, and direct scanout remain in this Smithay backend;
Vulkanalia only produces renderable buffers and completion synchronization for it. Connector discovery uses
Smithay master’s companion `smithay-drm-extras::DrmScanner`, from the same upstream revision as
Smithay core. It preserves connector-to-CRTC mappings across startup, udev hotplug, delayed mode
discovery, DP-MST removal, and session resume.

The scanner is an adapter, not Tensor's output model. Every connector is copied into a complete
device-local snapshot, including connected connectors that do not yet have a mode or CRTC. One
backend-wide `OutputPolicy` consumes snapshots from every DRM device and produces an ordered
`OutputPlan`; only that plan drives Smithay `Output`, Wayland global, and `Space` lifecycles. Future
EDID profiles, enablement, failover, mirroring, and CRTC allocation belong in this policy boundary.
The adapter may use a custom `CrtcMapper` or drop down to `ConnectorScanner` without changing the
protocol or renderer boundaries. DRM handles do not enter ECS or the renderer.
