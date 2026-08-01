# KDL library documentation

Design and notes for TensorDE’s dedicated **KDL 2.0** crate (high-performance
parse, process-macro typed decode/encode, and a documented KQL subset).

| Doc | Description |
|-----|-------------|
| [design.md](design.md) | Architecture, Glaze performance model, knus-inspired proc macros, phases |

## Local references (gitignored)

- Spec & tests: `references/kdl/`
- Glaze source: `references/glaze/`

## Status

- Design: [design.md](design.md)
- Glaze mechanical contract: [glaze-alignment.md](glaze-alignment.md) (cite before perf/error changes)
- Crates: `crates/tensor-kdl`, `crates/tensor-kdl-macros`
- Tests: `cargo test -p tensor-kdl`
- Official suite (needs `references/kdl`): included in tests; strict mode via `TENSOR_KDL_STRICT_SUITE=1`
- Conformance snapshot: **243/243** parse, **95/95** reject, **243/243** roundtrip
- Padded direct reads: `PaddedInput` + runtime/const options
- Typed write: monomorphized `write_node` / `WriteSink` dump (Glaze `to::op`);
  `Encode` / `EncodeScalar` / `EncodePartial` + `write` / `write_into` /
  `write_into_slice`
- Query subset (`QUERY-SPEC.md`): `top()`, `>` / `>>`, `+` / `++`, `||`,
  existence / `values()` / `props()`, `=` / `!=`, value type RHS `(tag)`,
  ordered compares, string `^=` / `$=` / `*=` (not full KQL)
- Macro UI: `cargo test -p tensor-kdl --test trybuild_ui`
- Benches: `cargo bench -p tensor-kdl`
- Suite report tool: `cargo run -p tensor-kdl --example suite_report`
