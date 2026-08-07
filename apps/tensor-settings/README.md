# Tensor Settings

Tensor Settings is a standalone ordinary Wayland application. It owns settings
navigation, search, validation previews, atomic edits, and privileged-change
confirmation. Tensor Shell owns only the panel/control-center entry that
launches it.

Its own configuration is typed KDL at
`$XDG_CONFIG_HOME/tensor/settings.kdl`. Product configuration remains owned by
each product. Settings uses product-specific schemas and reload routes; it does
not create a central settings daemon or proxy every product through Tensor WM.

The application model now loads every product endpoint, supports
product navigation/search, tracks clean, dirty, invalid, read-only, and
unsupported states, and validates KDL drafts before saving. Tensor Shell,
Launcher, Greeter, XDP, and Idle use typed previews whose bounds and enum
values mirror their runtime loaders; Tensorland keeps structural validation
until its much larger compositor schema is exposed as an editor model. Tensor
Files uses its typed `tensor/files.kdl` schema here too, so its draft is editable
and validated with the same field and enum rules as the runtime loader.

Writes use a same-directory temporary file, file and directory durability
barriers, and atomic replacement. A draft is rejected if the source changed on
disk after it was opened, and the application-level read-only and privileged
confirmation policies are enforced before I/O. A saved Land draft can borrow a
caller-owned `tensor-ipc` Compio client to request `ReloadConfig`; Settings
never creates a runtime, and accepted reloads remain observable through
Tensorland's versioned event stream. Unsupported product routes fail
explicitly instead of reporting a false reload.

Running `tensor-settings` opens an ordinary Wayland/Vulkan surface backed by
the shared strict presenter. Its surface controller supports product
filtering, keyboard and pointer navigation, KDL draft editing through
text-input-v3, UTF-8 cursor movement, Ctrl+S atomic save, privileged-change
confirmation, per-document state preview, and F5 reload from disk without
moving product configuration ownership into the UI. A saved Land draft can
request the owning compositor to reload through the caller-owned Compio IPC
client. Shared text/icon atlas rendering remains a visual-slice task;
`tensor-settings --check` loads this workspace and reports invalid product
documents. The model and reload transactions remain independent of renderer
code.
For integration diagnostics, `tensor-settings --reload-land` sends the same
versioned `ReloadConfig` request using a caller-owned Compio runtime.
