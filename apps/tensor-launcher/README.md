# Tensor Launcher

Tensor Launcher is TensorDE's standalone, native launcher product. Its catalog
and scoring model are process-owned; Tensor Shell keeps only the panel entry and
visibility coordination. Tensorland remains the authority for activation-aware
launch submission.

Catalog discovery scans XDG `applications` directories only on the cold path,
applies desktop-file precedence, `Hidden`, `NoDisplay`, `OnlyShowIn`,
`NotShowIn`, and `TryExec`, resolves localized names and descriptions, retains
normalized search text, and writes at most 64 ranked results into a reusable
vector. The query path does not rescan the filesystem or execute an unbounded
fuzzy candidate pass.

Configuration is KDL 2.0 at
`$TENSOR_LAUNCHER_CONFIG`, `$XDG_CONFIG_HOME/tensor/launcher.kdl`, or
`/etc/tensor/launcher.kdl`:

```kdl
max-results 12
max-catalog-entries 32768
max-diagnostics 32
application-directory "/home/me/.local/share/applications"
application-directory "/usr/share/applications"
```

Application execution is deliberately not configured here. Tensor Launcher
parses Desktop Entry `Exec` values into argv without invoking a shell, expands
standard no-file field codes, and wraps `Terminal=true` entries with
`xdg-terminal-exec`. Absolute Desktop Entry `Path` values cross versioned IPC
with the argv and become the child working directory. Relative paths fail at
catalog load and again at the compositor trust boundary. Tensor Launcher uses
the caller-driven Compio client in `tensor-ipc`; Tensor WM issues the Wayland
activation token and owns its bounded process/systemd launch worker.

Tensor Shell's panel entry does not embed a second launcher surface. Its typed
KDL retains a bounded launcher argv and submits that argv through the same
Tensorland `Spawn` path, so panel invocation starts this standalone product
with compositor-issued activation rather than direct process creation in the
Shell event loop.

Running `tensor-launcher` opens its ordinary Wayland surface. The retained
controller consumes configure/scale, keyboard, pointer, and text-input-v3
events, presents bounded result geometry through the shared strict Vulkan
presenter, and submits the selected launch plan over Tensorland IPC. A caller-
owned Compio runtime performs that IPC transaction. `tensor-launcher --surface
<query>` opens the same surface with an initial query.

`tensor-launcher --check` validates configuration and discovery. Passing text
prints the current bounded application matches. `tensor-launcher --launch
<query>` submits the highest-ranked result through the same path the native UI
will use. `tensor-launcher --watch` keeps a bounded filesystem fingerprint and
refreshes the retained catalog from the caller-owned Compio loop; transient
refresh failures keep the last usable catalog. `LauncherSession` retains the
native surface's query, UTF-8 cursor edits, text-input-v3 preedit, bounded
results, selection, and launch plan without renderer-owned application state.
The first Vulkan surface slice draws retained query/result/selection geometry;
shared text and icon atlas rendering remains the next visual slice rather than
being hidden behind an SHM or CPU fallback.
