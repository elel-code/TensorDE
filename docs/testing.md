# Testing

Tensor borrows test strategy from Niri and Hyprland without copying implementation code.

- Pure layout/state tests cover empty, singleton, uneven, invalid, and boundary inputs.
- ECS tests assert stable ordering and idempotent transitions rather than Bevy internals.
- KDL tests separate valid documents, malformed syntax, schema errors, includes, and reload races.
- IPC tests cover fragmented reads, multiple frames per read, malformed/oversized input, request-ID
  round trips, permissions, and socket ownership.
- Nested Wayland tests are added when globals and dispatch state exist; pure geometry never requires
  a compositor session.
- Vulkan tests are capability-gated and must report a missing descriptor heap explicitly.
- Scene snapshots are appropriate when many coordinates or render decisions form one behavior.

Every change runs:

```sh
cargo fmt --all -- --check
./scripts/check-file-lines.sh
cargo test --workspace --all-targets
```

