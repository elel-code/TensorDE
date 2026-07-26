# Upstream references

This crate is an original Wayland-only implementation based on public Wayland
protocol specifications and SCTK APIs. The following upstream work was used to
understand required behavior and failure modes; source was not copied into this
dual-licensed crate.

## winit popup work

- PR: <https://github.com/rust-windowing/winit/pull/4543>
- Reviewed head: `f1bf67b0a80fa325acb3642c662e34200aea1f69`
- Behavior retained here: full xdg-positioner state, initial-map popup grabs,
  compositor dismissal, reposition tokens, nested popup child-first teardown,
  parent lifetime and raw-handle lifetime considerations.

The winit implementation is shaped by a cross-platform window trait. This
crate does not reproduce its `CoreWindow` structure or cross-platform API.

## winit dialog work

- PR: <https://github.com/rust-windowing/winit/pull/4627>
- Reviewed head: `1327e17ce0bed2b9acd11759c114e1171d2f34b2`
- Behavior retained here: a dialog is a parented xdg-toplevel; xdg-dialog-v1
  augments that role with compositor-visible modality.

The PR was draft and contained popup history/TODO paths, so it was treated as
a behavioral reference rather than an implementation source.

## winit drag-and-drop work

- PR: <https://github.com/rust-windowing/winit/pull/4571>
- Reviewed merge commit: `156433eb912a62a1a07da0f9bbaa2d775270c788`
- Behavior retained here: the Wayland layer owns button serial selection,
  resolves the data device from the pointer-focused origin seat, starts the
  protocol drag before committing the icon surface for KDE compatibility, and
  reports drop acceptance without releasing the source or icon before
  `dnd_finished`/cancellation.

The public API remains Wayland-native, but follows the PR's important shared
data-transfer boundary: clipboard selections and DnD sources use the same MIME
content and pipe types. Unlike the PR's currently DnD-only backend, this crate
also routes clipboard selections through the main runtime connection.

## SCTK dialog support

- PR: <https://github.com/Smithay/client-toolkit/pull/532>
- Released in: `smithay-client-toolkit 0.21.0`
- API used here: `shell::xdg::dialog::Dialog`, `XdgShell::create_dialog`,
  parent assignment and `Dialog::set_modal`.

## Blur protocol

The preferred implementation uses the standardized staging
`ext-background-effect-v1` protocol exposed by SCTK 0.21, matching winit main
at `d84ec647d80d9ef76c5a42b948c852b8e0db9210`. Unlike winit's initial global
selection, this runtime consumes the protocol's dynamic capability event and
enables blur only while its `blur` bit is advertised. The legacy KDE blur
protocol is intentionally not supported.

Arbitrary `wl_region` requests are preserved. Whole-surface blur uses an
oversized region clipped by the compositor because a NULL region means
"disable", while updates remain double-buffered until the next
`wl_surface.commit`.

## Primary selection

- SCTK module: `primary_selection` in `smithay-client-toolkit` 0.21
- Behavior retained: seat-scoped device, pending-offer then selection attach
  (resist faulty compositors), async pipe receive, dual-write from regular
  clipboard `set_selection` when the global exists so middle-click paste works.
- Public API: `store_primary_selection` / `receive_primary_selection`; Fika
  middle-click uses async primary paste into the active pane (same worker path
  as Ctrl+V; never blocks the event loop).

## Idle inhibit

- Protocol: `zwp_idle_inhibit_manager_v1`
- Behavior: create/destroy per-surface inhibitors; destroy on surface teardown;
  `set_fullscreen(true)` best-effort enables inhibit, `false` releases it.

## Linux dmabuf

- Protocol: stable `zwp_linux_dmabuf_v1` (bind versions 3..=5; Mesa needs ≥3)
- SCTK module: `dmabuf` in `smithay-client-toolkit` 0.21
- Behavior retained:
  - v3: collect `modifier` events; ignore legacy `format` (duplicated by modifiers)
  - v4+: default + surface feedback (`format_table` mmap, main device, tranches)
  - `create_params` → async `create` / `create_immed` → owned `wl_buffer`
  - buffer `release` events for dmabuf-backed buffers only
  - surface destroy drops surface-scoped feedback objects
- Public API: `DmabufBufferParams` / `DmabufFeedback` / `create_dmabuf_buffer` /
  `attach_dmabuf_buffer` / feedback request helpers; events via `Event::Dmabuf`.
- Format-table mapping uses `rustix::mm` (no `memmap2`).
- **wgpu integration (Fika):** present still uses RWH → wgpu swapchain. Fika
  requests `Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF` when the adapter allows
  it, and can import single-plane dmabuf fds via
  `wgpu_hal::vulkan::Device::texture_from_dmabuf_fd` +
  `Device::create_texture_from_hal` (`shell/render/dmabuf.rs`). Default and
  surface feedback are requested on main toplevel create; snapshots are cached
  on `ActiveEventLoop`. `DmabufImportPlan` / `DmabufReadiness` combine GPU
  capability with feedback to pick a fourcc+modifier; startup logs readiness.
  An optional `/dev/udmabuf` smoke test exercises real import when the node
  is available. Business entry point is `acquire_external_texture` (dmabuf
  import with CPU `write_texture` fallback); icon/text atlases stay on their
  packed `write_texture` paths. Feedback events notify the app so readiness
  can be re-logged. Export (wgpu texture → dmabuf → `wl_buffer`) is not
  implemented; that would need a reverse path not yet in wgpu-hal.

## Core data-device and cursor behavior

DnD source/offer negotiation follows the `wl_data_device` protocol and SCTK
0.21 data-device APIs. Drag previews are independent `wl_surface` objects with
premultiplied ARGB8888 SHM buffers and remain owned until source completion or
cancellation. SCTK's `SeatState` binds seats already present in the initial
registry without calling `SeatHandler::new_seat`, so per-seat data devices are
also initialized idempotently from capability callbacks.

Clipboard and DnD sources share the main runtime's seat serials, data devices,
MIME content, and source request handling. An incoming DnD offer is retained
across its queued `Leave` event so application callbacks cannot observe an ID
whose protocol offer was already destroyed.

Cursor selection uses SCTK's themed-pointer path. It prefers
`wp_cursor_shape_manager_v1` and falls back to the system XCursor theme without
changing the crate-owned cursor vocabulary.

## Touch input

Touch behavior follows winit main at
`d84ec647d80d9ef76c5a42b948c852b8e0db9210` and SCTK 0.21 `seat/touch`:
surface-local down and motion, seat-scoped down/up serials and timestamps,
multi-point identity, shape/orientation, and cancellation are exposed without
leaking protocol objects.

The native shell owns a frame buffer for `wl_touch` events (down/up/motion/
shape/orientation), flushed on `Frame` or when the last active point is
released without a trailing frame (Weston workaround). Surface destruction and
touch capability removal clear tracked points and emit a single cancel.

## Fractional scaling and viewporter

Fractional scaling follows winit main at
`d84ec647d80d9ef76c5a42b948c852b8e0db9210` and the
`wp-fractional-scale-v1` protocol: the manager is usable only together with
`wp-viewporter`, preferred numerators use a denominator of 120, and legacy
integer output-scale updates are suppressed once a surface has a fractional
scale object.

The runtime owns both per-surface extension objects and destroys them before
the xdg/wl surface roles. Its public boundary keeps preferred scale as `f64`,
requires fractional clients to leave `wl_surface.buffer_scale` at one, and
exposes the logical viewport destination as explicit double-buffered surface
state. Fika rounds logical to physical toplevel dimensions halfway away from
zero, matching Wayland and winit rather than the usual floor conversion.

## XDG activation

Activation behavior follows winit main at
`d84ec647d80d9ef76c5a42b948c852b8e0db9210`: callers can asynchronously obtain
an opaque token for IPC, apply an externally supplied token to a toplevel, or
request user attention by obtaining a surface-associated token and activating
that same surface when the compositor replies.

The runtime additionally exposes all optional version-1 request context:
target app ID and a seat-scoped input/focus serial. Foreign-connection serials
are rejected before a protocol request is sent. Exported responses carry a
runtime request ID, user-attention requests are coalesced per surface, and each
`xdg_activation_token_v1` object is explicitly destroyed after `done`.

## XDG toplevel icons

Toplevel icon behavior was reviewed against winit main at
`d84ec647d80d9ef76c5a42b948c852b8e0db9210` and the staging
`xdg-toplevel-icon-v1` specification. The runtime supports more of the protocol
surface than winit's current single-RGBA path: XDG theme names, multiple pixel
representations and integer scales, compositor `icon_size`/`done` preferences,
and simultaneous name plus pixel fallback data.

Every pixel representation is validated as square SHM-compatible data and
copied from straight RGBA to immutable premultiplied native-endian ARGB8888.
The temporary icon object is destroyed after `set_icon`, and applied SHM
storage is retained with its toplevel until replacement or teardown. Assignment
and clearing retain the protocol's next-`wl_surface.commit` semantics.

The implementation lives in a self-contained `toplevel_icon` module. The core
runtime owns only the optional manager and the public orchestration methods;
surface state retains only the applied storage. The same `shm_format` module is
used by toplevel and drag icons so alpha conversion cannot diverge.

## Text input v3

Text-input behavior was reviewed against winit main at
`d84ec647d80d9ef76c5a42b948c852b8e0db9210` and the unstable
`text-input-unstable-v3` specification. The runtime follows its seat-scoped
focus model, always disables on leave, resends the complete client state after
enter, and preserves the protocol ordering of delete, commit, and preedit data
inside one `done` batch.

Unlike winit's cross-platform event stream, this crate exposes the atomic
Wayland batch directly. The reusable module validates UTF-8 byte offsets and
the 4000-byte surrounding-text limit, owns pending compositor data and the
per-seat proxy/session lifecycle, and drops `done` events that target a session
which is no longer enabled. `Runtime` retains only the desired state for each
surface and skips equal updates to avoid redundant protocol commits. The seat
identity travels in the proxy's dispatch data, so enter/leave/done use a direct
seat lookup instead of scanning every seat for a matching object.

## Pointer constraints and relative motion

Pointer capture was reviewed against winit main at
`d84ec647d80d9ef76c5a42b948c852b8e0db9210`, winit 0.30.13, and the
`pointer-constraints-unstable-v1` / `relative-pointer-unstable-v1`
specifications. The retained behaviors are persistent confinement and locking,
relative motion with unaccelerated deltas, and a
locked-pointer restoration hint. The runtime also exposes the protocol's full
region model: NULL surface-input regions, arbitrary rectangle unions, empty
regions, and double-buffered `set_region` updates without replacing an
otherwise unchanged constraint object.

This runtime additionally owns constraint activation state and lifecycle in a
dedicated per-seat session. It destroys a constraint before moving that pointer
to another surface, recreates it from declarative per-surface state on enter,
and exposes compositor locked/unlocked or confined/unconfined transitions.
Relative-pointer objects are lazy: they exist only while a focused surface has
subscribed to relative motion or requested a lock, avoiding an unconditional
second high-frequency pointer stream.

Last reviewed: 2026-07-23.
