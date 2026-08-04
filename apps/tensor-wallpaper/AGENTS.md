# Tensor Wallpaper repository instructions

These instructions apply to the entire repository.

## Non-negotiable contracts

- Target `Vulkan 1.4.328 + VP_KHR_roadmap_2026 revision 11`. Do not weaken the
  Vulkan 2026 profile, descriptor-heap-only model, or FIFO-latest-ready present
  contract with a fallback.
- Treat every behavior added to accept, emulate, alias, or silently translate
  previous Tensor Wallpaper code or data as a bug. Do not add old `.gscene` readers,
  schema aliases, compatibility branches, permissive parsing, legacy shader
  keys, CPU renderer fallbacks, or sample-specific repair paths. Delete obsolete
  behavior instead of retaining both paths.
- Use authored project data and verified WE semantics as correctness sources.
  Historical commits, old `.gscene` files, old screenshots, and previous Tensor Wallpaper
  output are diagnostic evidence only; they are not correctness authorities.
- Do not hard-code workshop IDs, object IDs, object names, graph indices, or
  current pixel colors in production logic. Express the underlying typed
  semantic or reject the unsupported input explicitly.
- Preserve independent runtime effect visibility. Object `color/alpha` is
  self-only visual modulation; parents propagate transform and visibility, not
  visual color.

## Architecture boundaries

- Keep the main path explicit:
  `we_ingest -> typed IR -> .gscene -> SceneStorage -> semantic ECS -> RenderingDevice graph -> Vulkan`.
- Keep convert work on the cold path and per-frame work on retained GPU/runtime
  state. Do not move font rasterization, asset parsing, graph reconstruction, or
  descriptor discovery into the frame loop.
- Preserve authored target, pass, blend, load/store, copy/swap, and ordering
  semantics. Fuse passes only after proving the transformed resource and
  command stream semantically equivalent at the affected boundary; fewer draws
  alone is not evidence.
- Follow Godot-style ownership boundaries without copying compatibility
  behavior. Keep renderer handles out of semantic ECS and Vulkan decisions out
  of the binary/IR layers.
- Keep Rust files at or below 800 lines, use semantic same-name file/directory
  modules, and do not add `mod.rs` or mechanical `__split` files. Run
  `uv run python scripts/tensor-wallpaper/scene_engine_constraints.py` after structural changes.

## Scene regression selection

Always convert affected raw projects again with the current converter. Do not
reuse a pre-change `.gscene`.

- Select a task-local corpus by authored capabilities, not by workshop ID,
  object name, or a permanently privileged sample. Cover every semantic and
  renderer boundary touched by the change.
- For animation, transforms, clipping, visibility, targets, shaders, texture
  coordinates, or ordering, include deterministic checkpoints that exercise
  meaningful authored state transitions such as open/closed masks and loop
  boundaries.
- For text or visual modulation, include authored fonts and metrics,
  multiline/layout cases, and parent/child color-isolation cases.
- For effects and framebuffer graphs, include enabled and disabled chains,
  intermediate target round-trips, and effect-only layers. A disabled chain
  must not alter scene pixels merely by resampling them.
- For particles, audio, scripts, or user properties, include all affected
  variants plus strict default, enabled, intentionally disabled, duplicate,
  spelling, case, and type behavior where applicable.

Use complete instruction evidence for correctness. At each deterministic
authored checkpoint, compare the verified WE D3D11 resource/state/command
stream with the fresh Tensor Wallpaper typed plan and Vulkan resource/state/command
stream. The compared evidence must include shader code, resource contents and
formats, constants, descriptors, targets, load/store/copy operations, barriers,
blend/depth/raster state, viewport/scissor, ordering, and draw/dispatch/indirect
arguments. A complete stream determines the rendered result; screenshots,
renderer readback, PNGs, hashes, RMSE, and visual inspection add no correctness
information and must not be used as a regression gate. Instruction graph/prefix
slices may locate the first divergent command, but the final proof must cover
the complete relevant frame command stream. Record concrete trace paths and
checkpoints in task evidence or the architecture document, not as permanent
repository policy.

## Performance evidence

- Before implementing an optimization, measure the current full-frame GPU
  timestamps, rank graph/pass costs, and enumerate the exact fresh-plan draws
  that would use the proposed path. Select the largest unblocked cost boundary;
  if a larger cost remains, record the concrete semantic or architectural
  blocker before working on a smaller one. A generic feature name or theoretical
  fast path without measured hit coverage is not an optimization target.
- Measure only a release binary at `3840x2160`, uncapped, for at least 10
  seconds, with command tracing and GPU timing disabled. Verify
  `gpu_timing: null`, `fifo-latest-ready`, and retained `Pss_Dirty < 40 MiB`.
- Treat traced-command FPS, debug FPS, timestamp-query FPS, and first-frame
  pipeline compilation as diagnostic data, never as the performance result.
- Investigate and do not accept any scene in the affected performance corpus
  below its established same-hardware baseline. Roughly 140 FPS is the current
  minimum regression floor when no stronger per-scene baseline is recorded; do
  not lower a baseline to make a regression pass. The longer-term project
  target remains 4K 240 Hz.
- Establish correctness before performance A/B. If an optimization changes the
  authored resource/state/command semantics, including targets, visibility,
  animated-mask behavior, or text metrics, revert it before reporting its
  speedup.
- Use paired, order-reversed formal A/B runs for a candidate. If it does not
  produce a repeatable improvement consistent with the measured target cost,
  remove the candidate before selecting the next target.

## Worktree and commits

- Inspect both staged and unstaged diffs before editing. Preserve unrelated user
  changes, especially files reported as `MM`.
- Keep `reverse-engineered/tensor-wallpaper/` on disk but ignored by Git. Do not force-add it.
- Store generated `.gscene`, command traces, plans, and performance reports under
  `/tmp` or ignored artifact directories; do not commit them.
- Run focused tests while iterating, then `cargo fmt --all`, relevant full tests,
  `git diff --check`, and the scene constraints audit.
- Commit coherent, verified slices at useful checkpoints. Separate engine
  semantics/runtime changes, repository ignore cleanup, and agent/skill files
  when their review boundaries differ.

## Durable recovery state

- Treat chat context and generated summaries as volatile. For any multi-step
  scene-engine task, maintain a task-local recovery ledger in the ignored
  architecture document. Record the current HEAD, tracked worktree state,
  exact artifact paths, commands and results, established facts, disproved
  hypotheses, unresolved blockers, and the next executable action.
- Update that ledger immediately after each material discovery, failed run,
  semantic decision, performance measurement, revert, and commit. Checkpoint it
  before a long-running command or any point where context compaction could
  interrupt the task.
- On resume, read the ledger and verify its HEAD, worktree state, and artifact
  existence before acting. Do not restart completed work or replace missing
  evidence with assumptions from a conversation summary.
- Keep this policy and the repository skill sample-independent. Concrete scene
  IDs, checkpoints, trace paths, performance numbers, and current hypotheses
  belong only in the task ledger and ignored evidence directories.

For the exact local workflow and commands, use the repository skill
`$tensor-wallpaper-scene-engine` in `.codex/skills/tensor-wallpaper-scene-engine/`.
