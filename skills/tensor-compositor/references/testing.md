# Tensor Test Patterns

The local Niri and Hyprland worktrees are behavioral references, not dependencies.

## Pure State and Layout

- Prefer table-driven unit tests for empty, singleton, uneven, invalid, and boundary values.
- Assert invariants such as conservation of output area, stable ordering, and idempotent state
  transitions.
- Keep geometry tests independent of Wayland, DRM, and GPU availability.

## Configuration and Watchers

- Parse representative KDL documents and malformed documents separately.
- Use `tempfile` directories for path resolution, missing files, includes, and replacement races.
- Test that a failed reload leaves the last valid configuration active.

## IPC

- Test fragmented reads, multiple frames per read, zero-length and oversized frames, malformed JSON,
  request-ID preservation, and socket ownership on drop.
- Treat protocol version and structured error shape as explicit assertions.

## Integration

- Add nested Wayland tests only after protocol globals exist; do not make geometry tests depend on a
  running compositor.
- Keep Vulkan tests capability-gated and report the missing descriptor-heap feature explicitly.
- Snapshot final extracted scene data when a visual regression is more useful than many individual
  coordinate assertions.

