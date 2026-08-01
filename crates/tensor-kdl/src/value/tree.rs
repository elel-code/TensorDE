//! Parse-tree types (feature `dom`) — Glaze `generic` role only.
//!
//! Not used by typed [`crate::read`] / [`crate::write`]. Enable with
//! `--features dom` for suite roundtrip, query, and tooling.

use super::{KdlStr, Value};

/// Entry on a node: positional argument or named property.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry<'a> {
    Argument {
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    },
    Property {
        key: KdlStr<'a>,
        type_name: Option<KdlStr<'a>>,
        value: Value<'a>,
    },
}

impl<'a> Entry<'a> {
    pub fn is_argument(&self) -> bool {
        matches!(self, Self::Argument { .. })
    }

    pub fn is_property(&self) -> bool {
        matches!(self, Self::Property { .. })
    }

    pub fn as_argument(&self) -> Option<(Option<&KdlStr<'a>>, &Value<'a>)> {
        match self {
            Self::Argument { type_name, value } => Some((type_name.as_ref(), value)),
            _ => None,
        }
    }

    pub fn as_property(&self) -> Option<(&KdlStr<'a>, Option<&KdlStr<'a>>, &Value<'a>)> {
        match self {
            Self::Property {
                key,
                type_name,
                value,
            } => Some((key, type_name.as_ref(), value)),
            _ => None,
        }
    }
}

/// A KDL node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node<'a> {
    pub type_name: Option<KdlStr<'a>>,
    pub name: KdlStr<'a>,
    pub entries: Vec<Entry<'a>>,
    pub children: Vec<Node<'a>>,
}

impl<'a> Node<'a> {
    pub fn arguments(&self) -> impl Iterator<Item = &Value<'a>> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Argument { value, .. } => Some(value),
            Entry::Property { .. } => None,
        })
    }

    pub fn properties(&self) -> impl Iterator<Item = (&str, &Value<'a>)> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Property { key, value, .. } => Some((key.as_str(), value)),
            Entry::Argument { .. } => None,
        })
    }

    pub fn property(&self, name: &str) -> Option<&Value<'a>> {
        // Rightmost wins per KDL spec.
        self.entries.iter().rev().find_map(|e| match e {
            Entry::Property { key, value, .. } if key.as_str() == name => Some(value),
            _ => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Node<'a>> {
        self.children.iter().find(|n| n.name.as_str() == name)
    }

    pub fn children_named<'b>(&'b self, name: &'b str) -> impl Iterator<Item = &'b Node<'a>> + 'b {
        self.children
            .iter()
            .filter(move |n| n.name.as_str() == name)
    }
}

/// Top-level KDL document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document<'a> {
    pub nodes: Vec<Node<'a>>,
}

impl<'a> Document<'a> {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node(&self, name: &str) -> Option<&Node<'a>> {
        self.nodes.iter().find(|n| n.name.as_str() == name)
    }
}
