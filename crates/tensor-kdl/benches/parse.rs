//! Criterion benchmarks for tensor-kdl.
//!
//! Run: `cargo bench -p tensor-kdl`

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use tensor_kdl::{
    Context, CountingVisitor, Decode, DecodeChildren, DecodeFromVisit, OPTS_DEFAULT, Opts, Parser,
    decode_node_str, decode_node_str_const, from_str, read_into, read_into_const, read_nodes_into,
    read_nodes_into_visit, visit_document,
};

const TINY: &str = r#"
node 1 key="value" {
    child
}
"#;

const MEDIUM: &str = r#"
package {
    name my-pkg
    version "1.2.3"
    dependencies {
        lodash "^3.2.1" optional=#true alias=underscore
        serde "1.0" features="derive"
        tokio "1" features="full,rt"
    }
    scripts {
        build "cargo build --release"
        test "cargo test --all"
        bench "cargo bench -p tensor-kdl"
    }
    matrix 1 2 3 \
           4 5 6 \
           7 8 9
}
"#;

const CI_LIKE: &str = include_str!("../../../references/kdl/examples/ci.kdl");

fn synthetic_wide(nodes: usize) -> String {
    let mut out = String::with_capacity(nodes * 48);
    for i in 0..nodes {
        out.push_str("item ");
        out.push_str(&i.to_string());
        out.push_str(" name=\"n");
        out.push_str(&i.to_string());
        out.push_str("\" enabled=#true\n");
    }
    out
}

fn synthetic_deep(depth: usize) -> String {
    let mut out = String::new();
    for i in 0..depth {
        out.push_str(&"  ".repeat(i));
        out.push_str("wrap {\n");
    }
    out.push_str(&"  ".repeat(depth));
    out.push_str("leaf 1\n");
    for i in (0..depth).rev() {
        out.push_str(&"  ".repeat(i));
        out.push_str("}\n");
    }
    out
}

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_document");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    for (name, src) in [("tiny", TINY), ("medium", MEDIUM), ("ci_example", CI_LIKE)] {
        group.throughput(Throughput::Bytes(src.len() as u64));
        group.bench_with_input(BenchmarkId::new("from_str", name), src, |b, src| {
            b.iter(|| {
                let doc = from_str(black_box(src)).expect("parse");
                black_box(doc.nodes.len())
            })
        });
    }

    let wide = synthetic_wide(2_000);
    group.throughput(Throughput::Bytes(wide.len() as u64));
    group.bench_with_input(BenchmarkId::new("from_str", "wide_2k"), &wide, |b, src| {
        b.iter(|| {
            let doc = from_str(black_box(src)).expect("parse");
            black_box(doc.nodes.len())
        })
    });

    let deep = synthetic_deep(64);
    group.throughput(Throughput::Bytes(deep.len() as u64));
    group.bench_with_input(BenchmarkId::new("from_str", "deep_64"), &deep, |b, src| {
        b.iter(|| {
            let doc = from_str(black_box(src)).expect("parse");
            black_box(doc.nodes.len())
        })
    });

    // niri default config if present (gitignored references tree)
    let niri_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../references/tensor/niri/resources/default-config.kdl"
    );
    // niri's default-config.kdl is largely KDL 1 style (bare `true`/`false`).
    // Only bench it when the document is valid KDL 2.
    if let Ok(niri) = std::fs::read_to_string(niri_path)
        && from_str(&niri).is_ok()
    {
        group.throughput(Throughput::Bytes(niri.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("from_str", "niri_default_config"),
            &niri,
            |b, src| {
                b.iter(|| {
                    let doc = from_str(black_box(src)).expect("parse");
                    black_box(doc.nodes.len())
                })
            },
        );
    }

    group.finish();
}

fn bench_swar_paths(c: &mut Criterion) {
    let mut group = c.benchmark_group("hot_paths");
    // Long run of spaces + a dense quoted-string document
    let spaces = format!("{}node\n", " ".repeat(4096));
    let quotes = {
        let mut s = String::new();
        for i in 0..500 {
            s.push_str("msg \"");
            s.push_str(&"x".repeat(32));
            s.push_str(&i.to_string());
            s.push_str("\"\n");
        }
        s
    };

    group.throughput(Throughput::Bytes(spaces.len() as u64));
    group.bench_function("leading_spaces_4k", |b| {
        b.iter(|| from_str(black_box(&spaces)).expect("parse"))
    });

    group.throughput(Throughput::Bytes(quotes.len() as u64));
    group.bench_function("many_quoted_strings", |b| {
        b.iter(|| from_str(black_box(&quotes)).expect("parse"))
    });

    group.finish();
}

/// Glaze perf doc: prefer in-place read + reused context over allocating each call.
fn bench_glaze_read_style(c: &mut Criterion) {
    #[derive(Debug, Default, Decode)]
    struct Pkg {
        #[kdl(child, unwrap(argument))]
        name: Option<String>,
        #[kdl(child, unwrap(argument))]
        version: Option<String>,
    }

    #[derive(Debug, Default, Decode)]
    struct Root {
        #[kdl(child)]
        package: Option<Pkg>,
    }

    let src = r#"
package {
    name my-pkg
    version "1.2.3"
}
"#;

    let mut group = c.benchmark_group("glaze_read_api");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Bytes(src.len() as u64));

    group.bench_function("from_str_dom", |b| {
        b.iter(|| {
            let doc = from_str(black_box(src)).expect("parse");
            black_box(doc.nodes.len())
        })
    });

    group.bench_function("read_into_alloc_ctx", |b| {
        b.iter(|| {
            let mut root = Root::default();
            let ec = read_into(&mut root, black_box(src));
            assert!(!ec.is_err());
            black_box(root.package.is_some())
        })
    });

    group.bench_function("read_into_reuse_ctx", |b| {
        let mut ctx = Context::new();
        let mut root = Root::default();
        b.iter(|| {
            let ec = tensor_kdl::read_into_with_context(&mut root, black_box(src), &mut ctx);
            assert!(!ec.is_err());
            black_box(root.package.as_ref().map(|p| p.name.is_some()))
        })
    });

    // Glaze parse::op shape: visit without retaining Document for counting.
    group.bench_function("visit_document_count", |b| {
        b.iter(|| {
            let mut n = 0usize;
            visit_document(black_box(src), Opts::new(), |_| {
                n += 1;
                Ok(())
            })
            .expect("visit");
            black_box(n)
        })
    });

    group.finish();
}

fn bench_read_nodes_into(c: &mut Criterion) {
    #[derive(Debug, Decode)]
    struct Item {
        #[kdl(argument)]
        #[allow(dead_code)]
        n: i64,
    }

    let wide = {
        let mut s = String::new();
        for i in 0..500 {
            s.push_str("item ");
            s.push_str(&i.to_string());
            s.push('\n');
        }
        s
    };

    let mut group = c.benchmark_group("read_nodes_into");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Bytes(wide.len() as u64));

    group.bench_function("decode_500_top_level", |b| {
        let mut ctx = Context::new();
        let mut out = Vec::new();
        b.iter(|| {
            let ec = read_nodes_into::<Item>(&mut out, black_box(&wide), &mut ctx, Opts::new());
            assert!(!ec.is_err());
            black_box(out.len())
        })
    });

    group.finish();
}

/// P-G3b stage gate: DOM `parse_node` vs counting visitor (no entry/child Vec growth).
///
/// Glaze cite: `json/read.hpp` writes members in place; skipping retention is the
/// `skip_value` / non-selected-field path. Lower is better for the visitor path
/// when the workload is “validate / count” rather than materialize.
fn bench_pg3b_node_visitor(c: &mut Criterion) {
    let mut fat = String::from("root");
    for i in 0..64 {
        fat.push(' ');
        fat.push_str(&i.to_string());
    }
    for i in 0..32 {
        fat.push_str(&format!(" k{i}={i}"));
    }
    fat.push_str(" {\n");
    for i in 0..32 {
        fat.push_str(&format!("  child{i} {i}\n"));
    }
    fat.push_str("}\n");

    let mut group = c.benchmark_group("pg3b_node_visitor");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(4));
    group.throughput(Throughput::Bytes(fat.len() as u64));

    group.bench_function("dom_parse_node", |b| {
        b.iter(|| {
            let mut p = Parser::new(black_box(&fat));
            let node = p.parse_document().expect("dom").nodes;
            black_box(node.len())
        })
    });

    group.bench_function("counting_visit_node", |b| {
        b.iter(|| {
            let mut p = Parser::new(black_box(&fat));
            // Single top-level node: visit_document one node with CountingVisitor
            // still builds nested child Nodes today; count top-level events only
            // via visit_node on the root line without children would under-parse.
            // Full document visit + count nodes:
            let mut n = 0usize;
            p.visit_document(Opts::new(), |node| {
                n += 1 + node.children.len() + node.entries.len();
                Ok(())
            })
            .expect("visit");
            black_box(n)
        })
    });

    group.bench_function("counting_visitor_root_only", |b| {
        // Pure visit_node on a root **without** parsing children into Vec — still
        // parses children for visitor.on_child which builds Node. Measures visitor
        // dispatch overhead vs raw DOM builder path for the same grammar walk.
        b.iter(|| {
            let mut p = Parser::new(black_box(&fat));
            let mut v = CountingVisitor::default();
            p.visit_node(Opts::new(), &mut v).expect("visit_node");
            black_box(v.arguments + v.properties + v.children)
        })
    });

    group.finish();
}

/// P-G3c stage gate: DOM `Decode::decode_node` vs `decode_node_str` (VisitBuilder).
fn bench_pg3c_decode_from_visit(c: &mut Criterion) {
    #[derive(Debug, Decode, PartialEq)]
    struct Item {
        #[kdl(argument)]
        n: i64,
        #[kdl(property)]
        name: String,
        #[kdl(property)]
        enabled: bool,
    }

    // Sanity: trait is implemented.
    let _ = <Item as DecodeFromVisit>::start_visit();

    let line = r#"row 42 name="widget" enabled=#true"#;
    let mut group = c.benchmark_group("pg3c_decode_from_visit");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(4));
    group.throughput(Throughput::Bytes(line.len() as u64));

    group.bench_function("dom_from_str_then_decode", |b| {
        b.iter(|| {
            let doc = from_str(black_box(line)).expect("parse");
            let item = Item::decode_node(&doc.nodes[0]).expect("decode");
            black_box(item.n)
        })
    });

    group.bench_function("decode_node_str_visit", |b| {
        b.iter(|| {
            let item: Item = decode_node_str(black_box(line), Opts::new()).expect("visit");
            black_box(item.n)
        })
    });

    group.finish();
}

/// P-G3d stage gate: nested child visit-fill + top-level `read_nodes_into_visit`.
///
/// Cite: Glaze nested `from::op` (`json/read.hpp`); array element loop without
/// retaining a generic value (`core/read.hpp`).
fn bench_pg3d_nested_visit(c: &mut Criterion) {
    #[derive(Debug, Decode, PartialEq)]
    struct Child {
        #[kdl(argument)]
        n: i64,
        #[kdl(property)]
        label: String,
    }

    #[derive(Debug, Decode, PartialEq)]
    struct Parent {
        #[kdl(property)]
        id: String,
        #[kdl(child)]
        child: Child,
    }

    let _ = <Child as DecodeFromVisit>::start_visit();
    let _ = <Parent as DecodeFromVisit>::start_visit();

    let nested = r#"parent id="p" { child 9 label="x" }"#;
    let mut group = c.benchmark_group("pg3d_nested_visit");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(4));
    group.throughput(Throughput::Bytes(nested.len() as u64));

    group.bench_function("dom_from_str_then_decode_nested", |b| {
        b.iter(|| {
            let doc = from_str(black_box(nested)).expect("parse");
            let p = Parent::decode_node(&doc.nodes[0]).expect("decode");
            black_box(p.child.n)
        })
    });

    group.bench_function("decode_node_str_nested_visit", |b| {
        b.iter(|| {
            let p: Parent = decode_node_str(black_box(nested), Opts::new()).expect("visit");
            black_box(p.child.n)
        })
    });

    // Wide top-level stream: DOM decode_node per row vs visit-fill.
    let wide: String = (0..200)
        .map(|i| format!(r#"row {i} name="n{i}""#))
        .collect::<Vec<_>>()
        .join("\n");
    group.throughput(Throughput::Bytes(wide.len() as u64));

    group.bench_function("read_nodes_into_dom_200", |b| {
        #[derive(Debug, Decode, PartialEq)]
        struct Row {
            #[kdl(argument)]
            n: i64,
            #[kdl(property)]
            name: String,
        }
        let mut out = Vec::new();
        let mut ctx = Context::new();
        b.iter(|| {
            out.clear();
            let ec = read_nodes_into::<Row>(&mut out, black_box(&wide), &mut ctx, Opts::new());
            assert!(!ec.is_err());
            black_box(out.len())
        })
    });

    group.bench_function("read_nodes_into_visit_200", |b| {
        #[derive(Debug, Decode, PartialEq)]
        struct Row {
            #[kdl(argument)]
            n: i64,
            #[kdl(property)]
            name: String,
        }
        let _ = <Row as DecodeFromVisit>::start_visit();
        let mut out = Vec::new();
        let mut ctx = Context::new();
        b.iter(|| {
            out.clear();
            let ec =
                read_nodes_into_visit::<Row>(&mut out, black_box(&wide), &mut ctx, Opts::new());
            assert!(!ec.is_err());
            black_box(out.len())
        })
    });

    group.finish();
}

/// P-G3e stage gate: `read_into(&mut Vec<T>)` streams via TopLevelFill.
fn bench_pg3e_read_into_vec(c: &mut Criterion) {
    #[derive(Debug, Decode, PartialEq)]
    struct Row {
        #[kdl(argument)]
        n: i64,
        #[kdl(property)]
        name: String,
    }

    #[derive(Debug, Decode, PartialEq)]
    struct Root {
        #[kdl(children)]
        rows: Vec<Row>,
    }

    let _ = <Row as DecodeFromVisit>::start_visit();

    let wide: String = (0..200)
        .map(|i| format!(r#"row {i} name="n{i}""#))
        .collect::<Vec<_>>()
        .join("\n");

    let mut group = c.benchmark_group("pg3e_read_into_stream");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(4));
    group.throughput(Throughput::Bytes(wide.len() as u64));

    group.bench_function("from_str_decode_root_200", |b| {
        b.iter(|| {
            let root: Root = tensor_kdl::from_str_decode(black_box(&wide)).expect("decode");
            black_box(root.rows.len())
        })
    });

    group.bench_function("read_into_vec_200", |b| {
        let mut rows: Vec<Row> = Vec::new();
        b.iter(|| {
            let ec = read_into(&mut rows, black_box(&wide));
            assert!(!ec.is_err());
            black_box(rows.len())
        })
    });

    group.bench_function("read_into_children_root_200", |b| {
        let mut root = Root { rows: Vec::new() };
        b.iter(|| {
            let ec = read_into(&mut root, black_box(&wide));
            assert!(!ec.is_err());
            black_box(root.rows.len())
        })
    });

    group.finish();
}

/// P-G4 stage gate: runtime `Opts` vs const-generic packed bits.
///
/// Cite: Glaze `template <auto Opts>` (`core/read.hpp` + `opts.hpp`). Rust uses
/// `const OPTS: u8` because structs are not valid const-generic types.
fn bench_pg4_const_opts(c: &mut Criterion) {
    #[derive(Debug, Decode, PartialEq)]
    struct Row {
        #[kdl(argument)]
        n: i64,
        #[kdl(property)]
        name: String,
        #[kdl(property)]
        enabled: bool,
    }

    let _ = <Row as DecodeFromVisit>::start_visit();
    let line = r#"row 42 name="widget" enabled=#true"#;
    let wide: String = (0..200)
        .map(|i| format!(r#"row {i} name="n{i}" enabled=#true"#))
        .collect::<Vec<_>>()
        .join("\n");

    let mut group = c.benchmark_group("pg4_const_opts");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(4));
    group.throughput(Throughput::Bytes(line.len() as u64));

    group.bench_function("decode_node_str_runtime_opts", |b| {
        b.iter(|| {
            let item: Row = decode_node_str(black_box(line), Opts::new()).expect("visit");
            black_box(item.n)
        })
    });

    group.bench_function("decode_node_str_const_opts", |b| {
        b.iter(|| {
            let item: Row = decode_node_str_const::<Row, OPTS_DEFAULT>(black_box(line)).expect("c");
            black_box(item.n)
        })
    });

    group.throughput(Throughput::Bytes(wide.len() as u64));

    group.bench_function("read_into_vec_runtime_200", |b| {
        let mut rows: Vec<Row> = Vec::new();
        b.iter(|| {
            let ec = read_into(&mut rows, black_box(&wide));
            assert!(!ec.is_err());
            black_box(rows.len())
        })
    });

    group.bench_function("read_into_vec_const_200", |b| {
        let mut rows: Vec<Row> = Vec::new();
        b.iter(|| {
            let ec = read_into_const::<Vec<Row>, OPTS_DEFAULT>(&mut rows, black_box(&wide));
            assert!(!ec.is_err());
            black_box(rows.len())
        })
    });

    group.finish();
}

/// P-G5 stage gate: multi-named children-only root streams without full Document.
fn bench_pg5_named_root_stream(c: &mut Criterion) {
    #[derive(Debug, Decode, PartialEq)]
    struct Entry {
        #[kdl(child, unwrap(argument))]
        name: String,
        #[kdl(child, unwrap(argument))]
        version: String,
        #[kdl(child, unwrap(argument))]
        license: Option<String>,
    }

    // Many repeated top-level named children (config-style document root).
    let mut src = String::new();
    for i in 0..100 {
        src.push_str(&format!("name pkg-{i}\nversion \"1.0.{i}\"\nlicense MIT\n"));
    }
    // First-wins: only first name/version/license matter for fields, but the
    // stream still walks all nodes (realistic wide config noise).

    let mut group = c.benchmark_group("pg5_named_root_stream");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(4));
    group.throughput(Throughput::Bytes(src.len() as u64));

    group.bench_function("from_str_then_decode_children", |b| {
        b.iter(|| {
            let doc = from_str(black_box(&src)).expect("parse");
            let e = Entry::decode_children(&doc.nodes).expect("decode");
            black_box(e.name.len() + e.version.len())
        })
    });

    group.bench_function("read_into_named_stream", |b| {
        let mut e = Entry {
            name: String::new(),
            version: String::new(),
            license: None,
        };
        b.iter(|| {
            let ec = read_into(&mut e, black_box(&src));
            assert!(!ec.is_err());
            black_box(e.name.len() + e.version.len())
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parse,
    bench_swar_paths,
    bench_glaze_read_style,
    bench_read_nodes_into,
    bench_pg3b_node_visitor,
    bench_pg3c_decode_from_visit,
    bench_pg3d_nested_visit,
    bench_pg3e_read_into_vec,
    bench_pg4_const_opts,
    bench_pg5_named_root_stream
);
criterion_main!(benches);
