# Event layer and runtime crates

Tensor owns compositor **event semantics**. Compio owns **async I/O as a completion
model** (submit operation → completion), with **io_uring** as the Linux product
driver. Smithay remains a mature **protocol / present adapter** until individual
bindings are replaced.

**I/O model:** Compio is **not** a readiness reactor. Do not design “register fd,
poll until readable.” Design “submit the op (read/accept/timer/wake); when it
**completes**, push a value event and run the semantic turn.” Compio's `polling`
Cargo feature is only an automatic host fallback if io_uring cannot be created.

## Performance design

| Rule | Why |
|------|-----|
| Fixed-capacity phase rings | No allocation on the input/vblank path |
| Coalesce pointer motion & per-output vblank | Device Hz must not become queue length |
| Phase order without sorting | O(1) push/pop; present cannot starve behind IPC unfairly, control runs after interactive work |
| Overflow is explicit counters | Never block libinput/DRM producers |
| Workers never hold DRM/Wayland objects | Avoid cross-thread master/fd races |
| Present & Vulkan record stay on the compositor thread | Predictable latency; Compio only posts completion-derived values |
| Bounded bridges (`try_send`) | Same contract as calloop channels / logging drain |
| **Completion** (io_uring driver), not readiness-as-architecture | Batch CQEs, lower syscall tax, one ring for files + wake + sockets |

Borrowings from Smithay/calloop (shape only):

- Source → callback → shared state (here: **completion** → **value event** → queue)
- Idle / between-wait work becomes explicit **phases** after drain
- Bounded cross-thread channels with drop-on-full, not unbounded queues

## Crate map

```text
tensor-event     pure value events, EventQueue, Phase, coalesce
tensor-runtime   Compio completion workers + WorkerBridge + run_turn + EventfdWake
tensor-host      mode / connector / format / present intent / raw input structs
tensor-input     device capabilities + Sample → Event (no libinput)
tensor-drm       topology plan + output rules (no libdrm)
tensor-present   present slot readiness + intent queue (no KMS FDs)
tensor-protocol  stable surface/buffer IDs + lifecycle + protocol tier policy
tensor-util      geometry / scale (existing)
tensor-compositor  policy, ECS, Smithay adapters, Vulkan
```

Future: `tensor-session`; protocol wire dispatch remains in the compositor's
temporary Smithay adapter while `tensor-protocol` owns value-only state. Each
adapter only emits `tensor-event::Event` (or thinner value types). Exit plan:
`docs/smithay-exit.md`.

## Dispatch turn (target)

```text
1. Completions: Compio/io_uring finishes submitted ops (I/O, wake read, timers)
2. inject_events(worker bridges → EventQueue)   // coalesce here
3. while let Some(ev) = queue.pop() {            // phase order
       match ev.phase() { ... policy ... }
   }
4. Render / present only if Scene/Gpu/Present demanded it
```

Step 1 is still calloop **readiness** for some Smithay-owned fds during
migration. The **target** is the same work as Compio-submitted ops that
**complete**. Steps 2–4 are Tensor-owned (`run_turn`). Worker→compositor wake:
write `EventfdWake`; a **submitted** read completes — not “poll the eventfd.”

## What not to put on the bus

- `WlSurface`, `DrmDevice`, `GbmDevice`, Vulkan handles
- Large IPC payloads (use `IpcCommandId` + owner-side storage)
- Unbounded `Vec` damage lists (use IDs + scene damage set)

## Migration stages

0. Crates land (`tensor-event`, `tensor-runtime`).
1. **Done:** `RuntimeState` owns `EventLoopState`; calloop idle runs
   `dispatch_event_turn` (inject → drain → coalesced redraw latch).
2. **Done:** `tensor-host` / `tensor-drm` / `tensor-present`; output policy is
   Smithay-free; present readiness table lives on the event loop.
3. **Done:** adapters push value events (pointer motion/button/axis, keyboard
   keycode, surface commit, vblank); seat/KMS side effects still run inline for
   latency. `PresentIntent` gates KMS submit.
4. **In progress:** more policy behind bus-only handling; reduce duplicate
   immediate redraw.
5. **In progress:** completion contracts (`run_turn`, `EventfdWake`,
   `EventfdCompletion`, `CompletionDriver::IoUring`); submitted Compio reads
   now complete worker eventfd wakes; IPC accept/read/write, Linux signalfd
   reads, one-shot GPU sync-file waits, security-context accept/close, Wayland
   listener accepts, and aggregate-display dispatch waits are Compio completion
   services. The display adapter submits one `PollOnce` operation against the
   backend-owned aggregate fd and rearms only after compositor-thread dispatch;
   it does not create a second client-fd readiness registry.
   calloop still owns the shared completion relay plus XWayland, libinput,
   session, and DRM notification adapters. Next: express those sources as
   Compio-completed ops (io_uring driver).
6. Replace Smithay backends with native input/DRM open path; delete Smithay
   (see `docs/smithay-exit.md`).

## Protocol tiers (related)

Protocol **selection** is independent of the event reactor but uses the same “borrow Smithay
maturity” idea: implement against wayland-protocols **tiers** (core / stable / staging-ext /
community). See `docs/protocol-surface.md`. Staging and `ext-*` are first-class; community
`zwlr_*` is a documented stopgap, not the default design target.
