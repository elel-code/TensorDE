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
6. At `count >= buffer.size()`, emit Glaze's exact `index N: <error>` form;
   do not manufacture a caret on the last character. Structured diagnostics may
   separately expose the friendly logical EOF insertion point.
7. Provide `format_error_named(error, input, filename)` for the public
   `generate_error_string(..., filename)` counterpart. `report_error_named`
   must forward the name through `miette::NamedSource`, not merely retain it in
   an inner diagnostic field.

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
| **P-G5** multi-named children-only root stream + SWAR ws | **Done** — derive `read_stream` first-wins named `child` / push `children` without full `Document`; real 8-byte SWAR `skip_ascii_horizontal_ws` (`util/parse.hpp` zero-byte detect) |
| **P-G6** unique-index key dispatch | **Done** — `find_unique_index` (Glaze `reflect.hpp`) for ≥3 property/child names with a unique column; outer byte switch + full-string verify |
| **P-G7** sized unique-index + modular perfect-hash | **Done** — `find_unique_sized_index` (byte+len); FNV-1a + seed modular table when no unique column; full-string verify; else `decode_linear` string match |
| **P-G8a** `front_hash` / `full_flat` | **Done** — Glaze `front_bytes_hash_info` + `full_flat` bucket tables (`key_hash.rs`, `primes_64` seeds, `bitmix`) |
| **P-G8b** non-children-only document root | **Done** — visit-fill-eligible structs implement `DecodeDocument` + `read_stream` for first top-level node (Glaze single root value) |
| **P-G8c** SIMD quote scan (bench-gated) | **Done** — feature `simd`: 16-byte dual-SWAR quote/escape scan (Glaze SSE2-width hot path); default remains 8-byte SWAR |
| **P-G9a** owned buffer padding | **Done** — `PaddedInput` / `PADDING_BYTES=16` (Glaze `padding_bytes`); `from_padded` / `read_into_padded` |
| **P-G9b** mixed children-only roots + flatten | **Done** — `read_stream` for named children + `#[kdl(flatten)]` via `DecodePartial::insert_child` |
| **P-G9c** richer miette | **Done** — primary + line-related labels; `report_error_named`; help with `line:column` |
| **P-G10a** Parser padded over-read | **Done** — `Parser::from_padded` / `logical_end`; SWAR skip/quote use content end + padding bytes |
| **P-G10b** mixed primary + sibling nodes | **Done** — single-node roots with unfiltered `#[kdl(children)]` collect later top-level nodes via `read_stream` / `decode_document` |
| **P-G11** padded typed read | **Done** — `DecodeDocument::read_stream_parser`; runtime/const padded APIs retain borrowed logical input while scanners consume the padded allocation |
| **P-G12** document-root stream without `Node` | **Done** — named children-only `read_stream` uses `visit_document_at_nodes` + `NestedProbe` / peel helpers; `WriteSink` grow path reserves `WRITE_PADDING_BYTES` (Glaze `write_padding_bytes`); fixed buffers exact-size only (`util/dump.hpp`) |
| **P-G13** nested unwrap + write dumpn + KQL | **Done** — visit `unwrap(argument\|property)` via peel in `take_child_after_header`; `WriteSink::push_byte_n` / grow-on-full dump; KQL `[name()]`/`[tag()]`, stacked matchers, `#inf`/`#-inf`/`#nan` RHS |
| **P-G14** write_chars / itoa + ctx write | **Done** — stack `write_i128`/`write_u128`/`write_f64`/`\u{…}` (Glaze `write_chars`/`itoa`); peel property key borrowed; `write_into_with_context` / `write_node_into_with_context` |
| **P-G15** raw delimiter / reusable input / named EOF diagnostics | **In progress** — allocation-free exact raw closer scan, `PaddedInput::replace`, Glaze EOF `index N` formatting, and `NamedSource` propagation |
| **P-G16** source-aware typed scalar decode | **Done** — entry-offset visitor callbacks, `DecodeScalar::decode_scalar_at`, and opt-in `Located<T>` preserve exact argument/property origins without DOM or success-path allocation |
| **P-G17** node completion validation | **Done** — node-start visitor callbacks plus `#[kdl(validate = "...")]` enforce cross-field invariants after typed fill; only opted-in builders retain one `usize` node offset |

Do not claim “zero DOM anywhere” for every derive shape; claim Glaze primary path for visit-eligible structs + nested visit children + homogeneous `Vec` / single-collector / multi-named children-only roots + single-node non-children roots (+ optional sibling collector). Flatten on document roots still requires feature `dom` + `DecodePartial` (unknown free nodes).

## 5. SWAR / performance (cite before changing)

| Technique | Glaze source | Status in tensor-kdl |
|-----------|--------------|----------------------|
| `repeat_byte` / quote-escape scan | `util/parse.hpp` | Present (`parse/swar.rs`) |
| Skip ASCII ws in chunks | `parse.hpp` `skip_ws` | **Done** — true 8-byte SWAR lane advance (P-G5) |
| Pad buffer +16 for SWAR | `opts.hpp` `padding_bytes`, `read.hpp` resize | **Done** (P-G9a) — `PaddedInput` for owned buffers; `&str` path unchanged |
| Reuse mutable input allocation | `docs/optimizing-performance.md` “Buffers” / “Reducing Memory Allocations” | **In progress** — `PaddedInput::replace` retains content capacity and restores its exact zero tail |
| Reuse `ctx.scratch` | `context.hpp`, perf doc | Present; ensure hot decode reuses `Context` |
| `error_on_unknown_keys` enables faster reject | perf doc | Policy present; unique-index / front_hash reject after miss |
| Forced inline on hot helpers | `util/inline.hpp` | Use `#[inline(always)]` only on measured SWAR helpers |
| SSE2-width string scan | perf doc SIMD section | **Done** behind feature `simd` (P-G8c); default SWAR-8 |

SIMD remains **opt-in** (`--features simd`) with bench comparison under `pg8`.

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
10. ~~P-G5 named root stream + SWAR ws~~ done.
11. ~~P-G6 unique-index key dispatch~~ done (`macros/key_dispatch.rs`).
12. ~~P-G7 sized + modular perfect-hash~~ done.
13. ~~P-G8a front_hash/full_flat; P-G8b single-node document roots; P-G8c simd feature~~ done.
14. ~~P-G9a pad; P-G9b flatten stream roots; P-G9c miette labels~~ done.
15. ~~P-G10a Parser padded over-read; P-G10b mixed primary+sibling children~~ done.
16. ~~P-G11 const/runtime typed reads on a padded parser~~ done.
17. ~~Typed encode completeness (enums, `unwrap(property)`, properties maps,
    broader `EncodeScalar`, `Flag`, `EncodePartial` flatten)~~ done.
18. ~~KQL subset expansion against `QUERY-SPEC.md`~~ done for selectors
    `+` / `++`, equality / inequality, same-type ordered compares, string
    `^=` / `$=` / `*=`, `values()` / `props()`, and value type RHS
    `[val() = (tag)]`. Still **not** full KQL / SCHEMA-SPEC.
19. ~~Glaze-shaped write + no public DOM on default path~~ done —
    typed `write_*` / `read_*` only. Feature `dom` (off by default) enables
    `Document`/`Node`, `from_str`, `format_document`, `query` (Glaze `generic`
    role). Without `dom`, nested decode requires `DecodeFromVisit` (no tree
    fallback in the public API).
20. ~~No tree code without `dom`~~ done — `value/tree`, `DomNodeBuilder`,
    DOM `Decode`/`NestedFill` fallback, and `from_str`/`query` are
    `cfg(feature = "dom")` only. Default build is visit+WriteSink only.
21. ~~P-G12 document-root stream without `Node`~~ done — peel
    `unwrap(argument|property)`; `WriteSink` grow padding vs fixed exact
    bounds (`dump.hpp`); flatten roots remain `dom`-gated.
22. ~~P-G13 nested unwrap peels + write dumpn + KQL~~ done — visit
    `unwrap` on nested children; `push_byte_n` indent; KQL `name()`/`tag()`
    existence, stacked accessors, keyword float RHS.
23. ~~P-G14 stack write_chars + write ctx~~ done — no heap on int/float
    dump; `write_into_with_context` mirrors Glaze `write(T, buffer, ctx)`.
24. **P-G15 (in progress):** use Glaze's direct string-scan / mutable-buffer
    mechanics for allocation-free raw delimiters and reusable `PaddedInput`;
    make filename and EOF behavior match `validate.hpp` in both text and
    miette output.
25. ~~P-G16 source-aware typed scalar decode~~ done — parser entry offsets
    flow through default-compatible visitor callbacks and
    `DecodeScalar::decode_scalar_at`; `Located<T>` retains positions only when
    requested. Typed streaming remains DOM-free and allocation-free.
26. ~~P-G17 node completion validation~~ done — node offsets flow through
    default-compatible header/child callbacks; derive invokes an inherent
    validator after fill and stores the offset only for opted-in builders.
27. **Next (optional):** further KQL only with QUERY-SPEC citations;
    SCHEMA-SPEC out of scope; optional minified / size opts like Glaze
    `opts_size` if binary size becomes a product constraint.

### Stage benchmark (P-G16)

```bash
cargo bench -p tensor-kdl --bench advanced --features "dom,diagnostics" -- pg16 --quick
```

Host-local snapshot for 200 `row count=N` nodes with a reused `Context`:

| Bench | time (approx) | retained source state |
|-------|---------------|-----------------------|
| `plain_scalar_200` | **~39.4 us** | none; normal source-aware callback path and no node-offset field |
| `validated_scalar_200` | **~49.3 us** | none; custom scalar performs a positive-value policy check |
| `located_scalar_200` | **~52.1 us** | one `usize` per decoded field |
| `node_validated_200` | **~43.2 us** | one `usize` per row builder plus one completion call |

The comparison is between user-visible modes, not a before/after regression
claim: all cases receive source cursors. On this quick run, node completion
validation was about 3.8 us per 200 nodes above the plain case (roughly 19 ns
per node); Criterion reported no statistically significant change for the
existing cases against their stored baselines. Ordinary builders retain no
node-offset field. Custom scalar validation, node validation, and explicit
`Located<T>` should remain opt-in where precise policy diagnostics require
them.

### Stage benchmark (P-G15)

```bash
cargo bench -p tensor-kdl --bench advanced --features "dom,derive" -- pg15
```

The three cases deliberately include raw `"#` / `"##` false candidates before
the `"###` closer, then compare ordinary input, newly allocated padded input,
and one reusable `PaddedInput::replace` allocation. Do not claim a speedup
until the local Criterion result is recorded.

### Stage benchmark (P-G13 / P-G14 write)

```bash
cargo bench -p tensor-kdl --bench advanced --features "dom,derive" -- pg13
```

### Stage benchmark (P-G10)

```bash
cargo bench -p tensor-kdl --bench advanced -- pg10
```

### Stage benchmark (P-G8)

```bash
cargo bench -p tensor-kdl --bench advanced -- pg8
# SIMD quote scan comparison:
cargo bench -p tensor-kdl --bench advanced --features simd -- pg8
```

**Snapshot** (`--quick`):

| Bench | default (SWAR-8) | `features = simd` (16-byte) |
|-------|------------------|-------------------------------|
| `from_str_decode_single_node` | **~471 ns** | **~476 ns** |
| `read_into_single_node_stream` | **~590 ns** | **~426 ns** (~1.4×) |
| `many_quoted_strings_scan` | **~116 µs** | **~96 µs** (~1.2×) |

**Honest read:** dense quote scan benefits from 16-byte dual-SWAR (`simd`);
single-node stream is competitive with DOM decode. Enable `simd` only when
quote-heavy inputs dominate (Glaze: most work stays portable SWAR).

### Stage benchmark (P-G7)

```bash
cargo bench -p tensor-kdl --bench parse -- pg7
```

**Snapshot** (`--quick`): `aa`/`ab`/`ba`/`bb` (no unique column → modular FNV).

| Bench | time (approx) | notes |
|-------|---------------|--------|
| `decode_node_str_modular_4` | **~487 ns** | single node, modular perfect-hash visit-fill |
| `read_into_vec_200x_modular` | **~141 µs** | 200 rows stream |

**Intent:** keys without a unique byte column exercise modular FNV perfect-hash
(Glaze modular role; not a full port of `hash_info` front_hash/full_flat).

### Stage benchmark (P-G6)

```bash
cargo bench -p tensor-kdl --bench parse -- pg6
```

**Snapshot** (`--quick`): 8 properties with distinct first letters.

| Bench | time (approx) | notes |
|-------|---------------|--------|
| `decode_node_str_8props` | **~1.19 µs** | single wide node, unique-index visit-fill |
| `read_into_vec_100x8props` | **~138 µs** | 100 rows × 8 props stream |

### Stage benchmark (P-G5)

```bash
cargo bench -p tensor-kdl --bench parse -- pg5
```

**Snapshot** (`--quick`): 100× repeated `name`/`version`/`license` top-level nodes.

| Bench | time (approx) | notes |
|-------|---------------|--------|
| `from_str_then_decode_children` | **~77 µs** | full `Document` + first-wins from slice |
| `read_into_named_stream` | **~53 µs** | stream fill (**~1.45×**); no retained `Document` |
| `hot_paths/leading_spaces_4k` | **~442 ns** | SWAR horizontal-ws (was ~1.1 µs class before real SWAR) |

**Intent:** `from_str` + `decode_children` (full top-level `Node` list) vs
`read_into` named stream (first-wins field fill, no `Document` retention).

### Stage benchmark (P-G4)

```bash
cargo bench -p tensor-kdl --bench parse -- pg4
```

**Snapshot** (`--quick`):

| Bench | time (approx) | notes |
|-------|---------------|--------|
| `decode_node_str_runtime_opts` | **~351 ns** | runtime `Opts` on single line |
| `decode_node_str_const_opts` | **~479 ns** | `const OPTS: u8` — micro overhead (extra monomorphized wrapper) |
| `read_into_vec_runtime_200` | **~111 µs** | 200 rows runtime opts |
| `read_into_vec_const_200` | **~91 µs** | 200 rows const opts (**~1.2×**) |

**Honest read:** const-generic opts are an API/mechanics alignment with Glaze
`template <auto Opts>`, not a free micro-bench win. On wider streams the const
path can fold policy better; on single-line micro the runtime path can win.
Prefer `*_const` when the call site is hot **and** opts are fixed at compile time.

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
