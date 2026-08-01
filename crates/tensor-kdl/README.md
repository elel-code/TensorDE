# tensor-kdl

High-performance **KDL 2.0** parser, typed decode/encode, and DOM query subset
for TensorDE.

Design notes: [`docs/kdl/design.md`](../../docs/kdl/design.md).

## Features

- Hand-written single-pass reader (no chumsky/pest on the hot path)
- SWAR-assisted whitespace and string scanning (Glaze-inspired)
- Friendly `ErrorCtx` with byte offsets and `format_error`
- Optional `#[derive(Decode)]` / `#[derive(DecodeScalar)]` via `tensor-kdl-macros`
- Typed `#[derive(Encode)]` / `#[derive(EncodeScalar)]`: monomorphized dump into
  `WriteSink` only (Glaze `to::op`; no dual DOM encode path)
- Glaze-shaped write: `write` / `write_into` / `write_into_slice`
  (`ErrorCtx.consumed` = bytes written)
- Glaze-style padded input for DOM and direct typed reads
- Deliberately small KQL subset (`top()`, `>` / `>>`, `+` / `++`, `||`,
  `values()` / `props()`, equality / type RHS / ordered / string matchers —
  see `QUERY-SPEC.md`)
- Criterion benches under `benches/`; derive UI via trybuild

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

Typed encode uses the same field roles:

```rust
use tensor_kdl::{Decode, Encode, to_string};

#[derive(Debug, Decode, Encode)]
struct Version {
    #[kdl(child, unwrap(argument))]
    version: String,
}

let text = to_string(&Version { version: "2".into() }).unwrap();
assert_eq!(text, "version 2\n");
```

## Benchmarks

```bash
cargo bench -p tensor-kdl
```
