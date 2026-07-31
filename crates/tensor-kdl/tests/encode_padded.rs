//! Typed encode and padded direct-read coverage.

use tensor_kdl::{
    CtxResult, Decode, DecodeDocument, DecodeScalar, Document, Encode, EncodeScalar, ErrorCtx,
    OPTS_PARTIAL, Opts, PaddedInput, Parser, read_into_padded, read_into_padded_const, to_string,
    to_string_node,
};

#[derive(Debug, DecodeScalar, EncodeScalar, PartialEq)]
enum Mode {
    #[kdl(name = "scrolling-1d")]
    Scrolling1d,
    Spatial2d,
}

#[derive(Debug, Decode, Encode, PartialEq)]
struct Item {
    #[kdl(argument)]
    id: i64,
    #[kdl(property)]
    mode: Mode,
}

#[derive(Debug, Decode, Encode, PartialEq)]
struct Widget {
    #[kdl(argument)]
    id: i64,
    #[kdl(property)]
    enabled: bool,
    #[kdl(child(name = "title"), unwrap(argument))]
    title: Option<String>,
    #[kdl(children(name = "item"))]
    items: Vec<Item>,
}

#[derive(Debug, Decode, Encode, PartialEq)]
struct Root {
    #[kdl(child(name = "version"), unwrap(argument))]
    version: String,
    #[kdl(children(name = "widget"))]
    widgets: Vec<Widget>,
}

#[test]
fn derive_encode_round_trips_node_and_document_shapes() {
    let widget = Widget {
        id: 7,
        enabled: true,
        title: Some("Main panel".to_owned()),
        items: vec![Item {
            id: 1,
            mode: Mode::Scrolling1d,
        }],
    };
    let node_text = to_string_node(&widget).unwrap();
    assert!(node_text.starts_with("widget 7 enabled=#true"));
    assert!(node_text.contains("title \"Main panel\""));
    assert!(node_text.contains("item 1 mode=scrolling-1d"));

    let node_doc = tensor_kdl::from_str(&node_text).unwrap();
    assert_eq!(Widget::decode_node(&node_doc.nodes[0]).unwrap(), widget);

    let root = Root {
        version: "2".to_owned(),
        widgets: vec![widget],
    };
    let document_text = to_string(&root).unwrap();
    assert!(document_text.starts_with("version \"2\"\nwidget 7"));
    let decoded: Root = tensor_kdl::from_str_decode(&document_text).unwrap();
    assert_eq!(decoded, root);
}

#[test]
fn encode_scalar_newtype_delegates_to_inner_value() {
    #[derive(Debug, EncodeScalar)]
    struct Count(i64);

    assert_eq!(Count(9).encode_scalar().unwrap().as_i128(), Some(9));
    assert_eq!(
        Mode::Spatial2d.encode_scalar().unwrap().as_str(),
        Some("spatial2d")
    );
}

#[test]
fn encode_enum_variants_and_unwrap_property_round_trip() {
    use tensor_kdl::{Flag, from_str_decode};

    #[derive(Debug, Decode, Encode, PartialEq)]
    enum Item {
        #[kdl(name = "bind")]
        Bind {
            #[kdl(argument)]
            key: String,
            #[kdl(property)]
            action: String,
        },
        Quit(Flag),
    }

    #[derive(Debug, Decode, Encode, PartialEq)]
    struct Layout {
        #[kdl(child, unwrap(property))]
        gaps: f64,
    }

    #[derive(Debug, Decode, Encode, PartialEq)]
    struct PropsNode {
        #[kdl(properties)]
        props: std::collections::HashMap<String, String>,
    }

    let items = [
        Item::Bind {
            key: "Mod+Q".into(),
            action: "quit".into(),
        },
        Item::Quit(Flag),
    ];
    let bind_text = to_string_node(&items[0]).unwrap();
    assert!(
        bind_text.contains("bind")
            && bind_text.contains("Mod+Q")
            && bind_text.contains("action=quit"),
        "unexpected bind encode: {bind_text:?}"
    );
    let quit_text = to_string_node(&items[1]).unwrap();
    assert_eq!(quit_text.trim(), "quit");

    let layout = Layout { gaps: 8.0 };
    // Children-only root: document encode emits top-level `gaps gaps=…`
    // (design §11 / suite Translation Rules via `format_document`).
    let layout_doc = to_string(&layout).unwrap();
    assert!(
        layout_doc.contains("gaps") && layout_doc.contains("8"),
        "unexpected layout encode: {layout_doc:?}"
    );
    let decoded_layout: Layout = from_str_decode(&layout_doc).unwrap();
    assert_eq!(decoded_layout, layout);

    let mut props = std::collections::HashMap::new();
    props.insert("b".into(), "two".into());
    props.insert("a".into(), "1".into());
    let props_node = PropsNode { props };
    let props_text = to_string_node(&props_node).unwrap();
    // Canonical formatter: properties alphabetical (suite translation rules).
    assert!(
        props_text.contains("a=") && props_text.contains("b="),
        "unexpected props encode: {props_text:?}"
    );
    let a_pos = props_text.find("a=").unwrap();
    let b_pos = props_text.find("b=").unwrap();
    assert!(
        a_pos < b_pos,
        "properties must be alphabetical: {props_text:?}"
    );
    let round: PropsNode = {
        let doc = tensor_kdl::from_str(&props_text).unwrap();
        PropsNode::decode_node(&doc.nodes[0]).unwrap()
    };
    assert_eq!(round.props.get("a").map(String::as_str), Some("1"));
    assert_eq!(round.props.get("b").map(String::as_str), Some("two"));
}

#[derive(Default)]
struct PaddedProbe {
    used_padding: bool,
    nodes: usize,
}

impl<'a> DecodeDocument<'a> for PaddedProbe {
    fn decode_document(doc: &Document<'a>) -> CtxResult<Self> {
        Ok(Self {
            used_padding: false,
            nodes: doc.nodes.len(),
        })
    }

    fn read_stream_parser(out: &mut Self, parser: &mut Parser<'a>, opts: Opts) -> CtxResult<()> {
        out.used_padding = parser.is_padded();
        out.nodes = 0;
        parser.visit_document(opts, |_| {
            out.nodes += 1;
            Ok(())
        })
    }
}

#[test]
fn padded_read_uses_live_padded_parser_and_const_opts() {
    let input = PaddedInput::new("row 1\nrow 2\n");
    let mut probe = PaddedProbe::default();
    let error = read_into_padded(&mut probe, &input);
    assert_eq!(error, ErrorCtx::ok(input.len()));
    assert!(probe.used_padding);
    assert_eq!(probe.nodes, 2);

    let mut partial = PaddedProbe::default();
    let error = read_into_padded_const::<PaddedProbe, OPTS_PARTIAL>(&mut partial, &input);
    assert!(error.is_ok());
    assert!(partial.used_padding);
    assert_eq!(partial.nodes, 1);
}
