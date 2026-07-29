# TensorDE documentation

Product documentation has one canonical location:

- [Fika](fika/README.md) — file-manager behavior, Dolphin call-chain notes,
  performance alignment, and refactor records.
- [Gilder](gilder/README.md) — scene-engine semantics, reverse-engineering
  conclusions, and performance evidence policy.
- [Tensor](tensor/README.md) — compositor architecture, protocols, rendering,
  startup, configuration, and testing.
- [KDL](kdl/README.md) — shared KDL 2.0 crate design (Glaze-style performance,
  knus-inspired process macros). Not Tensor’s shipped config format (TOML).

Third-party source trees belong under `references/<product>/`; generated or
machine-local evidence belongs under `artifacts/<product>/`; Gilder's durable
disassembly and reconstructed semantics belong under `reverse-engineered/gilder/`.
Do not duplicate those trees under an application directory or retain links to
an old checkout.
