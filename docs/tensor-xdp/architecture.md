# Tensor XDP architecture

Tensor XDP is a session-bus activated process with three strict ownership
edges:

1. `tensor-dbus` owns Compio-native D-Bus framing, authentication, ancillary
   file descriptors, and bounded messages. Tensor XDP owns portal method policy.
2. Tensor Shell owns consent and chooser surfaces. Portal methods that require
   visible user decisions will exchange bounded value requests with Shell; the
   backend will not embed a second desktop UI toolkit.
3. Tensorland owns capture sources, input seats, Wayland resources, ECS state,
   renderer resources, and KMS. ScreenCast, Screenshot, RemoteDesktop, and
   GlobalShortcuts may use explicit versioned compositor commands and stable
   IDs, never internal handles.

The service uses the same completion model as the rest of TensorDE. It creates
one capacity-sized io_uring Compio runtime and awaits session D-Bus operations;
it does not add zbus/async-io, a readiness reactor, or a private worker runtime.
Cold KDL parsing finishes before requesting the well-known bus name.

## Capability publication

`tensor.portal` is the authoritative capability list. The initial list contains
only `org.freedesktop.impl.portal.Settings` version 1. That implementation
supports exact and trailing-wildcard namespace filtering, caps filters before
allocating a result, reports unknown keys as structured portal errors, and
provides D-Bus Properties and introspection.

Future capabilities are added vertically:

- FileChooser requires a complete Tensor Shell chooser request/result and
  cancellation lifecycle; it does not restore the removed Tensor Files-owned
  backend.
- ScreenCast/Screenshot require Tensorland's standard image-copy-capture source,
  explicit permission/session state, PipeWire buffer negotiation, and bounded
  stream teardown.
- RemoteDesktop/InputCapture require creator-scoped transient seats, permission
  state, and loss-safe input revocation.

An unsupported interface remains absent so `xdg-desktop-portal` can select a
different installed backend. Tensor XDP never returns success with fabricated
or empty capability data.
