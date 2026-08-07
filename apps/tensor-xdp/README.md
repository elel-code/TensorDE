# Tensor XDP

Tensor XDP is TensorDE's dedicated `xdg-desktop-portal` backend. It is a
separate product process: it owns session D-Bus portal objects and future
PipeWire coordination, while Tensorland retains Wayland, ECS, Vulkan, capture,
input, and KMS ownership.

The first complete interface is `org.freedesktop.impl.portal.Settings` version
1. It publishes the standardized appearance keys `color-scheme`, `contrast`,
and `reduced-motion`. The backend also implements the required Properties,
Introspectable, and Peer methods. Unsupported portals are absent from
`tensor.portal`; an interface is advertised only after its request, cancellation,
permission, and result lifecycles are complete.

The Settings snapshot is live. The normal service path keeps the caller-owned
Compio/io_uring runtime in one loop, checks the configured KDL once per second,
retains the last valid values when parsing fails, and emits one
`SettingChanged(namespace, key, value)` signal per changed appearance key.
D-Bus method handlers read the same retained snapshot, so a reload cannot expose
partially updated settings.

Configuration is typed KDL. Validate it without touching D-Bus:

```sh
tensor-xdp --check --config apps/tensor-xdp/examples/config.kdl
```

The D-Bus activation template installs as
`share/dbus-1/services/org.freedesktop.impl.portal.desktop.tensor.service`; the
backend descriptor installs as
`share/xdg-desktop-portal/portals/tensor.portal`. The supplied
`tensorland-portals.conf` selects only Tensor's completed Settings backend and
allows the system portal configuration to select other implementations for
capabilities that Tensor XDP does not yet advertise.
