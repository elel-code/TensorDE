# Protocol completion roadmap

Tensorland counts a protocol as complete only when all five layers are present:

1. direct wire dispatch, ownership, privilege filtering, lifetime, and errors;
2. pending/current state at the protocol-defined commit or request boundary;
3. value-only product/ECS state with no Wayland resource crossing the boundary;
4. the real input, layout, renderer, capture, or KMS side effect;
5. wire, transition, malformed-request, destruction, and execution tests.

Creating a global or accepting a request is not completion. Capability flags
and `tensor-protocol` catalog entries are added only when the advertised path
has meaningful execution and explicit failure behavior.

## Implementation order

### 1. Presentation policy — implemented

Implement `wp_tearing_control_v1` as a per-surface double-buffered hint. The
default and object-destruction value is `vsync`; `async` is only eligible when
the committed surface is the selected fullscreen/direct presentation candidate,
the output supports an asynchronous atomic flip, no capture or dependent
composition consumer requires the frame, and session/output state is healthy.
Ignoring an eligible hint remains protocol-correct, but Tensorland must expose
the policy decision in a deterministic render/present plan and tests. FIFO
latest-ready remains the default and is never silently weakened globally.

The implemented path includes duplicate-object wire errors, commit-time state,
destroy-to-vsync on the next commit, unknown-enum no-mutation behavior,
value-only ECS/scene extraction, deterministic composed-frame rejection, DRM
capability discovery, modeset exclusion, and a KMS flag test proving that only
an eligible typed intent selects `PAGE_FLIP_ASYNC`. Native TTY evidence remains
hardware-dependent and does not weaken the hardware-independent completion
gates.

### 2. Portal-owned temporary input — implemented

Implement privileged `ext_transient_seat_v1` with one Tensorland owner for each
temporary seat. Creation publishes a distinct zero-capability `wl_seat` global;
destroying the handle, disconnecting the creator, session teardown, or policy
revocation removes that global and every virtual device attached to it.
Sandboxed security-context clients are denied, not given a weakened shared
seat. Virtual keyboard and virtual pointer objects must resolve the selected
seat owner instead of treating every `wl_seat` as the primary seat.

The implemented path publishes creator-scoped uniquely named `wl_seat` globals,
reports `ready` or `denied` exactly once, removes all globals on handle or
creator teardown, bounds the seat pool, and routes virtual pointer/keyboard
devices into transient capability and event state without mutating the primary
seat. Removing a seat clears its advertised capabilities before global
withdrawal; stale virtual-device events become inert. Sandboxed
security-context clients follow the same explicit `denied` policy.

### 3. Color and HDR — in progress

Implement `wp_color_management_v1` version 3 together with
`wp_color_representation_v1`. Color descriptions, ICC/profile parameters,
primaries, transfer functions, luminance, mastering metadata, range, and pixel
encoding are cold-path immutable values. Surface state is double-buffered and
becomes current on `wl_surface.commit`; output feedback follows the selected
output color state.

The shared renderer receives generic typed color transforms and target color
descriptors, never Wayland protocol objects. Tensorland selects Vulkan formats,
linear-light composition, tone mapping, and KMS connector/CRTC HDR metadata as
one output plan. Unsupported descriptions fail explicitly; SDR output does not
silently claim HDR or wide-gamut support.

Completion requires ICC/parameter validation limits, image-description
lifetime tests, surface/output feedback wire tests, SDR-to-SDR identity,
HDR-to-HDR and HDR-to-SDR plan tests, target-format capability rejection,
metadata hotplug/reset behavior, and native evidence on a capable output.

The `wp_color_representation_v1` wire owner now has real per-surface
pending/current state, duplicate/inert/unknown-enum errors, commit-time buffer
compatibility checks, value-only scene extraction, and a cached shared-renderer
color plan on every surface draw. Its production global remains completion
gated: wire tests bind a test-only global, while normal Tensorland does not
advertise it until the output color owner is installed. Shader execution now
has a distinct retained managed-color pipeline: non-identity draws append a
fixed 64-byte transfer/matrix/tone-map lane, while SDR identity draws retain
the original 64-byte shader and command path. Production advertisement still
waits for encoded-versus-sRGB client view selection, so limited-range and
electrical-alpha transforms cannot be applied after an implicit hardware sRGB
decode. The first production capability will be RGB identity coefficients
with full and limited range; multi-plane YCbCr stays unadvertised until its
import and conversion path is complete.

The shared renderer now has an allocation-free typed color planner. Matching
SDR remains a direct identity draw; transforms use an R16G16B16A16 floating
linear working format, HDR-to-SDR selects BT.2390 tone mapping, and an HDR
target fails closed unless both a 10-bit/float format and native metadata path
exist. The plan now retains a fixed-point source-to-target gamut matrix and
lowers allocation-free into the managed shader ABI; Tensorland records that
ABI in both direct and region-local multi-pass scene segments. The identity
pipeline remains branch-free. `wp_color_management_v1` image creators, output
feedback, encoded client views, HDR output post-encoding, and KMS metadata
ownership remain before the combined slice is complete.

### 4. DRM leasing

Implement `wp_drm_lease_v1` only for non-desktop connectors on the authoritative
Vulkan-selected DRM device. Tensorland owns connector enumeration, lease file
creation, connector withdrawal, revocation, hotplug, VT/session pause, and
device teardown. A leased connector is removed from ordinary output planning;
desktop heads are never accidentally offered for leasing.

Completion requires connector/object lifetime and duplicate-connector errors,
request rejection across DRM devices, lease-FD delivery, revoke-on-destroy,
hot-unplug and session-pause revocation, output-plan exclusion, and optional
TTY evidence with a real lease-capable connector.

### 5. Existing protocol depth

The following are extensions of already advertised protocols, not new globals:

- `zwlr_output_management_v1`: apply live mode changes through a tested atomic
  replacement while retaining old buffers and mode blobs until timeline/KMS
  retirement.
- `ext_image_copy_capture_v1`: assemble a toplevel crossing outputs, and add a
  cursor-only GPU tap for transformed or DMA-BUF cursor surfaces.
- `zwp_linux_dmabuf_v1`: import supported multi-plane/YUV layouts and define an
  explicit implicit-reservation-fence policy without weakening syncobj clients.
- pointer/cursor/tablet: finish fixed hardware cursor-slot rendering, binary
  fence handoff, joint Vulkan/KMS retirement, and tablet `set_cursor` wire
  error/lifetime coverage.
- `xdg_toplevel_drag_v1`: complete duplicate-source, duplicate-attach, selection
  misuse, and destroy-before-end wire errors in addition to the execution path.

Each depth item retains bounded per-output/per-surface storage, explicit failure
for unsupported capability combinations, and no CPU rendering or descriptor-set
fallback.

## Deliberate omissions

Tensorland does not add `zwlr_screencopy`, the wlr foreign-toplevel-management
twin, `zwp_linux_explicit_synchronization_v1`, proprietary Hyprland protocols,
or an X11 compositor backend. Higher-tier ext capture/foreign-toplevel protocols
and `wp_linux_drm_syncobj_v1` are the primary paths.

Legacy tier-3 protocols are migrated only when wayland-protocols publishes a
higher-tier replacement. `wp_drm_lease_v1` and `ext_transient_seat_v1` are
productized because they provide capabilities not covered by an existing
higher-tier Tensorland global.

## Validation matrix

Every completed slice runs focused protocol tests and the Tensorland gates in
`testing.md`. Hardware-independent state/plan tests are mandatory even when
optional TTY evidence cannot run. A capability-dependent global must be absent
or return the protocol-defined explicit failure when its required kernel,
Vulkan, KMS, or security gate is unavailable.
