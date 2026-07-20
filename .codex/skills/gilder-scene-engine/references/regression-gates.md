# Scene regression gates

## Correctness authority

Use each raw project plus verified authored or Wallpaper Engine semantics. Treat historical commits,
old Gilder artifacts, and old screenshots as localization aids only. Never use a pre-change
`.gscene` as a correctness reference.

Choose a task-local corpus that covers every affected capability. Concrete workshop IDs, object
names, graph indices, frame checkpoints, hashes, and pixel regions belong in the task evidence or
architecture document; they are not permanent skill scope.

Use this coverage matrix to select fixtures:

| Changed capability | Required evidence |
| --- | --- |
| Parser, typed IR, ABI, or storage | Fresh conversion of inputs containing every changed variant, round-trip tests, and strict rejection tests |
| Transform, animation, puppet, or clipping | Fixed-step checkpoints across meaningful authored states and loop boundaries, including mask producer/consumer order |
| Text, font, layout, or color | Authored font and raster metrics, multiline/layout extents, and parent/child modulation isolation |
| Effect, material, target, or graph | Enabled and disabled chains, effect-only and object-source layers, intermediate targets, and complete-frame composition |
| Particle, audio, script, or property | Every affected variant plus defaults, intentional disablement, duplicates, spelling/case, and type rejection |
| Runtime or Vulkan execution | Typed plan inspection, retained-state behavior, full-frame pixels, and release performance |

## Build and fresh conversion

Build the current tools:

```text
cargo build --release --bin gilder-convert
cargo build --release --features native-vulkan-renderer --bin gilder-native-vulkan
```

For every selected raw project, create a new artifact outside Git:

```text
target/release/gilder-convert wallpaper-engine <raw-project-root> /tmp/<case>-current.gscene
```

Print the lowered plan whenever draw, target, shader, activation, or ordering contracts matter:

```text
target/release/gilder-native-vulkan --scene-backend-plan --source /tmp/<case>-current.gscene
```

Verify the artifact reports the current `.gscene` version. A version bump replaces the previous
reader; do not add compatibility decoding, aliases, or silent translation.

## Deterministic correctness capture

Capture native 4K frames with a fixed step. Choose frame numbers from the authored timeline so the
sequence covers the relevant states instead of copying fixture-specific checkpoints:

```text
target/release/gilder-native-vulkan --run-scene --source /tmp/<case>-current.gscene --duration <seconds> --no-fps-limit --surface-width 3840 --surface-height 2160 --capture-frame /tmp/<case>-frame.png --capture-frame-number <frame> --capture-frame-time-step 0.016666667
```

Use `GILDER_NATIVE_VULKAN_CAPTURE_GRAPH_PREFIX=N` only to find the first divergent graph. Use
`--capture-scene-graph N` to inspect one graph. Remove both selectors for the final full-frame
proof. Capture FPS is never performance evidence.

Validate semantics, not file existence:

- Inspect the complete composition at native resolution and the smallest useful pixel regions.
- Compare deterministic frames or regions with hashes, RMSE, extents, and targeted pixel probes.
- Check authored pass order, target round-trips, load/store and blend state, activation policy,
  visibility, copies/swaps, and draw count.
- Require disabled effect-only graphs to leave the framebuffer unchanged; do not accept a
  passthrough resample as equivalent.
- Require object visual modulation to remain self-only and effect visibility to remain independently
  addressable.
- For runtime properties, prove defaults and intentional alternatives, then prove duplicate names,
  wrong case/spelling, and wrong types fail strictly when the schema requires uniqueness.

## Formal performance run

Run each selected artifact independently after correctness passes:

```text
uv run python scripts/scene_engine_runtime_smoke.py --duration 10 --source /tmp/<case>-current.gscene --artifact-dir /tmp/gilder-perf-<case>
```

Require release, 3840×2160, uncapped, `frame_capture=null`, `gpu_timing=null`,
`present_mode=fifo-latest-ready`, descriptor-heap-only, and retained `Pss_Dirty < 40960 KiB`.
Compare with the established same-hardware, same-command baseline. Investigate a selected fixture
below 140 FPS when no stronger baseline is recorded, and never lower a baseline to accept a
regression. Preserve run order and repeat suspicious results to distinguish thermal or order effects
from a code regression.

Use GPU timestamps only in a separate diagnostic run. Re-run the formal no-capture, no-timing
measurement after every candidate optimization.

## Repository checks

Run at minimum:

```text
cargo fmt --all
cargo test --lib
cargo test --features native-vulkan-renderer --bin gilder-native-vulkan
cargo check --features native-vulkan-renderer --bin gilder-native-vulkan
uv run python scripts/scene_engine_constraints.py
git diff --check
```

Inspect staged and unstaged changes separately, recheck every `MM` file, keep generated artifacts
outside Git, and commit coherent verified slices.
