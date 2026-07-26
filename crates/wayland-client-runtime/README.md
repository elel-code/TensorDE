# wayland-client-runtime

`wayland-client-runtime` is a Wayland-only client protocol, surface and event
layer. It intentionally models Wayland roles instead of imitating a
cross-platform `Window` API.

**Backend migration (in progress):** the long-term stack is **Compio + a native
protocol implementation** (no SCTK/calloop callbacks). See
[`ARCHITECTURE.md`](ARCHITECTURE.md) and Fika roadmap §6d. Today protocol state
still uses SCTK; display-fd readiness already uses Compio
(`Runtime::wait_display_readable` / `dispatch_pending`).

Native protocol code is organized like **wayland-protocols / Smithay**:
`core` · `stable` · `staging` · `unstable` · `ext` · `community/wlr`
(`src/native/protocols/`).

The crate is currently developed in the Fika workspace. Its public API is
general-purpose and contains no Fika-specific model or renderer dependency.

## Implemented boundary

| Area | Public behavior |
| --- | --- |
| Connection/event loop | Owns the Wayland connection and event queue; calloop dispatch remains for compatibility; Compio can await display readability |
| Toplevels | Creates xdg-toplevel surfaces, reports configure/close/frame/scale events, and starts compositor-driven move, eight-edge resize, and window-menu interactions |
| Dialogs | Creates parented xdg-dialog-v1 surfaces with modality; falls back to a parented xdg-toplevel when unsupported |
| Activation | Exports and consumes `xdg-activation-v1` tokens, carries optional app/seat/serial context, and supports coalesced user-attention requests |
| Toplevel icons | Uses `xdg-toplevel-icon-v1` with XDG theme names, one or more immutable SHM pixel fallbacks, compositor-preferred sizes, and explicit commit semantics |
| Layer surfaces | Exposes a backend-neutral layer API over deployed `zwlr_layer_shell_v1` through v5, including output targeting, dynamic layer, on-demand keyboard focus, exclusive-edge disambiguation, configure/closed events, and layer-parented popups |
| Popups | Exposes the complete xdg-positioner anchor, gravity, constraint, offset, reactive and reposition state; accepts only opaque press/down serials for grabs |
| Lifetimes | Owns a surface tree, removes descendants child-first, and makes every renderer lease retain its ancestors |
| Rendering | `SurfaceHandle` implements raw-window-handle 0.6 for both wgpu and direct Vulkan use |
| Scaling | Uses `wp-fractional-scale-v1` together with `wp-viewporter`, reports `f64` preferred scales, and retains integer output-scale fallback |
| Blur | Uses `ext-background-effect-v1` and preserves complete-surface or arbitrary surface-local rectangle regions |
| Data transfer | Clipboard and DnD share one MIME-content model, runtime connection, seat/serial state, data devices and pipe I/O; application-specific formats stay in the application |
| Drag and drop | Handles incoming/outgoing offers, action negotiation and lifecycle events; optional RGBA previews use owned SHM drag-icon surfaces |
| Input | Translates framed multi-touch, keyboard, pointer, and touchpad gesture events into crate-owned values; preserves continuous, discrete, value120, source, stop, and relative-direction axis data; uses cursor-shape when available and falls back to the system cursor theme |
| Pointer capture | Implements `zwp_pointer_constraints_v1` confinement/locking and lazily creates `zwp_relative_pointer_v1` only for subscribed or locked surfaces |
| Pointer gestures | Implements `zwp_pointer_gestures_v1` swipe, pinch/pan/rotation, and v3 hold lifecycles with per-seat objects and surface-safe routing |
| Text input | Implements seat-scoped `zwp_text_input_v3`, atomic preedit/commit/delete batches, retained editor state, UTF-8 byte offsets, content hints and cursor rectangles |

## Basic use

```no_run
use std::time::Duration;
use wayland_client_runtime::{Runtime, RuntimeOptions, ToplevelAttributes};

let mut runtime = Runtime::connect(RuntimeOptions::default())?;
let surface = runtime.create_toplevel(ToplevelAttributes {
    title: "Wayland application".into(),
    app_id: "dev.example.Application".into(),
    ..Default::default()
})?;

let renderer_handle = runtime.surface_handle(surface).unwrap();
// Pass renderer_handle (usually in an Arc) to wgpu, or use its raw handles
// to create VK_KHR_wayland_surface objects.

loop {
    runtime.dispatch(Some(Duration::from_millis(16)))?;
    for event in runtime.drain_events() {
        // Update application state. Handler calls never re-enter application code.
        let _ = event;
    }
}
# Ok::<(), wayland_client_runtime::RuntimeError>(())
```

Embedding event loops that must release a runtime borrow before invoking
application callbacks can reuse one `Vec<Event>` with
`Runtime::drain_events_into`. Pending events are stored contiguously and
appended in protocol order, avoiding a new collection allocation on every
dispatch iteration. `Runtime::drain_events` remains available for direct
iterator-based handling.

## Region blur

Unlike a whole-window boolean, the blur request preserves the protocol's
region support:

```no_run
use wayland_client_runtime::{BlurRegion, BlurState, LogicalRect};
# use wayland_client_runtime::{Runtime, SurfaceId};
# fn apply(runtime: &Runtime, surface: SurfaceId) -> Result<(), wayland_client_runtime::RuntimeError> {
runtime.set_blur(
    surface,
    BlurState::Enabled(BlurRegion::Rectangles(vec![
        LogicalRect::new(0, 0, 800, 56),
        LogicalRect::new(0, 56, 240, 544),
    ])),
)?;
# Ok(())
# }
```

Applications should inspect
`Runtime::capabilities().ext_background_effect`. The capability becomes true
only when the compositor advertises the protocol's dynamic `blur` bit;
compositors without it return `RuntimeError::Unsupported`.

ext-background-effect-v1 state is double-buffered with `wl_surface`. Call
`Runtime::commit` after changing blur, or let the next renderer buffer commit
apply it. This keeps blur updates inside the same explicit surface commit
boundary as geometry, scale, and renderer state.

## Drag icons and cursors

`Runtime::start_drag` derives the serial and data device from the pointer seat
focused on the origin surface; applications call it during the activating
pointer gesture and do not retain protocol serials. It accepts an optional
`DndIcon`, validates its buffer scale, converts straight RGBA into the
premultiplied native-endian ARGB8888 representation required by `wl_shm`, and
applies the logical hotspot offset. The icon is committed after `start_drag`
for KDE compatibility and remains owned until the source finishes or is
cancelled. `SourceDropped` reports acceptance immediately, while
`SourceFinished` marks the point where source and icon resources are released.

Clipboard selections use the same `TransferContent` and runtime-owned seat
serials as drag sources. `Runtime::store_selection` and
`Runtime::receive_selection` therefore require no second Wayland connection or
clipboard-only event thread. Incoming DnD offers remain valid until the queued
leave/drop event has been consumed and the application explicitly discards or
finishes them. Discarding is idempotent so a queued leave can safely race with
completion of the same dropped offer; finishing remains strict.

`Runtime::set_cursor` uses `wp_cursor_shape_manager_v1` when advertised. On
older compositors, SCTK loads the same semantic cursor from the configured
system XCursor theme; applications do not need a separate fallback path.

Touch dispatch preserves Wayland frame ordering and the Weston-compatible
last-up fallback. Down/up serials remain seat-scoped, active touch-down serials
can request popup grabs, and cancel/capability-removal events clear every
affected surface without leaving stale touch IDs behind.

## Toplevel pointer interactions

While handling a `PointerEventKind::Press`, call
`Runtime::begin_interactive_move`, `Runtime::begin_interactive_resize`, or
`Runtime::show_window_menu`. The runtime finds the focused seat and supplies
the active implicit-grab serial, so application code never stores raw Wayland
serials. `ResizeEdge` exposes all eight protocol directions.

Pressed buttons are tracked independently per seat. Releasing one button
invalidates only that button's serial, and an older button that remains held
can still drive an interaction. Surface destruction and pointer-capability
removal clear the associated state. Outgoing DnD uses this same active-press
tracker, preventing a release serial or a stale completed grab from starting a
drag.

## Layer surfaces

`Runtime::create_layer_surface` creates background, bottom, top, or overlay
surfaces through a backend-neutral public API. `LayerSurfaceState` retains the
complete double-buffered size, anchors, exclusive zone and v5 exclusive edge,
margins, keyboard interactivity, and layer. The runtime validates
compositor-selected zero-sized axes and protocol-version requirements before
creating an object or sending an update.

Creation performs the protocol-required initial commit without a buffer. Wait
for `Event::LayerSurface(LayerSurfaceEvent::Configure { .. })` before attaching
the first renderer buffer; configure serials are acknowledged inside protocol
dispatch. `Runtime::set_layer_surface_state` diffs against retained state and
sends only changed fields, then the caller applies them with one
`Runtime::commit`. A compositor `closed` event makes later updates fail instead
of silently sending requests to an unusable role.

An optional `OutputId` from `Runtime::outputs` targets a specific live output;
`None` delegates placement to the compositor. Output hotplug and metadata
changes are reported as `Event::Output`. An xdg-popup created with a layer
surface parent automatically uses the layer-shell `get_popup` association
before its initial commit.

The currently deployed backend is `zwlr_layer_shell_v1` v1-v5. The public API
does not expose that vendor namespace, allowing a future standardized
ext-layer-shell backend to be added without changing applications. Inspect the
four `layer_shell_*` capability fields before using versioned behavior.

`cargo run -p wayland-client-runtime --example layer_surface_smoke` probes
initial configure and closed events without attaching a renderer buffer.

## Pointer constraints and relative motion

`Runtime::set_pointer_constraint` retains `None`, `Confined`, or `Locked` for
one surface. A seat-scoped constraint object is created when its pointer enters
that surface, destroyed before focus moves elsewhere, and recreated from the
retained state on a later enter. This preserves the protocol's one-constraint
rule without leaking seat or proxy identities into application code.

`PointerConstraintRegion::SurfaceInput` sends the protocol's NULL region and
therefore follows the current surface input region. `Rectangles` preserves an
arbitrary union of surface-local rectangles, including an intentionally empty
region. Empty rectangle dimensions and values outside Wayland's signed integer
range are rejected before any request is sent. Changing the region of an
existing constraint uses `set_region`; the update is double-buffered and takes
effect on the next `Runtime::commit`.

`PointerConstraintEvent` reports whether the compositor has activated or
deactivated the requested lock/confinement. When `relative_pointer_v1` is
available, locked surfaces receive `RelativePointerEvent` automatically. An
unlocked surface can opt in with
`Runtime::set_relative_pointer_enabled`; opting out destroys the per-seat
relative-pointer object instead of merely dropping its high-frequency events.
Both accelerated and unaccelerated deltas and the compositor's microsecond
timestamp are preserved.

`Runtime::set_locked_pointer_position_hint` supplies the logical restoration
position used when a lock ends. It is a hint rather than a pointer warp and is
double-buffered with `wl_surface`, so apply it with the next
`Runtime::commit`. Inspect `pointer_constraints_v1` and `relative_pointer_v1`
in `RuntimeCapabilities` before enabling optional behavior.

`cargo run -p wayland-client-runtime --example pointer_capture_smoke` provides
an interactive confinement and relative-motion probe.

## Pointer axis frames

`PointerEventKind::Axis` preserves both axes of one `wl_pointer.frame` as
`PointerAxisValue`. Continuous compositor-coordinate deltas remain available
without conversion, while `value120` retains partial high-resolution wheel
steps and deprecated `discrete` values remain as a fallback. Per-axis stop and
relative physical direction plus the frame's wheel, finger, continuous, or
wheel-tilt source are also retained.

`PointerAxisValue::logical_steps` prefers `value120 / 120` and then discrete
steps. It returns `None` for touchpad/continuous-only input instead of guessing
a line-to-pixel ratio. Values retain Wayland's sign convention so policy layers
can apply their own coordinate convention exactly once.

## Pointer gestures

`RuntimeCapabilities::pointer_gestures_v1` reports protocol availability; it
does not create per-seat objects by itself. Call
`Runtime::set_pointer_gestures_enabled(surface, true)` for surfaces that
consume gestures. The first subscription lazily creates swipe and pinch
objects for live pointer seats, and removing the final subscription destroys
them. Applications such as Fika that do not subscribe therefore pay no
gesture protocol or event-queue overhead.

Unsubscribing immediately suppresses the remainder of an in-progress gesture
for that surface and does not fabricate an end serial or timestamp. Code that
initiates the unsubscribe should clear its own transient gesture state.

`Event::PointerGesture` reports the full begin/update/end lifecycle without
filtering finger counts. Swipe updates preserve the surface-coordinate
movement since the previous event. Pinch updates preserve center movement,
absolute scale relative to begin, and clockwise rotation in degrees since the
previous event, allowing policy layers to derive pan, zoom, and rotation
without a lossy cross-platform conversion.

Begin and end events retain opaque seat-scoped input serials and compositor
timestamps; an end also distinguishes completion from cancellation. Hold has
no update stage and is available when `pointer_gesture_hold_v1` is true (global
version 3). Gesture objects also follow the seat's `wl_pointer` capability,
and active routing is cleared when its target surface disappears, so a late
update cannot be delivered to a destroyed or unsubscribed surface.

`cargo run -p wayland-client-runtime --example pointer_gestures_smoke` checks
the lazy attach/detach lifecycle against the active compositor.

## Fractional scaling

Fractional scaling is enabled only when both `wp-fractional-scale-v1` and
`wp-viewporter` are available. Each managed surface then owns one fractional
scale object and one viewport for its complete lifetime. Preferred scale
numerators are divided by the protocol denominator of 120 and emitted as
`SurfaceEvent::ScaleFactorChanged { factor: f64, .. }`; legacy integer output
scale events are ignored for those surfaces.

Render buffers at `round(logical_size * factor)`, keep
`wl_surface.buffer_scale` at one, and call
`Runtime::set_viewport_destination(surface, Some(logical_size))` before the
commit that attaches the resized buffer. The viewport destination is
double-buffered with the surface. Without the paired globals, use the integer
scale event and `Runtime::set_buffer_scale` instead.

On legacy wl_surface v1/v2 compositors, scale one is the fixed protocol
default and is therefore a safe no-op. Larger integer scales return
`RuntimeError::Unsupported` instead of sending the v3-only request and
terminating the Wayland connection.

## Surface activation

When `Runtime::capabilities().xdg_activation_v1` is true,
`Runtime::request_activation_token` starts an asynchronous token request. Its
`ActivationTokenAttributes` can carry the target app ID and the opaque
`InputSerial` from the input or focus event that initiated the request. The
result arrives as `Event::Activation(ActivationEvent::TokenDone { .. })` with
a stable `ActivationRequestId`, so multiple outstanding requests remain
distinguishable.

Forward the resulting `ActivationToken` to another process using
`XDG_ACTIVATION_TOKEN`, D-Bus platform data, or another IPC channel. A receiving
client wraps that string with `ActivationToken::from_raw` and calls
`Runtime::activate_surface`. `Runtime::request_user_attention` implements the
winit behavior directly by requesting a surface-associated token and applying
it back to the same toplevel; duplicate requests are coalesced until the
compositor completes the first one. Token protocol objects are destroyed after
their one `done` event.

## Toplevel icons

When `Runtime::capabilities().xdg_toplevel_icon_v1` is true,
`Runtime::set_toplevel_icon` assigns an icon to a toplevel or dialog. A
`ToplevelIcon` may contain an XDG icon-theme name, multiple square RGBA buffers,
or both. Supplying both lets the compositor resolve the current theme first and
use the pixel representations as fallbacks. `Runtime::preferred_toplevel_icon_sizes`
returns the sorted, deduplicated logical sizes announced by the compositor; an
empty list means it has no preference (or the protocol is unavailable).

Pixel representations carry their own integer scale and are validated before
any protocol request is sent. The runtime copies straight RGBA into immutable,
premultiplied native-endian ARGB8888 SHM storage and keeps that storage alive
for the applied icon. Replacing the icon releases the previous storage. Passing
`None` restores the compositor's default icon.

Icon assignment is double-buffered. Call `Runtime::commit` after
`set_toplevel_icon`, or let the next renderer commit apply it. The temporary
`xdg_toplevel_icon_v1` object becomes immutable after assignment and is
destroyed immediately; the compositor keeps the assigned icon until it is
explicitly replaced or cleared.

## Text input v3

When `Runtime::capabilities().text_input_v3` is true, call
`Runtime::set_text_input_state(surface, Some(state))` for the active editor.
The runtime retains that complete state while the surface is unfocused and
resends it together with `enable` on the next seat-specific `enter`. Passing
`None` disables the focused session. Equal states are ignored, so a
declarative UI may synchronize editor state without generating redundant
protocol commits.

`TextInputSurroundingText` uses UTF-8 byte offsets, rejects offsets inside a
codepoint or strings containing NUL, and enforces the protocol's 4000-byte
limit. Cursor rectangles are surface-logical coordinates. Client requests are
double-buffered and become visible to the compositor with one text-input
`commit`; they do not require a `wl_surface.commit`.

Compositor `preedit_string`, `commit_string`, and
`delete_surrounding_text` events are accumulated until `done` and emitted as
one `TextInputEvent::Done` value. Apply that batch in protocol order: replace
the old preedit, delete committed surrounding text, insert the commit string,
publish the updated surrounding state, then render the new preedit separately
from committed text. A `leave` clears the pending batch and disables the seat
session, so the next `enter` always starts from a complete state.

## Internal module boundary

Protocol-specific ownership and dispatch are kept in focused modules:
`activation`, `fractional_scale`, `layer_shell`, `output`, `pointer_axis`,
`pointer_constraints`, `text_input`, `toplevel_icon`,
`toplevel_interaction`, `touch`, and the shared SHM pixel formatter. `Runtime`
binds those modules and translates their callbacks into crate-owned events,
while `SurfaceShared` only retains declarative per-surface state and protocol
resources. Pointer constraints, text input, layer surfaces, and toplevel
interaction tracking own their state machines and destruction paths. This
keeps generated Wayland proxy details out of the public API and prevents
application policy from leaking into reusable protocol code.

## Protocol lifetime rule

`Runtime::destroy_surface` removes a complete subtree in post-order. A
renderer-held `SurfaceHandle` can extend a role's lifetime; each child lease
retains a strong parent lease, so an externally retained nested popup still
cannot outlive its protocol parent. Drop renderer surfaces before requesting
final teardown when immediate destruction is required.

## License

Licensed under either Apache-2.0 or MIT, at your option. See
`LICENSE-APACHE` and `LICENSE-MIT`.
