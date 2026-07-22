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
layout "scrolling-1d"
ipc-socket "/run/user/1000/tensor.sock"
gpu "discrete"
# Optional DRM primary or render node. Without this, Smithay selects the seat's primary GPU.
# render-device "/dev/dri/renderD128"
systemd "auto"
xwayland true
spawn-at-startup "waybar"
spawn-at-startup "foot" "--server"
```

The initial layout surface has three layout families:

- `scrolling-1d` keeps views on one navigable axis and is the default policy.
- `spatial-2d` places views in a two-dimensional tiled arrangement.
- `master-stack` provides the traditional master-and-stack arrangement.

Floating, fullscreen, and monocle behavior are workspace or view state modifiers, not additional
layout families. `tabbed` is reserved for a later container-tree extension rather than being added
as an early compatibility mode.

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
`NOTIFY_SOCKET`, `SYSTEMD_EXEC_PID`, or `MANAGERPID` identifies a user-manager launch. Explicit
configuration overrides detection without changing the optional Cargo feature boundary.

`xwayland` defaults to `true`. It starts the rootless XWayland process and calloop source when the
compositor enters its event loop. This is not an X11 backend: Tensor rejects primary X11 sessions.

Each `spawn-at-startup` node contains one executable followed by zero or more arguments. Entries run
only for `--session` startup, after environment publication and readiness notification. Values are
passed directly to the executable: Tensor does not invoke a shell, expand variables, or interpret
pipes and redirections. Use a dedicated executable when orchestration is more complex than argv.

Future reloads parse and validate off the event-loop critical path. A failed reload must preserve
the last valid configuration and report a structured error through logs and IPC.
