# Tensor Shell Configuration

Tensor Shell owns a separate typed KDL configuration from Tensorland. The
default path is `$XDG_CONFIG_HOME/tensor/shell.kdl`, falling back to
`$HOME/.config/tensor/shell.kdl` and then `/etc/tensor/shell.kdl`.
`TENSOR_SHELL_CONFIG` selects an explicit path. A missing file uses defaults;
an unreadable or invalid file is a startup error.

The complete example is [../../apps/tensor-shell/examples/config.kdl](../../apps/tensor-shell/examples/config.kdl).

The `layout` node controls shell-owned layer-surface dimensions. The `panel`
node contains ordered `left`, `center`, and `right` widget arguments. Omitting
a section keeps its default; declaring an empty section hides it. Known widget
names are `launcher`, `workspaces`, `active-window`, `media`, `system-status`,
`clock`, `notifications`, and `control-center`. A widget may occur only once.

The optional `tensorland` node identifies the compositor integration endpoint:

```kdl
tensorland {
    config-path "/etc/tensor/config.kdl"
    ipc-socket "/run/user/1000/tensor.sock"
}
```

Without overrides, the config path follows Tensorland's `TENSOR_CONFIG` and
XDG resolution, and the socket follows `TENSOR_IPC_SOCKET` then
`$XDG_RUNTIME_DIR/tensor.sock`. Tensor Shell settings remain in `shell.kdl`;
compositor output, layout, appearance, and effect settings remain in
Tensorland's `config.kdl`; idle deadlines remain in Tensor Idle's `idle.kdl`.
The standalone Tensor Settings application will edit these product-owned files
and use each product's versioned reload/status transaction rather than creating
a second configuration dialect inside Shell.

There is deliberately no Tensor settings daemon. Panel applets are built into
Tensor Shell and receive bounded snapshots from the system services that own
the underlying state. Tensor Settings only edits product-owned typed KDL and
requests the owning product's reload; it does not stay resident to keep panel
state alive.

When `system-status` is present in the panel order, Tensor Shell starts its
signal-driven UPower adapter on the system bus. There is no polling interval or
backend selector to configure. A machine without UPower remains a supported
configuration: the retained applet becomes `unavailable` while Shell startup
and the rest of the panel continue normally. Battery percentage is bounded to
0–100; charging, low, critical/action and no-battery states are lowered before
the renderer sees them.
