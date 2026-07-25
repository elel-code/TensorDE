# Configuration

Tensor uses KDL parsed by `knus`. KDL is chosen for readable nested blocks, repeated rules, and
window/output matchers. Schema changes may be breaking while the project is pre-release.

Configuration path precedence is:

1. `--config PATH`
2. `TENSOR_CONFIG`
3. `$XDG_CONFIG_HOME/tensor/config.kdl`
4. `$HOME/.config/tensor/config.kdl`
5. `/etc/tensor/config.kdl`

`TENSOR_LAYOUT`, `TENSOR_IPC_SOCKET`, `TENSOR_GPU`, `TENSOR_RENDER_DEVICE`, `TENSOR_SYSTEMD`, and
`TENSOR_XWAYLAND` are development overrides applied after file parsing.
The current schema is intentionally small:

```kdl
layout "scrolling-1d" {
    gaps 8
    default-column-width proportion=0.5
    master-width proportion=0.55
}
ipc-socket "/run/user/1000/tensor.sock"
gpu "discrete"
# Optional DRM primary or render node. Without this, Smithay selects the seat's primary GPU.
# render-device "/dev/dri/renderD128"
systemd "auto"
xwayland true
appearance {
    # A compositor-owned outer ring: it does not consume client geometry.
    focus-ring {
        enabled true
        width 4
        color "#7fc8ff"
    }
}
spawn-at-startup "waybar"
spawn-at-startup "foot" "--server"
output "eDP-1" {
    # Explicit values are quantized to the exact N/120 representation used by
    # wp_fractional_scale_v1. If omitted, Tensor selects a DPI-based quarter step.
    scale 1.25
    # Optional. A mode without @refresh chooses the highest supported refresh
    # for this resolution; @refresh must match the connector mode exactly.
    # mode "2560x1600@239.760"
}
```

The initial layout surface has three layout families:

- `scrolling-1d` keeps views on one navigable axis and is the default policy.
- `spatial-2d` places views in a two-dimensional tiled arrangement.
- `master-stack` provides the traditional master-and-stack arrangement.

Floating, fullscreen, and monocle behavior are workspace or view state modifiers, not additional
layout families. `tabbed` is reserved for a later container-tree extension rather than being added
as an early compatibility mode.

The `layout` node owns geometry policy. `gaps` is measured in logical pixels.
`default-column-width` and `master-width` each accept exactly one property: `proportion` is relative
to the current output working area, while `fixed` is a logical-pixel width. For example,
`default-column-width fixed=900` keeps a 900-pixel scrolling column across output changes. Invalid,
zero, non-finite, or ambiguous widths reject the whole configuration instead of being silently
repaired.

Each `output` node matches the connector name reported by DRM. Its optional `scale` is constrained to
`0.1..=10.0` and quantized to the nearest `1/120`; this is the same representation sent by
`wp_fractional_scale_v1`. An output without a rule uses the Niri/Mutter-style DPI heuristic and a
quarter-step scale. Smithay exposes the resulting fractional value to clients, while `wl_output`
continues to receive the required rounded-up integer scale.

An optional `mode` uses `<width>x<height>` or `<width>x<height>@<refresh>`, where refresh has at
most three decimal places and is compared in exact millihertz. With a configured resolution but no
refresh, Tensor selects its highest supported progressive refresh; with an exact refresh, it selects
only that connector mode. With no `mode` rule, Tensor keeps the DRM-preferred/native resolution but
still selects its highest supported progressive refresh. This deliberately avoids the common EDID
case where a high-refresh monitor marks only its 60 Hz timing as `PREFERRED`. An unavailable rule is
logged and falls back to that native high-refresh policy rather than selecting an arbitrary mode.

The focused view receives the standard `xdg_toplevel` `Activated` state and a compositor-rendered
outer focus ring. The ring is scene data, not a client-side decoration, so it remains visible for
native Wayland and rootless XWayland applications without covering client pixels. `appearance`
currently owns the global `focus-ring`: `enabled` defaults to `true`, `width` defaults to four
logical pixels, and `color` defaults to `#7fc8ff`. Colors accept `#RRGGBB` or `#RRGGBBAA`; a zero
width or transparent color also produces no ring. The ring is rounded and clipped in physical
output coordinates at frame extraction, matching the same fractional-scale rules as client content.
Its inner radius follows the focused view's corner radius and its outer radius expands by the ring
width, so rounded clients never receive a rectangular focus artifact. The value-only appearance
object is the future theming boundary rather than a renderer-specific decoration API. Rendering keeps
the ring behind that view's client tree (including popups) and behind any later stacked view, while
the software cursor remains above all of them.

Scrolling layout follows the same core invariants as Niri without copying its renderer: every view
has min/max size constraints and an optional width override; each workspace owns an independent
horizontal offset; focusing a view moves only far enough to make its full column visible. Layout
output contains both unclipped geometry and its visible intersection with the output. Animation,
damage, shadows, and input hit testing consume that snapshot rather than recomputing positions.

`gpu` defaults to `discrete`: candidates are ranked discrete GPU, integrated GPU, virtual GPU, then
CPU. Candidates must provide Vulkan 1.4, `VK_EXT_descriptor_heap`, a graphics queue, a complete DRM
primary/render pair, external dma-buf memory with DRM modifiers, foreign queue-family ownership
transfer, and bidirectional binary `SYNC_FD` semaphores. Use `integrated` or `any` only when the
machine's topology requires it. `TENSOR_GPU` overrides the file for local development.

`render-device` constrains the common Vulkan and Smithay device. Either a primary node (`cardN`) or
render node (`renderDN`) is accepted. Tensor resolves its major/minor identity, selects only the
matching Vulkan physical device, requires its paired node, and passes the selected render node to
the tty backend. When omitted, Vulkan capability filtering and `gpu` ranking choose the pair.
`TENSOR_RENDER_DEVICE` overrides the file. A node that is not reported by Vulkan or unavailable to
the active libseat session fails startup.

`systemd` accepts `auto`, `enabled`, or `disabled`. The default `auto` mode activates when
the `systemd` Cargo feature is present and `NOTIFY_SOCKET`, `SYSTEMD_EXEC_PID`, or `MANAGERPID`
identifies a user-manager launch. Without that feature, `auto` resolves to the direct path while
`enabled` fails startup. Explicit configuration never manufactures unavailable integration.

`xwayland` defaults to `true`. It starts the rootless XWayland process and calloop source when the
compositor enters its event loop, then attaches Smithay's XWM after XWayland reports readiness.
Normal rootless X11 windows enter the same Wayland surface, ECS, and Vulkan scene path as native
clients. This is not an X11 backend: Tensor rejects primary X11 sessions, keeps layout coordinates
authoritative, and does not provide an X11 session entry. Override-redirect X11 menus and tooltips
are accepted only after XWM mapping, xwayland-shell association, and a managed `WM_TRANSIENT_FOR`
ancestor are all known; they render as popup content of that root view rather than independent
layout views. Normal X11 `WM_TRANSIENT_FOR` dialogs instead retain their own ECS/input/render node
while attaching to their immediate managed owner: their requested logical size is constrained and
centered over that owner, and X11 position requests are ignored. An unresolved owner keeps the
dialog outside the scene rather than creating a global X11 placement fallback.

Each `spawn-at-startup` node contains one executable followed by zero or more arguments. Entries run
only for `--session` startup. Tensor first prepares the runtime, installs `WAYLAND_DISPLAY`,
`XDG_CURRENT_DESKTOP`, `XDG_SESSION_TYPE`, `TENSOR_IPC_SOCKET`, and the allocated XWayland `DISPLAY`
when enabled, then waits for an active systemd user manager to accept the same snapshot and
publishes readiness. Inherited session values are cleared before this publication, so disabling
XWayland cannot leak a host `DISPLAY` into children. Only then does the one-shot autostart gate
queue commands in configuration order on the asynchronous launch worker. Process creation and
optional systemd scope setup complete off the compositor thread; outcomes are logged when the
calloop channel drains. `--check`, ordinary non-session startup, environment-sync failure, and
readiness failure launch none of them.

Values are passed directly to the executable: Tensor does not invoke a shell, expand variables, or
interpret pipes and redirections. Use a dedicated executable when orchestration is more complex
than argv.

Future reloads parse and validate off the event-loop critical path. A failed reload must preserve
the last valid configuration and report a structured error through logs and IPC.
