# Tensor XDP integration data

- `dbus-1/services/org.freedesktop.impl.portal.desktop.tensor.service.in` is
  the session-bus activation template. Packaging replaces `@bindir@`.
- `xdg-desktop-portal/portals/tensor.portal` advertises only completed backend
  interfaces.
- `xdg-desktop-portal/tensorland-portals.conf` is the Tensorland desktop
  preference file and installs as
  `share/xdg-desktop-portal/tensorland-portals.conf`.

Adding a Rust method without adding its complete portal lifecycle is not enough
to extend the `Interfaces` list.
