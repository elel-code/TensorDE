# Scene regression gates

## Correctness authority

Use each raw project plus verified authored or Wallpaper Engine semantics. Treat historical commits,
old Tensor Wallpaper artifacts, and old screenshots as localization aids only. Never use a pre-change
`.gscene` as a correctness reference.

Choose a task-local corpus that covers every affected capability. Concrete workshop IDs, object
names, graph indices, frame checkpoints, and trace ranges belong in the task evidence or
architecture document; they are not permanent skill scope.

Use this coverage matrix to select fixtures:

| Changed capability | Required evidence |
| --- | --- |
| Parser, typed IR, ABI, or storage | Fresh conversion of inputs containing every changed variant, round-trip tests, and strict rejection tests |
| Transform, animation, puppet, or clipping | Deterministic checkpoints across meaningful authored states and loop boundaries, including mask producer/consumer resources and command order |
| Text, font, layout, or color | Authored font and raster metrics, multiline/layout extents, and parent/child modulation isolation |
| Effect, material, target, or graph | Enabled and disabled chains, effect-only and object-source layers, intermediate targets, and complete-frame composition |
| Particle, audio, script, or property | Every affected variant plus defaults, intentional disablement, duplicates, spelling/case, and type rejection |
| Runtime or Vulkan execution | Typed plan inspection, complete Vulkan resource/state/command evidence, retained-state behavior, and release performance |

## Build and fresh conversion

Build the current tools:

```text
cargo build --release --bin tensor-wallpaper-convert
cargo build --release --features rendering-device --bin tensor-wallpaper
```

For every selected raw project, create a new artifact outside Git:

```text
target/release/tensor-wallpaper-convert wallpaper-engine <raw-project-root> /tmp/<case>-current.gscene
```

Print the lowered plan whenever draw, target, shader, activation, or ordering contracts matter:

```text
target/release/tensor-wallpaper --scene-execution-plan --source /tmp/<case>-current.gscene
```

Verify the artifact reports the current `.gscene` version. A version bump replaces the previous
reader; do not add compatibility decoding, aliases, or silent translation.

## Deterministic instruction proof

Choose checkpoints from the authored timeline so the sequence covers meaningful states and loop
boundaries instead of copying fixture-specific frame numbers. For every checkpoint, retain:

- the unmodified WE renderer's canonical D3D11 instruction stream;
- the fresh Tensor Wallpaper typed backend plan;
- Tensor Wallpaper's canonical Vulkan resource/state/command stream.

Normalize API-specific spelling, but do not infer missing state. A complete comparison includes
shader bytecode or equivalent shader semantics; texture/buffer identity, contents, dimensions,
formats, subresources, and views; constant and push-constant bytes; descriptors and samplers;
RTV/DSV/UAV or dynamic-rendering attachments; load/store, clear, copy, resolve, and swap behavior;
viewport/scissor; topology and vertex/index/instance ranges; blend, color mask, depth/stencil, and
raster state; D3D11 hazard unbinds; Vulkan layouts, stage/access masks, barriers, and queue order;
and every draw, indirect draw, and dispatch argument. These inputs completely determine the
rendered result.

Use command graph/prefix ranges only to locate the first divergent resource write. Final proof must
cover every producer and consumer that can affect the completed frame. Trace-mode throughput is
never performance evidence.

Validate semantics, not trace-file existence:

- Check authored pass order, target round-trips, load/store and blend state, activation policy,
  visibility, copies/swaps, and draw count against the WE stream.
- Require disabled effect-only graphs to leave the framebuffer unchanged; do not accept a
  passthrough resample as equivalent.
- Require object visual modulation to remain self-only and effect visibility to remain independently
  addressable.
- For runtime properties, prove defaults and intentional alternatives, then prove duplicate names,
  wrong case/spelling, and wrong types fail strictly when the schema requires uniqueness.
- Do not invoke renderer readback or use screenshots, PNGs, hashes, RMSE, pixel probes, or visual
  inspection. They are derived from and less complete than the instruction evidence.

## Formal performance run

Run each selected artifact independently after correctness passes:

```text
uv run python scripts/tensor-wallpaper/scene_engine_runtime_smoke.py --duration 10 --source /tmp/<case>-current.gscene --artifact-dir /tmp/tensor-wallpaper-perf-<case>
```

Require release, 3840×2160, uncapped, command tracing disabled, `gpu_timing=null`,
`present_mode=fifo-latest-ready`, descriptor-heap-only, and retained `Pss_Dirty < 40960 KiB`.
Compare with the established same-hardware, same-command baseline. Investigate a selected fixture
below 140 FPS when no stronger baseline is recorded, and never lower a baseline to accept a
regression. Preserve run order and repeat suspicious results to distinguish thermal or order effects
from a code regression.

Use GPU timestamps only in a separate diagnostic run. Re-run the formal no-trace, no-timing
measurement after every candidate optimization.

## Repository checks

Run at minimum:

```text
cargo fmt --all
cargo test --lib
cargo test --features rendering-device --bin tensor-wallpaper
cargo check --features rendering-device --bin tensor-wallpaper
uv run python scripts/tensor-wallpaper/scene_engine_constraints.py
git diff --check
```

Inspect staged and unstaged changes separately, recheck every `MM` file, keep generated artifacts
outside Git, and commit coherent verified slices.
