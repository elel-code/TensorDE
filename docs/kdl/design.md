# Tensor KDL crate design

Status: **implementation active** — suite **243/243**. Glaze contract:
[glaze-alignment.md](glaze-alignment.md). Through **P-G13**: nested
`unwrap` peels on the visit path; document-root `read_stream` without `Node`
(P-G12); padded typed reads (P-G11); monomorphized `WriteSink` dump
(`push_byte_n` / grow-on-full). KQL is an explicitly incomplete subset of
`references/kdl/QUERY-SPEC.md` (includes `name()`/`tag()`, stacked matchers,
keyword float RHS). Derive UI: trybuild. Stage benches: `cargo bench -p tensor-kdl`.
Audience: implementers of a high-performance, error-friendly KDL 2.0 library for TensorDE.  
Language: KDL **2.0.0** (finalized). Process macros are a first-class surface, not an afterthought.

## 1. Goals

| Goal | Meaning |
|------|---------|
| **Correct KDL 2.0** | Parse and emit according to `references/kdl/draft-marchan-kdl2.md`. Official test suite is the gate. |
| **Glaze-class performance** | Single-pass, direct-to-memory decode; SWAR hot paths; reuse scratch/context; compile-time field dispatch. |
| **Friendly diagnostics** | Byte offset + line/column + labeled spans + expected-token hints; usable without a DOM dump. |
| **Typed config via proc macros** | `Decode` / `DecodeScalar` and `Encode` / `EncodeScalar` derives map Rust types to nodes/args/props/children. |
| **Zero/low mandatory deps** | Core parser has no chumsky/pest stack. Diagnostics and fancy printing are feature-gated. |
| **Rust idioms** | Traits + derive + `Result`/`ErrorCtx`, not C++ templates; still mirror Glaze’s *ideas*. |

## 2. Non-goals (v1)

- Full KDL Query Language (`QUERY-SPEC.md`) or Schema Language (`SCHEMA-SPEC.md`); the crate exposes only a documented KQL subset.
- Drop-in API compatibility with `knus` / `knuffel` / `kdl` crates.
- Silent KDL 1.0 acceptance (optional `kdl1` feature may be a later migration aid).
- Runtime reflection (no `Any`, no serde-data-model indirection on the hot path).
- Streaming multi-GB documents (bounded configs first; streaming is a later Glaze-style extension).

## 3. Reference sources (local)

All under gitignored `references/` (do not force-add):

| Source | Path | Role |
|--------|------|------|
| **KDL 2.0 spec** | `references/kdl/draft-marchan-kdl2.md` | Authoritative grammar & semantics |
| KDL 1.0 | `references/kdl/SPEC_v1.md` | Migration / compatibility notes only |
| Official tests | `references/kdl/tests/test_cases/` | ~338 input / ~243 expected; conformance |
| Examples | `references/kdl/examples/` | Realistic documents |
| **Glaze** | `references/glaze/` @ `d759f74` | Performance, `error_ctx`, reflection *ideas* |
| Glaze perf docs | `references/glaze/docs/optimizing-performance.md`, `reading.md`, `options.md` | Design constraints |
| Glaze SWAR | `references/glaze/include/glaze/util/parse.hpp`, `bit.hpp`, `hash/` | Algorithms to port carefully |
| **knus 3.4** | crates.io `knus` / `knus-derive` | **Proc-macro UX and attribute vocabulary** (not parser stack) |

### 3.1 Why knus, not knuffel

- `knuffel` is unmaintained; **knus** is the maintained fork (same derive model, miette 7, edition 2024).
- knus’s value for us is the **typed decode surface** (`Decode` / `DecodeScalar` / field attributes), not chumsky-based parsing.
- knus README notes limited maintainer bandwidth; we do **not** depend on it at runtime. We reimplement the *ideas* with a Glaze-style core.

### 3.2 knus lessons (adopt / reject)

**Adopt (API shape):**

- Field roles: `argument` / `arguments`, `property` / `properties`, `child` / `children`.
- `unwrap(...)` for nested peeling (`unwrap(argument)`, `unwrap(property)`, …).
- `flatten` for partial structures; enum variants as node names.
- Default kebab-case rename for properties/children; explicit `name = "..."`.
- Root document = type with only children (or `Vec<T: Decode>`).
- Scalar type annotations via `type_name` / custom `DecodeScalar`.

**Reject / replace (implementation):**

| knus | Our direction |
|------|----------------|
| chumsky combinator parse → full AST → `decode_node` | **Single-pass reader** (optionally span-bearing) → direct field write; DOM only if requested |
| `SpannedNode` heap tree always | Arena / bump or zero-copy views; typed path never requires owning DOM |
| `Box<str>` everywhere | Prefer `&str` / `Cow` / `String` only when unescaping needs ownership |
| Error collection mid-decode with full AST spans | `ErrorCtx` with offset; optional fancy report layer |
| KDL 1-ish keyword literals (`true`/`false`/`null` bare) | **KDL 2 only**: `#true` `#false` `#null` `#inf` `#-inf` `#nan` |

## 4. Crate layout

Workspace members (names tentative):

```text
crates/
  tensor-kdl/              # runtime: parse, DOM (optional), decode traits, format_error
  tensor-kdl-macros/       # proc-macro crate: Decode/Encode + scalar derives
```

Optional later:

```text
  tensor-kdl-tests/        # conformance runner over references/kdl/tests (dev-only)
```

`Cargo.toml` sketch:

```toml
# crates/tensor-kdl/Cargo.toml
[package]
name = "tensor-kdl"
version = "0.1.0"
edition = "2024"
rust-version = "1.97"
description = "High-performance KDL 2.0 parser and typed decode for TensorDE"
license = "MIT OR Apache-2.0"

[features]
default = ["std", "derive"]
std = []
derive = ["dep:tensor-kdl-macros"]
# Fancy terminal reports (miette); core never requires it
diagnostics = ["dep:miette"]
# Build owned Document AST API (still KDL 2)
dom = []
# Emit / pretty-print
write = []

[dependencies]
tensor-kdl-macros = { path = "../tensor-kdl-macros", optional = true }
miette = { version = "7", optional = true }
```

No `chumsky`, `pest`, `nom` on the primary path. Hand-written recursive-descent + SWAR scanners.

## 5. Architecture overview

```text
                    ┌─────────────────────────────────────┐
  &str / &[u8] ──►  │ Reader (cursor, depth, scratch)     │
                    │  SWAR skip_ws / string / number     │
                    └──────────────┬──────────────────────┘
                                   │ events or direct writes
              ┌────────────────────┼────────────────────┐
              ▼                    ▼                    ▼
        Typed Decode         DOM Builder            Skip / Validate
     (proc-macro impl)      (feature = dom)       (unknown keys, /- )
              │
              ▼
         User struct T
```

Three consumption modes share one lexer/reader:

1. **Typed (`read::<T>`)** — primary product path (Glaze *direct to memory*).
2. **DOM (`Document`)** — tooling, pretty-print roundtrip, conformance expected_kdl.
3. **Validate / skip** — structural check or partial read without allocating values.

## 6. Performance model (from Glaze)

Primary references: Glaze `optimizing-performance.md`, `util/parse.hpp`, `core/context.hpp`, `hash/`.

### 6.1 Principles

1. **What is known at compile time lives in types**  
   Node names, property keys, child tags → monomorphized match / perfect hash, not `HashMap` lookup per field on the typed path.

2. **Direct-to-memory**  
   `read(&mut T, input, &mut ctx)` fills `T` in place. Prefer reusing `T` and `Context` across reloads (compositor config hot-reload).

3. **Single pass**  
   No tokenize-all-then-parse. Cursor advances; slashdash skips subtrees without building nodes.

4. **SWAR first, SIMD optional**  
   8-byte (and later 16/32) register scans for whitespace, `"`, `\`, structural chars. Portable SWAR is the default; arch SIMD is an opt-in fast path for string escape validation only if measured.

5. **Scratch reuse**  
   `Context` owns a reusable `Vec<u8>` / `String` for unescaping and temporary keys (Glaze `ctx.scratch`).

6. **Depth guard**  
   Hard cap (e.g. 256) on children nesting to prevent stack / memory bombs (`depth_guard`).

7. **Zero-copy strings when safe**  
   Identifier / raw / unescaped quoted strings → `&str` into the input buffer (`KdlStr<'a>`). Escaped quoted strings → owned via scratch or `String`.

8. **Compile-time options**  
   Const generic or `Opts` marker types (Glaze `opts`), not runtime bool soup on hot paths:

   - `error_on_unknown_keys` (default true for configs)
   - `error_on_missing_keys`
   - `linear_search` vs perfect-hash for property/child names (size vs speed)
   - `max_depth`, `max_string_len`, `max_children`
   - `preserve_property_order` (DOM only; typed props unordered per spec)

9. **Allocation discipline**  
   - Typed decode: allocate only for owned strings, `Vec` children, maps.  
   - Pad input only if we add a SIMD path that needs it (Glaze pads `std::string`); pure SWAR can avoid padding.  
   - Prefer `SmallVec` / array for tiny arg lists only if benchmarks justify.

### 6.2 Hot-path sketch (reader)

```text
skip_line_space:     SWAR over unicode-space + handle // /* */ escline
parse_node:          optional /-  → skip_node
                     optional (type) name
                     loop: prop | arg | children | terminator
skip_node / skip_value: structural skip without alloc (Glaze skip_string style)
parse_string:        ident | "..." | #...# raw | multiline """
parse_number:        decimal / 0x / 0o / 0b / #inf #nan
```

Property key dispatch for derived structs (N small):

- **N ≤ ~8–12**: linear `match` / sequential compare (Glaze `linear_search` sweet spot).
- **N larger**: compile-time perfect hash or dual-table generated by the proc macro (Glaze hash maps).

Child node name dispatch: same strategy; enums become a name → variant jump.

### 6.3 What we will *not* copy blindly from Glaze

- C++ reflection / P2996 — Rust uses **proc macros** explicitly.
- Forced `always_inline` everywhere — use `#[inline(always)]` only on measured SWAR helpers.
- Null-terminated buffer requirement — Rust `&str` is length-based; end checks stay, SWAR uses remaining-length tails.

## 7. Process macro design (central)

### 7.1 Crates and entry points

`tensor-kdl-macros` exports:

| Macro | Implements | Purpose |
|-------|------------|---------|
| `#[derive(Decode)]` | `Decode`, optionally `DecodeChildren`, `DecodePartial` | Nodes / documents |
| `#[derive(DecodeScalar)]` | `DecodeScalar` | Enums / newtypes from scalar values |
| `#[derive(Encode)]` | `Encode`, optionally `EncodeDocument` | Write KDL from structs |
| `#[derive(EncodeScalar)]` | `EncodeScalar` | Unit enums/newtypes to scalar values |

Attribute namespace: **`#[kdl(...)]`** (not `knus`), to avoid confusion and claim our dialect.

Re-export from `tensor-kdl` when feature `derive` is on:

```rust
pub use tensor_kdl_macros::{Decode, DecodeScalar};
```

### 7.2 Trait surface (runtime)

Designed so macros generate *thin* monomorphized code against the reader, not against a mandatory DOM.

```rust
/// Typed decode from a live reader (primary path).
pub trait Decode<'a>: Sized {
    fn decode<R: Reader<'a>>(reader: &mut R, ctx: &mut Context) -> Result<Self, ErrorCtx>;
}

/// Document root: sequence of top-level nodes / only-children shape.
pub trait DecodeDocument<'a>: Sized {
    fn decode_document<R: Reader<'a>>(reader: &mut R, ctx: &mut Context) -> Result<Self, ErrorCtx>;
}

/// Scalar (argument or property value).
pub trait DecodeScalar<'a>: Sized {
    fn decode_scalar(value: ValueRef<'a>, ctx: &mut Context) -> Result<Self, ErrorCtx>;
}

/// Optional flatten target (unknown child/prop insertion).
pub trait DecodePartial<'a>: Default {
    fn insert_child<R: Reader<'a>>(
        &mut self,
        name: &'a str,
        reader: &mut R,
        ctx: &mut Context,
    ) -> Result<bool, ErrorCtx>;

    fn insert_property(
        &mut self,
        key: &'a str,
        value: ValueRef<'a>,
        ctx: &mut Context,
    ) -> Result<bool, ErrorCtx>;
}
```

**Deliberate divergence from knus:** traits talk to a **`Reader`**, not `&SpannedNode`.  
A DOM adapter can implement `Reader` over an existing tree for tooling, but config load never builds that tree.

`ValueRef<'a>` is an enum of borrowed/owned scalars:

```rust
pub enum ValueRef<'a> {
    String(KdlStr<'a>),    // Cow-like: Borrowed(&'a str) | Owned(String)
    Int(i128),             // or split Int/Uint policy — decide in impl phase
    Float(f64),
    Bool(bool),
    Null,
}
```

Type annotations `(foo)` are available on nodes and values as `Option<KdlStr<'a>>`.

### 7.3 Attribute vocabulary (`#[kdl(...)]`)

Aligned with knus for familiarity; namespaced under `kdl`:

| Attribute | Field meaning |
|-----------|----------------|
| `argument` | Next positional argument |
| `arguments` | Remaining args → `FromIterator` |
| `property` / `property(name = "...")` | Named property (default kebab-case of field) |
| `properties` | Collect map of remaining props |
| `child` / `child(name = "...")` | Single child node by name |
| `children` / `children(name = "...")` | Many children; optional name filter |
| `node_name` | Capture this node’s name into a field |
| `type_name` | Capture `(annotation)` |
| `span` | Capture source span (byte range) when spans enabled |
| `default` / `default = expr` | Missing → default |
| `flatten` | Delegate unknown props/children to `DecodePartial` |
| `unwrap(argument)` / `unwrap(property)` / nested | Peel single-field wrappers |
| `str` | Parse scalar via `FromStr` |
| `skip` | Do not decode; `Default` |
| `rename_all = "kebab-case"` | Struct-level (also `snake_case`, `camelCase`) |

Enum `Decode`: each variant is a **node name** (kebab-case by default), tuple/struct variant fields use the same field attrs.

### 7.4 What the proc macro emits (performance-critical)

For a struct with properties `gaps`, `border`, children `bind`:

1. **Static key tables**  
   `&'static [&'static str]` or perfect-hash function `fn prop_key(s: &str) -> Option<u8>`.

2. **State machine over reader**  
   After node name matched by parent:
   - loop entries: if `=` peek → property path; else if `{` → children; else → argument path; else terminator.
   - property: compute key id → `DecodeScalar` into field / error unknown.
   - child name: match → recursive `Decode::decode`.

3. **Presence bitset**  
   For `error_on_missing_keys` / duplicate detection without `Option` thrash where fields are required.

4. **No intermediate `Vec<SpannedNode>`** on the success path.

Generated code must stay readable enough for `cargo expand` debugging; helper functions live in `tensor_kdl::decode::ops` to keep expansion small (Glaze reduces template bloat via shared ops / opts structs).

### 7.5 Macro implementation structure (`tensor-kdl-macros`)

Mirror knus-derive layout (proven), not its output style:

```text
tensor-kdl-macros/src/
  lib.rs           # derive entry, attributes(kdl)
  definition.rs    # parse struct/enum into IR
  kw.rs            # custom_keyword! for kdl attrs
  node.rs          # emit Decode / DecodeDocument / DecodePartial
  scalar.rs        # emit DecodeScalar
  variants.rs      # enum node-name dispatch
  hash.rs          # optional perfect-hash codegen for keys
```

Dependencies: `syn`, `quote`, `proc-macro2`. Prefer explicit `syn::Error` over `proc-macro-error` unless we need abort-style UX.

### 7.6 User-facing example (target API)

```rust
use tensor_kdl::Decode;

#[derive(Debug, Decode)]
struct Config {
    #[kdl(child)]
    input: Input,
    #[kdl(children(name = "output"))]
    outputs: Vec<Output>,
    #[kdl(child, unwrap(argument))]
    prefer_no_csd: Option<bool>,
}

#[derive(Debug, Decode)]
struct Input {
    #[kdl(child)]
    keyboard: Option<Keyboard>,
}

#[derive(Debug, Decode)]
struct Keyboard {
    #[kdl(child)]
    numlock: Option<Flag>,
}

#[derive(Debug, DecodeScalar)]
struct Flag; // presence-only / unit from child node — pattern TBD

fn load(text: &str) -> Result<Config, tensor_kdl::Report> {
    tensor_kdl::from_str(text)
}
```

Public helpers:

```rust
pub fn from_str<'a, T: DecodeDocument<'a>>(input: &'a str) -> Result<T, Error>;
pub fn from_str_with_ctx<'a, T: DecodeDocument<'a>>(
    input: &'a str,
    ctx: &mut Context,
) -> Result<T, ErrorCtx>;
pub fn format_error(err: &ErrorCtx, input: &str) -> String;
```

Glaze-style in-place reuse:

```rust
let mut cfg = Config::default();
let mut ctx = Context::new();
tensor_kdl::read_in_place(&mut cfg, text, &mut ctx)?;
```

## 8. Error model (friendly + cheap)

### 8.1 Core: `ErrorCtx` (Glaze-shaped)

```rust
pub struct ErrorCtx {
    pub code: ErrorCode,
    /// Byte offset into the input (start of offending token / structure).
    pub offset: usize,
    /// Bytes successfully consumed before failure (Glaze `count`).
    pub consumed: usize,
    pub message: Option<Cow<'static, str>>,
    pub expected: Option<&'static str>,
    pub depth: u32,
}

pub enum ErrorCode {
    None, // internal success sentinel if needed
    UnexpectedEof,
    Syntax,
    InvalidEscape,
    InvalidNumber,
    InvalidIdent,
    ExpectedNodeName,
    ExpectedValue,
    ExpectedEquals,
    ExpectedBrace,
    ExpectedTerminator,
    UnknownProperty,
    UnknownChild,
    MissingProperty,
    MissingArgument,
    MissingChild,
    DuplicateProperty,
    TypeMismatch,
    ExceededMaxDepth,
    ExceededLimit,
    // ...
}
```

Hot path returns `Result<T, ErrorCtx>` **without** allocating source snippets.  
`format_error(err, src)` computes line/column once for display (scan `\n` up to offset, or maintain optional line index in `Context` when `diagnostics` enabled).

### 8.2 Report layer (feature `diagnostics`)

- Implement `std::error::Error` + optional `miette::Diagnostic` with `SourceSpan`.
- Multiple related errors: only if we add a recover mode; v1 is **fail-fast** (faster, simpler).
- Match knus quality of labels (“expected property `url`, found child `route`”) without requiring AST.

### 8.3 Proc-macro compile errors

Invalid attribute combos fail at compile time with spans on the attribute tokens (e.g. `arguments` followed by `argument`).

## 9. KDL 2.0 compliance checklist

Parser must implement (from draft grammar):

- [ ] BOM + optional `/- kdl-version 2` version marker
- [ ] Nodes, args, props, children, line continuation `\`
- [ ] Slashdash on node / arg / prop / children block
- [ ] Type annotations `(...)` on nodes and values
- [ ] Identifier strings, quoted, raw `#...#`, multiline `"""` with dedent rules
- [ ] Numbers: decimal, hex, octal, binary, underscores; `#inf` `#-inf` `#nan`
- [ ] Keywords: `#true` `#false` `#null` (not bare `true`)
- [ ] Whitespace / newline tables from the spec (including exotic unicode spaces)
- [ ] Disallowed literal code points
- [ ] Nested `/* */` comments; `//` line comments
- [ ] Property same-key override (rightmost wins) on DOM; typed fields: last write or duplicate error per opts

Conformance: drive `references/kdl/tests/test_cases/input` → compare to `expected_kdl` or expected failure.

## 10. DOM module (feature `dom`)

Owned/borrowed document for roundtrip and tools:

```rust
pub struct Document<'a> { pub nodes: Vec<Node<'a>> }
pub struct Node<'a> {
    pub type_name: Option<KdlStr<'a>>,
    pub name: KdlStr<'a>,
    pub entries: Vec<Entry<'a>>, // args + props in source order
    pub children: Vec<Node<'a>>,
}
```

Typed path must not depend on DOM. DOM builder is a `Reader` consumer or a parallel event sink.

## 11. Write / encode (phase 2)

- `Encode` / `EncodeScalar` plus matching derives cover the decode field shapes
  with an unambiguous reverse mapping, including:
  - struct / unit / newtype nodes;
  - enum unit, newtype, and named-field variants (variant name → node name);
  - `unwrap(argument)` and `unwrap(property)` children;
  - `#[kdl(properties)]` maps (formatter sorts keys; see suite translation rules);
  - `Flag` presence-only nodes;
  - `#[kdl(flatten)]` via [`EncodePartial`] (entries then children after known fields).
- Glaze model: default features have **no** public parse tree.
  - Typed: `read` / `write` only (monomorphized).
  - Optional `--features dom`: `Document`/`Node`, `from_str`, `format_document`,
    `query` (Glaze `generic` / suite tooling).
- Output goes through the stable canonical formatter: rightmost property wins,
  then properties are sorted by key, matching
  `references/kdl/tests/README.md` Translation Rules.

## 12. Testing strategy

| Layer | What |
|-------|------|
| Unit | SWAR scanners, numbers, strings, slashdash, escline |
| Conformance | Official KDL 2 suite under `references/kdl/tests` |
| Derive | `trybuild` UI tests under `crates/tensor-kdl/tests/ui/`; integration happy paths |
| Error UX | Inst snapshots of `format_error` / miette reports |
| Bench | Criterion: niri-sized config, synthetic 1–10 MB KDL; compare optional baseline vs knus if linked in benches only |

Workspace validation still applies when the crate lands (`fmt`, `test`, `clippy -D warnings`, line limit ≤ 800).

## 13. Implementation phases

| Phase | Deliverable | Exit criteria |
|-------|-------------|----------------|
| **P0** | Design doc (this file) | Reviewed; attribute list frozen enough to code |
| **P1** | `tensor-kdl` reader + `ErrorCtx` + DOM optional | Subset of official tests green (nodes, values, comments) |
| **P2** | Full KDL 2 grammar | Full official input suite parse/fail correct |
| **P3** | `tensor-kdl-macros` `Decode` / `DecodeScalar` | knus-like samples + niri-shaped configs decode without DOM |
| **P4** | Perfect-hash / opts / scratch reuse polish | Benches + unknown/missing key policies |
| **P5** | `diagnostics` + `write` | Fancy errors; roundtrip pretty |
| **P6** | Product integration | Optional Tensor/desktop config experiments (Tensor remains TOML unless product decision changes) |

## 14. Product note (TensorDE)

Tensor’s **shipped** compositor config is TOML (`docs/tensor/configuration.md`); legacy KDL is rejected.  
This crate is still justified as:

1. Shared high-quality KDL 2 infrastructure for tools, generators, or future products.
2. A place to encode Glaze-grade parse technique in Rust for node-oriented configs.
3. A cleaner long-term alternative to knus if a product reintroduces KDL.

Do not reintroduce KDL into Tensor startup without an explicit product decision.

## 15. Open questions (resolve before or during P1–P3)

1. **Integer range:** `i128` always vs JSON-like `i64` + big-int string fallback?  
2. **Float:** `f64` only, or decimal preserve for DOM roundtrip?  
3. **Duplicate properties on typed fields:** last-wins (spec) vs error (stricter configs)? Default proposal: **error** when `error_on_unknown_keys`-style strict opts; last-wins under `lenient`.  
4. **Presence-only child nodes** (`numlock` with no args): unit struct / `Flag` pattern — mirror knus `Flag` or use `#[kdl(child)] enabled: bool` with bare child ⇒ true?  
5. **Crate public name:** `tensor-kdl` vs shorter `kdl2` — prefer `tensor-kdl` inside monorepo.  
6. **Span stored in typed structs:** opt-in field attr only, to avoid slowing default decode.

## 16. Summary

| Axis | Choice |
|------|--------|
| Spec | KDL 2.0.0 (`draft-marchan-kdl2.md`) |
| Perf teacher | Glaze (SWAR, direct memory, context/scratch, compile-time keys, error_ctx) |
| Macro teacher | **knus** attribute/trait UX (not knuffel; not chumsky) |
| Core strategy | Hand-written reader + **proc-macro monomorphized decode** |
| Errors | Cheap `ErrorCtx` + optional miette |
| DOM | Optional feature; never on typed hot path |

Next step after design approval: **P1** reader skeleton in `crates/tensor-kdl` with workspace membership, then P2 grammar completeness before investing heavily in macros (macros need a stable `Reader` API).
