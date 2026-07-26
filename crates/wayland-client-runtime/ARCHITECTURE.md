# Architecture

Performance-first, architecture-first. The long-term goal is a **native Wayland
client stack on Compio**, without Smithay Client Toolkit (SCTK) callback
handlers and without a second reactor (calloop/Tokio).

## North star

Applications write **linear async code**:

```rust
// Pseudocode target shape (Phase 2+)
loop {
    runtime.wait_display_readable().await?;
    runtime.dispatch_pending()?;
    for event in runtime.drain_events() {
        // handle Event
    }
}
```

Wayland remains message-oriented on a socket. Compio owns readiness and
scheduling; this crate owns protocol state and the public event model.

## Layering

```text
┌─────────────────────────────────────────────┐
│ Public API                                  │
│ Runtime, SurfaceId, Event, capabilities     │
├─────────────────────────────────────────────┤
│ Shell / seat / transfer state machines      │  ← replace SCTK Handlers
├─────────────────────────────────────────────┤
│ Wire / object map / protocol bindings       │  ← wayland-client or equivalent
├─────────────────────────────────────────────┤
│ Display I/O                                 │  ← Compio PollFd (Phase 1)
└─────────────────────────────────────────────┘
```

| Module (target) | Responsibility |
| --- | --- |
| `display_io` | Dup of `wl_display` fd, Compio readable/writable wait |
| `connection` (future) | Connect, flush, read, error mapping |
| `wire` / protocols (future) | Object map, message decode, globals |
| `seats` (future) | Pointer/keyboard/touch + xkb |
| `shell` (future) | xdg toplevel/dialog/popup, layer shell |
| `transfer` | Clipboard + DnD MIME model (keep public types) |
| `runtime` | Orchestration, event buffer, capabilities |

## Migration rules

1. **Do not break the public event vocabulary** without a Fika-side migration.
2. **One executor**: Compio. No Tokio island; calloop is transitional only.
3. **No permanent dual API**: SCTK path is deleted once native covers Fika.
4. **Prefer ownership of serials, surfaces, and seats** in this crate’s types.
5. **Hot path**: reusable event drain buffers, minimal allocs in dispatch.

## Phase status

| Phase | Goal | Status |
| --- | --- | --- |
| 0 | This document + roadmap §6d | Done |
| 1 | Compio display readiness + non-blocking dispatch helpers | In progress |
| 2 | Core shell without SCTK | Planned |
| 3 | Extended protocols | Planned |
| 4 | Remove SCTK/calloop | Planned |

## SCTK surface (to eliminate)

Rough dependency map today:

- calloop + `WaylandSource` → Compio `DisplayReadiness` + native read loop
- `CompositorState` / `XdgShell` / `Window` → native shell modules
- `SeatState` / pointer / keyboard handlers → native seats + xkbcommon
- data device manager → existing transfer model + native protocol objects
- fractional scale, text input, gestures, constraints → protocol modules already
  partially isolated under `src/*.rs`

## Testing

- Unit tests: pure helpers (`DisplayReadiness` with pipes, axis mapping, …).
- Examples: capabilities / toplevel / gestures when a compositor is present.
- Fika: workspace `cargo test` and file line gate after each phase.
