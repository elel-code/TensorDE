# Configuration

Tensor uses KDL parsed by `knuffel`. KDL is chosen for readable nested blocks, repeated rules, and
window/output matchers. Schema changes may be breaking while the project is pre-release.

Configuration path precedence is:

1. `--config PATH`
2. `TENSOR_CONFIG`
3. `$XDG_CONFIG_HOME/tensor/config.kdl`
4. `$HOME/.config/tensor/config.kdl`
5. `/etc/tensor/config.kdl`

`TENSOR_LAYOUT` and `TENSOR_IPC_SOCKET` are development overrides applied after file parsing.
The current schema is intentionally small:

```kdl
layout "niri-1d"
ipc-socket "/run/user/1000/tensor.sock"
gpu "discrete"
```

`gpu` defaults to `discrete`: candidates are ranked discrete GPU, integrated GPU, virtual GPU, then
CPU, and candidates without `VK_EXT_descriptor_heap` are rejected before ranking. Use `integrated`
or `any` only when the machine's topology requires it. `TENSOR_GPU` overrides the file for local
development.

Future reloads parse and validate off the event-loop critical path. A failed reload must preserve
the last valid configuration and report a structured error through logs and IPC.
