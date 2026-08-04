# Tensor Files ECS architecture

Tensor Files uses `bevy_ecs` as a retained data kernel, not the Bevy engine as
an application framework. ECS owns stable UI identities and queryable values;
Compio tasks, Wayland objects, and Vulkan device/resource ownership stay at
their existing runtime boundaries.

## Ownership boundary

The ECS may own:

- pane and item identity;
- paths, navigation generation, view mode, layout, and selection values;
- visible-item residency and metadata-generation values;
- animation/reflow state that targets stable entities;
- allocation-bounded render extraction values consumed by the shared Vulkan
  renderer.

The ECS must not own protocol dispatch, asynchronous I/O executors, Vulkan
command/resource owners, or cold parsing and decoding work. Those systems may
read snapshots from or publish values into the ECS at explicit boundaries.
Tensor Files does not depend on Tensorland; both products only share the
`bevy_ecs` model and reusable value standards where appropriate.

## Migration sequence

1. Implemented: `ShellPaneStates` stores each open pane as a stable entity in a
   persistent `World`. Its two-entry array is only an O(1) entity lookup cache.
   Replacing a pane state keeps the entity; closing a pane despawns it.
2. Implemented: each pane's visible range uses retained item entities. Paths
   and slot residency are components; leaving the visible range despawns the
   entity and returns its bounded GPU-facing slot for reuse.
3. Implemented: visible-item render extraction queries only changed slot
   components into a caller-retained buffer. Unchanged items produce no work;
   recycled slots publish one new path binding without cloning path payloads.
4. Split the compatibility pane component into pane identity, location,
   selection, view, model generation, and layout components. Fold the older
   `PaneController` and shell-side pane state into one authority while keeping
   I/O workers outside the world.
5. Extract the remaining immutable frame values from changed pane components. Rendering
   remains a consumer of ECS semantics and stays in `vulkan-renderer`; ECS does
   not record Vulkan commands.

Each stage must retain existing public behavior and focused tests before the
next state owner is removed. There must never be two writable authorities for
the same pane value after its compatibility stage ends.

## Performance contract

- Entity identity is stable across ordinary frames and in-place pane updates.
- ECS schedules run only for input, I/O completion, configuration, animation,
  or other dirty semantic state; the renderer does not scan a rebuilt world to
  discover unchanged work.
- Visible-item entities and GPU slots are retained and bounded. Off-screen
  items are recycled at the visible-range boundary.
- Paths and entry payloads are not cloned during frame extraction unless an
  ownership transfer explicitly requires it.

This follows Dolphin's persistent model/view and visible-widget reuse boundary
in `references/fika/dolphin/src/kitemviews/kitemlistview.cpp`, especially
`KItemListView::setModel`, `recycleInvisibleItems`, `createWidget`, and
`recycleWidget`. Tensor Files diverges by representing retained identities as
ECS entities and extracting native Vulkan draw values rather than retaining Qt
graphics widgets.
