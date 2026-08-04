# TensorDE monorepo instructions

These instructions apply to the whole repository. A nested `AGENTS.md` adds
product-specific constraints and takes precedence within its directory.

## Repository boundaries

- Keep product applications under `apps/`: `tensor-files`, `gilder`, `tensorland`, and
  `tensor-shell`.
- Keep reusable protocol, GPU, resource, and scheduling standards under
  `crates/`. Do not make a shared crate depend on an application.
- Treat the DE shell as a product, not as application UI. Tensor Files' internal UI
  belongs under `apps/tensor-files/src/ui`; panel, launcher, notification/OSD, control
  center, overview, and lock surfaces belong under `apps/tensor-shell`.
- Use Rust 2024 same-name file/directory modules. Do not add `mod.rs`, numbered
  split files, or `__split` modules. Every Rust file must be at most 800 lines.

## Rendering standard

- Build the common renderer on Vulkanalia and Vulkan `1.4.328` with
  `VP_KHR_roadmap_2026` revision 11. Do not add a direct `ash` dependency.
- Require `VK_EXT_descriptor_heap` on the primary path. Do not add legacy
  descriptor-set fallback, CPU rendering fallback, or silent capability
  weakening.
- Default presentation to FIFO latest-ready. Missing required device or
  surface support is an explicit error, not a hidden present-mode fallback.
- Keep cold parsing, tessellation, decoding setup, shader preparation, and
  resource discovery outside per-frame work. Retain GPU resources and bound
  memory; use timeline retirement and bounded caches.
- Do not introduce new wgpu paths. Existing Tensor Files wgpu code is migration debt;
  replace it through `vulkan-renderer` without local backend patches.

## Correctness sources

- For Tensor Files behavior, trace the relevant Dolphin call chain under
  `references/fika/dolphin`. Do not infer correctness from screenshots.
- For Gilder scene behavior, read `apps/gilder/AGENTS.md` and use
  `$gilder-scene-engine`. Treat `reverse-engineered/gilder/` as the first
  semantic source and the authored project plus verified command stream as the
  correctness evidence.
- Preserve Gilder's ignored knowledge and evidence under `docs/gilder/`,
  `reverse-engineered/gilder/`, `references/gilder/`, and `artifacts/gilder/`.
  Never force add them. The Windows VM, qcow2 images, TPM state, Podman store,
  workshop corpus, and traces are durable development infrastructure, not
  disposable build output.
- For Tensorland compositor work, read `apps/tensorland/AGENTS.md` and use
  `$tensor-compositor`. Keep Tensor's ignored evidence under
  `references/tensor/` and `artifacts/tensor/`, and its tracked design records
  under `docs/tensorland/`.
- Use `target/` only for reproducible build output. Do not store irreplaceable
  evidence there.

## Worktree and structure discipline

- Inspect staged and unstaged changes separately before editing. Preserve user
  work, especially Gilder scene-engine changes imported during migration.
- Keep changes semantic and reviewable. Separate repository migration, engine
  behavior, renderer standard changes, evidence maintenance, and line-limit
  refactors when their validation differs.
- Update paths in scripts and documents when moving a product. Avoid absolute
  repository paths in new tracked files.
- Do not run GUI or screenshot validation unless the user explicitly requests
  it. Prefer unit tests, protocol state, render plans, and command streams.

## Required validation

Run focused tests while iterating. Before committing a completed slice, run:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo doc -p vulkan-renderer --no-deps
scripts/check-rust-file-lines.sh
git diff --check
uv run python scripts/gilder/scene_engine_constraints.py
```

When a command is inapplicable to a documentation-only or ignored-evidence
change, record why instead of weakening the gate.
