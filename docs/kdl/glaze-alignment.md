# Glaze → tensor-kdl alignment contract

**Authority:** local checkout `references/glaze/` @ `d759f74`.  
**Rule:** Do not invent “Glaze-like” behavior. Every performance or error-API choice
below cites a concrete Glaze header or doc path. If Glaze and KDL 2.0 conflict,
**KDL 2.0 wins for syntax**; Glaze wins for *library mechanics* (read API shape,
error_ctx, context/scratch, SWAR, compile-time opts).

## 1. Read API (must match)

| Glaze | Source | tensor-kdl target |
|-------|--------|-------------------|
| `error_ctx read_json(T& value, Buffer&&)` | `include/glaze/json/read.hpp` ~4902 | `read(&mut T, &str) -> ErrorCtx` |
| `expected<T, error_ctx> read_json(Buffer&&)` | same ~4920 | `read::<T>(&str) -> Result<T, ErrorCtx>` |
| Prefer in-place + reused buffer/ctx | `docs/optimizing-performance.md` “Reducing Memory Allocations” | `read` + `read_with_context` |
| Return **always** includes bytes consumed | `include/glaze/core/read.hpp` ~119 `return {size_t(it - start), ctx.error, ...}` | `ErrorCtx.consumed` (= Glaze `count`) |

### Glaze `error_ctx` layout (verbatim intent)

From `include/glaze/core/context.hpp`:

```text
struct error_ctx {
  size_t count;                      // bytes processed
  error_code ec;                     // none on success
  string_view custom_error_message;
  explicit operator bool() const;    // true IFF error
};
```

**tensor-kdl mapping:**

| Field | Glaze | tensor-kdl |
|-------|-------|------------|
| bytes processed | `count` | `consumed` (alias / primary) |
| error code | `ec` | `code` (`ErrorCode`; success is absence of `Err` *or* a `None` sentinel if returning bare ctx) |
| message | `custom_error_message` | `message` |
| bool error? | `operator bool` → true on error | `ErrorCtx::is_err()` / `bool` via wrapper |

**Divergence allowed:** Rust `Result<T, ErrorCtx>` for allocating reads (maps to
`expected<T, error_ctx>`). In-place reads return `ErrorCtx` and use
`is_err()` like Glaze’s `if (ec)`.

**Divergence forbidden:** treating “offset of bad token” as separate from
consumed count *in the public error_ctx*. Glaze’s `format_error(ec, buffer)`
indexes with **`pe.count`** (`reflect.hpp` ~3310–3316). Source highlighting
must use `consumed` (Glaze `count`), not a second public cursor field unless
it is documented as equal to `count` at error time.

## 2. `format_error` (must match)

Sources:

- `include/glaze/core/reflect.hpp` `format_error(error_ctx, buffer)`
- `include/glaze/util/validate.hpp` `get_source_info` + `generate_error_string`

Behavior to copy:

1. Resolve line/column from **`count`** into the buffer.
2. Build a **context window** of the current line, **truncate ~64 cols** with `...` when long (`validate.hpp`).
3. Convert tabs to single spaces in the context only.
4. Emit roughly:  
   `line:column: <error type>\n   <context>\n   <spaces>^`  
   then append custom message if present.
5. Filename prefix optional (`filename:`).

Current `tensor_kdl::format_error` is a simplified variant; align toward
`generate_error_string` before adding miette (miette is an *extra* presentation
layer, not a replacement for Glaze-shaped `format_error`).

## 3. Context vs options

| Concern | Glaze | tensor-kdl |
|---------|-------|------------|
| Runtime parse state | `glz::context`: `error`, `custom_error_message`, `depth`, `scratch`, `current_file` | `Context`: `depth`, `scratch`, … |
| Policy flags | **Compile-time** `glz::opts` (`error_on_unknown_keys`, `error_on_missing_keys`, `partial_read`, …) | [`Opts`] value + packed `u8` for `const OPTS: u8` (P-G4); limits stay on `Context` |

**Alignment (P-G4 done):**

- Keep **runtime** limits that Glaze documents as *optional context extensions*
  (`max_string_length`, `max_array_size`, … in `context.hpp` comments) on `Context`.
- **Policy** lives on [`Opts`] with Glaze defaults:
  - `error_on_unknown_keys = true` (`opts.hpp`)
  - `error_on_missing_keys = false` (`opts.hpp`)
- Rust cannot use a struct as a const generic; pack bits via `Opts::bits()` /
  `OPTS_DEFAULT` and monomorphize `fn …<const OPTS: u8>` (Glaze
  `template <auto Opts>` role). Hot APIs: `visit_node_const`,
  `visit_document_at_nodes_const`, `read_into_const`, `decode_node_str_const`.
- `depth_guard`: on overflow set error and `operator bool() == false`; do not
  require `Result` for construction (`context.hpp` `depth_guard`).

## 4. Direct-to-memory (what Glaze actually does)

Glaze does **not** build a JSON DOM then reflect.  
`read<Opts>(T& value, buffer, ctx)` calls `parse<Format>::op` **into `value`**
(`core/read.hpp`).

For KDL that means:

| Layer | Glaze analogue | tensor-kdl |
|-------|----------------|------------|
| Lexer/cursor | `it`/`end` + SWAR skip | `Parser` |
| Fill user `T` | `parse::op` monomorphized | **Target:** decode without requiring a full owned `Document` clone |
| Generic DOM | `glz::generic` only when schema unknown | `Document` / `Node` for tools & suite |

**Honest gap / progress:**

| Stage | Status |
|-------|--------|
| **P-G1** API/error/context/`format_error` | Done |
| **P-G2** `DecodeChildren` without document-root clone | Done |
| **P-G3a** `visit_document` / `Opts` / `read_nodes_into` | Done |
| **P-G3b** `NodeVisitor` + `visit_node` | **Done** |
| **P-G3c** Derive `DecodeFromVisit` / `VisitBuilder` with linear property/child name match (`decode_linear`) | **Done** |
| **P-G3d** Nested visit-fill (no child `Node` heap when child: `DecodeFromVisit`); top-level `read_nodes_into_visit` | **Done** — `NestedProbe` autoref specialization; DOM fallback for `Decode`-only children (`unwrap`, enums) |
| **P-G3e** `read_into` → `DecodeDocument::read_stream` | **Done** — `Vec<T>` streams via `TopLevelFill` (visit when available); single unfiltered `#[kdl(children)]` document root reuses that path; multi-named children roots still buffer top-level `Node`s for lookup |
| **P-G4** const-generic opts + diagnostics polish | **Done** — packed `u8` opts (`OPTS_DEFAULT` / `read_into_const` / `visit_*_const`); feature `diagnostics` → `report_error` miette `Report` with source span (Glaze `format_error` remains primary) |

Do not claim “zero DOM anywhere” for every derive shape; claim Glaze primary path for visit-eligible structs + nested visit children + homogeneous `Vec` / single-collector roots.

## 5. SWAR / performance (cite before changing)

| Technique | Glaze source | Status in tensor-kdl |
|-----------|--------------|----------------------|
| `repeat_byte` / quote-escape scan | `util/parse.hpp` | Partial (`parse/swar.rs`) |
| Skip ASCII ws in chunks | `parse.hpp` `skip_ws` | Partial |
| Pad buffer +16 for SWAR | `opts.hpp` `padding_bytes`, `read.hpp` resize | **Not** done (Rust `&str` immutable; only apply if we own `String` input) |
| Reuse `ctx.scratch` | `context.hpp`, perf doc | Present; ensure hot decode reuses `Context` |
| `error_on_unknown_keys` enables faster reject | perf doc | Policy flag present; not tied to perfect hash yet |
| Forced inline on hot helpers | `util/inline.hpp` | Use `#[inline(always)]` only on measured SWAR helpers |

**Do not** add SIMD/AVX paths without a bench gate and a Glaze cite
(`docs/optimizing-performance.md` SIMD section).

## 6. What we will *not* copy

| Glaze | Why not |
|-------|---------|
| C++ reflection / `glz::meta` | Rust uses proc-macros (`tensor-kdl-macros`) |
| `null_terminated` default true | Rust slices are length-based; end checks stay |
| Exceptions helpers | `glaze_exceptions.hpp` optional; we stay `Result`/`ErrorCtx` |
| JSON-only key hash tables as-is | KDL has args+props+children; hash strategy must be KDL-shaped |

## 7. Checklist before merging a “perf” or “error” PR

- [ ] Cite Glaze path + symbol in the PR/commit body.
- [ ] In-place `read` returns `ErrorCtx` with **consumed == format_error index**.
- [ ] No new DOM requirement on the typed success path unless feature `dom`.
- [ ] Suite still **243/243** (or justify with KDL spec, not Glaze).
- [ ] Bench: in-place + ctx reuse vs allocate (`optimizing-performance.md` guidance).

## 8. Immediate implementation order (Glaze-gated)

1. ~~Align `ErrorCtx` + `format_error`~~ done.
2. ~~`read` / `read_into` APIs~~ done.
3. ~~`DecodeChildren`~~ done.
4. ~~`Opts` + `visit_document` + `read_nodes_into` (P-G3a)~~ done.
5. ~~P-G3b~~ done.
6. ~~P-G3c derive `DecodeFromVisit`~~ done (`macros/visit_emit.rs`).
7. ~~P-G3d nested visit-fill~~ done.
8. ~~P-G3e `read_into` / `read_stream`~~ done (`DecodeDocument::read_stream`, `TopLevelFill`, single-`children` root).
9. ~~P-G4 const-generic opts + miette `report_error`~~ done.
10. **Next (optional):** multi-named children root incremental fill; SWAR/SIMD only with bench gate; property perfect-hash if measured.

### Stage benchmark (P-G4)

```bash
cargo bench -p tensor-kdl --bench parse -- pg4
```

**Intent:** runtime `Opts` path vs `const OPTS: u8` monomorphization. Expect near-
parity on micro inputs (policy checks are cheap); the win is branch folding /
inlining for unknown-key and partial-read paths, matching Glaze’s compile-time
opts model rather than a large wall-time delta.

### Stage benchmark (P-G3e)

```bash
cargo bench -p tensor-kdl --bench parse -- pg3e
```

**Snapshot** (`--quick`): 200 lines `row N name="nN"`.

| Bench | time (approx) | notes |
|-------|---------------|--------|
| `pg3e_read_into_stream/from_str_decode_root_200` | **~63 µs** | full DOM `Document` then `decode_document` |
| `pg3e_read_into_stream/read_into_vec_200` | **~65 µs** | `read_into(&mut Vec)` → `TopLevelFill` stream |
| `pg3e_read_into_stream/read_into_children_root_200` | **~64 µs** | single-`children` root `read_stream` override |

On this width, parse cost dominates allocation; the stream path still avoids
retaining a full `Document`/`Node` list for the root (Glaze array `from::op`
shape). Nested visit wins are larger (see P-G3d ~1.9× on nested micro).

### Stage benchmark (P-G3c)

```bash
cargo bench -p tensor-kdl --bench parse -- pg3c
```

**Snapshot** (`--quick`): single line `row 42 name="widget" enabled=#true`.

| Bench | time (approx) | notes |
|-------|---------------|--------|
| `pg3c_decode_from_visit/dom_from_str_then_decode` | **~418 ns** | DOM parse + `Decode::decode_node` |
| `pg3c_decode_from_visit/decode_node_str_visit` | **~370 ns** | `visit_node` + derived `VisitBuilder` (improved vs earlier ~444 ns) |

Visit path is competitive with DOM+decode on micro inputs; multi-node and
nested paths show clearer wins (P-G3d/P-G3e).

### Stage benchmark (P-G3b)

```bash
cargo bench -p tensor-kdl --bench parse
# or quick: cargo bench -p tensor-kdl --bench parse -- --quick
```

**Snapshot** (`--quick`, host-local, not a CI gate): fat node ≈ 64 args + 32 props + 32 children.

| Bench | time (approx) | notes |
|-------|---------------|--------|
| `pg3b_node_visitor/dom_parse_node` | ~15.5 µs | full DOM document |
| `pg3b_node_visitor/counting_visitor_root_only` | ~17.2 µs | `visit_node` + `CountingVisitor` |
| `glaze_read_api/read_into_reuse_ctx` | ~0.79 µs | small package config |
| `glaze_read_api/from_str_dom` | ~0.80 µs | same input, DOM |
| `read_nodes_into/decode_500_top_level` | ~111 µs | 500 top-level items |
| `hot_paths/leading_spaces_4k` | ~1.35 µs | SWAR ws skip |

P-G3b foundation does **not** yet beat DOM on fat nodes: nested `on_child` still
builds `Node` values. Expect counting ≤ DOM only after P-G3c (no child heap) or
when the visitor skips nested materialization.
