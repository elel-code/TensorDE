# Protocol surface comparison

Tensor aims to exceed Niri/Hyprland/Nourish **client-facing protocol coverage** for
desktop-class clients, while keeping AGENTS.md ownership boundaries (Smithay owns
Wayland/KMS; value-only messages cross async/portal edges).

## Tensor (current)

Advertised via `ProtocolCapabilities` / `ProtocolGlobals`:

| Area | Protocols |
|------|-----------|
| Core | compositor, subcompositor, shm, seat, data-device, xdg-shell, xdg-output |
| Scale / viewport | viewporter, fractional-scale, presentation-time |
| Decor / buffer | xdg-decoration (CSD), single-pixel-buffer, alpha-modifier, content-type |
| Input | relative-pointer, pointer-gestures, pointer-constraints, cursor-shape, pointer-warp, tablet-v2 (+ libinput path), keyboard-shortcuts-inhibit |
| Idle | idle-notify, idle-inhibit |
| Shell | wlr-layer-shell (render, exclusive zone, input, popups) |
| Activation / selection | xdg-activation (+ spawn tokens), primary-selection, **wlr-data-control**, **ext-data-control** |
| IME | text-input-v3, input-method-v2, virtual-keyboard-v1 |
| Session / sandbox | session-lock, security-context (calloop sandboxed clients) |
| Desktop list | ext-foreign-toplevel-list, xdg-foreign |
| Misc | system-bell, background-effect, toplevel-icon/tag, fifo, commit-timing, xwayland-keyboard-grab |
| GPU | linux-dmabuf, linux-drm-syncobj (when device supports) |

## Niri

**Built-in Smithay:** largely the same core set; Tensor now matches or exceeds
Niri's Smithay-native globals (including data-control, fifo, commit-timing).

**Niri custom (`src/protocols/`):** virtual-pointer, gamma-control, screencopy,
output-management, ext-workspace, foreign-toplevel (wlr+ext), mutter-x11-interop.

Tensor keeps Niri-style `virtual_pointer` / `gamma_control` sources under
`src/protocol/extensions/` pending a `Dispatch2` port (Tensor uses
`delegate_dispatch2!`; those ports still use legacy `Dispatch`).

## Hyprland (`ProtocolManager.cpp`)

Hyprland additionally exposes (among proprietary globals):

| Protocol | Tensor status |
|----------|---------------|
| tearing-control | not yet (no Smithay state machine; optional later) |
| gamma-control | extension stub + tty hooks; Dispatch2 pending |
| virtual-pointer | extension stub; Dispatch2 pending |
| output-management / output-power | not yet (policy + KMS) |
| screencopy / image-copy-capture | not yet (value-only capture pipeline) |
| wlr-foreign-toplevel (zwlr) | have **ext**-foreign-toplevel-list; wlr twin optional |
| wlr/ext data-control | **done** |
| kde-server-decoration | not planned (CSD policy) |
| hyprland-* proprietary | out of scope |
| color-management | Nourish-class; optional later |
| drm-lease | optional niche |

## Nourish (y5)

Emphasizes:

- **color-management** (`wp_color_manager_v1`) as a first-class wire module
- **deep tablet** (pad/ring/strip) beyond tool axis
- **foreign-toplevel** world-switching mirrors
- multi-world orchestration (not a Tensor goal)

Tensor prioritizes single-session compositor completeness over multi-world
orchestration. Color-management and tablet-pad are follow-ups after capture/gamma.

## Priority backlog (after current tree)

1. Port `extensions/virtual_pointer` + `extensions/gamma_control` to Dispatch2
2. wlr-foreign-toplevel twin (bar ecosystem) if ext-list is insufficient
3. output-management + config precedence
4. value-only screencopy → optional portal
5. color-management (Nourish) once HDR path exists
6. tearing-control when presentation policy is ready
