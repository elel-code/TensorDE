//! KDL values and zero-copy string views.

use std::borrow::Cow;
use std::fmt;

/// Borrowed or owned KDL string (Glaze `string_view` analogue when borrowed).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KdlStr<'a>(Cow<'a, str>);

impl<'a> KdlStr<'a> {
    pub fn borrowed(s: &'a str) -> Self {
        Self(Cow::Borrowed(s))
    }

    pub fn owned(s: String) -> Self {
        Self(Cow::Owned(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_owned(self) -> String {
        self.0.into_owned()
    }

    pub fn into_cow(self) -> Cow<'a, str> {
        self.0
    }
}

impl AsRef<str> for KdlStr<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for KdlStr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for KdlStr<'_> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for KdlStr<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// Scalar value with optional type annotation.
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    String(KdlStr<'a>),
    /// Integer with full KDL range preserved as i128 when possible.
    Int(i128),
    /// Floating value. `raw` preserves the lexical form when `f64` cannot
    /// round-trip extreme exponents (e.g. `1.23E+1000`).
    Float {
        value: f64,
        raw: Option<KdlStr<'a>>,
    },
    Bool(bool),
    Null,
}

impl<'a> Value<'a> {
    pub fn float(value: f64) -> Self {
        Self::Float { value, raw: None }
    }

    pub fn float_raw(value: f64, raw: KdlStr<'a>) -> Self {
        Self::Float {
            value,
            raw: Some(raw),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_i128(&self) -> Option<i128> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float { value, .. } => Some(*value),
            Self::Int(n) => Some(*n as f64),
            _ => None,
        }
    }
}

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
