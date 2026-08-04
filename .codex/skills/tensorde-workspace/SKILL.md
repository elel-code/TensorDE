---
name: tensorde-workspace
description: Evolve and validate the GPU-first TensorDE monorepo containing Tensor Files, Tensor Wallpaper, Tensorland, Tensor Shell, shared tensor crates, vulkan-renderer, and wayland-client-runtime. Use when changing TensorDE repository structure, shared Vulkan or client-runtime standards, compositor and DE shell surfaces or services, cross-product APIs, Tensor Wallpaper VM/evidence infrastructure, Tensor Files or Tensorland integration, dependency ownership, performance paths, or monorepo-wide policy and validation.
---

# TensorDE Workspace

## Establish scope

1. Read the root `AGENTS.md` completely, then read the nearest nested
   `AGENTS.md` for every path to be changed.
2. Inspect `git status --short`, staged diff, and unstaged diff separately.
   Preserve user changes and identify imported Tensor Wallpaper dirty files before
   staging or formatting.
3. Classify the task as product-local, shared-standard, repository migration,
   or durable-infrastructure work. Put reusable capabilities in `crates/` and
   product policy in `apps/`.

## Route domain work

- For Tensor Wallpaper scene semantics, conversion, rendering graphs, shaders, or
  performance, also use `.codex/skills/tensor-wallpaper-scene-engine/` and read the
  relevant section of `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`.
- For Tensor Files behavior, follow the Dolphin source call chain in
  `references/fika/dolphin`; use screenshots only as diagnostics, never as
  the correctness source.
- For DE shell work, keep panel, launcher, notifications/OSD, control center,
  overview, and lock surfaces in `apps/tensor-shell`. Express strict surface
  plans before wiring protocol objects and GPU rendering.
- For Tensor compositor, session, direct protocol, input, DRM/KMS, XWayland,
  ECS, or renderer work, also use `.codex/skills/tensor-compositor/` and read
  `apps/tensorland/AGENTS.md` plus the relevant record under `docs/tensorland/`.
- For shared rendering work, evolve `crates/vulkan-renderer` through typed,
  WebGPU-style descriptors while preserving explicit Vulkan semantics.

## Preserve hard contracts

- Keep Vulkan `1.4.328`, Roadmap 2026 revision 11,
  `VK_EXT_descriptor_heap`, and FIFO latest-ready required on the primary path.
  Do not add CPU, legacy descriptor-set, present-mode, or compatibility
  fallbacks.
- Use Vulkanalia through the shared renderer. Do not add direct `ash`, local
  dependency patches, or new wgpu paths.
- Keep parsing, shader generation, format lowering, and resource discovery on
  cold paths. Keep frame paths retained, allocation-bounded, and timeline-safe.
- Use modern same-name modules without `mod.rs`. Keep every Rust file at or
  below 800 lines and split by responsibility rather than arbitrary ranges.

## Handle durable local infrastructure

- Treat `docs/tensor-wallpaper`, `reverse-engineered/tensor-wallpaper`, `references/tensor-wallpaper`,
  `artifacts/tensor-wallpaper`, `references/tensor`, and `artifacts/tensor` as durable
  local state. Inventory and verify these paths before moving or deleting an
  old checkout. Tensorland's tracked design records remain under `docs/tensorland`.
- Before moving the Windows VM, confirm no QEMU/swtpm process or mount uses it.
  On the same filesystem prefer an atomic move; preserve qcow2 sparseness,
  ownership, TPM state, and Podman storage. Update every consumer to the final
  path and remove the old checkout; never retain an old-path compatibility link.
- Never stage secrets, VM state, generated scenes, traces, screenshots, or
  third-party reference repositories.

## Implement and validate

1. Update code, scripts, documentation, and workspace paths together. Avoid
   absolute paths in tracked files.
2. Run focused tests first. Establish semantic correctness before measuring
   performance.
3. Run the complete validation matrix from root `AGENTS.md`. For Tensor Wallpaper
   structural work also run
   `uv run python scripts/tensor-wallpaper/scene_engine_constraints.py` from the
   workspace root.
4. Recheck staged and unstaged diffs. Exclude preserved user work from
   structural commits and keep ignored evidence ignored.
5. Commit coherent verified slices. State exact test counts, remaining dirty
   files, and any deferred migration debt.
