# Tensor Greeter

Tensor Greeter is TensorDE's standalone greetd frontend. Like DMS's separate
greeter product, it is not part of the desktop shell process. Unlike the
session lock, it normally runs before Tensorland starts and therefore does not
bind Tensorland's `ext-session-lock-v1` global.

The implemented product core provides:

- typed KDL configuration and deterministic Tensorland session defaults;
- bounded user/session state and generation-tagged authentication attempts;
- exact greetd length-prefixed JSON values with fragmented-frame decoding;
- a zeroed outbound frame type so passwords and challenge answers are never
  retained by the model or exposed through `Debug`.

Configuration resolves from `$TENSOR_GREETER_CONFIG`,
`$XDG_CONFIG_HOME/tensor/greeter.kdl`, then `/etc/tensor/greeter.kdl`:

```kdl
greetd-socket "/run/greetd.sock"
max-users 128
max-auth-message-bytes 4096

session "tensorland" {
    label "Tensorland"
    command "tensor-session"
    environment "XDG_CURRENT_DESKTOP=Tensorland" "XDG_SESSION_TYPE=wayland"
}
```

Commands are argv lists, never shell strings. PAM and session creation remain
owned by greetd; Tensor Greeter does not read `/etc/shadow`, host a PAM stack,
or own compositor/DRM/Vulkan objects. The next vertical slice adds Compio Unix
transport and an ordinary Wayland/Vulkan login surface around this tested core.
