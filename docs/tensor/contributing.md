# Contributing

The project is pre-release. Prefer a coherent breaking refactor over a compatibility layer that
would preserve a design already known to be wrong.

Use module-first commit messages:

```text
where: imperative summary
```

Examples are `render: require descriptor heap`, `ecs: stabilize workspace ordering`, and
`docs: record startup gates`. Do not require `feat():` Conventional Commit prefixes. Add a concise
body for non-obvious tradeoffs and list the verification commands used.

Hand-written source files are limited to 800 lines by `uv run scripts/tensor/check_file_lines.py`. Generated
protocol bindings and explicit data-heavy fixtures may be excluded only with a documented reason.
Dependency ranges use broad compatible major/minor constraints, never `"*"`.

The complete workspace must pass `uv run scripts/tensor/check_crate_boundaries.py`. No package may depend
on or import Smithay, and no adapter crate or compatibility feature may reintroduce it. The same
check requires `tensor-runtime` to expose Compio async operations with the io_uring driver and
rejects direct readiness-reactor dependencies. Compio's defaults and `polling` feature remain
disabled.

TTY builds still compile the remaining descriptor-heap client and cursor GLSL
with `glslangValidator`. Migrated Slang sources instead ship checked-in,
validated SPIR-V, so users and binary packages do not need Slang, LLVM, or
SPIR-V Tools at runtime. Shader authors regenerating those assets need the
exact `slangc` version declared by `vulkan-renderer-build` plus `spirv-val`.
On CachyOS/Arch, install the shader compiler with:

```sh
sudo pacman -S shader-slang
```

The similarly named `slang` package is the unrelated JedSoft interpreter.
Regenerate and byte-for-byte verify Tensor's first Slang shader with:

```sh
cargo run -p vulkan-renderer-build -- \
  compile apps/tensor/shaders/focus_ring.slang vertexMain vertex \
  apps/tensor/shaders/spirv/focus_ring.vert.spv 64 descriptor-free
cargo run -p vulkan-renderer-build -- \
  compile apps/tensor/shaders/focus_ring.slang fragmentMain fragment \
  apps/tensor/shaders/spirv/focus_ring.frag.spv 64 descriptor-free
cargo run -p vulkan-renderer-build -- \
  verify apps/tensor/shaders/focus_ring.slang vertexMain vertex \
  apps/tensor/shaders/spirv/focus_ring.vert.spv 64 descriptor-free
cargo run -p vulkan-renderer-build -- \
  verify apps/tensor/shaders/focus_ring.slang fragmentMain fragment \
  apps/tensor/shaders/spirv/focus_ring.frag.spv 64 descriptor-free
```

Default features also link system `libudev`, `libinput`, `libseat`, `libdrm`, `libgbm`,
`libwayland-server`, and `libxkbcommon`. TTY builds require **libinput ≥ 1.26** (tablet-pad
dials and device bus type). Ubuntu 24.04 only provides 1.25, so use a newer distro or
build libinput yourself. On Debian/Ubuntu install at least:

```sh
sudo apt-get install -y \
  pkg-config \
  libudev-dev \
  libinput-dev \
  libseat-dev \
  libdrm-dev \
  libgbm-dev \
  libwayland-dev \
  libxkbcommon-dev \
  glslang-tools
```

The Tensor GitHub Actions workflow runs on `ubuntu-26.04` and installs the same set before
`cargo test`.
