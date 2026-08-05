# Tensor Settings

Tensor Settings is a standalone ordinary Wayland application. It owns settings
navigation, search, validation previews, atomic edits, and privileged-change
confirmation. Tensor Shell owns only the panel/control-center entry that
launches it.

Its own configuration is typed KDL at
`$XDG_CONFIG_HOME/tensor/settings.kdl`. Product configuration remains owned by
each product. Settings uses product-specific schemas and reload routes; it does
not create a central settings daemon or proxy every product through Tensor WM.

`tensor-settings --check` currently validates the application configuration
and product endpoint registry. The native Vulkan/Wayland settings surface,
schema-backed editors, atomic KDL writes, and reload/diagnostic transactions
remain the next vertical slices.
