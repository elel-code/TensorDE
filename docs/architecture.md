# Architecture

Tensor is a Rust Wayland compositor built around five ownership domains:

1. **`tensor-event` / `tensor-runtime`** own compositor event semantics (phase order, coalescing,
   fixed-capacity queues) and Compio-backed worker bridges. See `docs/event-layer.md`.
2. **Host contracts** (`tensor-host` / `tensor-drm` / `tensor-present`) own display/input/present
   *values* without Smithay. **Smithay** remains a transitional adapter for protocol objects and,
   today, input/session/DRM bindings plus the calloop wait layer. Exit plan: `docs/smithay-exit.md`.
3. **Bevy ECS** owns compositor intent: stable IDs, lifecycle, workspace membership, focus, geometry.
4. **Vulkanalia** owns GPU handles, descriptor heaps, frame extraction, rendering, and
   synchronization. It returns dma-bufs and fences for present instead of owning KMS state.
5. **IPC / portal** adapters translate external requests into validated commands / event IDs.

## Async execution and the event layer

The long-term I/O runtime is **Compio (completion model) + io_uring driver**
(`tensor-runtime`); the semantic loop is always `tensor-event::EventQueue`. Turn contracts live
in `tensor_runtime::{run_turn, EventfdWake, CompletionDriver::IoUring}`. Compio is **submit →
complete**, not a readiness poll loop. On Linux the product driver is **io_uring**; Compio's
`polling` feature is only an automatic host fallback when io_uring cannot be created — not the
architecture we design for. calloop still owns some Smithay fds as a **readiness** loop during
migration (direct dependency, not via Smithay reexports). Tensor-owned local I/O (logging, IPC,
udev hotplug waits, libinput waits, blocked process signals, and worker notifications) uses Compio
completions and posts value-only messages across bounded bridges. Workers never own
Smithay objects or DRM/KMS descriptors. Present and Vulkan record stay on the compositor thread
for latency predictability. Input samples go through `tensor-input` at the bus edge.

Vulkan output submissions export a binary sync-file for KMS. Tensor duplicates only that sync-file
and submits a one-shot io_uring fence wait on a Compio service; the worker never receives a Vulkan
object, dma-buf, DRM device, or KMS state. Fence signal produces a value-only GPU timeline event and
replaces the former timer-driven timeline query loop.

Diagnostics are the first such service. Tracing formats a record on its caller, caps it at 8 KiB,
then performs a non-blocking enqueue into an 8,192-record fan-in queue. One Compio drain thread
owns either the selected `TENSOR_LOG_FILE` or `stderr` (and therefore the systemd journal in a
service). Writes complete through Compio (io_uring driver on capable Linux; host fallback only if
io_uring setup fails). Queue saturation is intentionally lossy, but the drain emits a later
dropped-record notice; this is preferable to allowing logging to delay input, page-flip, or frame
submission. The worker has no Smithay, Vulkan, DRM/KMS, or ECS ownership.

Application launch is the second. `ProcessLauncher` still owns double-fork and optional
transient-systemd scope setup, but those waits never run inside a calloop callback. The
compositor submits value-only `LaunchRequest` messages to a dedicated launch worker; the worker
returns value-only `LaunchOutcome` results through a bounded Tensor bridge. A submitted Compio
eventfd read must complete before the transitional calloop notification drains those outcomes.
Queue saturation rejects new submissions or drops late completion logs rather than blocking the
compositor thread. The worker owns neither Smithay objects nor DRM/KMS descriptors.

IPC also carries runtime **workspace** and **output layout** commands
(`set-workspace`, `set-output-position` / `enabled` / `scale`) that mutate value-only
policy and replan through the tty backend without a second configuration language. IPC accept,
read, and response writes are submitted Compio operations. Decoded requests and critical runtime
state cross separate bounded bridges; only the compositor thread touches policy state.

Tensor intentionally has no general-purpose network control plane: its compositor control
protocol is local Unix IPC.

Lua is intentionally not a compositor-core dependency and is not a second
configuration language. The TOML/`serde` boundary remains authoritative for
startup, device, layout, and session policy; live control stays on versioned
IPC. If scripting is added later,
it must be an optional capability boundary: scripts may receive value-only
snapshots and issue versioned IPC or policy commands, but may not hold Smithay
resources, Vulkan handles, Bevy entities, or portal ownership. Execution
budgets, transactional reload, an explicit API version, and a process or
sandbox boundary are prerequisites so the core event loop and readiness gates
remain deterministic when scripting is disabled.

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

Scene extraction is a separate once-per-frame boundary. Nodes are stored in stable `ViewId` order
for linear snapshot comparison and carry an independent stacking-order index for drawing. Effect
styles resolve conservative visual bounds (including shadows and clipped output edges); damage
merges adjacent regions, caps pathological fragmentation, and expands regions that feed a
backdrop-blur dependency. Vulkan descriptor allocation consumes this compact scene data after ECS
queries finish. `render/frame.rs` now owns per-output scene history, damage, descriptor-heap range
allocation, three native output slots, and timeline retirement, and is connected to `RuntimeState`
output lifecycle. The Vulkan executor binds resource and sampler heap ranges, samples imported
client images through a push-index dynamic-rendering pipeline, releases foreign ownership, exports
a Tensor-owned dma-buf description plus binary `SYNC_FD`, and hands both to the tty adapter for the
current Smithay atomic KMS path. Renderer production code does not depend on Smithay types. The
current slice is intentionally limited to one-plane RGB; implicit dma-buf synchronization,
multi-plane formats, and damage-driven partial rendering remain later gates. Explicit clients use Smithay's
`wp_linux_drm_syncobj_v1` owner: DRM timeline points stay in the protocol layer, while only exported
sync-file fds and stable `SurfaceId` values reach Vulkanalia.

Compositor-owned appearance crosses the configuration boundary as a small value-only `SceneAppearance`
object. ECS extraction resolves an active view's configured `FocusRingStyle` into a `FocusOutline`;
the frame planner maps its inner and outer edges independently onto the physical output grid, then
the descriptor-free focus-ring pipeline cuts the rounded inner rectangle out of the rounded outer
rectangle in one SDF draw. The inner radius is the view corner radius; the outer radius adds the
ring width. It is clipped at the output edge and the frame plan emits it before that view's client
tree, including popups; later scene nodes cover earlier nodes, and the software cursor remains last.
This follows Niri's front-to-back element contract, Hyprland's active-window border semantics, and
Nourish's single focused-surface ownership without copying their renderer ownership models. It is not
a descriptor-set fallback: sampled client images continue to use `VK_EXT_descriptor_heap` exclusively.

Output scale is a shared value primitive, represented exactly in the `N/120` units of
`wp_fractional_scale_v1`. DRM mode dimensions and Vulkan native targets remain physical pixels;
Smithay output geometry, ECS layout, XDG configure sizes, hit testing, and relative-pointer
locations remain logical coordinates. Frame extraction maps shared logical rectangle edges to the
same rounded physical edge, while damage and scissors round outward and clip to the physical mode.
This keeps adjacent tiles gapless without allowing partial-edge damage to leave stale pixels.

The protocol owner traverses each mapped toplevel, subsurface tree, and popup tree in Smithay draw
order and copies only stable identities, buffer metadata, placement, and layer policy into the flat
ECS content table. Synchronized subsurface callbacks are deferred until their non-synchronized
ancestor applies the complete Smithay transaction; explicit acquire/release points follow the same
gate. Popup content remains owned by its toplevel but is clipped by the output rather than the
layout tile, and its old/new bounds participate in scene damage.

Wayland and IPC boundaries address views by compositor-owned stable IDs, never Bevy `Entity`
values. The ECS owner maintains the ID-to-entity index, rejects duplicate IDs, and is solely
responsible for lifecycle, workspace membership, focus uniqueness, and geometry updates.

The renderer requires Vulkan 1.4 plus `VK_EXT_descriptor_heap`. Descriptor sets and descriptor
buffers are not alternative backends. A device that lacks usable resource and embedded-sampler heap
limits fails startup before any long-lived renderer state is created.

Physical-device ranking lives in `render/device.rs`. The policy is configurable but the default
prefers a discrete GPU, then integrated/virtual hardware, with CPU devices last. Vulkanalia probing
in `render/vulkan.rs` creates a Vulkan 1.4 instance, verifies both the descriptor-heap extension and
feature bit, and requires a graphics queue, timeline semaphores, usable descriptor-heap limits, a
complete primary/render node pair reported through `VK_EXT_physical_device_drm`, external dma-buf
memory, explicit DRM modifiers, foreign
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
backend. Tensor ships only a Wayland session entry and rejects an inherited X11-only session. Niri's
current integration obtains its clean scaling boundary by letting `xwayland-satellite` expose X11
windows as ordinary Wayland surfaces. Tensor's direct Smithay XWayland/XWM path follows the same
architectural invariant: on XWayland readiness, Tensor starts an XWM and uses the
`xwayland-shell` association to place each normal mapped X11 window into the ordinary Smithay
`Window`, ECS view, surface tree, scene, and logical-to-physical output path. X11 configure
requests are hints; layout remains authoritative and sends its existing logical rectangle back to
XWayland. Tensor never derives a second X11 coordinate space from an output scale.

Pointer hit-testing stays on the associated Wayland surface tree, including ordinary popup input
regions. Keyboard activation deliberately targets `X11Surface` for an X11 root window so Smithay
performs the ICCCM input-focus / `WM_TAKE_FOCUS` handshake; it targets the native root surface for
Wayland clients. Either route updates the same ECS focus, stacking, and layout reflow state, and a
destroyed root clears keyboard focus before its surface association disappears.

Override-redirect X11 windows use popup/parent semantics rather than becoming independent tiled
views. Tensor admits one only after its map state and `xwayland-shell` association are both known
and its `WM_TRANSIENT_FOR` chain resolves to a managed X11 root. The popup's configured X11
location becomes an offset relative to that root's configured rectangle; it is mapped in Smithay's
input/output space and flattened into the root view's scene content. Its stacking, explicit-sync
ownership, frame callbacks, and presentation feedback therefore remain attached to the root view.
An unowned popup is rejected instead of creating a fallback global X11 placement path.

Normal X11 `WM_TRANSIENT_FOR` dialogs keep independent ECS view IDs, surface trees, input targets,
and renderer synchronization, but use the same parent contract. They are excluded from primary
tiling and derive a constrained, centered logical rectangle from their immediate managed owner.
Client size requests update that preferred size; X11 position requests never become layout
authority. Nested dialog ownership is explicit, owner teardown removes dependent views first, and
an unresolved transient remains outside the scene until an owner is managed rather than falling
back to a global X11 position.

Modules use `foo.rs` plus `foo/*.rs`; `mod.rs` is prohibited. Shared dependency-light primitives
belong in `crates/tensor-util`, while protocol, renderer, and compositor-specific types stay in their
own crates/modules.

The protocol layer owns long-lived globals as a single `ProtocolGlobals` capability set. Alongside
compositor/subcompositor, xdg-shell, SHM, xdg-output, seat, data-device, and popup tracking, Tensor
advertises viewporter, fractional-scale, xdg-decoration, primary selection, relative pointer,
pointer gestures, pointer-constraints, presentation-time v2, cursor-shape, xdg-activation,
idle-notify, idle-inhibit, wlr-layer-shell, single-pixel-buffer, keyboard-shortcuts-inhibit,
tablet-v2, text-input-v3, input-method-v2, virtual-keyboard-v1, ext-session-lock-v1,
wp-security-context-v1, ext-foreign-toplevel-list-v1, ext-image-capture-source-v1,
ext-image-copy-capture-v1 (idle SHM silhouette + optional SHM client blit),
wlr-output-management (enable/position/scale via OutputRule; mode switch deferred),
xdg-foreign, xdg-system-bell, pointer-warp,
content-type, alpha-modifier, ext-background-effect (blur), xdg-toplevel-icon, xdg-toplevel-tag, fifo,
commit-timing, xwayland-keyboard-grab, ext-data-control (preferred) plus wlr-data-control
(compat), virtual-pointer, gamma-control, and linux-dmabuf when the selected Vulkan device exposes
a non-empty validated client-import format list. Protocol work follows wayland-protocols /
Smithay-style **tiers** (core → stable → staging/`ext` → unstable → community → proprietary);
higher tiers win for the same capability, and `zwlr_*` is community-only when no standard path
exists (see `docs/protocol-surface.md`). Prefer Smithay Dispatch2 modules; Tensor-local ports stay
value-only at the ECS/event boundary. Layer surfaces map through Smithay's per-output `LayerMap`. On commit, Tensor arranges exclusive
zones, sends pending configures, and merges mapped layer content into the output frame as value-only
scene nodes (outside the ECS view graph), including committed `ext-background-effect` blur flags.
Workspace layout uses the non-exclusive zone so panels
reserve space. Pointer hit-testing prefers Overlay/Top layers, then windows, then Bottom/Background.
Keyboard focus prefers exclusive Overlay/Top, then the last on-demand layer (click or new map), then
exclusive Bottom/Background only when the workspace has no views. xdg popups unconstrain against the
output (or non-exclusive zone for Background/Bottom parents); layer-shell roots may grab, while
window popup grabs are dismissed when an interactive Overlay/Top layer holds focus. Layer content
trees include their xdg popup children in the value-only scene merge. The linux-drm-syncobj global is
added only after Smithay opens the
Vulkan-selected primary device and verifies syncobj eventfd support. Compositor-owned launches
(IPC `spawn` and `spawn_at_startup`) mint external `xdg-activation` tokens and export them as
`XDG_ACTIVATION_TOKEN` / `DESKTOP_STARTUP_ID` on the child environment. Acquire points become temporary
binary Vulkan semaphore payloads; release points receive the latest GPU-read completion fence only
when their surface attachment retires. Preferred integer/fractional scale and transform follow the output
selected by the authoritative layout placement. Decoration policy currently requires client-side
decoration; server-side mode will only be exposed when decoration geometry and rendering share the
scene snapshot. Presentation feedback is captured from the output-intersecting surface tree in the
submitted scene and completed only by its output/timeline page flip. Full opaque-region occlusion
tracking will be added with render-element state; the protocol layer does not claim it before that
state exists. Alpha-modifier surface values remain pending a dedicated Vulkan sample path;
`ext-background-effect` blur is advertised and drives scene damage via `BackdropBlur` until the GPU
blur pass samples the backdrop.

A toplevel is assigned a stable `ViewId` at creation and removed idempotently from both Smithay's
`Space<Window>` and ECS when either the shell or surface destruction callback fires.
Tensor does not reimplement DRM/KMS, GBM, libinput, or libseat. Its small udev adapter owns only
enumeration, the monitor fd, and value-only hotplug events; Compio completes each submitted monitor
wait and the compositor thread performs reconciliation. The tty backend owns session activation,
libinput seat assignment, DRM notifier tokens, GBM lifetime, and per-output native-format
validation. It opens the primary/render pair selected during
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
The plan also carries the selected progressive DRM mode, native fourcc, explicit modifier, plane
count, and resolved output scale. Explicit TOML connector rules win; otherwise the policy preserves
the connector's native/preferred resolution and picks its highest available refresh, avoiding stale
60 Hz EDID `PREFERRED` flags on high-refresh panels. A format, mode, or scale change is therefore an
output change rather than hidden backend state. The adapter may use a custom `CrtcMapper` or drop
down to `ConnectorScanner` without changing the protocol or renderer boundaries. DRM handles do not
enter ECS or the renderer.

Focused state is one contract across ECS, protocol, and rendering: a `Focused` ECS component
extracts a value-only focus outline into the scene, the selected Smithay window receives
`xdg_toplevel::State::Activated` (or the corresponding XWayland activation), and the seat owns the
keyboard focus. A true active-view transition raises the same attachment family in ECS and Smithay,
updates activation, then delivers seat focus; a later seat-focus repair does not alter stacking or
emit a duplicate scene transition. Smithay suppresses equal keyboard targets, so a repair does not
emit redundant `wl_keyboard.enter`/`leave` events. The Vulkan path preserves each view's order as
ring, client tree, then later stacked nodes, with the cursor last; this is compositor-owned geometry,
not a descriptor-set fallback for client or output resources.

For each planned output, the renderer owns a bounded three-slot set of Vulkan images and exports
Tensor-owned `ExportedDmabuf` descriptions. The tty adapter converts those descriptions to Smithay
dma-bufs immediately before GBM import, creates framebuffers, and submits atomic/page-flip state
with the renderer's `IN_FENCE_FD`; vblank advances the scanout state. Initial output resource
construction is a startup gate: failure aborts backend preparation before readiness.
Hotplug resource failures are isolated to the affected output and do not invalidate already-live
outputs. Session resume re-reads every live Smithay DRM surface, quarantines slots that may still be
scanned out, drains already-ready DRM events, and then schedules a repaint. Exhausting all three
slots escalates to a device state reset rather than reusing an uncertain buffer.
