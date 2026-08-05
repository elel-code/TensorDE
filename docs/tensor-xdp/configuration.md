# Tensor XDP configuration

Tensor XDP loads typed KDL before connecting to the session bus. The resolution
order is `TENSOR_XDP_CONFIG`, `$XDG_CONFIG_HOME/tensor/xdp.kdl`,
`$HOME/.config/tensor/xdp.kdl`, then `/etc/tensor/xdp.kdl`. A missing file uses
neutral defaults; malformed, unreadable, unknown, or larger-than-1-MiB input is
an explicit startup error.

Use `tensor-xdp --check [--config PATH]` for configuration-only validation.
That mode never creates a D-Bus or PipeWire connection.

```kdl
appearance color-scheme="no-preference" contrast="normal" reduced-motion=#false
```

`color-scheme` accepts `no-preference`, `dark`, or `light`. `contrast` accepts
`normal` or `high`. `reduced-motion` is a boolean. They lower to the standard
`org.freedesktop.appearance` XDP unsigned integer values; they are portal-facing
host preferences, not compositor render controls.

Tensorland output/layout/effect policy remains in `tensor/config.kdl`, and
Tensor Shell panel/surface policy remains in `tensor/shell.kdl`. Tensor XDP does
not parse TOML or duplicate those products' settings.
