# Protocol surface

Tensor aims to exceed Niri/Hyprland/Nourish **client-facing protocol coverage** for
desktop-class clients, while keeping ownership boundaries: Tensor's direct `wayland-server`
bindings own protocol objects; ECS/render/event bus stay value-only.

Protocol **organization and priority** follow
[wayland-protocols](https://gitlab.freedesktop.org/wayland/wayland-protocols) categories and a
one-state-owner-per-protocol layout, not ad hoc “whatever Niri ships.”

## Protocol tiers

| Tier | Source | Name cues | Tensor policy |
|------|--------|-----------|----------------|
| **0 · Core** | `wayland.xml` | `wl_*` | Always required; Tensor compositor/seat/shm |
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
   implementing staging where the feature is desired; Tensor does the same.
4. **Community protocols are stopgaps.** When a staging/stable replacement lands upstream,
   new investment moves up-tier; community globals may remain for transitional clients.
5. **Implementation vehicle:** use direct `wayland-server::Dispatch` implementations and Tensor's
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
| Tearing | `wp_tearing_control` (2) when presentation policy ready | — |

## Tensor (current), by tier

### Tier 0 — Core
compositor, subcompositor, shm, seat, data-device, …

### Tier 1 — Stable standard
xdg-shell, xdg-output, xdg-decoration (CSD), xdg-activation, viewporter, presentation-time,
linux-dmabuf, linux-drm-syncobj (when device supports), primary-selection, …

### Tier 2 — Staging / ext (and mature wp used as desktop baseline)
fractional-scale, cursor-shape, content-type, alpha-modifier, single-pixel-buffer, fifo,
commit-timing, pointer-warp, idle-notify, security-context, text-input-v3,
**ext-session-lock**, **ext-foreign-toplevel-list**, **ext-data-control**,
**ext-background-effect**, **ext-image-capture-source**, **ext-image-copy-capture** (idle SHM silhouette + optional live SHM client blit; no page-flip GPU readback),
**ext-workspace**,
xdg-foreign, xdg-system-bell, xdg-toplevel-icon/tag, …

### Tier 3 — Unstable (as still common in the ecosystem)
pointer-gestures, pointer-constraints, relative-pointer, tablet-v2, idle-inhibit,
keyboard-shortcuts-inhibit, input-method-v2, virtual-keyboard-v1, xwayland-keyboard-grab, …
(Prefer staging replacements when they supersede these.)

### Tier 4 — Community (documented exceptions)
wlr-layer-shell (no ext equivalent; full stack),
wlr-data-control (compat beside ext-data-control),
zwlr-virtual-pointer, zwlr-gamma-control (KMS LUT apply),
**wlr-output-management** (enable/position/scale via `OutputRule`; mode switch deferred), …

### Tier 5 — Out of scope
hyprland-\*, kde-server-decoration (CSD policy), mutter-x11-interop, …

## Niri / Hyprland / Nourish (reference only)

**Niri custom:** virtual-pointer, gamma-control, screencopy (**wlr**), output-management (wlr),
ext-workspace, foreign-toplevel (wlr+ext). Tensor does **not** copy wlr-screencopy as the
long-term capture design.

**Hyprland:** many community + proprietary globals; Tensor takes **tier** guidance, not a
checklist of every Hyprland XML.

**Nourish:** color-management (`wp`) and deep tablet — tier-2 `wp` when HDR path exists.

## Priority backlog (tier-aware)

1. ~~KMS gamma LUT~~ (tier 4, no ext) done
2. ~~ext-foreign-toplevel-list deepen~~ (tier 2) done
3. ~~**ext-image-copy-capture** idle SHM silhouette + SHM client blit~~ done; Vulkan/GPU readback still open (must stay off page-flip)

4. ~~**ext-workspace** + multi-workspace host~~ done (pool, IPC list/switch/move+follow, Super+digit/Page, activate)

5. ~~**wlr-output-management** (tier 4 stopgap)~~ enable/disable/position/scale via `OutputRule` done; mode switch deferred until live KMS replacement is safe

6. **wp color-management** (tier 2) once HDR path exists
7. **wp tearing-control** (tier 2) when presentation policy is ready
8. Migrate remaining tier-3 surfaces upward when wayland-protocols promotes them

## Code layout guidance

- Prefer one Tensor `*State` owner and direct dispatch module per protocol.
- Tensor-local wire adapters live under `src/protocol/extensions/`; value-only
  protocol policy and lifecycle state belong in `tensor-protocol`. Group by
  **tier** in docs and capability flags, not by “whatever file was handy.”
- `ProtocolCapabilities` remains a **flat advertise set** for IPC/tests.
- Machine-readable tier index: `src/protocol/tier.rs` (`PROTOCOL_CATALOG`, `ProtocolTier`);
  keep aligned with this document when adding globals.
- Event-layer integration: protocol commits and control outcomes post **value events**
  (`tensor-event`); do not put `WlSurface` on the bus.
