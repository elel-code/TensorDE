# Native rendering standard 0.1

This document defines the renderer's own public standard. The words **MUST**,
**MUST NOT**, **SHOULD**, and **MAY** are normative.

The standard adopts WebGPU's strongest API lessons—descriptor-driven object
creation, capability discovery before enablement, deterministic validation,
and implementation-independent contracts—but it is not a WebGPU
implementation and does not inherit browser portability limits.

## Goals

1. One application-facing contract MUST cover desktop compositors, file
   managers, game/scene renderers, compute visualization, and media pipelines.
2. The core backend MUST target Vulkan 1.4. The forward profile MUST track
   `VP_KHR_roadmap_2026` by published revision and API patch level.
3. A requested capability MUST either be fully enabled or fail explicitly.
   Silent feature removal and hidden fallback are non-conforming.
4. GPU execution, synchronization, residency, and presentation MUST remain
   observable and controllable. The standard MUST NOT introduce CPU readback
   as an implicit rendering operation.
5. Public objects MUST have deterministic ownership and destruction rules.
   Raw Vulkan interoperability MUST be isolated behind documented `unsafe`
   contracts.

## Object and validation model

The normative discovery path is:

```text
InstanceDescriptor -> Instance -> RequestAdapterOptions -> Adapter
Adapter + DeviceDescriptor -> Device + Queue
```

Adapters report immutable support. Devices expose only enabled capabilities.
Power preference ranks already-compatible adapters and MUST NOT weaken hard
requirements. Descriptors are validated before Vulkan mutation whenever the
required information is already known.

Validation failures SHOULD identify the exact feature, limit, extension,
resource, pass, or state transition that violated the contract. Implementations
MUST NOT convert a validation failure into a slower execution path unless the
descriptor explicitly names that path.

## Standard path

The default standard path requires:

- Vulkan 1.4 renderer baseline and buffer device address;
- `VK_EXT_descriptor_heap` as the only shader-resource binding model;
- synchronization2 and timeline-semaphore submission;
- dynamic rendering rather than compatibility render passes;
- `VK_KHR_present_mode_fifo_latest_ready` as the default present capability;
- block-suballocated device/upload/readback memory;
- explicit render-graph stage, access, layout, and queue ownership states.

Legacy descriptor sets, implicit render passes, queue-idle frame pacing, and
CPU raster fallback are outside the standard path.

The graphics-pipeline contract is deliberately narrower than raw Vulkan:
shader resources are described only by descriptor-heap mappings; pipeline
layouts and compatibility render passes are always null; color/depth/stencil
formats are declared for dynamic rendering; viewport and scissor are dynamic.
This removes layout/render-pass cache-key duplication while preserving exact
attachment compatibility validation at command recording time.

## Deliberate advances beyond WebGPU

| Area | This standard |
|---|---|
| Binding | Native descriptor heaps, device addresses, explicit shader mappings |
| Synchronization | Exact stage/access/layout/queue-family states and timeline values |
| Memory | Location-aware block allocation, persistent mapping, explicit retirement and trimming |
| Presentation | Surface-specific FIFO latest-ready validation with explicit caller fallback |
| Profiles | Vulkan 1.4 plus an exact Roadmap 2026 conformance gate |
| Interop | Auditable unsafe raw-handle path without weakening the safe standard path |
| Scheduling | Render-graph dependencies compile directly to synchronization2 barriers |
| Linux interop | Explicit DRM modifiers, dedicated dma-buf memory, FOREIGN ownership, SYNC_FD semaphores |

The standard Wayland presentation chain is fully native:

```text
raw-window-handle lease -> VkSurfaceKHR -> compatible graphics/present queue
-> acquire(binary) -> submit2(binary wait + timeline/binary signal)
-> present(binary wait)
```

The compositor interop chain is equally explicit:

```text
modifier capability query -> dedicated dma-buf import/export
-> SYNC_FD wait -> FOREIGN acquire -> descriptor-heap render/compute
-> FOREIGN release -> SYNC_FD signal -> timeline retirement
```

The surface owns the host lease, and queue submission and presentation share
one host lock. FIFO latest-ready remains the first standard preference, but a
surface configuration fails unless the caller explicitly lists an acceptable
fallback.

These capabilities are not optional implementation details. Once requested,
they form part of the returned device contract and MUST be reflected by object
behavior and diagnostics.

## Resource lifetime

Command encoders have a single recording-to-finished transition. Queue submit
consumes finished command buffers. Timeline value allocation and queue submit
form one host-serialized transaction, including under concurrent callers.

Descriptor ranges and transient resources MUST NOT be reused until their
submission timeline completes. Buffers, images, image views, heaps, and
pipelines referenced through raw interoperability MUST remain alive until the
same point. Explicit submission leases provide this ownership boundary for
decoder frames, imported dma-buf host owners, and temporary renderer objects.
A command encoder may carry arbitrary host leases or standardized
`SubmissionResource` ownership; finishing and ordinary managed submission
propagate them automatically into timeline retirement. Buffer update/copy,
vertex/index binding, image-to-image copy, and buffer-to-image copy retain all
owned buffer/image operands automatically.
A successful retained submission MUST hold each lease through its timeline
token; a failed submission MUST release leases immediately. Retirement MUST
keep the device alive long enough to destroy leased device resources, but its
ownership graph MUST NOT make the device own the retirement state and thereby
form a reference cycle. Resource classes which do not use shared internal
ownership MUST remain explicitly unsafe or be registered through
`CommandEncoder::retain`; the standard MUST NOT claim automatic retention for
a raw handle.

## Dynamic device-local buffers

`DynamicBuffer` is the standard convenience owner for frequently changing
geometry, index, storage, or indirect data. It allocates device-local memory,
adds `TRANSFER_DST`, uses an existing `UploadBatch` rather than opening a
second submission, grows geometrically, and hashes byte-identical content to
avoid redundant copy commands. It does not hide synchronization: the consumer
still declares the transfer-to-read transition in its render graph. Replaced
buffers remain safe because the command encoder retains recorded buffers until
their submission timeline completes.

## Memory classes

Buffer, linear-image, and optimal-image suballocations are distinct classes.
They MUST NOT share a block unless a future allocator explicitly implements
and validates `bufferImageGranularity` separation. Driver-required dedicated
allocations MUST be honored; driver-preferred dedicated allocations SHOULD be
honored. Ordinary resources SHOULD share bounded reusable blocks.

Repeated CPU-to-GPU writes SHOULD use `UploadBelt`. Its persistently mapped
chunks have independent hard byte/count bounds, roll back failed batches, and
become reusable only after the submission timeline completes. Upload and
rendering commands MAY share one encoder so resource initialization does not
force a second submission.

## Extension layers

The standard has three ordered layers:

1. **Core** — portable object, validation, synchronization, memory, command,
   shader, pipeline, and presentation contracts.
2. **Native acceleration** — descriptor-heap mappings, device addresses,
   external memory/video, sparse residency, and device-generated work.
3. **Host integration** — Wayland, other window systems, media decoders, and
   application-specific scene formats.

Higher layers MAY add requirements but MUST NOT redefine lower-layer behavior.
Host integration modules do not belong in the core crate.

## Conformance

An implementation conforms only when:

1. descriptor and capability validation tests pass;
2. object lifetime and failed-transaction tests pass;
3. render-graph barriers match synchronization2 output;
4. default device creation rejects adapters missing descriptor heap or FIFO
   latest-ready support;
5. at least one real-device smoke path creates resources, writes and binds
   descriptor heaps, records barriers, submits, waits, and reclaims by timeline;
6. public API documentation builds without warnings.

The conformance surface is versioned independently of any one application.
Consumer projects may add host-integration profiles,
but they MUST consume the same lower-level descriptors and MUST NOT introduce
project-named behavior into this crate.

The detailed field-level requirements remain in
[`standard-contract.md`](standard-contract.md).
