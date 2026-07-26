# Protocol surface comparison

Tensor aims to exceed Niri/Hyprland/Nourish **client-facing protocol coverage** for
desktop-class clients, while keeping AGENTS.md ownership boundaries (Smithay owns
Wayland/KMS; value-only messages cross async/portal edges).

## Selection policy: ext over wlr

When choosing what to implement next:

1. Prefer **`ext-*`**, then other standardized **`wp-*` / `xdg-*`** protocols.
2. Implement **`zwlr_*` only when**:
   - there is **no** standard/ext equivalent yet (example: `wlr-layer-shell`), or
   - a **critical** client cannot bind the ext global and the gap is documented.
3. **Do not** add a wlr twin only because Hyprland or Niri still advertise one, if Tensor
   already has the ext path (example: do **not** prioritize `zwlr-foreign-toplevel` while
   `ext-foreign-toplevel-list` is live).
4. Capture / portal work targets **`ext-image-copy-capture`** and
   **`ext-image-capture-source`**, not `zwlr-screencopy`, as the primary design.
5. Existing wlr globals that remain useful for transitional clients may stay until those
   clients migrate; new investment goes into the ext side first.

| Capability | Prefer | Avoid as primary |
|------------|--------|------------------|
| Foreign toplevel list | `ext-foreign-toplevel-list` | `zwlr-foreign-toplevel-management` |
| Data control / clipboard mgr | `ext-data-control` | `zwlr-data-control` (compat only) |
| Session lock | `ext-session-lock` | proprietary lock notifiers |
| Capture | `ext-image-copy-capture` + `ext-image-capture-source` | `zwlr-screencopy` |
| Workspace (future) | `ext-workspace` | compositor-private workspace IPC only |
| Layer shell | *(no ext yet)* `wlr-layer-shell` | — |
| Gamma | *(no ext yet)* `zwlr-gamma-control` | — |
| Virtual pointer | *(no ext yet)* `zwlr-virtual-pointer` | — |
| Output management | *(no ext yet)* `zwlr-output-management` | — |

## Tensor (current)

Advertised via `ProtocolCapabilities` / `ProtocolGlobals`:

| Area | Protocols |
|------|-----------|
| Core | compositor, subcompositor, shm, seat, data-device, xdg-shell, xdg-output |
| Scale / viewport | viewporter, fractional-scale, presentation-time |
| Decor / buffer | xdg-decoration (CSD), single-pixel-buffer, alpha-modifier, content-type |
| Input | relative-pointer, pointer-gestures, pointer-constraints, cursor-shape, pointer-warp, tablet-v2 (+ libinput path), keyboard-shortcuts-inhibit, virtual-pointer (`zwlr`, no ext yet) |
| Idle | idle-notify, idle-inhibit |
| Shell | wlr-layer-shell (no ext equivalent; full stack) |
| Activation / selection | xdg-activation (+ spawn tokens), primary-selection, **ext-data-control** (preferred), wlr-data-control (compat) |
| IME | text-input-v3, input-method-v2, virtual-keyboard-v1 |
| Session / sandbox | session-lock (`ext`), security-context (`wp`) |
| Desktop list | **ext-foreign-toplevel-list** (no wlr twin), xdg-foreign |
| Misc | system-bell, background-effect, toplevel-icon/tag, fifo, commit-timing, xwayland-keyboard-grab, gamma-control (`zwlr`, no ext yet; KMS apply pending) |
| GPU | linux-dmabuf, linux-drm-syncobj (when device supports) |

## Niri

**Built-in Smithay:** largely the same core set; Tensor matches or exceeds Niri's
Smithay-native globals (data-control, fifo, commit-timing, Dispatch2 extensions).

**Niri custom (`src/protocols/`):** virtual-pointer, gamma-control, screencopy (wlr),
output-management (wlr), ext-workspace, foreign-toplevel (wlr+ext), mutter-x11-interop.

Tensor ports virtual-pointer and gamma-control on Dispatch2. Capture work should **not**
copy Niri's wlr-screencopy as the long-term design; prefer Smithay's ext image-capture modules.

## Hyprland (`ProtocolManager.cpp`)

| Protocol | Tensor status | Policy note |
|----------|---------------|-------------|
| tearing-control | not yet | wp, fine to add later |
| gamma-control | advertised; KMS LUT pending | no ext; wlr OK |
| virtual-pointer | advertised + seat forward | no ext; wlr OK |
| output-management / power | not yet | no ext; wlr OK when needed |
| screencopy / image-copy-capture | not yet | **prefer ext image-copy-capture** |
| wlr-foreign-toplevel | **not planned** | have ext-list |
| wlr/ext data-control | both; **prefer ext** | wlr kept for older managers |
| kde-server-decoration | not planned | CSD policy |
| hyprland-* proprietary | out of scope | — |
| color-management | not yet | wp; Nourish-class later |
| drm-lease | optional niche | wp |

## Nourish (y5)

Emphasizes color-management (`wp_color_manager_v1`), deep tablet pad, and multi-world
foreign-toplevel mirrors. Tensor stays single-session; color-management is a later `wp`
item, not a wlr path.

## Priority backlog

1. KMS gamma LUT apply for `zwlr_gamma_control` (no ext equivalent)
2. Deepen **ext-foreign-toplevel-list** (activate/close if needed) — **not** wlr twin
3. **ext-image-copy-capture** + **ext-image-capture-source** (value-only frames → optional portal)
4. **ext-workspace** when multi-workspace model lands
5. wlr-output-management only if no suitable ext/wp exists for runtime layout
6. color-management (`wp`) once HDR path exists
7. tearing-control (`wp`) when presentation policy is ready
