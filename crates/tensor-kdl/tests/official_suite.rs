//! Drive the official KDL 2 test suite under `references/kdl/tests/test_cases`.
//!
//! Cases are soft-gated: the suite reports parse/fail/roundtrip stats and fails
//! the build only when `TENSOR_KDL_STRICT_SUITE=1` is set, so incremental
//! conformance work can land without blocking CI on every remaining gap.

use std::fs;
use std::path::{Path, PathBuf};

use tensor_kdl::{format_document, from_str};

fn suite_root() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .join("../../references/kdl/tests/test_cases")
        .canonicalize()
        .ok()?;
    if root.join("input").is_dir() {
        Some(root)
    } else {
        None
    }
}

fn normalize_expected(s: &str) -> String {
    // Tolerate trailing whitespace differences on blank lines.
    let mut lines: Vec<&str> = s.lines().map(|l| l.trim_end()).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

#[derive(Default)]
struct Stats {
    should_parse: usize,
    parsed_ok: usize,
    roundtrip_ok: usize,
    parse_fail_unexpected: Vec<String>,
    roundtrip_mismatch: Vec<String>,
    should_fail: usize,
    correctly_rejected: usize,
    false_accept: Vec<String>,
}

#[test]
fn official_kdl_test_suite() {
    let Some(root) = suite_root() else {
        eprintln!("official KDL suite not found under references/; skipping");
        return;
    };

    let input_dir = root.join("input");
    let expected_dir = root.join("expected_kdl");
    let mut stats = Stats::default();

    let mut inputs: Vec<PathBuf> = fs::read_dir(&input_dir)
        .expect("read input dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "kdl"))
        .collect();
    inputs.sort();

    for path in inputs {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let expect_path = expected_dir.join(format!("{name}.kdl"));
        let should_parse = expect_path.is_file();

        if should_parse {
            stats.should_parse += 1;
            match from_str(&src) {
                Ok(doc) => {
                    stats.parsed_ok += 1;
                    let got = normalize_expected(&format_document(&doc));
                    let expected = normalize_expected(&fs::read_to_string(&expect_path).unwrap());
                    if got == expected {
                        stats.roundtrip_ok += 1;
                    } else {
                        stats.roundtrip_mismatch.push(name);
                    }
                }
                Err(err) => {
                    stats.parse_fail_unexpected.push(format!("{name}: {err}"));
                }
            }
        } else {
            stats.should_fail += 1;
            match from_str(&src) {
                Ok(_) => stats.false_accept.push(name),
                Err(_) => stats.correctly_rejected += 1,
            }
        }
    }

    eprintln!(
        "official suite: parse {}/{}  roundtrip {}/{}  reject {}/{}  false_accept {}  unexpected_fail {}",
        stats.parsed_ok,
        stats.should_parse,
        stats.roundtrip_ok,
        stats.should_parse,
        stats.correctly_rejected,
        stats.should_fail,
        stats.false_accept.len(),
        stats.parse_fail_unexpected.len(),
    );

    // Always assert we found the suite and exercised cases.
    assert!(stats.should_parse > 0, "no should-parse cases found");
    assert!(stats.should_fail > 0, "no should-fail cases found");

    // Full official suite gate (non-strict still prints diagnostics above).
    assert_eq!(
        stats.parsed_ok,
        stats.should_parse,
        "unexpected parse failures:\n{}",
        stats.parse_fail_unexpected.join("\n")
    );
    assert_eq!(
        stats.correctly_rejected,
        stats.should_fail,
        "false accepts:\n{}",
        stats.false_accept.join("\n")
    );
    assert_eq!(
        stats.roundtrip_ok,
        stats.should_parse,
        "roundtrip mismatches (first 30):\n{}",
        stats
            .roundtrip_mismatch
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );

    if std::env::var_os("TENSOR_KDL_STRICT_SUITE").is_some() {
        assert!(
            stats.parse_fail_unexpected.is_empty(),
            "strict: unexpected parse failures:\n{}",
            stats.parse_fail_unexpected.join("\n")
        );
        assert!(
            stats.false_accept.is_empty(),
            "strict: false accepts:\n{}",
            stats.false_accept.join("\n")
        );
        assert_eq!(
            stats.roundtrip_ok,
            stats.should_parse,
            "strict: roundtrip mismatches (first 30):\n{}",
            stats
                .roundtrip_mismatch
                .iter()
                .take(30)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    } else if !stats.parse_fail_unexpected.is_empty() {
        eprintln!(
            "sample unexpected parse failures:\n{}",
            stats
                .parse_fail_unexpected
                .iter()
                .take(15)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// Focused smoke: a few known-good official inputs must always pass.
#[test]
fn official_smoke_known_good() {
    let Some(root) = suite_root() else {
        return;
    };
    for name in [
        "all_node_fields",
        "arg_bare",
        "binary",
        "block_comment",
        "empty",
        "escline",
        "prop_true_type",
    ] {
        let path = root.join("input").join(format!("{name}.kdl"));
        if !path.is_file() {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap();
        from_str(&src).unwrap_or_else(|e| panic!("{name}: {e}\n{}", e.format_with_source(&src)));
    }
}

#[allow(dead_code)]
fn path_exists(p: &Path) -> bool {
    p.exists()
}
