---
name: tensor-wallpaper-scene-engine
description: Validate and evolve Tensor Wallpaper's scene engine across Wallpaper Engine ingest, typed IR, semantic ECS, RenderingDevice graphs, shaders, and renderer-owned command execution. Use when changing scene semantics, `.gscene` conversion or ABI, effects, targets, materials, text, particles, animation or clipping, visibility, runtime properties, visual correctness, or rendering performance for any scene wallpaper.
---

# Tensor Wallpaper Scene Engine

Use this workflow to keep semantic correctness and performance evidence coupled without treating
old Tensor Wallpaper output as the specification.

## Establish the contract

1. Read the repository `AGENTS.md` and the relevant section of
   `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`.
2. Read [regression-gates.md](references/regression-gates.md) before modifying
   or validating scene semantics, rendering, conversion, or performance.
3. Inspect `git status`, the staged diff, and the unstaged diff separately. Preserve unrelated
   edits and identify every `MM` file before staging.
4. State the authored semantic being preserved and select a task-local regression corpus by
   affected capabilities. Use raw project assets and verified WE evidence; use historical
   commits/captures only to locate a regression.
5. Reject any proposed old-reader, alias, fallback, permissive parser, compatibility shader, or
   sample-specific branch. Remove obsolete behavior instead of maintaining two paths.

## Preserve recovery state

1. Treat conversation context as a volatile cache. Keep the current task's durable recovery ledger
   in the ignored architecture document, never only in chat or an automatically generated summary.
2. Record the current HEAD and tracked worktree state, exact evidence paths, commands and results,
   proven facts, invalidated hypotheses, remaining blockers, and one next executable action.
3. Update the ledger after every material discovery, failure, semantic decision, performance run,
   revert, and commit. Write a checkpoint before long-running commands or likely context compaction.
4. After a restart or compressed-context handoff, read the ledger first, validate its HEAD and
   artifacts against the filesystem, then continue from the recorded next action without repeating
   completed work.
5. Keep concrete fixtures and task conclusions out of this skill. The ledger supplements formal
   instruction and performance evidence; it never substitutes for those gates.

## Implement through typed boundaries

1. Put parsing, script-property application, font rasterization, and asset lowering in the convert
   cold path.
2. Carry facts explicitly through typed IR, `.gscene`, `SceneStorage`, semantic ECS, RenderingDevice
   graph, and renderer-owned resource/command state. Do not skip a layer with renderer-side guesses.
3. Preserve authored pass order, target round-trips, blend/load/store state, visibility, and
   copy/swap dependencies. Fuse only a chain whose complete resource/state/command behavior is
   formally matched at every affected boundary.
4. Keep object visual modulation self-only and effect visibility independently addressable.
5. Keep the Vulkan route on the exact 1.4.328 roadmap-2026 revision-11 and descriptor-heap-only
   contract. Fail explicitly when the contract is unavailable.

## Validate correctness first

1. Run the narrow unit tests for the changed semantic and add a regression test that fails under
   the old behavior.
2. Build the current converter and renderer, then convert every affected raw project into a fresh
   `/tmp` artifact. Never test with an old `.gscene`. Do not promote one fixture into a permanent
   special case.
3. At deterministic authored checkpoints, capture the complete verified WE D3D11 instruction
   stream and the complete Tensor Wallpaper typed-plan/Vulkan instruction stream. Compare shader code,
   resources, constants, descriptors, targets, copies, barriers, fixed-function state, ordering,
   and draw/dispatch arguments. Instruction slices may locate the first divergence; confirm the
   complete relevant frame stream afterward.
4. Exercise authored state transitions whenever transforms, clipping, visibility, targets,
   shaders, materials, UVs, effects, or graph order can affect animated masks or composition.
5. Prove the command inputs that determine composition, font metrics, animated masks,
   intermediate targets, draw count, and strict property behavior. Do not use screenshots,
   renderer readback, PNGs, hashes, RMSE, or visual inspection as correctness evidence.

## Measure performance second

1. Before implementing performance code, measure the current complete frame with GPU timestamps,
   rank graph/pass costs, and enumerate the exact draws in each fresh plan that would hit the
   proposed path. Work on the largest unblocked cost boundary. If a higher-ranked cost is skipped,
   record its concrete semantic or architectural blocker; a theoretical fast path without measured
   hit coverage is not a valid target.
2. Use only the release renderer at 3840×2160, uncapped, for at least 10 seconds, without command
   tracing or GPU timing.
3. Verify the report says `gpu_timing: null`, `fifo-latest-ready`, and retained
   `Pss_Dirty < 40 MiB`.
4. Compare every affected performance fixture with its established same-hardware baseline.
   Investigate results below the current 140 FPS regression floor when no stronger baseline is
   recorded. Do not redefine the baseline downward or cite instruction-trace FPS as performance.
5. Use paired, order-reversed formal A/B runs. Use GPU timestamps only in a separate diagnostic run
   to attribute cost. Re-run the formal
   no-timing measurement after every candidate.
6. Revert an optimization immediately when it changes authored resource/state/command semantics.
   Remove a performance candidate when its paired result is not a repeatable improvement consistent
   with the measured target cost; retain its failure evidence in the architecture notes when it
   prevents repetition.

## Close the change

1. Run formatting, relevant full tests, `git diff --check`, and
   `uv run python scripts/tensor-wallpaper/scene_engine_constraints.py`.
2. Update the architecture document with current evidence and explicitly invalidate superseded
   conclusions. Keep concrete fixture IDs and checkpoints there or in task-local evidence, not in
   this skill or `AGENTS.md`.
3. Keep generated scenes, command traces, plans, and reports outside Git. Keep
   `reverse-engineered/tensor-wallpaper/` ignored.
4. Stage the final working-tree state deliberately, rechecking `MM` files, and commit coherent
   verified slices with reviewable messages.
