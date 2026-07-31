//! KQL subset coverage against `references/kdl/QUERY-SPEC.md`.

use tensor_kdl::{Document, Entry, KdlStr, Node, Value, from_str, query, query_node};

fn package_document() -> Document<'static> {
    let input = r#"
            package {
                name foo
                version "1"
                dependencies platform=windows {
                    winapi "1"
                }
                dependencies {
                    miette "2" dev=#true
                }
            }
        "#;
    let parsed = from_str(input).unwrap();
    Document {
        nodes: parsed
            .nodes
            .into_iter()
            .map(|node| node_into_owned(&node))
            .collect(),
    }
}

fn node_into_owned(node: &Node<'_>) -> Node<'static> {
    Node {
        type_name: node
            .type_name
            .as_ref()
            .map(|value| KdlStr::owned(value.as_str().to_owned())),
        name: KdlStr::owned(node.name.as_str().to_owned()),
        entries: node
            .entries
            .clone()
            .into_iter()
            .map(entry_into_owned)
            .collect(),
        children: node.children.iter().map(node_into_owned).collect(),
    }
}

fn entry_into_owned(entry: Entry<'_>) -> Entry<'static> {
    match entry {
        Entry::Argument { type_name, value } => Entry::Argument {
            type_name: type_name.map(|name| KdlStr::owned(name.into_owned())),
            value: value_into_owned(value),
        },
        Entry::Property {
            key,
            type_name,
            value,
        } => Entry::Property {
            key: KdlStr::owned(key.into_owned()),
            type_name: type_name.map(|name| KdlStr::owned(name.into_owned())),
            value: value_into_owned(value),
        },
    }
}

fn value_into_owned(value: Value<'_>) -> Value<'static> {
    match value {
        Value::String(value) => Value::String(KdlStr::owned(value.into_owned())),
        Value::Int(value) => Value::Int(value),
        Value::Float { value, raw } => Value::Float {
            value,
            raw: raw.map(|raw| KdlStr::owned(raw.into_owned())),
        },
        Value::Bool(value) => Value::Bool(value),
        Value::Null => Value::Null,
    }
}

#[test]
fn spec_name_child_descendant_and_property_examples() {
    let doc = package_document();
    assert_eq!(query(&doc, "package >> name").len(), 1);
    assert_eq!(query(&doc, "top() > package >> name").len(), 1);
    assert_eq!(query(&doc, "dependencies").len(), 2);
    assert_eq!(query(&doc, "dependencies[platform]").len(), 1);
    assert_eq!(query(&doc, "dependencies[prop(platform)]").len(), 1);
    assert_eq!(query(&doc, "dependencies > []").len(), 2);
}

#[test]
fn top_union_and_accessors_are_unique_and_document_ordered() {
    let doc = from_str("a 1 flag=#true\nb 2\n").unwrap();
    assert_eq!(query(&doc, "top() > []").len(), 2);
    let union = query(&doc, "b || a || a");
    assert_eq!(
        union
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(query(&doc, "a[val()]").len(), 1);
    assert_eq!(query(&doc, "a[val(1)]").len(), 0);
    assert_eq!(query(&doc, "a[flag]").len(), 1);
}

#[test]
fn type_annotation_and_node_subtree() {
    let doc = from_str("(role)widget { child }\n").unwrap();
    assert_eq!(query(&doc, "(role)widget").len(), 1);
    assert_eq!(query(&doc, "() > child").len(), 1);
    assert_eq!(query_node(&doc.nodes[0], "widget || child").len(), 2);
}

#[test]
fn equality_and_string_matchers() {
    let doc = from_str(
        r#"
            item name=alpha version=1
            item name=alphabet version=2
            row "hello-world"
            flagged enabled=#true
            mixed label="1" count=1
        "#,
    )
    .unwrap();
    assert_eq!(query(&doc, "item[name = alpha]").len(), 1);
    assert_eq!(query(&doc, r#"item[prop(name) = "alpha"]"#).len(), 1);
    assert_eq!(query(&doc, "item[version = 1]").len(), 1);
    assert_eq!(query(&doc, "item[version != 1]").len(), 1);
    assert_eq!(query(&doc, "item[name ^= alph]").len(), 2);
    assert_eq!(query(&doc, "item[name $= bet]").len(), 1);
    assert_eq!(query(&doc, "item[name *= phabe]").len(), 1);
    assert_eq!(query(&doc, r#"row[val() = "hello-world"]"#).len(), 1);
    assert_eq!(query(&doc, "flagged[enabled = #true]").len(), 1);
    assert_eq!(query(&doc, r#"[name() = item]"#).len(), 2);
    // QUERY-SPEC: no cross-type coercion between string "1" and int 1.
    assert_eq!(query(&doc, r#"mixed[label = 1]"#).len(), 0);
    assert_eq!(query(&doc, r#"mixed[count = "1"]"#).len(), 0);
    assert_eq!(query(&doc, "mixed[count = 1]").len(), 1);
    assert_eq!(query(&doc, r#"mixed[label = "1"]"#).len(), 1);
}

#[test]
fn ordered_comparisons_same_type_only() {
    let doc = from_str(
        r#"
            n version=1
            n version=2
            n version=3
            s label=aa
            s label=bb
        "#,
    )
    .unwrap();
    assert_eq!(query(&doc, "n[version > 1]").len(), 2);
    assert_eq!(query(&doc, "n[version >= 2]").len(), 2);
    assert_eq!(query(&doc, "n[version < 3]").len(), 2);
    assert_eq!(query(&doc, "n[version <= 1]").len(), 1);
    assert_eq!(query(&doc, "s[label > aa]").len(), 1);
    // Different types → fail (string vs int).
    assert_eq!(query(&doc, r#"s[label > 1]"#).len(), 0);
}

#[test]
fn sibling_operators() {
    let doc = from_str(
        r#"
            parent {
                a
                b
                c
            }
        "#,
    )
    .unwrap();
    assert_eq!(query(&doc, "a + b").len(), 1);
    assert_eq!(query(&doc, "a + c").len(), 0);
    assert_eq!(query(&doc, "a ++ c").len(), 1);
    assert_eq!(query(&doc, "b ++ a").len(), 0);

    let top = from_str("first\nsecond\nthird\n").unwrap();
    assert_eq!(query(&top, "first + second").len(), 1);
    assert_eq!(query(&top, "first ++ third").len(), 1);
}

#[test]
fn values_props_existence_and_value_type_rhs() {
    // QUERY-SPEC: values()/props() existence; [val() = (tag)] matches value type.
    let doc = from_str(
        r#"
            plain
            args 1 2
            props only=1
            both 1 only=2
            typed (sri)"deadbeef"
            prop_typed integrity=(sri)sha512-deadbeef
        "#,
    )
    .unwrap();
    // nodes with any argument: args, both, typed
    assert_eq!(query(&doc, "[values()]").len(), 3);
    // nodes with any property: props, both, prop_typed
    assert_eq!(query(&doc, "[props()]").len(), 3);
    assert_eq!(query(&doc, r#"typed[val() = (sri)]"#).len(), 1);
    assert_eq!(
        query(&doc, r#"prop_typed[prop(integrity) = (sri)]"#).len(),
        1
    );
    assert_eq!(query(&doc, r#"prop_typed[integrity = (sri)]"#).len(), 1);
    assert_eq!(query(&doc, r#"args[val() = (sri)]"#).len(), 0);
    assert_eq!(query(&doc, r#"typed[val() = ()]"#).len(), 1);
}
