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
  retained by the model or exposed through `Debug`;
- caller-driven Compio Unix transport with retained completion operations and
  an explicit unusable state after a cancelled protocol exchange;
- bounded AccountsService discovery over `tensor-dbus`, with concurrent typed
  property requests, login-policy filtering, and stable display-name ordering;
- a complete greetd transaction that maps create-session, visible/secret/info/
  error prompts, direct responses, authentication results, start-session, and
  cancel-session into generation-tagged model transitions. A successful
  start-session advances the model to a terminal session-started state, so a
  duplicate request cannot be sent on the same transaction.

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
or own compositor/DRM/Vulkan objects. Normal initialization resolves the
configured user bound through AccountsService and constructs the complete
user/session model. `tensor-greeter --check-greetd` verifies
the completion-native socket path without starting an authentication attempt.
Running `tensor-greeter` opens an ordinary Wayland/Vulkan login surface. It
retains bounded user and session selection, routes visible/secret/info/error
prompts through the complete `GreeterTransaction`, clears the temporary
response buffer after each greetd exchange, and starts the selected configured
session after authentication. The surface uses the shared strict Vulkan
presenter; text/icon atlas rendering and richer session cards remain a later
visual slice. `tensor-greeter --check` validates KDL without requiring
AccountsService or greetd to be available.
