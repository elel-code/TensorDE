# TensorDE documentation

Product documentation has one canonical location:

- [Tensor Files](tensor-files/README.md) — file-manager behavior, Dolphin call-chain notes,
  performance alignment, and refactor records.
- [Gilder](gilder/README.md) — scene-engine semantics, reverse-engineering
  conclusions, and performance evidence policy.
- [Tensorland](tensorland/README.md) — compositor architecture, protocols, rendering,
  startup, configuration, and testing.
- [Tensor Shell](tensor-shell/architecture.md) — desktop surface semantics,
  [functional alignment](tensor-shell/alignment.md), product-local pass planning,
  and shared-renderer boundaries.
- [KDL](kdl/README.md) — shared KDL 2.0 crate design (Glaze-style performance,
  typed decoding, diagnostics, and Tensorland's shipped configuration format).

Third-party source trees belong under `references/<product>/`; generated or
machine-local evidence belongs under `artifacts/<product>/`; Gilder's durable
disassembly and reconstructed semantics belong under `reverse-engineered/gilder/`.
Do not duplicate those trees under an application directory or retain links to
an old checkout.
