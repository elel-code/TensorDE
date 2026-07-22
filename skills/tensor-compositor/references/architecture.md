# Tensor Architecture Reference

## ECS Boundary

The ECS world owns compositor intent: view IDs, workspace membership, focus state, geometry, and
render extraction data. Smithay surfaces, Wayland clients, file descriptors, and Vulkan handles
remain in protocol/renderer owners. Systems translate external events into component changes and
extract a compact immutable scene for Vulkan submission.

## Capability Gates

Required gates fail startup with actionable errors. Optional gates (systemd and xdg-desktop-portal)
are compiled and registered only when enabled. No fallback renderer or legacy IPC protocol should
be introduced before the core design has stabilized.

## Reference Worktrees

See `references/SOURCES.md` for the pinned local snapshots of Niri, Hyprland, Nourish, and Bevy.
Use those trees for behavior and lifecycle research, not as dependencies or code to copy wholesale.
