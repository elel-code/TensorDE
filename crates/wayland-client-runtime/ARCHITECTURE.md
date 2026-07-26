# Architecture

Performance-first, architecture-first. The crate is a **native Wayland client
protocol stack** with an **optional Compio event loop**. Protocol state never
depends on a specific reactor so other projects can embed it under calloop,
winit, tokio, or a custom poll loop.

## Layering

```text
┌──────────────────────────────────────────────────────────┐
│ Public types                                             │
│ Event, SurfaceId, TransferContent, capabilities, …       │
├──────────────────────────────────────────────────────────┤
│ Protocol layer (always on; no async runtime)             │
│   NativeConnection · NativeShell · NativePump            │
│   try_read_and_dispatch / dispatch_pending / drain_*     │
│   native/protocols/{core,stable,staging,unstable,ext,wlr}│
├──────────────────────────────────────────────────────────┤
│ Compio adapter (feature = "compio", default ON)          │
│   NativeRuntime · CompioFdReady · WakeHandle             │
│   (readiness only — not the Wayland read path)           │
└──────────────────────────────────────────────────────────┘
```

### Fds and readiness

- The display socket is a **plain non-blocking fd** (`O_NONBLOCK` enforced at
  connect). Protocol `read` always goes through `wayland-client`.
- Compio does **not** need a special “PollFd protocol”. It only needs a
  long-lived readiness watch (`CompioFdReady`) so the proactor can complete
  when that ordinary fd becomes readable. Name history: Compio’s type is
  `PollFd`; it submits io_uring poll-add style ops, not a userspace `poll`
  loop.
- Hot path: construct `CompioFdReady` **once** per fd (connect time), then
  wait → protocol `try_read` / `prepare_read`+`read` → dispatch.

### Protocol vs event loop

| API | Needs Compio? | Role |
| --- | --- | --- |
| `NativeShell::connect_to_env` | no | Bind globals, own surfaces/input |
| `NativeShell::display_fd` | no | Ordinary non-blocking fd for *your* loop |
| `NativeShell::try_read_and_dispatch` | no | Non-blocking read + dispatch |
| `NativeShell::dispatch_pending` | no | Drain already-queued messages |
| `NativeShell::drain_events` | no | Consume protocol events |
| `NativePump::pump_pending` | no | Registry-only pending pump |
| `NativeShell::pump_once` / `NativePump::pump_once` | **yes** | Wait (reused watch) + read |
| `NativeRuntime` / `Runtime` | **yes** | Full public API + Compio waits |

Disable the loop dependency:

```toml
wayland-client-runtime = { version = "0.1", default-features = false }
```

### Protocol classes (Smithay / wayland-protocols style)

| Class | Upstream tree | Policy |
| --- | --- | --- |
| **core** | `wayland.xml` (`wl_*`) | Required |
| **stable** | `wayland-protocols` `stable/` | Baseline desktop |
| **staging** | `wayland-protocols` `staging/` | Optional capability |
| **unstable** | `wayland-protocols` `unstable/` | Legacy |
| **ext** | `wayland-protocols` `ext/` | Optional |
| **community** | wlr / … | Optional |

Layout: `src/native/protocols/{core,stable,staging,unstable,ext,community/wlr}/`.

`ProtocolClass`, `ProtocolSpec`, and `PROTOCOL_MATRIX` (alias
`FIKA_PROTOCOL_MATRIX`) document which globals this crate understands.
Implementations live under `native/shell` with dispatch split by concern
(`dispatch_*.rs`).

## North star (with Compio)

```rust
// Fika / default feature path
loop {
    runtime.dispatch(timeout)?;
    runtime.drain_events_into(&mut events);
    for event in events.drain(..) { /* … */ }
}
```

## North star (without Compio)

```rust
// External loop owns readiness
loop {
    // wait until shell.display_fd() is readable (or a wake fd)
    shell.try_read_and_dispatch()?;
    for event in shell.drain_events() { /* … */ }
}
```

## NativeShell capability matrix

| Area | Status |
| --- | --- |
| Protocol shell without Compio | yes |
| Compio display pump (optional) | yes (`feature = "compio"`) |
| xdg toplevel + dialog + CSD | yes |
| popup + layer shell | yes |
| fractional scale + viewporter | yes |
| pointer / keyboard / touch / gestures | yes |
| xkb composed text | yes |
| text-input-v3 (full state + cursor rect) | yes |
| clipboard + DnD | yes |
| blur / activation / icons | yes |
| wp_presentation | yes |
| raw-window-handle 0.6 (wgpu → Vulkan) | yes |

### wgpu

Fika uses wgpu with `Backends::VULKAN` (GL fallback). The same
`SurfaceHandle` feeds wgpu's Wayland surface creation; no separate ash path
is required for the app.

## Testing

- Unit tests: pure helpers and protocol smokes (headless OK when no display).
- Examples: `native_toplevel_smoke` (Compio), capabilities listing.
- `cargo test -p wayland-client-runtime --no-default-features` must stay green.
- Fika: workspace `cargo test` / release run with default `compio` feature.
