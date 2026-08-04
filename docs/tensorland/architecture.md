# Architecture

Tensorland is a Rust Wayland compositor built around five ownership domains:

1. **`tensor-event` / `tensor-runtime`** own compositor event semantics (phase order, coalescing,
   fixed-capacity queues) and Compio-backed worker bridges. See `docs/tensorland/event-layer.md`.
2. **Native host and protocol owners** (`tensor-host` / `tensor-drm` / `tensor-present` /
   `tensor-protocol` plus `src/protocol` and `src/backend`) own values, wire state, input/session,
   XWayland, and DRM/KMS directly. Smithay and calloop are absent from the dependency graph.
3. **Bevy ECS** owns compositor intent: stable IDs, lifecycle, workspace membership, focus, geometry.
4. **`vulkan-renderer`** owns the Vulkanalia loader/device boundary, GPU handles, shared command
   pool/timeline, native dma-buf resources, and synchronization. Tensorland owns frame extraction and
   product-specific recording, then receives dma-bufs and fences for present without giving the
   renderer KMS state.
5. **IPC / portal** adapters translate external requests into validated commands / event IDs.

## Async execution and the event layer

The I/O runtime is **Compio (completion model) + io_uring driver**
(`tensor-runtime`); the semantic loop is always `tensor-event::EventQueue`. Turn contracts live
in `tensor_runtime::{run_turn, EventfdWake, CompletionDriver::IoUring}`. Compio is **submit →
complete**, not a readiness poll loop. On Linux the product driver is **io_uring**; Compio's
`polling` feature is disabled, so inability to create io_uring is a runtime initialization error.
Logging, IPC, Wayland listener/display operations, X11 property reads, udev hotplug waits, libinput
waits, libseat session waits, blocked process signals, and worker notifications use Compio
completions and bounded bridges. Workers never own Wayland objects or DRM/KMS descriptors. Present
and Vulkan record stay on the compositor thread
for latency predictability. Native and virtual input sources publish Tensorland-owned values directly
into `tensor-event` at the bus edge.

Every Compio service constructs its ring through `tensor_runtime::io_uring_runtime` with the
service's fixed maximum number of submitted operations. Capacities are rounded to a power of two
once at startup; Tensorland neither allocates Compio's 1024-entry default ring per service nor grows a
ring on a latency-sensitive path.

`wayland-backend` exposes one opaque aggregate fd whose internal client membership changes after
an accepted connection is inserted. Tensorland dispatches any already-buffered registry burst at that
accept completion, then publishes one atomically coalesced refresh command. The existing
`IORING_OP_POLL_ADD` is explicitly cancelled, its cancellation CQE is consumed, and only then is a
replacement operation submitted. This is a submit → complete operation lifecycle; it is not an
epoll/readiness loop, and the fixed command bridge reserves capacity for both one completion
disposition and one concurrent membership refresh.

Vulkan output submissions export a binary sync-file for KMS. Tensorland duplicates only that sync-file
and submits a one-shot io_uring fence wait on a Compio service; the worker never receives a Vulkan
object, dma-buf, DRM device, or KMS state. Fence signal produces a value-only GPU timeline event and
replaces the former timer-driven timeline query loop.

Diagnostics are the first such service. Tracing formats a record on its caller, caps it at 8 KiB,
then performs a non-blocking enqueue into an 8,192-record fan-in queue. One Compio drain thread
owns either the selected `TENSOR_LOG_FILE` or `stderr` (and therefore the systemd journal in a
service). Writes complete through Compio's io_uring driver; failure to initialize that driver
fails startup. Queue saturation is intentionally lossy, but the drain emits a later
dropped-record notice; this is preferable to allowing logging to delay input, page-flip, or frame
submission. The worker has no Wayland, Vulkan, DRM/KMS, or ECS ownership.

Application launch is the second. `ProcessLauncher` still owns double-fork and optional
transient-systemd scope setup, but those waits never run on the compositor thread. The
compositor submits value-only `LaunchRequest` messages to a dedicated launch worker; the worker
returns value-only `LaunchOutcome` results through a bounded Tensorland bridge. A submitted Compio
eventfd read must complete before the compositor drains those outcomes.
Queue saturation rejects new submissions or drops late completion logs rather than blocking the
compositor thread. The worker owns neither Wayland objects nor DRM/KMS descriptors.

IPC also carries runtime **workspace** and **output layout** commands
(`set-workspace`, `set-output-position` / `enabled` / `scale`) that mutate value-only
policy and replan through the tty backend without a second configuration language. IPC accept,
read, and response writes are submitted Compio operations. Decoded requests and critical runtime
state cross separate bounded bridges; only the compositor thread touches policy state. Request
bridge saturation returns a request-ID-correlated `queue_full` response without discarding the
connection. A stopped compositor bridge flushes `service_unavailable` before the Compio task closes
that connection. If the bridge accepted a request but the compositor later drops its one-shot
response, the connection likewise flushes one `service_unavailable` response for every request in
the decoded batch before EOF. Every complete frame decoded from one bounded read batch is submitted
before the connection awaits the batch's one-shot completions; replies are still written in input
order, so submission gains concurrency without making policy order completion-dependent.

Tensorland intentionally has no general-purpose network control plane: its compositor control
protocol is local Unix IPC.

Workspace identity is divided into a configurable ordered regular pool and named hidden
workspaces. Only the regular pool is activatable and advertised through `ext-workspace-v1`;
hidden workspaces remain compositor-owned storage that overview may expose selectively. One hidden
workspace is the configured minimize target. Minimize moves the existing ECS view family there and
retains its regular origin as value-only state; restore reverses that move rather than recreating a
Wayland surface, imported image, descriptor, or scene identity. This makes minimized windows a
workspace policy instead of a parallel lifecycle state.

The overview boundary consumes the same regular/hidden metadata and retained workspace snapshots.
Tensorland owns overview geometry and hit testing; Tensor Shell owns visual chrome and accessible
controls. The compositor compiles the primary output's exclusive-zone-adjusted work area into a
bounded integer plan: workspace cards preserve that area's aspect ratio, source window rectangles
become destination rectangles plus clips, and the plan retains the existing back-to-front order.
The hit tester walks those exact clips front-to-back, so rendering and input cannot drift. An
attached dialog is hit by its own stable view ID while retaining its root family ID. Inventory
records retain last valid geometry across workspace moves and join window metadata through the
stable `ext-foreign-toplevel-list-v1` identifier. Snapshot extraction builds that bounded identity
join once per request, so retained-window lookup stays linear instead of rescanning WindowSpace for
every overview entry.

The plan contains no client pixels and creates no descriptor identity. Overview entry, exit, and
whole-workspace rearrangement therefore reuse imported images and descriptor-heap bindings; a later
scene integration only changes per-view transform/clip values. Hidden workspaces marked
`show-in-overview=#false` are filtered before planning and hit testing but remain available to
explicit IPC actions. Versioned IPC accepts only stable view IDs for overview selection and
cross-workspace movement. Tensorland resolves an attached ID to its root for family movement while
retaining the exact requested member for focus; selecting a minimized member restores its retained
origin first, whereas an unrelated hidden-workspace member is rejected rather than implicitly
exposing hidden policy. Moving a focused family without following it reconciles the source
workspace and seat focus before the old surface becomes invisible. Closing targets the exact stable
view rather than its attachment root: Wayland clients receive `xdg_toplevel.close`, while XWayland
uses `WM_DELETE_WINDOW` when supported and otherwise terminates the owning X11 client. Client-owned
protocol teardown remains asynchronous. Tensor Shell therefore needs no ECS, Wayland,
workspace-family, or renderer ownership. Pointer-to-command wiring, animation state, and Tensor
Shell's rendered overview surface remain later integration slices; the geometry, hit-test,
stable-view command, and IPC representation kernels are complete.

Configuration diagnostics follow the product boundary rather than becoming compositor UI.
`tensor-kdl` produces parser/typed-decode context and an optional named-source miette report;
Tensorland retains that structured value, renders it for `--validate-config` and logs, and lowers
reload failures into bounded, source-free diagnostic metadata. `ConfigTransaction` commits only a
fully valid candidate and rejects missing reload files without replacing the active policy.
The filesystem watcher and one-entry request queue feed a dedicated cold worker; only its bounded,
value-only outcome crosses back to the compositor turn. Layout, appearance, cursor, and debug
policy replace live state atomically. Startup-owned IPC, GPU/DRM, output, systemd, XWayland,
environment, and autostart changes report `reload_requires_restart` until those resources have
atomic swaps.
The future IPC event carries location and summary metadata, not the complete source file;
`tensor-shell` owns the short visual and accessible notification.

Lua is intentionally not a compositor-core dependency and is not a second
configuration language. The typed `tensor-kdl` boundary remains authoritative for
startup, device, layout, and session policy; live control stays on versioned
IPC. If scripting is added later,
it must be an optional capability boundary: scripts may receive value-only
snapshots and issue versioned IPC or policy commands, but may not hold Wayland
resources, Vulkan handles, Bevy entities, or portal ownership. Execution
budgets, transactional reload, an explicit API version, and a process or
sandbox boundary are prerequisites so the core event loop and completion gates
remain deterministic when scripting is disabled.

Wayland and Vulkan objects do not become ordinary ECS components. Thread-affine protocol state
stays in the protocol owner or a Bevy `NonSend` resource. `RuntimeState` is the compositor owner and
serializes direct dispatch, popup/seat state, surface-to-view indexing, layout intent, and ECS
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
commands recompute geometry. The resulting snapshot relocates Tensorland `WindowSpace` entries without
changing their stacking order and supplies both the XDG suggested size and output-relative bounds.
`ProtocolWindow` owns stable window identity and cached surface-tree bounds on the compositor
thread. Its `Rc` clones share state without copying geometry. Tensorland owns the underlying stable XDG,
core-surface, seat/input, and XWayland/XWM objects directly.

Scene extraction is a separate once-per-frame boundary. Nodes are stored in stable `ViewId` order
for linear snapshot comparison and carry an independent stacking-order index for drawing. Effect
styles resolve conservative visual bounds (including shadows and clipped output edges); damage
merges adjacent regions, caps pathological fragmentation, and expands regions that feed a
backdrop-blur dependency. Vulkan descriptor allocation consumes this compact scene data after ECS
queries finish. `render/frame.rs` owns per-output scene history, damage, descriptor-range frame
policy, and three native output-slot policy, and is connected to `RuntimeState` output lifecycle.
The shared renderer owns the command pool, timeline, and device-local descriptor heaps; Tensorland's
frame scheduler holds the resource heap's allocator handle and therefore allocates the same ranges
that the shared descriptor encoder/uploads use. Tensorland reserves a shared timeline value before
preparing a frame. Its renderer path records only shared typed encoders, descriptor sources,
uploads, and semantic barrier batches; it has no product-owned command buffer, raw synchronization,
or SHM allocation lifecycle. The Vulkan executor binds resource and sampler heap ranges, samples imported
client images through a push-index dynamic-rendering pipeline, releases foreign ownership, exports
a Tensorland-owned dma-buf description plus binary `SYNC_FD`, and hands both to the tty adapter for the
Tensorland-owned atomic KMS path. Renderer production code depends only on Tensorland value contracts. The
current slice is intentionally limited to one-plane RGB; implicit dma-buf synchronization,
multi-plane formats, and damage-driven partial rendering remain later gates. Explicit clients use
Tensorland's `wp_linux_drm_syncobj_v1` owner: DRM timeline points stay in the protocol layer, while only exported
sync-file fds and stable `SurfaceId` values reach `vulkan-renderer`.

Pass compilation follows the same product/mechanism boundary. Tensorland owns semantic phases,
effect meaning, scene order, damage, and the decision to fuse or split physical passes. A normal
scene, lock surface, overlay, or picker phase does not become a separate GPU pass merely because it
has a semantic name: dependency-free batches are fused into one direct dynamic-rendering scope.
Only an explicit read-after-write dependency such as backdrop sampling, capture, or color
post-processing splits that scope. “Region-local” is therefore Tensorland policy. The shared
`vulkan-renderer` sees only generic physical facts—image extent, render area, viewport, scissor,
copy/blit offsets, dispatch dimensions, resource use, and timeline lifetime—and never receives a
Tensorland window, workspace, lock-screen, or backdrop-effect type. Vulkan synchronization remains
image/subresource based; reduced work comes from region-sized retained targets and bounded draw or
dispatch ranges, not from pretending that an image barrier is pixel-local.
The current value plan carries each active backdrop's exact physical damage-local composite
rectangles and their radius-expanded bounding sample region. Logical scene damage first propagates
through every expanded background dependency; the physical pass plan then intersects the bounded
native-slot `render_damage` with each effect. Disjoint intersections remain exact for composite
draws, including holes created by protocol-region subtraction, while one bounding rectangle permits
one sample/copy plus two filter stages per effect. This retains one scene-order boundary per effect
instead of creating an unbounded pass list. Scene-ordered affected effects share two ping-pong
descriptor lanes sized to the largest active local sample region in that frame. Unaffected retained
regions stay preserved, so a cursor or surface update elsewhere keeps the direct command stream and
reserves neither lane. That capacity lowers to the shared renderer's semantic-free retained-target
request. The
shared pool now provides bounded image/view allocation and timeline-safe LRU reuse; Tensorland's actual
executor acquires both lanes atomically only after scene/descriptor preflight, encodes them into the
frame's reserved heap suffix, and records output-copy, horizontal filter, vertical filter, and
output-composite boundaries in scene order. Pre-submit failure releases both reservations and an
accepted queue submission retires both at the shared frame timeline. The direct branch never enters
this pool or command path.

An effect is complete only when its whole execution contract exists: a typed config or protocol
source with lifetime/commit semantics; value-only ECS and scene meaning; damage and pass selection;
real GPU command/shader execution through `vulkan-renderer`; and bounded failure, metrics, and tests.
An advertised global, an `EffectStyle` field, or conservative visual bounds alone are not completion.
Opacity and rounded clipping execute in the client-image shader; focus rings and rounded Gaussian
shadows execute as descriptor-free analytic draws in the default direct pass; background blur
executes through the region-local retained multi-pass path. Future effects must satisfy the same
contract rather than adding marker scene state.

Compositor-owned appearance crosses the KDL configuration boundary as a small value-only
`SceneAppearance` object. Window shadow and corner policies resolve on the cold/reload path, then
ECS extraction merges them with per-view protocol effects without placing parser or renderer state
in components. ECS extraction resolves an active view's configured `FocusRingStyle` into a `FocusOutline`;
the frame planner maps its inner and outer edges independently onto the physical output grid, then
the shared renderer's descriptor-heap focus-ring pipeline cuts the rounded inner rectangle out of
the rounded outer rectangle in one SDF draw. Its shader has no resource descriptors, but its
64-byte record uses the same `PushDataEXT` ABI as the client-image and analytic-cursor pipelines.
The inner radius is the view corner radius; the outer radius adds the ring width. It is clipped at
the output edge and the frame plan emits it before that view's client tree, including popups; later
scene nodes cover earlier nodes, and the software cursor remains last. This follows Niri's
front-to-back element contract, Hyprland's active-window border semantics, and Nourish's single
focused-surface ownership without copying their renderer ownership models. It is not a descriptor-set
fallback: every graphics pipeline uses the required `VK_EXT_descriptor_heap` contract.

Output scale is a shared value primitive, represented exactly in the `N/120` units of
`wp_fractional_scale_v1`. DRM mode dimensions and Vulkan native targets remain physical pixels;
Tensorland output geometry, ECS layout, XDG configure sizes, hit testing, and relative-pointer
locations remain logical coordinates. Frame extraction maps shared logical rectangle edges to the
same rounded physical edge, while damage and scissors round outward and clip to the physical mode.
This keeps adjacent tiles gapless without allowing partial-edge damage to leave stale pixels.

The protocol owner traverses each mapped toplevel, subsurface tree, and popup tree in Tensorland's
back-to-front window order and copies only stable identities, buffer metadata, placement, and layer
policy into the flat ECS content table. Synchronized subsurface callbacks are deferred until their non-synchronized
ancestor applies the complete Tensorland surface transaction; explicit acquire/release points follow the same
gate. Popup content remains owned by its toplevel but is clipped by the output rather than the
layout tile, and its old/new bounds participate in scene damage.

Popup topology and explicit grabs are compositor-thread Tensorland state. A topology change rebuilds a
compact topmost-to-bottom node index and parent indices; frame traversal borrows that index directly
through an exact double-ended iterator, with no popup clones, mutex, or staging vector. Dynamic
locations walk only the bounded popup parent chain. Destruction removes a complete descendant tree,
and destroying a parent before its live XDG child reports the required `not_the_topmost_popup`
protocol error immediately. Tensorland-owned XDG role handles share the same compositor-thread `Rc`
state as popup topology; the Tensorland seat grab receives only raw resource identity, so
render, hit-test, and commit paths gain no `Arc`, mutex, or handle copy.

Wayland and IPC boundaries address views by compositor-owned stable IDs, never Bevy `Entity`
values. The ECS owner maintains the ID-to-entity index, rejects duplicate IDs, and is solely
responsible for lifecycle, workspace membership, focus uniqueness, and geometry updates.

The renderer requires Vulkan 1.4 plus `VK_EXT_descriptor_heap`. Descriptor sets and descriptor
buffers are not alternative backends. A device that lacks usable resource and embedded-sampler heap
limits fails startup before any long-lived renderer state is created.

Physical-device ranking lives in `render/device.rs`. The policy is configurable but the default
prefers a discrete GPU, then integrated/virtual hardware, with CPU devices last. The shared
`vulkan-renderer` instance/adapter probe verifies Vulkan 1.4, both the descriptor-heap extension and
feature bit, a graphics queue, timeline semaphores, usable descriptor-heap limits, and a
complete primary/render node pair reported through `VK_EXT_physical_device_drm`, external dma-buf
memory, explicit DRM modifiers, foreign
queue-family ownership transfer, and bidirectional binary `SYNC_FD` semaphore support. The logical
device it creates enables only the extensions for this native path and descriptor heap; there is no
descriptor-set or descriptor-buffer fallback.
Before ranking, each otherwise eligible device must also expose at least one explicit DRM modifier
that supports the real native image usage and exportable dma-buf memory. Import and export support
remain separate values. After the Tensorland tty backend opens the selected DRM pair, `render/format.rs` intersects
that Vulkan snapshot with each active output's primary-plane `FormatSet` and GBM capability. Only
fourcc, modifier, plane count, and capability flags cross this boundary; neither side receives the
other subsystem's handles.
The selected DRM identity is then passed to the tty backend as the sole native device choice. An explicit
`render-device` filters Vulkan candidates by major/minor before ranking; Vulkan and the tty backend
never choose devices independently. Pure ranking remains testable without a GPU. The complete
buffer and synchronization contract is recorded in `docs/tensorland/rendering.md`.

Session-manager selection uses one `SystemdMode` policy for startup and child supervision. `auto`
follows the detected user-manager environment, while `enabled` and `disabled` are explicit.
`ProcessLauncher` is the compositor-owned client boundary. It accepts an executable and argument
list, never a shell string, and uses a double-fork so the compositor does not retain client
children. When systemd is active it creates an `app-tensor-*.scope` through the D-Bus
`StartTransientUnit` API, holding both forked PIDs until the job is ready. A direct path remains
available when systemd integration is inactive; `enabled` mode fails closed if the scope cannot be
created.

XWayland is a rootless compatibility server for individual applications, never a compositor
backend. Tensorland ships only a Wayland session entry and rejects an inherited X11-only session. Niri's
current integration obtains its clean scaling boundary by letting `xwayland-satellite` expose X11
windows as ordinary Wayland surfaces. Tensorland's direct XWayland/XWM path follows the same
architectural invariant: on XWayland readiness, Tensorland starts an XWM and uses the
`xwayland-shell` association to place each normal mapped X11 window into the ordinary Tensorland
`ProtocolWindow`, ECS view, surface tree, scene, and logical-to-physical output path. X11 configure
requests are hints; layout remains authoritative and sends its existing logical rectangle back to
XWayland. Tensorland never derives a second X11 coordinate space from an output scale.

Pointer hit-testing stays on the associated Wayland surface tree, including ordinary popup input
regions. Keyboard activation deliberately targets `X11Surface` for an X11 root window so Tensorland
performs the ICCCM input-focus / `WM_TAKE_FOCUS` handshake; it targets the native root surface for
Wayland clients. Either route updates the same ECS focus, stacking, and layout reflow state, and a
destroyed root clears keyboard focus before its surface association disappears.

Override-redirect X11 windows use popup/parent semantics rather than becoming independent tiled
views. Tensorland admits one only after its map state and `xwayland-shell` association are both known
and its `WM_TRANSIENT_FOR` chain resolves to a managed X11 root. The popup's configured X11
location becomes an offset relative to that root's configured rectangle; it is mapped in Tensorland's
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
Tensorland-owned compositor/subcompositor and seat globals, Tensorland-owned xdg-shell, SHM,
xdg-output, data-device, and popup tracking, Tensorland
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
a non-empty validated client-import format list. Protocol work follows wayland-protocols
**tiers** (core → stable → staging/`ext` → unstable → community → proprietary);
higher tiers win for the same capability, and `zwlr_*` is community-only when no standard path
exists (see `docs/tensorland/protocol-surface.md`). Tensorland-local ports stay value-only at the ECS/event
boundary. Stable tablet-v2 is also direct Tensorland state: libinput device groups define physical
tablets, compact fixed-size values carry tool and pad frames, and compositor-thread owners retain
all Wayland resources. Tool focus is independent of pointer focus, respects session lock and
button/tip grabs, and pad groups expose buttons, modes, rings, strips, and version-2 dials without
placing libinput or Wayland handles in Compio workers. Pointer and tool cursors remain independent;
the frame boundary carries at most one pointer plus 64 tool overlays in a fixed-capacity batch and
damages only sources whose geometry changed.

Tensorland directly owns stable xdg-shell v7: `xdg_wm_base`, positioners, surfaces,
toplevels, popups, client/server double-buffering, metadata, parents, and configure ACK lifecycle.
Role and root lookup use exact resource/surface indices. Toplevel and popup configure backlogs are
fixed at 16 entries, and every unmap advances a mapping generation so a delayed ACK can be consumed
without authorizing a new mapping. A detach commit never emits the next initial configure; a new
empty commit is required first. Popup placement uses saturating coordinates and bounded parent
walks. There is no alternate XDG state, wrapper, cached state, or parallel path.

Text entry is direct Tensorland state: `zwp_text_input_v3` owns per-client double-buffered application
state and commit serials, while the single `zwp_input_method_v2` owner receives
activate/deactivate transactions and returns edits against the matching done serial. Text and
preedit indices are validated as UTF-8 byte boundaries and strings are capped at 4000 bytes;
invalid active objects fail closed, while an unavailable second input-method object is completely
inert as required by the protocol. Keyboard grabs receive keymap, repeat, and modifier state and
pin each press/release pair to its original application or input-method route. Input-method popup
surfaces use a startup-reserved fixed-capacity stacking list and are ordinary Tensorland surface trees
anchored below the focused cursor rectangle. They inherit the owning view's fractional scale and
transform, participate in output enter/leave, frame callbacks, presentation feedback, damage, and
hit testing, and disappear on deactivation.

Virtual keyboard injection is direct Tensorland state as well: only unrestricted clients can bind
`zwp_virtual_keyboard_manager_v1`; each device has a bounded, validated XKB keymap and modifier
state, while sealed keymap memfds are shared by identity on the event path. Device count, keycode
range, and aggregate pressed-key ownership are fixed-capacity, and replacing a keymap or destroying
a device releases its remaining keys before removing the seat capability.

Tensorland also directly owns the wlr-layer-shell v5 global, wire requests, double-buffered role
state, and fixed-capacity configure queues; there is no alternate layer-shell handler or wrapper.
Layer surfaces map through compositor-thread Tensorland state: exact root/output indices make commit
lookup O(1), while one compact `Vec` per active output preserves creation/stacking order without a
per-frame hashed staging collection. On commit, Tensorland arranges exclusive zones, sends pending
configures, and merges mapped layer content into the output frame as value-only scene nodes
(outside the ECS view graph), including committed `ext-background-effect` blur flags. Layer view
IDs are allocated once at creation rather than hashed per frame. Workspace layout uses the
non-exclusive zone so panels
reserve space. Pointer hit-testing prefers Overlay/Top layers, then windows, then Bottom/Background.
Keyboard focus prefers exclusive Overlay/Top, then the last on-demand layer (click or new map), then
exclusive Bottom/Background only when the workspace has no views. xdg popups unconstrain against the
output (or non-exclusive zone for Background/Bottom parents); layer-shell roots may grab, while
window popup grabs are dismissed when an interactive Overlay/Top layer holds focus. Layer content
trees include their xdg popup children in the value-only scene merge. The linux-drm-syncobj global is
added only after Tensorland opens the
Vulkan-selected primary device and verifies syncobj eventfd support. Compositor-owned launches
(IPC `spawn` and `spawn-at-startup`) mint external `xdg-activation` tokens and export them as
`XDG_ACTIVATION_TOKEN` / `DESKTOP_STARTUP_ID` on the child environment. Acquire points become temporary
binary Vulkan semaphore payloads; release points receive the latest GPU-read completion fence only
when their surface attachment retires. Preferred integer/fractional scale and transform follow the output
selected by the authoritative layout placement. Decoration policy currently requires client-side
decoration; server-side mode will only be exposed when decoration geometry and rendering share the
scene snapshot. Presentation feedback is captured from the output-intersecting surface tree in the
submitted scene and completed only by its output/timeline page flip. Full opaque-region occlusion
tracking will be added with render-element state; the protocol layer does not claim it before that
state exists. Alpha-modifier surface values multiply the client-image shader's opacity.
`ext-background-effect` owns copied, double-buffered surface-local regions and maps their exact
add/subtract result into scene damage, retained backdrop passes, and final GPU composite draws; a
null region removes the effect on the next `wl_surface.commit`.

A toplevel is assigned a stable `ViewId` at creation and removed idempotently from both Tensorland
`WindowSpace` and ECS when either the shell or surface destruction callback fires. `WindowSpace`
owns protocol mapping, stacking, hit testing, and output enter/leave without allocating a temporary
output snapshot on refresh; ECS and scene values remain authoritative for policy and rendering.
Tensorland keeps DRM/KMS and GBM behind the tty adapter while directly owning udev, libinput, and
libseat completion adapters. Compio completes each submitted source operation and the compositor
thread performs reconciliation. The tty backend owns session activation, libinput seat assignment,
compositor-thread DRM completion fds, GBM lifetime, and per-output native-format validation. It
opens the primary/render pair selected during
Vulkan probing and requires that pair to be available through the active libseat session. Tensorland
owns atomic surface creation, modesetting, page flips, and strict explicit-modifier plane probing;
Vulkanalia only produces renderable buffers and completion synchronization for it. Tensorland's tty
adapter scans connector resources in place and preserves connector-to-CRTC mappings across startup,
udev hotplug, delayed mode discovery, DP-MST removal, and session resume.

DRM leasing stays at this same native boundary. The scanner reads the kernel
`non-desktop` property and allocates CRTCs to desktop heads first. Shared
`tensor-drm` policy retains only bounded connector identities and lease tokens;
Tensorland's compositor thread retains the DRM fd, Wayland resources, and real
create/revoke ioctls. A lease contains the connector, its assigned CRTC, and a
unique compatible primary plane. The direct `wp_drm_lease_v1` owner follows
the authoritative Vulkan-selected primary node, opens a separate verified
non-master FD for each bound client, and removes a leased/non-desktop head from
ordinary output planning without teaching ECS or `vulkan-renderer` about DRM
leases.

The scanner is an adapter, not Tensorland's output model. Every connector is copied into a complete
device-local snapshot, including connected connectors that do not yet have a mode or CRTC. One
backend-wide `OutputPolicy` consumes snapshots from every DRM device and produces an ordered
`OutputPlan`; only that plan drives Wayland globals and Tensorland `WindowSpace`
lifecycles. Future
EDID profiles, enablement, failover, mirroring, and CRTC allocation belong in this policy boundary.
The plan also carries the selected progressive DRM mode, native fourcc, explicit modifier, plane
count, and resolved output scale. Explicit KDL connector rules win; otherwise the policy preserves
the connector's native/preferred resolution and picks its highest available refresh, avoiding stale
60 Hz EDID `PREFERRED` flags on high-refresh panels. A format, mode, or scale change is therefore an
output change rather than hidden backend state. The adapter may use a custom `CrtcMapper` or drop
down to `ConnectorScanner` without changing the protocol or renderer boundaries. DRM handles do not
enter ECS or the renderer.

Focused state is one contract across ECS, protocol, and rendering: a `Focused` ECS component
extracts a value-only focus outline into the scene, the selected Tensorland `ProtocolWindow` updates
`xdg_toplevel::State::Activated` (or the corresponding XWayland activation), and the seat owns the
keyboard focus. A true active-view transition raises the same attachment family in ECS and Tensorland `WindowSpace`,
updates activation, then delivers seat focus; a later seat-focus repair does not alter stacking or
emit a duplicate scene transition. The Tensorland seat suppresses equal keyboard targets, so a repair does not
emit redundant `wl_keyboard.enter`/`leave` events. The Vulkan path preserves each view's order as
ring, client tree, then later stacked nodes, with the cursor last; this is compositor-owned geometry,
not a descriptor-set fallback for client or output resources.

For each planned output, the renderer owns a bounded three-slot set of Vulkan images and exports
Tensorland-owned `ExportedDmabuf` descriptions. The tty adapter imports those descriptions directly
through GBM, creates framebuffers, and submits atomic/page-flip state
with the renderer's `IN_FENCE_FD`; vblank advances the scanout state. Initial output resource
construction is a startup gate: failure aborts backend preparation before readiness.
Hotplug resource failures are isolated to the affected output and do not invalidate already-live
outputs. Session resume rebuilds every live Tensorland KMS surface's properties and mode blob,
quarantines slots that may still be scanned out, drains completed DRM events, and then schedules a
repaint. Exhausting all three
slots escalates to a device state reset rather than reusing an uncertain buffer.
