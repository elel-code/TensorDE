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

Hand-written source files are limited to 800 lines by `uv run scripts/check_file_lines.py`. Generated
protocol bindings and explicit data-heavy fixtures may be excluded only with a documented reason.
Dependency ranges use broad compatible major/minor constraints, never `"*"`.

The complete workspace must pass `uv run scripts/check_crate_boundaries.py`. No package may depend
on or import Smithay, and no adapter crate or compatibility feature may reintroduce it. The same
check requires `tensor-runtime` to expose Compio async operations with the io_uring driver and
rejects direct readiness-reactor dependencies. Compio's defaults and `polling` feature remain
disabled.

TTY builds compile the descriptor-heap client shaders with `glslangValidator`; install the Vulkan
shader tools before running the renderer-enabled checks. The generated SPIR-V is build output and
does not bypass the 800-line gate.
