//! Advanced derive coverage: named enum variants, flatten, unwrap(property), type_name.

use std::collections::HashMap;

use tensor_kdl::{
    Decode, DecodePartial, DecodeScalar, Flag, Node, Value, from_str, from_str_decode,
};

#[test]
fn named_enum_variants() {
    #[derive(Debug, Decode, PartialEq)]
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

    #[derive(Debug, Decode, PartialEq)]
    struct Root {
        #[kdl(children)]
        items: Vec<Item>,
    }

    let root: Root = from_str_decode(
        r#"
        bind "Mod+Q" action="quit"
        quit
    "#,
    )
    .unwrap();
    assert_eq!(
        root.items[0],
        Item::Bind {
            key: "Mod+Q".into(),
            action: "quit".into(),
        }
    );
    assert!(matches!(root.items[1], Item::Quit(_)));
}

#[test]
fn unwrap_property_on_child() {
    #[derive(Debug, Decode, PartialEq)]
    struct Layout {
        #[kdl(child, unwrap(property))]
        gaps: f64,
    }

    #[derive(Debug, Decode, PartialEq)]
    struct Root {
        #[kdl(child)]
        layout: Layout,
    }

    // Child node `gaps` with property `gaps=8` — unwrap(property) peels that key.
    let root: Root = from_str_decode(
        r#"
        layout {
            gaps gaps=8.0
        }
    "#,
    )
    .unwrap();
    assert_eq!(root.layout.gaps, 8.0);
}

#[test]
fn type_name_and_node_name() {
    #[derive(Debug, Decode, PartialEq)]
    struct Annotated {
        #[kdl(type_name)]
        kind: Option<String>,
        #[kdl(node_name)]
        name: String,
        #[kdl(argument)]
        value: i64,
    }

    let doc = from_str("(role)widget 42").unwrap();
    let w = Annotated::decode_node(&doc.nodes[0]).unwrap();
    assert_eq!(w.kind.as_deref(), Some("role"));
    assert_eq!(w.name, "widget");
    assert_eq!(w.value, 42);
}

#[test]
fn flatten_extra_children() {
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
    struct Config {
        #[kdl(child, unwrap(argument))]
        version: Option<String>,
        #[kdl(flatten)]
        extra: Extra,
    }

    let cfg: Config = from_str_decode(
        r#"
        version "1"
        alpha
        beta
    "#,
    )
    .unwrap();
    assert_eq!(cfg.version.as_deref(), Some("1"));
    assert_eq!(cfg.extra.flags, vec!["alpha", "beta"]);
}

#[test]
fn decode_scalar_rename() {
    #[derive(Debug, DecodeScalar, PartialEq)]
    enum Mode {
        #[kdl(name = "scrolling-1d")]
        Scrolling1d,
        Spatial2d,
    }

    let doc = from_str(r#"layout "scrolling-1d""#).unwrap();
    let mode = Mode::decode_scalar(doc.nodes[0].arguments().next().unwrap()).unwrap();
    assert_eq!(mode, Mode::Scrolling1d);
}

#[test]
fn properties_map() {
    #[derive(Debug, Decode, PartialEq)]
    struct NodeCfg {
        #[kdl(properties)]
        props: HashMap<String, String>,
    }

    let doc = from_str(r#"item a="1" b="two""#).unwrap();
    let n = NodeCfg::decode_node(&doc.nodes[0]).unwrap();
    assert_eq!(n.props.get("a").map(String::as_str), Some("1"));
    assert_eq!(n.props.get("b").map(String::as_str), Some("two"));
}
