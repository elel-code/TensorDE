# Performance Alignment

Fika performance work follows Dolphin first. The local Dolphin source tree at
`references/fika/dolphin` is the first reference for file-manager performance
architecture, behavior-preserving optimizations, and regression gates.

## Rule

Every performance optimization or performance-affecting adjustment must include
a concrete Dolphin reference before the change is considered complete.

A valid reference names:

- the local Dolphin file path, and the relevant class, function, or data flow;
- the Dolphin behavior or performance boundary being copied, adapted, or
  intentionally not copied;
- the matching Fika module or code path;
- any deliberate divergence and why it is needed for Fika's native Vulkan shell;
- the verification command, log, benchmark, or smoke gate used for the change.

If Dolphin has no direct equivalent, the change must say so explicitly and cite
the closest Dolphin reference plus the reason the reference is only partial.

## Reference Format

Use this shape in performance notes, commit messages, PR descriptions, or the
implementation summary:

```text
Dolphin reference:
- Source: references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp
- Symbol: KFileItemModelRolesUpdater::setVisibleIndexRange / startUpdating
- Dolphin boundary: visible items are prioritized before background role work.
- Fika mapping: src/ui/... or src/core/...
- Divergence: ...
- Verification: ...
```

## Common References

- Item model, refresh, filtering, sorting, and role storage:
  `references/fika/dolphin/src/kitemviews/kfileitemmodel.cpp`,
  `references/fika/dolphin/src/kitemviews/kfileitemmodel.h`,
  `references/fika/dolphin/src/kitemviews/private/kfileitemmodelsortalgorithm.h`,
  `references/fika/dolphin/src/kitemviews/private/kfileitemmodelfilter.cpp`.
- Metadata roles, preview scheduling, visible index priority, async role
  resolution, directory size counting, and MIME/Baloo role updates:
  `references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.cpp`,
  `references/fika/dolphin/src/kitemviews/kfileitemmodelrolesupdater.h`,
  `references/fika/dolphin/src/kitemviews/private/kdirectorycontentscounter.cpp`,
  `references/fika/dolphin/src/kitemviews/private/kbaloorolesprovider.cpp`.
- Visible item virtualization, widget reuse, scroll/layout boundaries,
  column sizing, rubber-band, and item view geometry:
  `references/fika/dolphin/src/kitemviews/kitemlistview.cpp`,
  `references/fika/dolphin/src/kitemviews/kitemlistview.h`,
  `references/fika/dolphin/src/kitemviews/private/kitemlistviewlayouter.cpp`,
  `references/fika/dolphin/src/kitemviews/private/kitemlistsizehintresolver.cpp`.
- Item painting, icon/pixmap handling, text caching, role text layout, and
  selection/hover visuals:
  `references/fika/dolphin/src/kitemviews/kitemlistwidget.cpp`,
  `references/fika/dolphin/src/kitemviews/kstandarditemlistwidget.cpp`,
  `references/fika/dolphin/src/views/dolphinfileitemlistwidget.cpp`.
- Dolphin view integration and mode-specific behavior:
  `references/fika/dolphin/src/views/dolphinview.cpp`,
  `references/fika/dolphin/src/views/dolphinitemlistview.cpp`,
  `references/fika/dolphin/src/views/viewmodecontroller.cpp`,
  `references/fika/dolphin/src/views/viewproperties.cpp`.
- Places behavior and device sidebar integration:
  `references/fika/dolphin/src/panels/places/placespanel.cpp`,
  `references/fika/dolphin/src/dolphinplacesmodelsingleton.cpp`.

## Review Checklist

- Does the change include a Dolphin reference with local file path and symbol?
- Does the implementation preserve Dolphin's boundary between model data,
  role resolution, view layout, and painting unless a divergence is stated?
- Does the verification measure the same user-visible path that the reference
  covers, such as scrolling, sorting, refresh, thumbnails, Places, or DnD?
- Are new caches, queues, or retained resources bounded and invalidated at the
  same lifecycle boundary as the Dolphin reference or a justified Fika boundary?
- Are benchmark or smoke results attached when the change claims a performance
  improvement?
