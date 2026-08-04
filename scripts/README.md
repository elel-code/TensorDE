# TensorDE scripts

Run repository automation from the workspace root. Product-specific tools have
one canonical location:

- [Tensor Files scripts](tensor-files/README.md)
- [Gilder scripts](gilder/README.md)
- [Tensorland scripts](tensorland/README.md)

The monorepo-wide Rust line gate is
`scripts/check-rust-file-lines.sh`. New long-lived policy should be implemented
in Rust or Python as appropriate to its owner; do not add old-checkout wrappers
or compatibility launchers.
