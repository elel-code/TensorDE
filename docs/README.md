# TensorDE documentation

Product documentation has one canonical location:

- [Tensor Files](tensor-files/README.md) — file-manager behavior, Dolphin call-chain notes,
  performance alignment, and refactor records.
- [Tensor Wallpaper](tensor-wallpaper/README.md) — scene-engine semantics, reverse-engineering
  conclusions, and performance evidence policy.
- [Tensorland](tensorland/README.md) — compositor architecture, protocols, rendering,
  startup, configuration, and testing.
- [Tensor Shell](tensor-shell/architecture.md) — desktop surface semantics,
  [functional alignment](tensor-shell/alignment.md), product-local pass planning,
  and shared-renderer boundaries.
- [Tensor Launcher](../apps/tensor-launcher/README.md) — desktop-entry discovery,
  retained search, and standalone launcher ownership.
- [Tensor Greeter](../apps/tensor-greeter/README.md) — greetd protocol, bounded
  authentication state, and the pre-session security boundary.
- [Tensor Settings](../apps/tensor-settings/README.md) — standalone settings
  ownership, product schema boundaries, and reload routing.
- [Tensor Idle](../apps/tensor-idle/README.md) — AC/battery idle policy and the
  lock, output-power, and suspend boundary.
- [Tensor control plane](control-plane.md) — product-owned IPC/session servers and
  the independently packaged `tensor-msg` frontend.
- [Tensor XDP](tensor-xdp/README.md) — portal capability publication,
  Compio-native D-Bus ownership, configuration, and compositor/Shell gates.
- [KDL](kdl/README.md) — shared KDL 2.0 crate design (Glaze-style performance,
  typed decoding, diagnostics, and Tensorland's shipped configuration format).

Third-party source trees belong under `references/<product>/`; generated or
machine-local evidence belongs under `artifacts/<product>/`; Tensor Wallpaper's durable
disassembly and reconstructed semantics belong under `reverse-engineered/tensor-wallpaper/`.
Do not duplicate those trees under an application directory or retain links to
an old checkout.
