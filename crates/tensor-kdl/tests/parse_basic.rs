use tensor_kdl::{Decode, KdlStr, Value, format_error, from_str, from_str_decode};

#[test]
fn parses_simple_node() {
    let doc = from_str(r#"foo 1 key="val" 3"#).unwrap();
    assert_eq!(doc.nodes.len(), 1);
    let n = &doc.nodes[0];
    assert_eq!(n.name.as_str(), "foo");
    let args: Vec<_> = n.arguments().collect();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0], &Value::Int(1));
    assert_eq!(args[1], &Value::Int(3));
    assert_eq!(
        n.property("key"),
        Some(&Value::String(KdlStr::borrowed("val")))
    );
}

#[test]
fn parses_children_and_type() {
    let doc = from_str(
        r#"
        parent {
            (role)child 1 2
        }
    "#,
    )
    .unwrap();
    let child = doc.nodes[0].child("child").unwrap();
    assert_eq!(child.type_name.as_ref().unwrap().as_str(), "role");
    assert_eq!(child.arguments().count(), 2);
}

#[test]
fn slashdash_skips_node() {
    let doc = from_str(
        r#"
        keep
        /-gone 1 2 {
            nested
        }
        also
    "#,
    )
    .unwrap();
    let names: Vec<_> = doc.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["keep", "also"]);
}

#[test]
fn kdl2_keywords() {
    let doc = from_str("flags on=#true off=#false empty=#null").unwrap();
    let n = &doc.nodes[0];
    assert_eq!(n.property("on"), Some(&Value::Bool(true)));
    assert_eq!(n.property("off"), Some(&Value::Bool(false)));
    assert_eq!(n.property("empty"), Some(&Value::Null));
}

#[test]
fn numbers() {
    let doc = from_str("n 0x10 0o10 0b10 1_000 1.5 1e2").unwrap();
    let args: Vec<_> = doc.nodes[0].arguments().cloned().collect();
    assert_eq!(args[0], Value::Int(16));
    assert_eq!(args[1], Value::Int(8));
    assert_eq!(args[2], Value::Int(2));
    assert_eq!(args[3], Value::Int(1000));
    assert_eq!(args[4].as_f64(), Some(1.5));
    assert_eq!(args[5].as_f64(), Some(100.0));
}

#[test]
fn raw_string() {
    // Use ## so the inner #"..."# style is unambiguous in the Rust source.
    let doc = from_str(r##"s #"hello\nworld"#"##).unwrap();
    assert_eq!(
        doc.nodes[0].arguments().next().unwrap().as_str(),
        Some(r"hello\nworld")
    );
}

#[test]
fn comments() {
    let doc = from_str(
        r#"
        // line
        a /* block /* nest */ ok */
        b
    "#,
    )
    .unwrap();
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(doc.nodes[0].name.as_str(), "a");
    assert_eq!(doc.nodes[1].name.as_str(), "b");
}

#[test]
fn line_continuation() {
    let doc = from_str("n 1 2 \\\n   3 4\n").unwrap();
    assert_eq!(doc.nodes[0].arguments().count(), 4);
}

#[test]
fn format_error_shows_snippet() {
    // Glaze format_error(pe, buffer) indexes with pe.count (== consumed).
    let src = "ok\nbad \0 stuff\n";
    let err = from_str(src).unwrap_err();
    let formatted = format_error(&err.ctx, src);
    // Glaze generate_error_string: `line:column: <msg>` then context + `^`
    assert!(
        formatted.contains(':'),
        "expected line:column prefix, got {formatted:?}"
    );
    assert!(formatted.contains('^'), "expected caret, got {formatted:?}");
}

#[test]
fn glaze_read_into_reuses_value() {
    use tensor_kdl::{Decode, read_into};

    #[derive(Debug, Default, PartialEq, Decode)]
    struct Root {
        #[kdl(child, unwrap(argument))]
        name: Option<String>,
    }

    let mut root = Root::default();
    let ec = read_into(
        &mut root,
        r#"
        name hello
    "#,
    );
    assert!(ec.is_ok(), "{}", tensor_kdl::format_error_code(&ec));
    assert!(ec.consumed > 0, "Glaze count must be bytes processed");
    assert_eq!(root.name.as_deref(), Some("hello"));
}

#[test]
fn empty_kdl_document_is_valid() {
    // Official suite `empty.kdl` — KDL allows zero nodes (Glaze JSON would use no_read_input).
    let doc = from_str("").unwrap();
    assert!(doc.is_empty());
}

#[test]
fn decode_from_visit_derive() {
    // P-G3c: Glaze decode_linear — derive VisitBuilder fills T without DOM Node for root.
    use tensor_kdl::{Decode, DecodeFromVisit, Opts, VisitBuilder, decode_node_str};

    #[derive(Debug, Decode, PartialEq)]
    struct Item {
        #[kdl(argument)]
        n: i64,
        #[kdl(property)]
        name: String,
    }

    assert!(
        VisitBuilder::finish(<Item as DecodeFromVisit>::start_visit()).is_err(),
        "builder without events should miss required fields"
    );

    let item: Item = decode_node_str(r#"item 7 name="x""#, Opts::new()).unwrap();
    assert_eq!(
        item,
        Item {
            n: 7,
            name: "x".into()
        }
    );
}

#[test]
fn nested_visit_fill_child_without_dom() {
    // P-G3d: Glaze nested from::op — child DecodeFromVisit fills without parent
    // retaining a child Node (take_child_after_header + NestedProbe).
    use tensor_kdl::{Decode, DecodeFromVisit, Opts, decode_node_str};

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

    // Both implement DecodeFromVisit (visit-fill eligible structs).
    let _ = <Child as DecodeFromVisit>::start_visit();
    let _ = <Parent as DecodeFromVisit>::start_visit();

    let parent: Parent =
        decode_node_str(r#"parent id="p1" { child 9 label="nested" }"#, Opts::new()).unwrap();
    assert_eq!(
        parent,
        Parent {
            id: "p1".into(),
            child: Child {
                n: 9,
                label: "nested".into(),
            },
        }
    );
}

#[test]
fn read_nodes_into_visit_streams_without_dom_nodes() {
    // P-G3d top-level: visit_document_at_nodes + DecodeFromVisit (Glaze array from::op).
    use tensor_kdl::{Context, Decode, DecodeFromVisit, Opts, read_nodes_into_visit};

    #[derive(Debug, Decode, PartialEq)]
    struct Row {
        #[kdl(argument)]
        n: i64,
        #[kdl(property)]
        name: String,
    }

    let _ = <Row as DecodeFromVisit>::start_visit();
    let mut out: Vec<Row> = Vec::new();
    let mut ctx = Context::new();
    let ec = read_nodes_into_visit(
        &mut out,
        r#"row 1 name="a"
row 2 name="b"
"#,
        &mut ctx,
        Opts::new(),
    );
    assert!(!ec.is_err(), "{ec:?}");
    assert_eq!(
        out,
        [
            Row {
                n: 1,
                name: "a".into()
            },
            Row {
                n: 2,
                name: "b".into()
            },
        ]
    );
}

#[test]
fn visit_node_counting_visitor() {
    // P-G3b: Glaze decode_index delivers members without requiring the caller
    // to retain a full Node (CountingVisitor drops values).
    use tensor_kdl::{CountingVisitor, Opts, Parser};

    let mut parser = Parser::new(r#"n 1 2 k="v" { c }"#);
    let mut v = CountingVisitor::default();
    parser.visit_node(Opts::new(), &mut v).unwrap();
    assert_eq!(v.arguments, 2);
    assert_eq!(v.properties, 1);
    assert_eq!(v.children, 1);
}

#[test]
fn visit_document_streams_top_level_nodes() {
    // Glaze primary path: deliver structure as parsed (core/read.hpp → parse::op).
    use tensor_kdl::{Opts, visit_document};

    let mut names = Vec::new();
    visit_document("a 1\nb 2\nc 3\n", Opts::new(), |node| {
        names.push(node.name.as_str().to_owned());
        Ok(())
    })
    .unwrap();
    assert_eq!(names, ["a", "b", "c"]);
}

#[test]
fn partial_read_stops_after_first_node() {
    // Glaze opts.partial_read (opts.hpp + docs/partial-read.md).
    use tensor_kdl::{Opts, read_document_with_opts};

    let doc = read_document_with_opts("first 1\nsecond 2\nthird 3\n", Opts::partial()).unwrap();
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(doc.nodes[0].name.as_str(), "first");
}

#[test]
fn read_nodes_into_decodes_each_top_level() {
    // Glaze array-element fill: decode each element as the cursor advances.
    use tensor_kdl::{Context, Decode, Opts, read_nodes_into};

    #[derive(Debug, Decode, PartialEq)]
    struct Item {
        #[kdl(node_name)]
        name: String,
        #[kdl(argument)]
        n: i64,
    }

    let mut items = Vec::new();
    let mut ctx = Context::new();
    let ec = read_nodes_into::<Item>(&mut items, "alpha 1\nbeta 2\n", &mut ctx, Opts::new());
    assert!(ec.is_ok(), "{}", tensor_kdl::format_error_code(&ec));
    assert_eq!(
        items,
        [
            Item {
                name: "alpha".into(),
                n: 1
            },
            Item {
                name: "beta".into(),
                n: 2
            }
        ]
    );
}

#[test]
fn read_into_vec_streams_via_toplevel_fill() {
    // P-G3e: DecodeDocument::read_stream for Vec uses TopLevelFill
    // (visit-fill when T: DecodeFromVisit — Glaze array from::op).
    use tensor_kdl::{Decode, DecodeFromVisit, read_into};

    #[derive(Debug, Decode, PartialEq)]
    struct Row {
        #[kdl(argument)]
        n: i64,
        #[kdl(property)]
        name: String,
    }

    let _ = <Row as DecodeFromVisit>::start_visit();
    let mut rows: Vec<Row> = Vec::new();
    let ec = read_into(
        &mut rows,
        r#"row 1 name="a"
row 2 name="b"
"#,
    );
    assert!(ec.is_ok(), "{}", tensor_kdl::format_error_code(&ec));
    assert_eq!(
        rows,
        [
            Row {
                n: 1,
                name: "a".into()
            },
            Row {
                n: 2,
                name: "b".into()
            },
        ]
    );
}

#[test]
fn const_generic_opts_and_read_into() {
    // P-G4: packed u8 opts monomorphize like Glaze template <auto Opts>.
    use tensor_kdl::{
        Decode, DecodeFromVisit, OPTS_DEFAULT, OPTS_LENIENT, OPTS_PARTIAL, Opts,
        decode_node_str_const, read_into_const,
    };

    assert_eq!(Opts::new().bits(), OPTS_DEFAULT);
    assert_eq!(Opts::lenient().bits(), OPTS_LENIENT);
    assert_eq!(Opts::partial().bits(), OPTS_PARTIAL);

    #[derive(Debug, Decode, PartialEq)]
    struct Row {
        #[kdl(argument)]
        n: i64,
        #[kdl(property)]
        name: String,
    }

    let _ = <Row as DecodeFromVisit>::start_visit();
    let row: Row = decode_node_str_const::<Row, OPTS_DEFAULT>(r#"row 3 name="c""#).unwrap();
    assert_eq!(
        row,
        Row {
            n: 3,
            name: "c".into()
        }
    );

    let mut rows: Vec<Row> = Vec::new();
    let ec = read_into_const::<Vec<Row>, OPTS_DEFAULT>(
        &mut rows,
        r#"row 1 name="a"
row 2 name="b"
"#,
    );
    assert!(ec.is_ok(), "{}", tensor_kdl::format_error_code(&ec));
    assert_eq!(rows.len(), 2);

    // partial_read: only first top-level node.
    let mut one: Vec<Row> = Vec::new();
    let ec = read_into_const::<Vec<Row>, OPTS_PARTIAL>(
        &mut one,
        r#"row 1 name="a"
row 2 name="b"
"#,
    );
    assert!(ec.is_ok(), "{}", tensor_kdl::format_error_code(&ec));
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].n, 1);
}

#[test]
fn read_into_children_only_root_streams() {
    // P-G3e: single `#[kdl(children)]` document root overrides read_stream.
    use tensor_kdl::{Decode, DecodeFromVisit, read_into};

    #[derive(Debug, Decode, PartialEq)]
    struct Item {
        #[kdl(argument)]
        n: i64,
    }

    #[derive(Debug, Decode, PartialEq)]
    struct Root {
        #[kdl(children)]
        items: Vec<Item>,
    }

    let _ = <Item as DecodeFromVisit>::start_visit();
    let mut root = Root { items: Vec::new() };
    let ec = read_into(&mut root, "item 1\nitem 2\nitem 3\n");
    assert!(ec.is_ok(), "{}", tensor_kdl::format_error_code(&ec));
    assert_eq!(root.items, [Item { n: 1 }, Item { n: 2 }, Item { n: 3 }]);
}

#[test]
fn derive_decode_document() {
    #[derive(Debug, Decode, PartialEq)]
    struct Package {
        #[kdl(child, unwrap(argument))]
        name: String,
        #[kdl(child, unwrap(argument))]
        version: String,
    }

    #[derive(Debug, Decode, PartialEq)]
    struct Root {
        #[kdl(child)]
        package: Package,
    }

    let root: Root = from_str_decode(
        r#"
        package {
            name my-pkg
            version "1.2.3"
        }
    "#,
    )
    .unwrap();
    assert_eq!(root.package.name, "my-pkg");
    assert_eq!(root.package.version, "1.2.3");
}

#[test]
fn ci_example_parses() {
    let src = include_str!("../../../references/kdl/examples/ci.kdl");
    let doc = from_str(src).expect("ci.kdl should parse");
    assert!(!doc.nodes.is_empty());
}
