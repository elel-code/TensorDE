# Tensor scripts

Run these commands from the TensorDE workspace root.

## Repository gates

```sh
uv run scripts/tensor/check_file_lines.py
uv run scripts/tensor/check_crate_boundaries.py
```

`check_file_lines.py` covers Tensor and the shared `tensor-*` crates.
`check_crate_boundaries.py` enforces the Smithay-free, completion-only runtime
contract.

## TTY and hardware validation

- `uv run scripts/tensor/tty.py` is the canonical TTY/KMS launcher.
- `scripts/tensor/tty-all-clients.sh` launches the standard bounded client
  matrix.
- `tty_clients.py` and `tty_support.py` are implementation modules imported by
  `tty.py`; they are not separate entry points.

Logs and hardware evidence are written under `artifacts/tensor/`. There is no
launcher at an old Tensor checkout path.
