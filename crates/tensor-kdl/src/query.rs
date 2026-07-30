//! Deliberately small KDL Query Language subset over the DOM.
//!
//! The syntax follows `references/kdl/QUERY-SPEC.md`, but this is not a full
//! KQL implementation. Supported today:
//!
//! - node names, `[]`, `(tag)` and `(tag)name`;
//! - `top()`, direct-child `>`, descendant `>>`, and union `||`;
//! - existence matchers `[val()]`, `[val(n)]`, `[prop(name)]`, and `[name]`.
//!
//! Sibling operators and value comparisons intentionally remain unsupported.

use std::collections::{HashMap, HashSet};

use crate::value::{Document, Node};

#[derive(Clone, Copy)]
enum Operator {
    Child,
    Descendant,
}

/// Run a supported KQL selector against `doc`.
///
/// Results are de-duplicated and returned in document depth-first order,
/// including when union arms are written in a different order.
pub fn query<'a>(doc: &'a Document<'a>, selector: &str) -> Vec<&'a Node<'a>> {
    query_roots(&doc.nodes, selector)
}

/// Run a supported KQL selector against one node subtree, including `root`.
pub fn query_node<'a>(root: &'a Node<'a>, selector: &str) -> Vec<&'a Node<'a>> {
    query_roots(std::slice::from_ref(root), selector)
}

fn query_roots<'a>(roots: &'a [Node<'a>], selector: &str) -> Vec<&'a Node<'a>> {
    let mut matches = Vec::new();
    for arm in split_union(selector.trim()) {
        select_arm(roots, arm, &mut matches);
    }

    let mut order = HashMap::new();
    let mut next = 0usize;
    for root in roots {
        index_subtree(root, &mut next, &mut order);
    }
    let mut seen = HashSet::new();
    matches.retain(|node| seen.insert(*node as *const Node<'a>));
    matches.sort_by_key(|node| {
        order
            .get(&(*node as *const Node<'a>))
            .copied()
            .unwrap_or(usize::MAX)
    });
    matches
}

fn index_subtree<'a>(
    node: &'a Node<'a>,
    next: &mut usize,
    order: &mut HashMap<*const Node<'a>, usize>,
) {
    order.insert(node as *const Node<'a>, *next);
    *next += 1;
    for child in &node.children {
        index_subtree(child, next, order);
    }
}

fn split_union(selector: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut brackets = 0u32;
    let mut parentheses = 0u32;
    let bytes = selector.as_bytes();
    let mut index = 0usize;
    while index + 1 < bytes.len() {
        match bytes[index] {
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.saturating_sub(1),
            b'|' if brackets == 0 && parentheses == 0 && bytes[index + 1] == b'|' => {
                let part = selector[start..index].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                index += 2;
                start = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    let tail = selector[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

fn select_arm<'a>(roots: &'a [Node<'a>], selector: &str, out: &mut Vec<&'a Node<'a>>) {
    let Some((filters, operators)) = parse_chain(selector) else {
        return;
    };
    let Some(first) = filters.first().copied() else {
        return;
    };

    let (mut current, mut operator_index) = if first == "top()" {
        if operators.is_empty() {
            (roots.iter().collect(), 0)
        } else {
            let next_filter = filters[1];
            let selected = match operators[0] {
                Operator::Child => roots
                    .iter()
                    .filter(|node| node_matches(node, next_filter))
                    .collect(),
                Operator::Descendant => {
                    let mut selected = Vec::new();
                    for root in roots {
                        collect_matching(root, next_filter, true, &mut selected);
                    }
                    selected
                }
            };
            (selected, 1)
        }
    } else {
        let mut selected = Vec::new();
        for root in roots {
            collect_matching(root, first, true, &mut selected);
        }
        (selected, 0)
    };

    while operator_index < operators.len() {
        let filter = filters[operator_index + 1];
        let mut next = Vec::new();
        match operators[operator_index] {
            Operator::Child => {
                for node in current {
                    next.extend(
                        node.children
                            .iter()
                            .filter(|child| node_matches(child, filter)),
                    );
                }
            }
            Operator::Descendant => {
                for node in current {
                    for child in &node.children {
                        collect_matching(child, filter, true, &mut next);
                    }
                }
            }
        }
        current = next;
        operator_index += 1;
    }
    out.extend(current);
}

fn parse_chain(selector: &str) -> Option<(Vec<&str>, Vec<Operator>)> {
    let bytes = selector.as_bytes();
    let mut filters = Vec::new();
    let mut operators = Vec::new();
    let mut brackets = 0u32;
    let mut parentheses = 0u32;
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.saturating_sub(1),
            b'>' if brackets == 0 && parentheses == 0 => {
                let filter = selector[start..index].trim();
                if filter.is_empty() {
                    return None;
                }
                filters.push(filter);
                if bytes.get(index + 1) == Some(&b'>') {
                    operators.push(Operator::Descendant);
                    index += 2;
                } else {
                    operators.push(Operator::Child);
                    index += 1;
                }
                start = index;
                continue;
            }
            b'+' if brackets == 0 && parentheses == 0 => return None,
            _ => {}
        }
        index += 1;
    }
    let tail = selector[start..].trim();
    if tail.is_empty() || brackets != 0 || parentheses != 0 {
        return None;
    }
    filters.push(tail);
    (filters.len() == operators.len() + 1).then_some((filters, operators))
}

fn collect_matching<'a>(
    node: &'a Node<'a>,
    filter: &str,
    include_self: bool,
    out: &mut Vec<&'a Node<'a>>,
) {
    if include_self && node_matches(node, filter) {
        out.push(node);
    }
    for child in &node.children {
        collect_matching(child, filter, true, out);
    }
}

fn node_matches(node: &Node<'_>, filter: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() || filter == "top()" {
        return false;
    }

    let (type_filter, rest) = parse_type_filter(filter);
    if let Some(expected) = type_filter {
        let Some(actual) = node.type_name.as_ref().map(|name| name.as_str()) else {
            return false;
        };
        if !expected.is_empty() && actual != expected {
            return false;
        }
    }
    if rest.is_empty() {
        return type_filter.is_some();
    }

    let (name, accessors) = match split_name_and_accessors(rest) {
        Some(parts) => parts,
        None => return false,
    };
    if !name.is_empty() && name != "[]" && node.name.as_str() != name {
        return false;
    }
    accessors
        .into_iter()
        .all(|accessor| accessor_matches(node, accessor))
}

fn parse_type_filter(filter: &str) -> (Option<&str>, &str) {
    let Some(rest) = filter.strip_prefix('(') else {
        return (None, filter);
    };
    let Some(end) = rest.find(')') else {
        return (Some("\0"), "");
    };
    (Some(rest[..end].trim()), rest[end + 1..].trim())
}

fn split_name_and_accessors(filter: &str) -> Option<(&str, Vec<&str>)> {
    let first = filter.find('[').unwrap_or(filter.len());
    let name = filter[..first].trim();
    let mut accessors = Vec::new();
    let mut rest = &filter[first..];
    while !rest.is_empty() {
        let inner = rest.strip_prefix('[')?;
        let end = inner.find(']')?;
        accessors.push(inner[..end].trim());
        rest = inner[end + 1..].trim();
    }
    if name.is_empty() && accessors.is_empty() {
        None
    } else {
        Some((name, accessors))
    }
}

fn accessor_matches(node: &Node<'_>, accessor: &str) -> bool {
    if accessor.is_empty() {
        return true;
    }
    if let Some(argument) = accessor
        .strip_prefix("val(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let index = if argument.trim().is_empty() {
            0
        } else if let Ok(index) = argument.trim().parse::<usize>() {
            index
        } else {
            return false;
        };
        return node.arguments().nth(index).is_some();
    }
    if let Some(property) = accessor
        .strip_prefix("prop(")
        .and_then(|value| value.strip_suffix(')'))
    {
        return node.property(property.trim()).is_some();
    }
    if accessor.contains(['=', '<', '>', '^', '$', '*', '!', '(', ')']) {
        return false;
    }
    node.property(accessor).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_str;

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
                .map(|value| crate::KdlStr::owned(value.as_str().to_owned())),
            name: crate::KdlStr::owned(node.name.as_str().to_owned()),
            entries: node
                .entries
                .clone()
                .into_iter()
                .map(entry_into_owned)
                .collect(),
            children: node.children.iter().map(node_into_owned).collect(),
        }
    }

    fn entry_into_owned(entry: crate::Entry<'_>) -> crate::Entry<'static> {
        match entry {
            crate::Entry::Argument { type_name, value } => crate::Entry::Argument {
                type_name: type_name.map(|name| crate::KdlStr::owned(name.into_owned())),
                value: value_into_owned(value),
            },
            crate::Entry::Property {
                key,
                type_name,
                value,
            } => crate::Entry::Property {
                key: crate::KdlStr::owned(key.into_owned()),
                type_name: type_name.map(|name| crate::KdlStr::owned(name.into_owned())),
                value: value_into_owned(value),
            },
        }
    }

    fn value_into_owned(value: crate::Value<'_>) -> crate::Value<'static> {
        match value {
            crate::Value::String(value) => {
                crate::Value::String(crate::KdlStr::owned(value.into_owned()))
            }
            crate::Value::Int(value) => crate::Value::Int(value),
            crate::Value::Float { value, raw } => crate::Value::Float {
                value,
                raw: raw.map(|raw| crate::KdlStr::owned(raw.into_owned())),
            },
            crate::Value::Bool(value) => crate::Value::Bool(value),
            crate::Value::Null => crate::Value::Null,
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
}
