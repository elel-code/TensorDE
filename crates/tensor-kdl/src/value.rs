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

mod tree;

#[cfg(feature = "dom")]
pub use tree::{Document, Entry, Node};

#[cfg(not(feature = "dom"))]
pub(crate) use tree::{Document, Entry, Node};
