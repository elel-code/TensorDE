//! Basic parse, visit-fill, and Glaze-aligned API tests.

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
}

#[test]
fn line_continuation() {
    let doc = from_str("a 1 \\\n 2").unwrap();
    assert_eq!(doc.nodes[0].arguments().count(), 2);
}

#[test]
fn format_error_shows_snippet() {
    let err = from_str("ok\nbad {\n").unwrap_err();
    let formatted = format_error(&err.ctx, "ok\nbad {\n");
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
    assert!(ec.consumed > 0);
    assert_eq!(root.name.as_deref(), Some("hello"));
}

#[test]
fn empty_kdl_document_is_valid() {
    let doc = from_str("").unwrap();
    assert!(doc.is_empty());
}

#[test]
fn decode_from_visit_derive() {
    use tensor_kdl::{Decode, DecodeFromVisit, Opts, VisitBuilder, decode_node_str};

    #[derive(Debug, Decode, PartialEq)]
    struct Item {
        #[kdl(argument)]
        n: i64,
        #[kdl(property)]
        name: String,
    }

    assert!(VisitBuilder::finish(<Item as DecodeFromVisit>::start_visit()).is_err());
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

    let _ = <Child as DecodeFromVisit>::start_visit();
    let _ = <Parent as DecodeFromVisit>::start_visit();
    let parent: Parent =
        decode_node_str(r#"parent id="p1" { child 9 label="nested" }"#, Opts::new()).unwrap();
    assert_eq!(parent.child.n, 9);
    assert_eq!(parent.child.label, "nested");
}

#[test]
fn nested_unwrap_peels_without_node_on_visit_path() {
    // P-G13: unwrap(argument|property) on nested children uses peel helpers
    // via take_child_after_header — no temporary Node on the visit path.
    use tensor_kdl::{Decode, DecodeFromVisit, Opts, decode_node_str};

    #[derive(Debug, Decode, PartialEq)]
    struct Layout {
        #[kdl(child, unwrap(argument))]
        title: String,
        #[kdl(child, unwrap(property))]
        gaps: f64,
        #[kdl(child, unwrap(argument))]
        optional_note: Option<i64>,
    }

    let _ = <Layout as DecodeFromVisit>::start_visit();
    let layout: Layout = decode_node_str(
        r#"layout {
            title "hello"
            gaps gaps=8.0
        }"#,
        Opts::new(),
    )
    .unwrap();
    assert_eq!(layout.title, "hello");
    assert!((layout.gaps - 8.0).abs() < f64::EPSILON);
    assert_eq!(layout.optional_note, None);

    let with_note: Layout = decode_node_str(
        r#"layout {
            title "x"
            gaps gaps=1.5
            optional-note 42
        }"#,
        Opts::new(),
    )
    .unwrap();
    assert_eq!(with_note.optional_note, Some(42));
    assert!((with_note.gaps - 1.5).abs() < f64::EPSILON);
}

#[test]
fn read_nodes_into_visit_streams_without_dom_nodes() {
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
    assert_eq!(out.len(), 2);
}

#[test]
fn visit_node_counting_visitor() {
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
    use tensor_kdl::{Opts, read_document_with_opts};

    let doc = read_document_with_opts("first 1\nsecond 2\nthird 3\n", Opts::partial()).unwrap();
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(doc.nodes[0].name.as_str(), "first");
}

#[test]
fn read_nodes_into_decodes_each_top_level() {
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
    assert_eq!(items.len(), 2);
}

#[test]
fn read_into_vec_streams_via_toplevel_fill() {
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
    assert_eq!(rows.len(), 2);
}

#[test]
fn read_into_children_only_root_streams() {
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
fn const_generic_opts_and_read_into() {
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
    assert_eq!(row.n, 3);

    let mut one: Vec<Row> = Vec::new();
    let ec = read_into_const::<Vec<Row>, OPTS_PARTIAL>(
        &mut one,
        r#"row 1 name="a"
row 2 name="b"
"#,
    );
    assert!(ec.is_ok());
    assert_eq!(one.len(), 1);
}

#[test]
fn unique_index_property_dispatch() {
    use tensor_kdl::{Decode, DecodeFromVisit, Opts, decode_node_str};

    #[derive(Debug, Decode, PartialEq)]
    struct Wide {
        #[kdl(property)]
        alpha: i64,
        #[kdl(property)]
        beta: i64,
        #[kdl(property)]
        gamma: i64,
        #[kdl(property)]
        delta: i64,
    }

    let _ = <Wide as DecodeFromVisit>::start_visit();
    let w: Wide = decode_node_str(r#"row alpha=1 beta=2 gamma=3 delta=4"#, Opts::new()).unwrap();
    assert_eq!(w.delta, 4);
    assert!(
        decode_node_str::<Wide>(r#"row alpha=1 beta=2 gamma=3 delta=4 zeta=9"#, Opts::new())
            .is_err()
    );
}

#[test]
fn sized_and_modular_key_dispatch() {
    use tensor_kdl::{Decode, DecodeFromVisit, Opts, decode_node_str};

    #[derive(Debug, Decode, PartialEq)]
    struct SizedKeys {
        #[kdl(property, name = "a")]
        a: i64,
        #[kdl(property, name = "ab")]
        ab: i64,
        #[kdl(property, name = "abc")]
        abc: i64,
    }

    let _ = <SizedKeys as DecodeFromVisit>::start_visit();
    let s: SizedKeys = decode_node_str(r#"n a=1 ab=2 abc=3"#, Opts::new()).unwrap();
    assert_eq!(s.abc, 3);

    #[derive(Debug, Decode, PartialEq)]
    struct ModularKeys {
        #[kdl(property, name = "aa")]
        aa: i64,
        #[kdl(property, name = "ab")]
        ab: i64,
        #[kdl(property, name = "ba")]
        ba: i64,
        #[kdl(property, name = "bb")]
        bb: i64,
    }

    let _ = <ModularKeys as DecodeFromVisit>::start_visit();
    let m: ModularKeys = decode_node_str(r#"n aa=1 ab=2 ba=3 bb=4"#, Opts::new()).unwrap();
    assert_eq!(m.bb, 4);
}

#[test]
fn single_node_document_root_streams() {
    use tensor_kdl::{Decode, DecodeDocument, DecodeFromVisit, read_into};

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
    let mut w = Widget {
        id: 0,
        name: String::new(),
        enabled: false,
    };
    let ec = read_into(
        &mut w,
        r#"widget 42 name="panel" enabled=#true
ignored-second 99
"#,
    );
    assert!(ec.is_ok(), "{}", tensor_kdl::format_error_code(&ec));
    assert_eq!(w.id, 42);
    assert_eq!(w.name, "panel");
    assert!(w.enabled);

    let doc = from_str(r#"widget 7 name="x" enabled=#false"#).unwrap();
    let w2 = Widget::decode_document(&doc).unwrap();
    assert_eq!(w2.id, 7);
}

#[test]
fn padded_input_and_flatten_stream_root() {
    use tensor_kdl::{
        Decode, DecodePartial, Node, PADDING_BYTES, PaddedInput, Value, from_padded, read_into,
    };

    let pad = PaddedInput::new("node 1\n");
    assert_eq!(pad.padded_bytes().len(), pad.len() + PADDING_BYTES);
    let doc = from_padded(&pad).unwrap();
    assert_eq!(doc.nodes.len(), 1);

    #[derive(Debug, Default, PartialEq)]
    struct Extra {
        flags: Vec<String>,
    }
    impl<'a> DecodePartial<'a> for Extra {
        fn insert_child(&mut self, node: &Node<'a>) -> tensor_kdl::CtxResult<bool> {
            self.flags.push(node.name.as_str().to_owned());
            Ok(true)
        }
        fn insert_property(
            &mut self,
            _key: &str,
            _value: &Value<'a>,
        ) -> tensor_kdl::CtxResult<bool> {
            Ok(false)
        }
    }

    #[derive(Debug, Decode, PartialEq)]
    struct Cfg {
        #[kdl(child, unwrap(argument))]
        version: Option<String>,
        #[kdl(flatten)]
        extra: Extra,
    }

    let mut cfg = Cfg {
        version: None,
        extra: Extra::default(),
    };
    let ec = read_into(
        &mut cfg,
        r#"
        version "2"
        alpha
        beta
        "#,
    );
    assert!(ec.is_ok(), "{}", tensor_kdl::format_error_code(&ec));
    assert_eq!(cfg.version.as_deref(), Some("2"));
    assert_eq!(cfg.extra.flags, vec!["alpha", "beta"]);
}

#[test]
fn padded_parser_overread_and_mixed_siblings() {
    // P-G10a / P-G10b
    use tensor_kdl::{
        Decode, DecodeDocument, DecodeFromVisit, PADDING_BYTES, PaddedInput, Parser, from_padded,
        read_into,
    };

    let pad = PaddedInput::new(r#"msg "hello""#);
    assert!(pad.padded_bytes().len() >= pad.len() + PADDING_BYTES);
    let mut parser = Parser::from_padded(&pad);
    assert!(parser.is_padded());
    let doc = parser.parse_document().unwrap();
    assert_eq!(doc.nodes[0].name.as_str(), "msg");

    let pad2 = PaddedInput::new("a 1\nb 2\n");
    assert_eq!(from_padded(&pad2).unwrap().nodes.len(), 2);

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
        #[kdl(property)]
        label: String,
        #[kdl(children)]
        rest: Vec<Item>,
    }

    let _ = <Mixed as DecodeFromVisit>::start_visit();
    let mut m = Mixed {
        id: 0,
        label: String::new(),
        rest: Vec::new(),
    };
    let ec = read_into(
        &mut m,
        r#"
        root 7 label="main"
        alpha 1
        beta 2
        "#,
    );
    assert!(ec.is_ok(), "{}", tensor_kdl::format_error_code(&ec));
    assert_eq!(m.id, 7);
    assert_eq!(m.label, "main");
    assert_eq!(m.rest.len(), 2);
    assert_eq!(m.rest[0].name, "alpha");
    assert_eq!(m.rest[1].n, 2);

    let doc = from_str(
        r#"
        root 1 label="x"
        sib 9
        "#,
    )
    .unwrap();
    let m2 = Mixed::decode_document(&doc).unwrap();
    assert_eq!(m2.rest.len(), 1);
    assert_eq!(m2.rest[0].name, "sib");
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
    assert!(!doc.is_empty());
}
