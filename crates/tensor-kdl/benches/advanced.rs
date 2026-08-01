//! Stage benches P-G6..P-G8 for tensor-kdl.
//!
//! Run: `cargo bench -p tensor-kdl --bench advanced`

use std::time::Duration;

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use tensor_kdl::{Decode, DecodeFromVisit, Opts, decode_node_str, from_str, read_into};

/// P-G6 stage gate: wide property sets with unique-index byte dispatch.
///
/// Cite: Glaze `find_unique_index` / `hash_type::unique_index` (`reflect.hpp`).
fn bench_pg6_unique_index_props(c: &mut Criterion) {
    // 8 properties with distinct first letters → unique_index at byte 0.
    #[derive(Debug, Decode, PartialEq)]
    struct Wide {
        #[kdl(property)]
        alpha: i64,
        #[kdl(property)]
        bravo: i64,
        #[kdl(property)]
        charlie: i64,
        #[kdl(property)]
        delta: i64,
        #[kdl(property)]
        echo: i64,
        #[kdl(property)]
        foxtrot: i64,
        #[kdl(property)]
        golf: i64,
        #[kdl(property)]
        hotel: i64,
    }

    let _ = <Wide as DecodeFromVisit>::start_visit();

    let line = concat!(
        r#"row alpha=1 bravo=2 charlie=3 delta=4 "#,
        r#"echo=5 foxtrot=6 golf=7 hotel=8"#
    );
    // Many rows: stress property dispatch volume.
    let wide: String = (0..100)
        .map(|i| {
            format!(
                "row alpha={i} bravo={i} charlie={i} delta={i} echo={i} foxtrot={i} golf={i} hotel={i}\n"
            )
        })
        .collect();

    let mut group = c.benchmark_group("pg6_unique_index_props");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(4));
    group.throughput(Throughput::Bytes(line.len() as u64));

    group.bench_function("decode_node_str_8props", |b| {
        b.iter(|| {
            let w: Wide = decode_node_str(black_box(line), Opts::new()).expect("visit");
            black_box(w.alpha + w.hotel)
        })
    });

    group.throughput(Throughput::Bytes(wide.len() as u64));
    group.bench_function("read_into_vec_100x8props", |b| {
        let mut rows: Vec<Wide> = Vec::new();
        b.iter(|| {
            let ec = read_into(&mut rows, black_box(&wide));
            assert!(!ec.is_err());
            black_box(rows.len())
        })
    });

    group.finish();
}

/// P-G7 stage gate: modular perfect-hash when keys lack a unique column.
///
/// Cite: Glaze modular integer perfect-hash role (`reflect.hpp`); string keys
/// use FNV-1a + seed search (`macros/key_dispatch.rs`).
fn bench_pg7_modular_hash_props(c: &mut Criterion) {
    // aa/ab/ba/bb — forces modular path (no unique or sized column).
    #[derive(Debug, Decode, PartialEq)]
    struct Grid {
        #[kdl(property, name = "aa")]
        aa: i64,
        #[kdl(property, name = "ab")]
        ab: i64,
        #[kdl(property, name = "ba")]
        ba: i64,
        #[kdl(property, name = "bb")]
        bb: i64,
    }

    let _ = <Grid as DecodeFromVisit>::start_visit();
    let line = r#"n aa=1 ab=2 ba=3 bb=4"#;
    let wide: String = (0..200)
        .map(|i| format!("n aa={i} ab={i} ba={i} bb={i}\n"))
        .collect();

    let mut group = c.benchmark_group("pg7_modular_hash_props");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(4));
    group.throughput(Throughput::Bytes(line.len() as u64));

    group.bench_function("decode_node_str_modular_4", |b| {
        b.iter(|| {
            let g: Grid = decode_node_str(black_box(line), Opts::new()).expect("visit");
            black_box(g.aa + g.bb)
        })
    });

    group.throughput(Throughput::Bytes(wide.len() as u64));
    group.bench_function("read_into_vec_200x_modular", |b| {
        let mut rows: Vec<Grid> = Vec::new();
        b.iter(|| {
            let ec = read_into(&mut rows, black_box(&wide));
            assert!(!ec.is_err());
            black_box(rows.len())
        })
    });

    group.finish();
}

/// P-G8 stage gate: single-node document root + quote-scan width.
fn bench_pg8_single_node_and_quote_scan(c: &mut Criterion) {
    #[derive(Debug, Decode, PartialEq)]
    struct Widget {
        #[kdl(argument)]
        id: i64,
        #[kdl(property)]
        name: String,
        #[kdl(property)]
        enabled: bool,
    }

    let _ = <Widget as DecodeFromVisit>::start_visit();
    let line = r#"widget 42 name="panel-with-a-reasonably-long-label" enabled=#true"#;
    // Dense quoted strings for quote/escape scan (SWAR-8 vs feature simd 16-byte).
    let quotes = {
        let mut s = String::new();
        for i in 0..400 {
            s.push_str("msg \"");
            s.push_str(&"x".repeat(48));
            s.push_str(&i.to_string());
            s.push_str("\"\n");
        }
        s
    };

    let mut group = c.benchmark_group("pg8_single_node_simd");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(4));
    group.throughput(Throughput::Bytes(line.len() as u64));

    group.bench_function("from_str_decode_single_node", |b| {
        b.iter(|| {
            let w: Widget = tensor_kdl::from_str_decode(black_box(line)).expect("decode");
            black_box(w.id)
        })
    });

    group.bench_function("read_into_single_node_stream", |b| {
        let mut w = Widget {
            id: 0,
            name: String::new(),
            enabled: false,
        };
        b.iter(|| {
            let ec = read_into(&mut w, black_box(line));
            assert!(!ec.is_err());
            black_box(w.id)
        })
    });

    group.throughput(Throughput::Bytes(quotes.len() as u64));
    group.bench_function("many_quoted_strings_scan", |b| {
        b.iter(|| from_str(black_box(&quotes)).expect("parse"))
    });

    group.finish();
}

/// P-G10: padded parser + mixed primary/sibling document root.
fn bench_pg10_padded_and_mixed(c: &mut Criterion) {
    use tensor_kdl::{PaddedInput, from_padded, from_str};

    let src: String = {
        let mut s = String::new();
        for i in 0..100 {
            s.push_str(&format!(r#"row {i} name="n{i}""#));
            s.push('\n');
        }
        s
    };

    #[derive(Debug, Decode, PartialEq)]
    struct Item {
        #[kdl(node_name)]
        name: String,
        #[kdl(argument)]
        n: i64,
    }

    #[derive(Debug, Decode, PartialEq)]
    struct Mixed {
        #[kdl(argument)]
        id: i64,
        #[kdl(children)]
        rest: Vec<Item>,
    }

    let mixed_src = {
        let mut s = String::from("root 1\n");
        for i in 0..50 {
            s.push_str(&format!("item {i}\n"));
        }
        s
    };

    let mut group = c.benchmark_group("pg10_padded_mixed");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Bytes(src.len() as u64));

    group.bench_function("from_str_100_rows", |b| {
        b.iter(|| from_str(black_box(&src)).expect("parse"))
    });

    group.bench_function("from_padded_100_rows", |b| {
        b.iter(|| {
            let p = PaddedInput::new(black_box(&src));
            let doc = from_padded(&p).expect("parse");
            black_box(doc.nodes.len())
        })
    });

    group.throughput(Throughput::Bytes(mixed_src.len() as u64));
    group.bench_function("read_into_mixed_siblings", |b| {
        let mut m = Mixed {
            id: 0,
            rest: Vec::new(),
        };
        b.iter(|| {
            let ec = read_into(&mut m, black_box(&mixed_src));
            assert!(!ec.is_err());
            black_box(m.rest.len())
        })
    });

    group.finish();
}

/// P-G13: monomorphized WriteSink dump (Glaze `to::op` / `util/dump.hpp`).
///
/// Compares allocate-each-time `write` vs in-place `write_into` buffer reuse
/// (`docs/optimizing-performance.md` guidance).
fn bench_pg13_write_sink(c: &mut Criterion) {
    use tensor_kdl::{Encode, write, write_into, write_into_slice};

    #[derive(Debug, Encode)]
    struct Row {
        #[kdl(argument)]
        n: i64,
        #[kdl(property)]
        name: String,
    }

    #[derive(Debug, Encode)]
    struct Doc {
        #[kdl(children)]
        rows: Vec<Row>,
    }

    let doc = Doc {
        rows: (0..100)
            .map(|i| Row {
                n: i,
                name: format!("n{i}"),
            })
            .collect(),
    };
    let sample = write(&doc).expect("write sample");

    let mut group = c.benchmark_group("pg13_write_sink");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.throughput(Throughput::Bytes(sample.len() as u64));

    group.bench_function("write_alloc_100_rows", |b| {
        b.iter(|| {
            let s = write(black_box(&doc)).expect("write");
            black_box(s.len())
        })
    });

    group.bench_function("write_into_reuse_100_rows", |b| {
        let mut buf = String::with_capacity(sample.len() + 64);
        b.iter(|| {
            let ec = write_into(black_box(&doc), &mut buf);
            assert!(!ec.is_err());
            black_box(ec.consumed)
        })
    });

    group.bench_function("write_into_slice_100_rows", |b| {
        let mut fixed = vec![0u8; sample.len()];
        b.iter(|| {
            let ec = write_into_slice(black_box(&doc), &mut fixed);
            assert!(!ec.is_err());
            black_box(ec.consumed)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_pg6_unique_index_props,
    bench_pg7_modular_hash_props,
    bench_pg8_single_node_and_quote_scan,
    bench_pg10_padded_and_mixed,
    bench_pg13_write_sink
);
criterion_main!(benches);
