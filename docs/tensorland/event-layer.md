# Event layer and completion runtime

Tensorland owns compositor **event semantics**. Compio owns asynchronous I/O as a **completion model**:
submit an operation, consume its completion, then explicitly submit the next operation. Linux uses
the io_uring driver and fails startup when it cannot be created. Compio defaults and the `polling`
fallback are disabled.

Do not design “register an fd and poll until readable.” Design “submit read/accept/write/wait; when
its CQE arrives, publish a bounded value and run the semantic turn.” A type named `PollFd` still
follows this rule: with the io_uring driver it submits one `IORING_OP_POLL_ADD`, resolves from its
CQE, and is explicitly rearmed after owner-side consumption. It is not a readiness registry.

## Performance contract

| Rule | Reason |
|------|--------|
| Fixed-capacity phase rings | No allocation on input or vblank paths |
| Coalesced pointer motion and per-output vblank | Device frequency cannot become queue length |
| Fixed phase order without sorting | O(1) push/pop and deterministic policy |
| Explicit overflow counters or hard capability failure | Producers never block the compositor |
| Value-only worker bridges | Wayland, Vulkan, and DRM/KMS ownership stays thread-affine |
| Present and Vulkan record on the compositor thread | Predictable latency and scanout ownership |
| Ring capacity equals each service's operation budget | No 1024-entry default ring or hot growth |
| Persistent staging stores and borrowed slices | Avoid per-turn vectors, clones, and copies |

## Crate map

```text
tensor-event       input/device values, phases, fixed queues, coalescing
tensor-runtime     io_uring runtime construction, completion helpers, bounded bridges
tensor-host        mode, connector, format, and present-intent values
tensor-drm         topology plans and output rules
tensor-present     present-slot readiness and intent queue
tensor-protocol    stable protocol IDs, lifecycle, and tier catalog
tensor-util        geometry and exact scale primitives
tensorland  direct Wayland/input/session/XWayland/DRM ownership, ECS, Vulkan
```

## Dispatch turn

```text
1. Submitted Compio/io_uring operations complete.
2. Drain bounded completion bridges into Tensorland values.
3. inject_events(worker bridges -> EventQueue), coalescing on ingress.
4. Pop EventQueue by fixed phase order and apply compositor policy.
5. Record/render/present only when scene or output state demands it.
6. Rearm one-shot fd operations after owner-side consumption.
```

The compositor thread owns one Compio runtime. Its base submitted-operation budget covers the
worker eventfd read and the two optional timerfd waits; active DRM devices add one page-flip wait
each. No empty calloop aggregate or relay operation consumes a slot. Workers signal `EventfdWake`;
the compositor continues only after the submitted eight-byte read completes.

Wayland listener accepts and display dispatch, IPC reads/writes, signalfd, security-context sockets,
GPU sync files, udev, libinput, libseat, timerfds, DRM page flips, XWayland displayfd, X11 socket
notification, and X11 property reads are completion services. An owner drains a bounded amount of
work after a CQE and rearms explicitly. Page flips, atomic KMS submits, Wayland dispatch, and X11
policy remain on the compositor thread.

The rootless XWM uses two completion paths:

- Its event fd has one submitted `PollFd` operation. After the CQE the compositor drains `x11rb`
  events into a persistent `VecDeque`, flushes writes, and rearms.
- A dedicated Compio Unix connection submits fixed `GetProperty` batches. It returns only
  `X11PropertyResult` values through a 64-entry bridge. Runtime mapping never performs a blocking
  property reply or a synchronous compatibility query.

## What must not cross the bus

- `WlSurface`, Wayland resources, X11 connections, DRM/GBM devices, Vulkan handles
- Large IPC payloads; use stable IDs plus owner-side storage
- Unbounded damage or event vectors
- Readiness registrations or callbacks whose arrival order defines policy

## Protocol tiers

Protocol selection is independent of the I/O runtime. Tensorland follows wayland-protocols tiers:
core, stable, staging/`ext`, unstable, community, then proprietary. A higher standard tier wins for
the same capability. See `docs/tensorland/protocol-surface.md`.
