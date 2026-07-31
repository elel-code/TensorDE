//! Deliberately small KDL Query Language subset over the DOM.
//!
//! Authority: `references/kdl/QUERY-SPEC.md` (KQL `next`, unreleased). This is
//! **not** a full KQL implementation — see the supported list below. Design
//! non-goal: full QUERY-SPEC / SCHEMA-SPEC (`docs/kdl/design.md` §2).
//!
//! Supported today:
//!
//! - node names, `[]`, `(tag)` and `(tag)name`;
//! - `top()`, direct-child `>`, descendant `>>`, immediate sibling `+`,
//!   general sibling `++`, and union `||`;
//! - existence matchers `[val()]`, `[val(n)]`, `[prop(name)]`, and bare `[name]`;
//! - equality / inequality (`=`, `!=`) on `val()`, `prop()`, bare props,
//!   `name()`, `tag()` — **no cross-type coercion** (QUERY-SPEC: `"1"` is never
//!   equal to `1`);
//! - same-type ordered comparisons (`<`, `>`, `<=`, `>=`) on `val()` / `prop()`
//!   numbers or strings;
//! - string operators `^=` / `$=` / `*=` on string `val()`, `prop()`, `tag()`,
//!   or `name()` values.
//!
//! Still unsupported (QUERY-SPEC has them; we deliberately omit):
//!
//! - `values()` / `props()` accessors;
//! - type-annotation match on the right-hand side (`[val() = (foo)]`);
//! - whitespace-significant multi-token filters beyond the subset above.

use std::collections::{HashMap, HashSet};

use crate::value::{Document, Node, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operator {
    Child,
    Descendant,
    ImmediateSibling,
    GeneralSibling,
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

    // Sibling map: child pointer → (parent children slice, index in that slice).
    let mut sibling_map: HashMap<*const Node<'a>, (&'a [Node<'a>], usize)> = HashMap::new();
    for root in roots {
        index_siblings(root, roots, &mut sibling_map);
    }

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
                Operator::ImmediateSibling | Operator::GeneralSibling => {
                    // `top()` has no siblings in the document model.
                    Vec::new()
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
            Operator::ImmediateSibling => {
                for node in current {
                    if let Some((siblings, index)) = sibling_map.get(&(node as *const Node<'a>))
                        && let Some(sib) = siblings.get(index + 1)
                        && node_matches(sib, filter)
                    {
                        next.push(sib);
                    }
                }
            }
            Operator::GeneralSibling => {
                for node in current {
                    if let Some((siblings, index)) = sibling_map.get(&(node as *const Node<'a>)) {
                        for sib in siblings.iter().skip(index + 1) {
                            if node_matches(sib, filter) {
                                next.push(sib);
                            }
                        }
                    }
                }
            }
        }
        current = next;
        operator_index += 1;
    }
    out.extend(current);
}

fn index_siblings<'a>(
    node: &'a Node<'a>,
    siblings: &'a [Node<'a>],
    map: &mut HashMap<*const Node<'a>, (&'a [Node<'a>], usize)>,
) {
    // Top-level roots share one sibling list; nested children share theirs.
    if let Some(index) = siblings
        .iter()
        .position(|candidate| std::ptr::eq(candidate, node))
    {
        map.insert(node as *const Node<'a>, (siblings, index));
    }
    for child in &node.children {
        index_siblings(child, &node.children, map);
    }
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
            b'+' if brackets == 0 && parentheses == 0 => {
                let filter = selector[start..index].trim();
                if filter.is_empty() {
                    return None;
                }
                filters.push(filter);
                if bytes.get(index + 1) == Some(&b'+') {
                    operators.push(Operator::GeneralSibling);
                    index += 2;
                } else {
                    operators.push(Operator::ImmediateSibling);
                    index += 1;
                }
                start = index;
                continue;
            }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    StartsWith,
    EndsWith,
    Contains,
}

fn accessor_matches(node: &Node<'_>, accessor: &str) -> bool {
    if accessor.is_empty() {
        return true;
    }

    if let Some((left, op, right)) = split_comparison(accessor) {
        return compare_accessor(node, left, op, right);
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
    // Bare property existence: reject anything that looks like a call or operator.
    if accessor.contains(['=', '<', '>', '^', '$', '*', '!', '(', ')']) {
        return false;
    }
    node.property(accessor).is_some()
}

fn split_comparison(accessor: &str) -> Option<(&str, CmpOp, &str)> {
    // Longest operator first (QUERY-SPEC `matcher-operator`).
    for (token, op) in [
        ("!=", CmpOp::Ne),
        (">=", CmpOp::Ge),
        ("<=", CmpOp::Le),
        ("^=", CmpOp::StartsWith),
        ("$=", CmpOp::EndsWith),
        ("*=", CmpOp::Contains),
        (">", CmpOp::Gt),
        ("<", CmpOp::Lt),
        ("=", CmpOp::Eq),
    ] {
        if let Some(idx) = find_operator(accessor, token) {
            let left = accessor[..idx].trim();
            let right = accessor[idx + token.len()..].trim();
            if left.is_empty() || right.is_empty() {
                return None;
            }
            return Some((left, op, right));
        }
    }
    None
}

fn find_operator(accessor: &str, token: &str) -> Option<usize> {
    let bytes = accessor.as_bytes();
    let t = token.as_bytes();
    let mut parentheses = 0u32;
    let mut index = 0usize;
    while index + t.len() <= bytes.len() {
        match bytes[index] {
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.saturating_sub(1),
            _ if parentheses == 0 && bytes[index..].starts_with(t) => {
                // Avoid treating a short operator as the prefix of a longer one
                // (`=` inside `!=` / `^=` / `>=` / …; `>` inside `>=`).
                if token.len() == 1 {
                    if index > 0 {
                        let prev = bytes[index - 1];
                        if matches!(prev, b'!' | b'^' | b'$' | b'*' | b'<' | b'>') {
                            index += 1;
                            continue;
                        }
                    }
                    if index + 1 < bytes.len() {
                        let next = bytes[index + 1];
                        if matches!(next, b'=' | b'>' | b'<') {
                            index += 1;
                            continue;
                        }
                    }
                }
                return Some(index);
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn compare_accessor(node: &Node<'_>, left: &str, op: CmpOp, right: &str) -> bool {
    let Some(actual) = resolve_accessor_value(node, left) else {
        return false;
    };
    // QUERY-SPEC: RHS may be `$type | $string | $number | $keyword`. Type
    // annotations on the RHS (`(foo)`) are not implemented in this subset.
    if right.trim_start().starts_with('(') {
        return false;
    }
    let expected = parse_matcher_literal(right);

    match op {
        CmpOp::Eq => values_equal(&actual, &expected),
        CmpOp::Ne => actual.exists() && !values_equal(&actual, &expected),
        CmpOp::Lt | CmpOp::Gt | CmpOp::Le | CmpOp::Ge => values_ordered(&actual, &expected, op),
        CmpOp::StartsWith | CmpOp::EndsWith | CmpOp::Contains => {
            let Some(hay) = actual.as_str() else {
                return false;
            };
            let Some(needle) = expected.as_str() else {
                return false;
            };
            match op {
                CmpOp::StartsWith => hay.starts_with(needle),
                CmpOp::EndsWith => hay.ends_with(needle),
                CmpOp::Contains => hay.contains(needle),
                _ => false,
            }
        }
    }
}

enum AccessorValue<'a> {
    NodeName(&'a str),
    Tag(&'a str),
    Scalar(&'a Value<'a>),
    OwnedString(String),
    OwnedInt(i128),
    OwnedFloat(f64),
    OwnedBool(bool),
    Null,
}

impl AccessorValue<'_> {
    fn as_str(&self) -> Option<&str> {
        match self {
            Self::NodeName(s) | Self::Tag(s) => Some(s),
            Self::Scalar(Value::String(s)) => Some(s.as_str()),
            Self::OwnedString(s) => Some(s),
            _ => None,
        }
    }

    fn exists(&self) -> bool {
        !matches!(self, Self::Null)
    }

    fn as_i128(&self) -> Option<i128> {
        match self {
            Self::Scalar(Value::Int(n)) => Some(*n),
            Self::OwnedInt(n) => Some(*n),
            _ => None,
        }
    }

    fn as_f64_strict(&self) -> Option<f64> {
        match self {
            Self::Scalar(Value::Float { value, .. }) => Some(*value),
            Self::OwnedFloat(n) => Some(*n),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Scalar(Value::Bool(b)) => Some(*b),
            Self::OwnedBool(b) => Some(*b),
            _ => None,
        }
    }

    fn is_null(&self) -> bool {
        matches!(self, Self::Null | Self::Scalar(Value::Null))
    }
}

fn resolve_accessor_value<'a>(node: &'a Node<'a>, left: &str) -> Option<AccessorValue<'a>> {
    let left = left.trim();
    if left == "name()" {
        return Some(AccessorValue::NodeName(node.name.as_str()));
    }
    if left == "tag()" {
        return node
            .type_name
            .as_ref()
            .map(|t| AccessorValue::Tag(t.as_str()));
    }
    if let Some(argument) = left
        .strip_prefix("val(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let index = if argument.trim().is_empty() {
            0
        } else {
            argument.trim().parse::<usize>().ok()?
        };
        return node.arguments().nth(index).map(AccessorValue::Scalar);
    }
    if let Some(property) = left
        .strip_prefix("prop(")
        .and_then(|value| value.strip_suffix(')'))
    {
        return node.property(property.trim()).map(AccessorValue::Scalar);
    }
    // Bare property name on the left of a comparison (QUERY-SPEC `[name = 1]`).
    if !left.contains(['(', ')', '=', '<', '>', '!', '^', '$', '*']) {
        return node.property(left).map(AccessorValue::Scalar);
    }
    None
}

fn parse_matcher_literal(raw: &str) -> AccessorValue<'static> {
    let raw = raw.trim();
    if raw == "#true" {
        return AccessorValue::OwnedBool(true);
    }
    if raw == "#false" {
        return AccessorValue::OwnedBool(false);
    }
    if raw == "#null" {
        return AccessorValue::Null;
    }
    if let Some(inner) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return AccessorValue::OwnedString(inner.to_owned());
    }
    if let Ok(n) = raw.parse::<i128>() {
        return AccessorValue::OwnedInt(n);
    }
    if let Ok(n) = raw.parse::<f64>() {
        return AccessorValue::OwnedFloat(n);
    }
    // Bare identifier string (QUERY-SPEC examples use unquoted strings).
    AccessorValue::OwnedString(raw.to_owned())
}

fn values_equal(left: &AccessorValue<'_>, right: &AccessorValue<'_>) -> bool {
    // QUERY-SPEC: no cross-type coercion (`"1"` never equals `1`).
    if left.is_null() && right.is_null() {
        return true;
    }
    if let (Some(a), Some(b)) = (left.as_bool(), right.as_bool()) {
        return a == b;
    }
    if let (Some(a), Some(b)) = (left.as_i128(), right.as_i128()) {
        return a == b;
    }
    if let (Some(a), Some(b)) = (left.as_f64_strict(), right.as_f64_strict()) {
        return a == b;
    }
    match (left.as_str(), right.as_str()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn values_ordered(left: &AccessorValue<'_>, right: &AccessorValue<'_>, op: CmpOp) -> bool {
    // QUERY-SPEC: operator fails when types differ; no universal ordering.
    let ord = if let (Some(a), Some(b)) = (left.as_i128(), right.as_i128()) {
        a.cmp(&b)
    } else if let (Some(a), Some(b)) = (left.as_f64_strict(), right.as_f64_strict()) {
        a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
    } else if let (Some(a), Some(b)) = (left.as_str(), right.as_str()) {
        a.cmp(b)
    } else {
        return false;
    };
    match op {
        CmpOp::Lt => ord.is_lt(),
        CmpOp::Gt => ord.is_gt(),
        CmpOp::Le => ord.is_le(),
        CmpOp::Ge => ord.is_ge(),
        _ => false,
    }
}
