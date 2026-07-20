# Gilder repository instructions

These instructions apply to the entire repository.

## Non-negotiable contracts

- Target `Vulkan 1.4.328 + VP_KHR_roadmap_2026 revision 11`. Do not weaken the
  Vulkan 2026 profile, descriptor-heap-only model, or FIFO-latest-ready present
  contract with a fallback.
- Treat every behavior added to accept, emulate, alias, or silently translate
  previous Gilder code or data as a bug. Do not add old `.gscene` readers,
  schema aliases, compatibility branches, permissive parsing, legacy shader
  keys, CPU renderer fallbacks, or sample-specific repair paths. Delete obsolete
  behavior instead of retaining both paths.
- Use authored project data and verified WE semantics as correctness sources.
  Historical commits, old `.gscene` files, old screenshots, and previous Gilder
  output are diagnostic evidence only; they are not correctness authorities.
- Do not hard-code workshop IDs, object IDs, object names, graph indices, or
  current pixel colors in production logic. Express the underlying typed
  semantic or reject the unsupported input explicitly.
- Preserve independent runtime effect visibility. Object `color/alpha` is
  self-only visual modulation; parents propagate transform and visibility, not
  visual color.

## Architecture boundaries

- Keep the main path explicit:
  `we_ingest -> typed IR -> .gscene -> SceneStorage -> semantic ECS -> RenderingDevice graph -> native Vulkan`.
- Keep convert work on the cold path and per-frame work on retained GPU/runtime
  state. Do not move font rasterization, asset parsing, graph reconstruction, or
  descriptor discovery into the frame loop.
- Preserve authored target, pass, blend, load/store, copy/swap, and ordering
  semantics. Fuse passes only after proving the transformation pixel-equivalent
  at the affected boundary; fewer draws alone is not evidence.
- Follow Godot-style ownership boundaries without copying compatibility
  behavior. Keep renderer handles out of semantic ECS and Vulkan decisions out
  of the binary/IR layers.
- Keep Rust files at or below 1000 lines, use semantic same-name file/directory
  modules, and do not add `mod.rs` or mechanical `__split` files. Run
  `uv run python scripts/scene_engine_constraints.py` after structural changes.

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

Use deterministic 4K fixed-step captures for correctness. Graph/prefix captures
are diagnostic tools for locating the first divergent write; always confirm the
complete frame. Record concrete fixture paths and checkpoints in task evidence
or the architecture document, not as permanent repository policy.

## Performance evidence

- Measure only a release binary at `3840x2160`, uncapped, for at least 10
  seconds, with capture and GPU timing disabled. Verify `frame_capture: null`,
  `gpu_timing: null`, `fifo-latest-ready`, and retained `Pss_Dirty < 40 MiB`.
- Treat capture FPS, debug FPS, timestamp-query FPS, and first-frame pipeline
  compilation as diagnostic data, never as the performance result.
- Investigate and do not accept any scene in the affected performance corpus
  below its established same-hardware baseline. Roughly 140 FPS is the current
  minimum regression floor when no stronger per-scene baseline is recorded; do
  not lower a baseline to make a regression pass. The longer-term project
  target remains 4K 240 Hz.
- Establish correctness before performance A/B. If an optimization changes
  pixels, target semantics, visibility, animated-mask behavior, or text metrics,
  revert it before reporting its speedup.

## Worktree and commits

- Inspect both staged and unstaged diffs before editing. Preserve unrelated user
  changes, especially files reported as `MM`.
- Keep `reverse-engineered/` on disk but ignored by Git. Do not force-add it.
- Store generated `.gscene`, captures, plans, and performance reports under
  `/tmp` or ignored artifact directories; do not commit them.
- Run focused tests while iterating, then `cargo fmt --all`, relevant full tests,
  `git diff --check`, and the scene constraints audit.
- Commit coherent, verified slices at useful checkpoints. Separate engine
  semantics/runtime changes, repository ignore cleanup, and agent/skill files
  when their review boundaries differ.

For the exact local workflow and commands, use the repository skill
`$gilder-scene-engine` in `.codex/skills/gilder-scene-engine/`.
