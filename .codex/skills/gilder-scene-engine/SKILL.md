---
name: gilder-scene-engine
description: Validate and evolve Gilder's Vulkan 2026 scene engine across Wallpaper Engine ingest, typed IR, semantic ECS, RenderingDevice graphs, shaders, and native Vulkan runtime. Use when changing scene semantics, `.gscene` conversion or ABI, effects, targets, materials, text, particles, animation or clipping, visibility, runtime properties, visual correctness, or rendering performance for any scene wallpaper.
---

# Gilder Scene Engine

Use this workflow to keep semantic correctness and performance evidence coupled without treating
old Gilder output as the specification.

## Establish the contract

1. Read the repository `AGENTS.md` and the relevant section of
   `docs/gilder-scene-engine-architecture.md`.
2. Read [regression-gates.md](references/regression-gates.md) before modifying or validating scene
   semantics, rendering, conversion, or performance.
3. Inspect `git status`, the staged diff, and the unstaged diff separately. Preserve unrelated
   edits and identify every `MM` file before staging.
4. State the authored semantic being preserved and select a task-local regression corpus by
   affected capabilities. Use raw project assets and verified WE evidence; use historical
   commits/captures only to locate a regression.
5. Reject any proposed old-reader, alias, fallback, permissive parser, compatibility shader, or
   sample-specific branch. Remove obsolete behavior instead of maintaining two paths.

## Implement through typed boundaries

1. Put parsing, script-property application, font rasterization, and asset lowering in the convert
   cold path.
2. Carry facts explicitly through typed IR, `.gscene`, `SceneStorage`, semantic ECS, RenderingDevice
   graph, and native Vulkan state. Do not skip a layer with renderer-side guesses.
3. Preserve authored pass order, target round-trips, blend/load/store state, visibility, and
   copy/swap dependencies. Fuse only a formally matched chain with fixed-step pixel evidence.
4. Keep object visual modulation self-only and effect visibility independently addressable.
5. Keep the Vulkan route on the exact 1.4.328 roadmap-2026 revision-11 and descriptor-heap-only
   contract. Fail explicitly when the contract is unavailable.

## Validate correctness first

1. Run the narrow unit tests for the changed semantic and add a regression test that fails under
   the old behavior.
2. Build the current converter and renderer, then convert every affected raw project into a fresh
   `/tmp` artifact. Never test with an old `.gscene`. Do not promote one fixture into a permanent
   special case.
3. Capture deterministic 4K frames with a fixed time step. Use isolated graph/prefix captures only
   to locate the first divergent write; confirm the full scene afterward.
4. Exercise authored state transitions whenever transforms, clipping, visibility, targets,
   shaders, materials, UVs, effects, or graph order can affect animated masks or composition.
5. Inspect pixels and metrics appropriate to the changed capability: composition, font metrics,
   animated mask states, intermediate targets, draw count, and strict property behavior—not just
   whether a PNG exists.

## Measure performance second

1. Before implementing performance code, measure the current complete frame with GPU timestamps,
   rank graph/pass costs, and enumerate the exact draws in each fresh plan that would hit the
   proposed path. Work on the largest unblocked cost boundary. If a higher-ranked cost is skipped,
   record its concrete semantic or architectural blocker; a theoretical fast path without measured
   hit coverage is not a valid target.
2. Use only the release renderer at 3840×2160, uncapped, for at least 10 seconds, without capture
   or GPU timing.
3. Verify the report says `frame_capture: null`, `gpu_timing: null`, `fifo-latest-ready`, and retained
   `Pss_Dirty < 40 MiB`.
4. Compare every affected performance fixture with its established same-hardware baseline.
   Investigate results below the current 140 FPS regression floor when no stronger baseline is
   recorded. Do not redefine the baseline downward or cite correctness-capture FPS as performance.
5. Use paired, order-reversed formal A/B runs. Use GPU timestamps only in a separate diagnostic run
   to attribute cost. Re-run the formal
   no-timing measurement after every candidate.
6. Revert an optimization immediately when it changes authored pixels or semantics. Remove a
   performance candidate when its paired result is not a repeatable improvement consistent with the
   measured target cost; retain its failure evidence in the architecture notes when it prevents
   repetition.

## Close the change

1. Run formatting, relevant full tests, `git diff --check`, and
   `uv run python scripts/scene_engine_constraints.py`.
2. Update the architecture document with current evidence and explicitly invalidate superseded
   conclusions. Keep concrete fixture IDs and checkpoints there or in task-local evidence, not in
   this skill or `AGENTS.md`.
3. Keep generated scenes, PNGs, plans, and reports outside Git. Keep `reverse-engineered/` ignored.
4. Stage the final working-tree state deliberately, rechecking `MM` files, and commit coherent
   verified slices with reviewable messages.
