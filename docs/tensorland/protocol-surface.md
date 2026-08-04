# Protocol surface

Tensorland aims to exceed Niri/Hyprland/Nourish **client-facing protocol coverage** for
desktop-class clients, while keeping ownership boundaries: Tensorland's direct `wayland-server`
bindings own protocol objects; ECS/render/event bus stay value-only.

Advertising a global is not an implementation claim. Every protocol is tracked through five
separate depths:

1. wire dispatch, ownership, lifetime, privilege filtering, and protocol errors;
2. pending/current state and the exact `wl_surface.commit` or request activation boundary;
3. value-only ECS/product state with no Wayland resource crossing the boundary;
4. its real input, layout, damage, render, capture, or KMS side effect; and
5. wire, state-transition, and execution tests, including malformed and destruction paths.

Capability tables describe bindable globals; prose below them records execution depth. A marker
global, request parser, or scene field without its required product side effect remains incomplete.

Protocol **organization and priority** follow
[wayland-protocols](https://gitlab.freedesktop.org/wayland/wayland-protocols) categories and a
one-state-owner-per-protocol layout, not ad hoc “whatever Niri ships.”

## Protocol tiers

| Tier | Source | Name cues | Tensorland policy |
|------|--------|-----------|----------------|
| **0 · Core** | `wayland.xml` | `wl_*` | Always required; Tensorland compositor/seat/shm |
| **1 · Stable standard** | wayland-protocols `stable/` + mature shell | `xdg_*`, non-`z` `wp_*` when stable | Prefer; first-class investment |
| **2 · Staging / ext** | wayland-protocols `staging/`, `ext` module | `ext_*`, many modern `wp_*` | Prefer for new desktop features; implement when the product needs the capability |
| **3 · Unstable (legacy)** | wayland-protocols `unstable/` | interface prefix `z` | Avoid new work; migrate to staging/stable when available |
| **4 · Community** | wlr / plasma / misc crates | `zwlr_*`, `kde_*`, … | Only if tier 1–2 has **no** equivalent, or a critical client cannot bind the standard global; document the gap |
| **5 · Proprietary** | compositor-private XML | `hyprland_*`, … | Out of scope |

### Selection rules

1. **Same capability → higher tier wins.** Example: `ext-foreign-toplevel-list` (tier 2) over
   `zwlr-foreign-toplevel-management` (tier 4).
2. **Do not add a community twin** only because Hyprland or Niri still advertise one.
3. **Staging is first-class**, not “experimental second-class.” wayland-protocols encourages
   implementing staging where the feature is desired; Tensorland does the same.
4. **Community protocols are stopgaps.** When a staging/stable replacement lands upstream,
   new investment moves up-tier; community globals may remain for transitional clients.
5. **Implementation vehicle:** use direct `wayland-server::Dispatch` implementations and Tensorland's
   zero-cost delegation bridge, with one explicit state owner per protocol. Wire owners still post
   **value-only** events into `tensor-event`.
6. **Security / privilege:** privileged globals (data-control, gamma, virtual-pointer, security
   context, session lock) keep unrestricted-client filters; staging does not mean “open to all.”

### Capability map (prefer / avoid)

| Capability | Prefer (tier) | Avoid as primary |
|------------|---------------|------------------|
| Foreign toplevel list | `ext-foreign-toplevel-list` (2) | `zwlr-foreign-toplevel-management` (4) |
| Data control | `ext-data-control` (2) | `zwlr-data-control` (4, compat only) |
| Session lock | `ext-session-lock` (2) | proprietary lock notifiers (5) |
| Capture | `ext-image-copy-capture` + `ext-image-capture-source` (2) | `zwlr-screencopy` (4) |
| Background blur | `ext-background-effect` (2) | compositor-private blur IPC (5) |
| Workspace (future) | `ext-workspace` (2) | private workspace IPC only |
| Layer shell | `wlr-layer-shell` (4, **no** tier-2 yet) | — |
| Gamma | `zwlr-gamma-control` (4, no tier-2 yet) | — |
| Virtual pointer | `zwlr-virtual-pointer` (4, no tier-2 yet) | — |
| Output management | `zwlr-output-management` if needed (4) | — until `wp`/`ext` exists |
| Color / HDR | `wp_color_manager` family (2) | wlr-only color hacks |
| Pixel encoding / range | `wp_color_representation` (2) | compositor-private buffer tags |
| Tearing | `wp_tearing_control` (2), implemented with fail-closed KMS policy | — |
| DRM connector leasing | `wp_drm_lease` (2) for non-desktop heads | private lease IPC |
| Temporary portal seat | `ext-transient-seat` (2), implemented and creator-scoped | shared-seat impersonation |

## Tensorland (current), by tier

### Tier 0 — Core
compositor, subcompositor, shm, seat, data-device, …

Core data-device drag icons are rendered as pointer-relative cached surface overlays, including
committed offsets, output membership/preferred scale, and submitted-frame callbacks.

### Tier 1 — Stable standard
xdg-shell, xdg-output, xdg-decoration (CSD), xdg-activation, viewporter, presentation-time,
linux-dmabuf, linux-drm-syncobj (when device supports), primary-selection, tablet-v2, …

### Tier 2 — Staging / ext (and mature wp used as desktop baseline)
fractional-scale, cursor-shape, content-type, alpha-modifier, single-pixel-buffer, fifo,
commit-timing, pointer-warp, idle-notify, security-context, text-input-v3,
**xdg-dialog**, **xdg-toplevel-drag**,
**ext-session-lock**, **ext-foreign-toplevel-list**, **ext-data-control**,
**ext-background-effect**, **ext-image-capture-source**, **ext-image-copy-capture** (TTY output and single-output toplevel capture use a retained GPU region tap plus deferred timeline SHM readback; headless capture keeps bounded idle SHM composition),
**ext-workspace**,
**wp-drm-lease** (completion-gated to real TTY non-desktop connectors),
xdg-foreign, xdg-system-bell, xdg-toplevel-icon/tag, …

`xdg-dialog` is a direct ancillary-object implementation with one live dialog object per
`xdg_toplevel`. A dialog with an `xdg_toplevel` parent is lowered to the value-only ECS attached-view
model, retains its own scene node, and is removed from primary tile allocation. Modal dialogs
redirect parent focus to the deepest live modal child. Destroying the ancillary object, unsetting
the parent, or tearing down the parent restores/detaches placement without leaving an ECS edge.

`xdg-toplevel-drag` is tied to the core `wl_data_source` state machine rather than running a
parallel drag path. It rejects reused/selection sources, enforces attach and end-before-destroy
lifetime rules, and lowers an attached toplevel to retained ECS floating placement while the core
DnD grab moves it. The dragged window is excluded from DnD target hit testing, and its last
position remains the floating placement when the drag ends. Real wire tests verify manager errors
for duplicate and selection-used sources plus drag-object errors for duplicate live attachments and
destruction before the underlying drag ends.

`ext-background-effect` is a direct implementation rather than a private blur request. A
`set_blur_region` request copies the complete `wl_region` add/subtract result, permits immediate
region destruction, and becomes current only on the next surface commit; null removes the effect.
Tensorland normalizes the surface-local region into at most 128 exact non-overlapping rectangles,
clips it to the committed surface, maps it into window or layer scene coordinates, and fails closed
to an empty effect when pathological region complexity exceeds the bound. Damage propagation uses
the exact rectangles plus filter radius. GPU execution performs one bounded backdrop sample/copy
and two separable filters per affected scene-order effect, then composites each exact rectangle so
subtracted holes stay untouched. Shared `vulkan-renderer` sees only generic retained color targets,
regions, barriers, descriptors, and timeline retirement—never a Tensorland blur protocol type.

### Tier 3 — Unstable (as still common in the ecosystem)
pointer-gestures, pointer-constraints, relative-pointer, idle-inhibit,
keyboard-shortcuts-inhibit, input-method-v2, virtual-keyboard-v1, xwayland-keyboard-grab, …
(Prefer staging replacements when they supersede these.)

`text-input-v3` and `input-method-v2` are paired direct implementations, not advertised marker
globals. Tensorland owns focus enter/leave, double-buffered state, commit/done serial validation, the
single-input-method rule, and exclusive keyboard routing. The input-method global is hidden from
security-context sandbox clients. Candidate popups are compositor-thread surface trees attached
to the active text cursor; they use the focused view's output scale/transform and normal Tensorland
scene, output, callback, presentation, and input paths. There is no Smithay adapter or alternate
popup placement path.

`virtual-keyboard-v1` is a direct implementation, not a marker global. It is hidden from
security-context sandbox clients, accepts only bounded valid XKB keymaps, reuses sealed keymap
memfds on the key path, and tracks a fixed 32-device set plus aggregate pressed-key ownership.
Keymap replacement and object destruction synthesize the required releases before lifetime state
is removed.

`tablet-v2` is the stable wayland-protocols definition and a direct Tensorland implementation.
Libinput devices are coalesced by physical device-group, while tools and pads retain distinct
fixed-capacity identities. Tool focus uses scene hit testing independently of pointer focus and is
intercepted by session lock; proximity, grabs, tip/buttons, all axes, pad groups/modes,
rings/strips/dials, and validated tool cursors are emitted from compositor-thread state. The Linux
adapter expands a hardware frame only through a fixed ring of compact value events; no libinput or
Wayland object crosses into an async worker.

Core pointer, cursor-shape, tablet cursor, and data-device drag-icon status plus their remaining
native plane work are tracked together in [`cursor.md`](cursor.md). Hardware-plane assignment must
not fork these protocol owners.

### Tier 4 — Community (documented exceptions)
wlr-layer-shell (no ext equivalent; full stack),
wlr-data-control (compat beside ext-data-control),
zwlr-virtual-pointer, zwlr-gamma-control (KMS LUT apply),
**wlr-output-management** (enable/position/scale via `OutputRule`; mode switch deferred), …

### Tier 5 — Out of scope
hyprland-\*, kde-server-decoration (CSD policy), mutter-x11-interop, …

## Niri / Hyprland / Nourish (reference only)

**Niri custom:** virtual-pointer, gamma-control, screencopy (**wlr**), output-management (wlr),
ext-workspace, foreign-toplevel (wlr+ext). Tensorland does **not** copy wlr-screencopy as the
long-term capture design.

**Hyprland:** many community + proprietary globals; Tensorland takes **tier** guidance, not a
checklist of every Hyprland XML.

**Nourish:** color-management (`wp`) and deep tablet — tier-2 `wp` when HDR path exists.

`wp_color_representation_v1` has a test-bindable wire owner for the RGB
combinations Tensorland imports: identity coefficients in full or limited
range and all three alpha modes. State is per-surface and double-buffered,
reaches each extracted surface draw, and produces a protocol-neutral renderer
color plan. A separate retained managed-color pipeline now consumes
non-identity plans in direct and region-local multi-pass recording without
adding work to the SDR identity shader. Its production global and multi-plane
YUV combinations remain unadvertised until the output color owner and
multi-plane import make YUV execution exact. Encoded/UNORM and sRGB client
views are now selected explicitly, so RGB limited-range and managed-alpha
transforms enter the shader before hardware transfer decoding.

The companion `wp_color_management_v1` v3 now has a completion-gated direct
wire owner. Its parametric creator validates one-shot properties, named and
custom primaries, named and power transfer functions, luminance and contained
mastering volumes, and permanent non-zero identities. Immutable descriptions
emit `ready2` or an explicit `failed`; surface descriptions are copy-attached,
double-buffered, and reach the existing value-only renderer color plan.
Output and surface feedback currently expose the same information-capable SDR
parametric description, while ICC and Windows presets fail as unadvertised
features. Production advertising remains disabled until live output color
selection, HDR post-encoding, KMS HDR metadata/reset, and corresponding output
change notifications are complete.

## Priority backlog (tier-aware)

1. ~~KMS gamma LUT~~ (tier 4, no ext) done
2. ~~ext-foreign-toplevel-list deepen~~ (tier 2) done
3. ~~**ext-image-copy-capture GPU readback and cursor option**~~ done: bounded protocol queue, retained output-region tap before foreign release, separate same-queue timeline readback, BGRA/RGBA/10-bit SHM publication, and `PaintCursors`-controlled tap placement. A toplevel wholly contained by one output is lowered through the shared fractional-scale boundary to an output-local physical GPU crop; cross-output toplevels fail explicitly until multi-output assembly exists. Headless wire tests retain the bounded idle path.

   Pointer cursor sessions are no longer marker objects: they own a distinct base capture session,
   publish `enter`/`leave`/`position`/`hotspot` in transformed source-buffer coordinates, and capture
   an ARGB cursor image separately from the desktop. Named-cursor pixels are retained from the cold
   XCursor upload and reused without parsing or GPU readback; identity-transformed SHM cursor
   surfaces use their live alpha buffer. DMA-BUF or transformed cursor surfaces stop explicitly
   until a cursor-image-only GPU tap exists, rather than returning desktop pixels or claiming a
   false constraint set.

4. ~~**ext-workspace** + multi-workspace host~~ done (pool, IPC list/switch/move+follow, Super+digit/Page, activate)

5. ~~**wlr-output-management** (tier 4 stopgap)~~ enable/disable/position/scale via `OutputRule` done; mode switch deferred until live KMS replacement is safe

6. ~~**wp tearing-control** (tier 2)~~: double-buffered surface hint, value-only
   ECS extraction, explicit single-pass/exclusive policy, and typed atomic KMS
   async submission are implemented; FIFO latest-ready remains the default.
7. ~~**ext-transient-seat** (tier 2)~~: creator-scoped temporary `wl_seat`
   globals, explicit denial/capacity, teardown, and isolated virtual pointer and
   keyboard capability routing are implemented.
8. **wp color-management v3 + wp color-representation** (tier 2): implement as
   one HDR/color slice spanning surface state, shared renderer transforms,
   output feedback, formats, tone mapping, and KMS metadata. Parametric image
   descriptions, fixed SDR feedback, and surface commit state are implemented
   behind the completion gate; production output/HDR ownership remains.
9. ~~**wp DRM lease** (tier 2)~~: completion-gated non-desktop connector
   advertisement, verified non-master device FD, real kernel lease FD,
   connector/CRTC/primary-plane reservation, revocation on destroy, hotplug,
   session pause and device removal, plus ordinary output-plan exclusion.
10. Finish already-advertised protocol depth: live output mode replacement,
    cross-output capture, cursor-only GPU capture, multi-plane/YUV dma-buf,
    implicit reservation-fence policy, and atomic cursor planes. Tablet cursor
    and xdg-toplevel-drag wire coverage are complete.
11. Migrate remaining tier-3 surfaces upward when wayland-protocols promotes them.

The required ownership, failure, hardware, and test gates for items 6–10 are
defined in [`protocol-roadmap.md`](protocol-roadmap.md). An item is not complete
merely because its global appears in `ProtocolCapabilities`.

## Code layout guidance

- Prefer one Tensorland `*State` owner and direct dispatch module per protocol.
- Tensorland-local wire adapters live under `src/protocol/extensions/`; value-only
  protocol policy and lifecycle state belong in `tensor-protocol`. Group by
  **tier** in docs and capability flags, not by “whatever file was handy.”
- `ProtocolCapabilities` remains a **flat advertise set** for IPC/tests.
- Machine-readable tier index: `src/protocol/tier.rs` (`PROTOCOL_CATALOG`, `ProtocolTier`);
  keep aligned with this document when adding globals.
- Event-layer integration: protocol commits and control outcomes post **value events**
  (`tensor-event`); do not put `WlSurface` on the bus.
