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
│ Protocol state by class (native/protocols)  │  ← replace SCTK Handlers
│   core | stable | staging | unstable        │
│   ext  | community/wlr                      │
├─────────────────────────────────────────────┤
│ Wire / object map                           │  ← wayland-client + protocols*
├─────────────────────────────────────────────┤
│ Display I/O                                 │  ← Compio PollFd
└─────────────────────────────────────────────┘
```

### Protocol classes (Smithay / wayland-protocols style)

| Class | Upstream tree | Policy |
| --- | --- | --- |
| **core** | `wayland.xml` (`wl_*`) | Required; missing = cannot run shell |
| **stable** | `wayland-protocols` `stable/` | Baseline desktop (xdg-shell, viewporter) |
| **staging** | `wayland-protocols` `staging/` | Optional capability; version-cap binds |
| **unstable** | `wayland-protocols` `unstable/` | Legacy only; prefer staging replacements |
| **ext** | `wayland-protocols` `ext/` | Optional (`ext_*`) |
| **community** | wlr / plasma / … | Optional; never block core startup |

Code layout: `src/native/protocols/{core,stable,staging,unstable,ext,community/wlr}/`.

`ProtocolClass`, `ProtocolSpec`, and `FIKA_PROTOCOL_MATRIX` document which globals
Fika needs and how hard they are. Implementations land module-by-module in the
matching class directory (not a flat `handlers_*.rs` dump).

| Module (target) | Responsibility |
| --- | --- |
| `display_io` | Dup of `wl_display` fd, Compio readable/writable wait |
| `native/connection` | Connect, flush, readiness |
| `native/registry` | Global list + late add/remove |
| `native/pump` | Compio read/dispatch loop |
| `native/protocols/*` | State machines per protocol class |
| `transfer` (public) | Clipboard + DnD MIME model |
| `runtime` (SCTK→native) | Orchestration, event buffer, capabilities |

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
| 1 | Compio display readiness + non-blocking dispatch helpers | Done |
| 2 | Native connection + registry + Compio pump (no shell yet) | In progress |
| 2b | Core shell without SCTK (compositor/shm/xdg/seat) | Planned |
| 3 | Extended protocols | Planned |
| 4 | Remove SCTK/calloop | Planned |

### Phase 2 modules (`src/native/`)

| Module | Role |
| --- | --- |
| `connection` | `wayland_client::Connection` + `DisplayReadiness` |
| `registry` | `registry_queue_init`, global snapshot, late global tracking |
| `pump` | `flush → prepare_read → await readable → read → dispatch_pending` |

Public types: `NativeConnection`, `NativeRegistry`, `NativePump`, `GlobalAdvertisement`.

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
