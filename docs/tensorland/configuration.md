# Configuration

Tensorland uses KDL 2.0 parsed through the shared `tensor-kdl` crate for cold-start policy. Typed
decoding rejects unknown nodes and properties by default, then Tensorland resolves the parsed values
into renderer-independent policy. Runtime control stays on the versioned Unix-socket IPC surface
rather than growing a second hot-reload dialect. Schema changes may be breaking while the project
is pre-release. TOML is not retained as a compatibility parser.

Run `tensorland --validate-config` to validate the resolved default path, or
`tensorland --validate-config --config PATH` for an explicit file. Parser and typed-decode
failures retain `tensor_kdl::ErrorCtx`, the complete source, and its path; Tensorland renders them with
the optional `tensor-kdl` miette adapter as a named source, primary label, source line, error code,
and help. This mode exits before logging, Wayland, Vulkan, IPC, or ECS initialization. `--check`
continues farther and validates those startup gates too.

Exact source labeling covers syntax, type errors, and single-field scalar policy decoded through
`DecodeScalar::decode_scalar_at`: output scale/mode/refresh caps and layout gap/proportion/fixed
limits point at their originating property without reparsing or retaining a DOM. The shared
`#[kdl(validate = "...")]` completion hook handles node-local cross-field policy. Choosing both
`proportion` and `fixed` points at the later property; choosing neither points at the width node
name. Duplicate output rules and environment assignments remain Tensorland document-level policy
errors because their conflicts span separate nodes.

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
layout "scrolling-1d" gaps=8 {
    default-column-width proportion=0.5
    master-width proportion=0.55
}

workspaces default-count=1 {
    hidden "minimized" minimize-target=#true
    // hidden "communication" show-in-overview=#true
}

ipc-socket "/run/user/1000/tensor.sock"
gpu "discrete"
// Optional DRM primary or render node. Without this, Tensorland capability ranking selects the pair.
// render-device "/dev/dri/renderD128"
systemd "auto"
xwayland #true

appearance {
    // A compositor-owned outer ring: it does not consume client geometry.
    focus-ring enabled=#true width=4 color="#7fc8ff"
    window-corners radius=12
    // Analytic window shadow; no offscreen target or extra pass.
    window-shadow enabled=#true offset-x=0 offset-y=6 blur-radius=18 spread=0 color="#00000070"
}

spawn-at-startup "waybar"
spawn-at-startup "foot" "--server"

environment {
    // Names removed before session publication.
    clear "GTK_IM_MODULE"
    // Extra variables for launched clients. Session-owned names are ignored.
    set "EDITOR" "hx"
}

cursor {
    xcursor-theme "default"
    xcursor-size 24
    // Presence enables these policies; omit a node to disable it.
    // hide-when-typing
    // hide-after-inactive-ms 1000
}

debug frame-stats=#false force-full-redraw=#false

output "eDP-1" scale=1.25 {
    // Explicit values are quantized to the exact N/120 representation used by
    // wp_fractional_scale_v1. If omitted, Tensorland selects a DPI-based quarter step.
    // Optional. A mode without @refresh chooses the highest supported refresh
    // for this resolution; @refresh must match the connector mode exactly.
    // mode="2560x1600@239.760"
    position x=0 y=0
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
`default-column-width fixed=900` keeps a 900-pixel scrolling column across output changes.
Invalid, zero, non-finite, or ambiguous widths reject the whole configuration instead of being
silently repaired.

`workspaces.default-count` creates `1..=32` regular workspaces; the default is one. Regular
workspaces alone participate in next/previous navigation and `ext-workspace-v1` advertisement.
Up to 16 named `hidden` workspaces may be declared. Exactly one explicit hidden list entry must set
`minimize-target=#true`; when the complete `workspaces` node or its hidden list is omitted,
Tensorland supplies `hidden "minimized" minimize-target=#true`. `show-in-overview` defaults to
true and controls whether the hidden workspace enters the bounded `get-overview` inventory.
Regular workspaces always enter that inventory. This option does not expose a hidden workspace to
normal activation, numeric bindings, or `ext-workspace-v1`.

Minimizing does not create a second window lifecycle: Tensorland moves the existing view family to
the configured hidden target and records its regular origin. Restore moves the same IDs and retained
surface/GPU resources back, optionally activating the origin workspace. Workspace topology is a
startup identity policy, so changing this KDL node through hot reload reports
`reload_requires_restart`; runtime minimize, restore, and navigation use versioned IPC.

Each `output` node matches the connector name in its first argument. Its optional `scale` property is
constrained to `0.1..=10.0` and quantized to the nearest `1/120`; this is the same representation
sent by `wp_fractional_scale_v1`. An output without a rule uses the Niri/Mutter-style DPI heuristic
and a quarter-step scale. Tensorland exposes the resulting fractional value to clients, while
`wl_output` continues to receive the required rounded-up integer scale.

An optional `mode` uses `<width>x<height>` or `<width>x<height>@<refresh>`, where refresh has at
most three decimal places and is compared in exact millihertz. With a configured resolution but no
refresh, Tensorland selects its highest supported progressive refresh; with an exact refresh, it selects
only that connector mode. With no `mode` rule, Tensorland keeps the DRM-preferred/native resolution but
still selects its highest supported progressive refresh. This deliberately avoids the common EDID
case where a high-refresh monitor marks only its 60 Hz timing as `PREFERRED`. An unavailable rule is
logged and falls back to that native high-refresh policy rather than selecting an arbitrary mode.

An optional `position x=… y=…` child places the output in logical compositor space (Niri-style:
configured positions are applied first; automatic placement packs remaining heads left-to-right and
rejects overlapping configured origins). `enabled=#false` keeps the connector discovered but out of
the scanout plan. `max-refresh-millihertz` caps automatic refresh selection without overriding an
exact `@refresh` mode when that mode exists.

Redraw scheduling is output-local: workspace content and pointer motion target only the affected
head so dual high-refresh layouts do not resubmit every CRTC on each commit. Empty secondary heads
skip GPU work after their first page flip has armed the CRTC.

The focused view receives the standard `xdg_toplevel` `Activated` state and a compositor-rendered
outer focus ring. The ring is scene data, not a client-side decoration, so it remains visible for
native Wayland and rootless XWayland applications without covering client pixels. `appearance`
owns the global `focus-ring`: `enabled` defaults to `true`, `width` defaults to four
logical pixels, and `color` defaults to `#7fc8ff`. Colors accept `#RRGGBB` or `#RRGGBBAA`; a zero
width or transparent color also produces no ring. The ring is rounded and clipped in physical
output coordinates at frame extraction, matching the same fractional-scale rules as client content.
Its inner radius follows the focused view's corner radius and its outer radius expands by the ring
width, so rounded clients never receive a rectangular focus artifact. The value-only appearance
object is the future theming boundary rather than a renderer-specific decoration API. Rendering keeps
the ring behind that view's client tree (including popups) and behind any later stacked view, while
the software cursor remains above all of them.

`appearance.window-corners` accepts a bounded logical `radius` and defaults to zero. The resolved
radius is clamped to half of each individual window's smaller dimension, then shared by client-image
coverage, the focus-ring hole, and the analytic shadow shape. It is therefore one scene semantic,
not three renderer-specific approximations, and it adds no pass or descriptor work.

`appearance.window-shadow` controls the compositor-owned shadow for native Wayland and rootless
XWayland views. It is disabled by default; when enabled, offsets, `blur-radius`, and `spread` are
logical pixels and `color` accepts the same `#RRGGBB`/`#RRGGBBAA` form. The configuration boundary
rejects any geometric value beyond 100000 logical pixels. Scene extraction resolves this policy
into `ShadowStyle`, damage includes its complete support, and the descriptor-free analytic Gaussian
shader draws it immediately before that view's focus ring and client tree. It therefore preserves
the default single-pass path and does not allocate a retained target. Appearance changes hot-reload
atomically and invalidate output content without changing Wayland objects or stable view IDs.

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

`render-device` constrains the common Vulkan and Tensorland tty device. Either a primary node (`cardN`) or
render node (`renderDN`) is accepted. Tensorland resolves its major/minor identity, selects only the
matching Vulkan physical device, requires its paired node, and passes the selected render node to
the tty backend. When omitted, Vulkan capability filtering and `gpu` ranking choose the pair.
`TENSOR_RENDER_DEVICE` overrides the file. A node that is not reported by Vulkan or unavailable to
the active libseat session fails startup.

`systemd` accepts `auto`, `enabled`, or `disabled`. The default `auto` mode activates when
the `systemd` Cargo feature is present and `NOTIFY_SOCKET`, `SYSTEMD_EXEC_PID`, or `MANAGERPID`
identifies a user-manager launch. Without that feature, `auto` resolves to the direct path while
`enabled` fails startup. Explicit configuration never manufactures unavailable integration.

`xwayland` defaults to `true`. It starts Tensorland's rootless XWayland process, awaits the displayfd
through a Compio completion, then starts the direct XWM on the compositor thread. X11 event
notification uses a one-shot io_uring poll operation with explicit CQE rearm. Runtime property
reads use a dedicated Compio X11 connection and fixed-capacity completion bridge.
Normal rootless X11 windows enter the same Wayland surface, ECS, and Vulkan scene path as native
clients. This is not an X11 backend: Tensorland rejects primary X11 sessions, keeps layout coordinates
authoritative, and does not provide an X11 session entry. Override-redirect X11 menus and tooltips
are accepted only after XWM mapping, xwayland-shell association, and a managed `WM_TRANSIENT_FOR`
ancestor are all known; they render as popup content of that root view rather than independent
layout views. Normal X11 `WM_TRANSIENT_FOR` dialogs instead retain their own ECS/input/render node
while attaching to their immediate managed owner: their requested logical size is constrained and
centered over that owner, and X11 position requests are ignored. An unresolved owner keeps the
dialog outside the scene rather than creating a global X11 placement fallback.

The `environment` node extends the session publication snapshot applied to launched children. Each
`clear` child removes
matching names from the inherited process environment before the managed set is written. `set` adds
or replaces user variables. Session-owned names (`WAYLAND_DISPLAY`, `DISPLAY`, `XDG_CURRENT_DESKTOP`,
`XDG_SESSION_TYPE`, `TENSOR_IPC_SOCKET`) cannot be cleared or overridden from this node; Tensorland keeps
the compositor-published values authoritative.

The `cursor` node intentionally uses Niri's cursor vocabulary and defaults: `xcursor-theme`
defaults to `"default"`, and `xcursor-size` defaults to 24 logical pixels. The configured nominal
size is multiplied by each output's physical scale, then the closest raster group is selected once
and retained. `hide-when-typing` is a presence-only node that hides the pointer after a key press
until pointer or tablet activity; an active tablet tool is not hidden by typing. The optional
`hide-after-inactive-ms` hides pointer and tablet overlays after the same pointer/tablet activity
set used by Niri (motion, buttons, axes, proximity and tip events). Activity restores visibility
without changing Wayland focus.

The final theme and size are also published as `XCURSOR_THEME` and `XCURSOR_SIZE` to session
clients. Cursor animation and inactivity share one timerfd submitted through the existing
io_uring completion path. Motion updates only a value deadline and keeps an already-earlier arm,
so high-rate pointer input does not perform a timerfd rearm syscall per sample; a harmless early
wake validates and rearms the latest deadline. No timer polling or frame-time theme lookup is used.

The `debug` node is for development only. `frame-stats` logs per-output submit latency at info level.
`force-full-redraw` disables output-local redraw targeting so every CRTC is resubmitted on each
workspace or pointer damage path; use it only when isolating scheduling bugs.

Each `spawn-at-startup` node contains direct argv arguments: one executable followed by zero or more
arguments.
Entries run only for `--session` startup. Tensorland first prepares the runtime, installs
`WAYLAND_DISPLAY`, `XDG_CURRENT_DESKTOP`, `XDG_SESSION_TYPE`, `TENSOR_IPC_SOCKET`, and the allocated
XWayland `DISPLAY` when enabled, then waits for an active systemd user manager to accept the same
snapshot and publishes readiness. Inherited session values are cleared before this publication, so
disabling XWayland cannot leak a host `DISPLAY` into children. Only then does the one-shot autostart
gate queue commands in configuration order on the asynchronous launch worker. Process creation and
optional systemd scope setup complete off the compositor thread; outcomes are logged when the
completion bridge drains. `--check`, ordinary non-session startup, environment-sync failure, and
readiness failure launch none of them.

Values are passed directly to the executable: Tensorland does not invoke a shell, expand variables, or
interpret pipes and redirections. Use a dedicated executable when orchestration is more complex
than argv.

`ConfigTransaction` is the cold-path commit boundary for reload. It replaces the active value and
increments its generation only after the entire candidate loads, decodes, validates, and applies
environment overrides. Rejection preserves both the last valid value and generation. Startup may
use defaults when its KDL file does not exist; reload deliberately uses a required-file loader, so
an editor rename window or deleted file is an I/O rejection rather than a default-config commit.

The transaction retains the complete diagnostic for logs and materializes source-free, bounded
metadata containing category, path, error code, line, column, short summary, and the
`tensorland --validate-config` remediation command. Tensorland watches the nearest existing
ancestor of the resolved path recursively, so a missing `$XDG_CONFIG_HOME/tensor` directory can be
created later and atomic editor replacement is observed without Tensorland creating user directories.
The callback only coalesces a value-only reload request into a one-entry queue. A dedicated worker
performs file I/O, KDL parsing, typed validation, and environment overrides; its two-entry outcome
bridge is drained on the compositor turn before IPC requests.

Layout kind/options, appearance, cursor, and debug policy apply live as one transaction. Changes to
`ipc-socket`, `gpu`, `render-device`, `output`, `systemd`, `xwayland`, `spawn-at-startup`, or
`environment` are rejected with `reload_requires_restart` until their startup-owned resources have
an atomic replacement path. Rejection preserves the old generation and live state. Use
`tensorctl reload-config` for an explicit request and `tensorctl get-config-status` to inspect the
active generation and last bounded failure.

The current request/reply IPC never puts complete KDL source on the wire. A versioned event
subscription is still required for unsolicited failure notification. `tensor-shell`, not the
compositor renderer, will own that transient visual and accessible notification. This mirrors
Niri's useful split: the on-screen notice is short while its `validate` command and logs carry the
detailed miette report.
