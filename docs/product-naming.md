# Tensor product naming

TensorDE uses one product family rather than unrelated application brands. The
public application names follow the same model as the Hyprland ecosystem: the
compositor carries the distinctive family name and companion applications use
the `Tensor` prefix plus their role.

| Product role | Product name | Cargo package / executable | Repository path |
| --- | --- | --- | --- |
| Wayland compositor | Tensorland | `tensorland` | `apps/tensorland` |
| Desktop shell | Tensor Shell | `tensor-shell` | `apps/tensor-shell` |
| File manager | Tensor Files | `tensor-files` | `apps/tensor-files` |
| Scene wallpaper engine | Tensor Wallpaper | `tensor-wallpaper` | `apps/tensor-wallpaper` |

The compositor companion commands are `tensorctl`, `tensorland-session`, and
`tensorland-dmabuf-smoke`. The wallpaper product made a coordinated hard cutover from
Gilder to Tensor Wallpaper. Its only active commands are `tensor-wallpaper`,
`tensor-wallpaper-convert`, `tensor-wallpaperd`, and `tensor-wallpaperctl`; its only active
application path is `apps/tensor-wallpaper`.

## Stable family interfaces

The rename does not change shared `tensor-*` crate names, KDL vocabulary,
the compositor's `TENSOR_*` environment variables, the `tensor.sock` IPC default, or durable
correctness/evidence roots under `references/fika`, `references/tensor`, and
`artifacts/tensor`. Those names identify the TensorDE family or retained source
evidence rather than a public application executable.

System integration uses the new compositor identity directly:

- `tensorland.desktop`, `DesktopNames=Tensorland`, and `Exec=tensorland-session`;
- `tensorland.service` and `tensorland-shutdown.target`;
- `tensorland --session` as the compositor service command.

Tensor Files uses `TENSOR_FILES_*` for its product-specific runtime controls,
`org.tensorde.TensorFiles1.Privileged` for its privileged D-Bus API, and
`org.freedesktop.impl.portal.desktop.tensor_files` for its FileChooser portal
backend. Its Wayland app-id is `org.tensorde.TensorFiles`.

TensorDE is pre-release, so source paths, Cargo package names, executable names,
and system integration make a coordinated hard cutover. Do not add duplicate
old binaries, packages, service units, or hidden runtime fallbacks. A diagnostic
may mention the replacement command when rejecting an obsolete invocation or
configuration, but there is only one active implementation.
