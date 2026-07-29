# tensor-kdl

High-performance **KDL 2.0** parser and typed decode for TensorDE.

Design notes: [`docs/kdl/design.md`](../../docs/kdl/design.md).

## Features

- Hand-written single-pass reader (no chumsky/pest on the hot path)
- SWAR-assisted whitespace and string scanning (Glaze-inspired)
- Friendly `ErrorCtx` with byte offsets and `format_error`
- Optional `#[derive(Decode)]` / `#[derive(DecodeScalar)]` via `tensor-kdl-macros`
- Criterion benches under `benches/`

## Quick start

```rust
use tensor_kdl::{from_str, Document};

let doc: Document = from_str(r#"
    package {
        name my-pkg
        version "1.2.3"
    }
"#).unwrap();
```

Typed decode:

```rust
use tensor_kdl::Decode;

#[derive(Debug, Decode)]
struct Package {
    #[kdl(child, unwrap(argument))]
    name: String,
    #[kdl(child, unwrap(argument))]
    version: String,
}

#[derive(Debug, Decode)]
struct Root {
    #[kdl(child)]
    package: Package,
}
```

## Benchmarks

```bash
cargo bench -p tensor-kdl
```
