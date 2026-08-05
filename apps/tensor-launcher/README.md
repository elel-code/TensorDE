# Tensor Launcher

Tensor Launcher is TensorDE's standalone, native launcher product. Its catalog
and scoring model are process-owned; Tensor Shell keeps only the panel entry and
visibility coordination. Tensorland remains the authority for activation-aware
launch submission.

The implemented first slice scans XDG `applications` directories only on the
cold path, applies desktop-file hiding and precedence rules, retains normalized
search text, and writes at most 64 ranked results into a reusable vector. The
query path does not rescan the filesystem and does not use an unbounded fuzzy
candidate list.

Configuration is KDL 2.0 at
`$TENSOR_LAUNCHER_CONFIG`, `$XDG_CONFIG_HOME/tensor/launcher.kdl`, or
`/etc/tensor/launcher.kdl`:

```kdl
max-results 12
max-catalog-entries 32768
max-diagnostics 32
systemd-mode "auto"
application-directory "/home/me/.local/share/applications"
application-directory "/usr/share/applications"
```

`systemd-mode` is `auto`, `enabled`, or `disabled`. Auto uses a user scope when
available; enabled treats unavailable systemd integration as an error; disabled
uses direct argv launch. All modes preserve the Wayland activation token and
none invoke a shell.

`tensor-launcher --check` validates configuration and discovery. Passing text
prints the current bounded application matches. The native Vulkan surface,
text/input scene, catalog watcher, and shared activation IPC client are the next
vertical slice; the headless query command is diagnostic, not a second UI path.
